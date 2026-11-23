/* usage-agent for PocketBook: tapping the app starts a background daemon that records which
 * book is open and when the device sleeps, and sends it to usageStatServer as mac_usage
 * blocks. Tapping it again shows whether it runs and how much is waiting to be sent. */
#include "core.h"

#include <curl/curl.h>
#include <dirent.h>
#include <errno.h>
#include <fcntl.h>
#include <signal.h>
#include <stdarg.h>
#include <stdio.h>
#include <stdlib.h>
#include <string.h>
#include <sys/stat.h>
#include <time.h>
#include <unistd.h>
#include <pthread.h>
#include <poll.h>
#include <linux/input.h>

#ifndef NO_INKVIEW
#include <inkview.h>
#endif

#define DIR_PATH "/mnt/ext1/system/usage-agent"
#define CONFIG_PATH DIR_PATH "/config"
#define SPOOL_PATH DIR_PATH "/spool.jsonl"
#define LOG_PATH DIR_PATH "/log"

#define TICK_S 30
#define SEND_EVERY_S 300
#define SUSPEND_THRESHOLD_MS 15000
#define MAX_BATCH 500
#define CURRENT_BOOK "/tmp/.current"

static char server_url[256];
static char token[128];
static Device device = {"pocketbook", "PocketBook"};
/* Longest sleep inside an open book still counted as reading (config reading_gap_s). */
static int64_t reading_gap_s = 600;

static void logf_(const char *fmt, ...) {
    FILE *f = fopen(LOG_PATH, "a");
    if (!f) return;
    time_t t = time(NULL);
    char ts[32];
    strftime(ts, sizeof ts, "%Y-%m-%d %H:%M:%S", localtime(&t));
    fprintf(f, "%s ", ts);
    va_list ap;
    va_start(ap, fmt);
    vfprintf(f, fmt, ap);
    va_end(ap);
    fputc('\n', f);
    fclose(f);
}

static int64_t now_ms(clockid_t clock) {
    struct timespec ts;
    clock_gettime(clock, &ts);
    return (int64_t)ts.tv_sec * 1000 + ts.tv_nsec / 1000000;
}

/* config: key=value lines (server_url, token, device_id, device_name). */
static int load_config(void) {
    FILE *f = fopen(CONFIG_PATH, "r");
    if (!f) return 0;
    char line[512];
    while (fgets(line, sizeof line, f)) {
        line[strcspn(line, "\r\n")] = 0;
        char *eq = strchr(line, '=');
        if (!eq || line[0] == '#') continue;
        *eq = 0;
        const char *k = line, *v = eq + 1;
        if (!strcmp(k, "server_url")) snprintf(server_url, sizeof server_url, "%s", v);
        else if (!strcmp(k, "token")) snprintf(token, sizeof token, "%s", v);
        else if (!strcmp(k, "device_id")) snprintf(device.device_id, sizeof device.device_id, "%s", v);
        else if (!strcmp(k, "reading_gap_s")) reading_gap_s = atoll(v);
        else if (!strcmp(k, "device_name")) snprintf(device.device_name, sizeof device.device_name, "%s", v);
    }
    fclose(f);
    return server_url[0] != 0;
}

static int64_t mtime_of(const char *path) {
    struct stat st;
    return stat(path, &st) == 0 ? (int64_t)st.st_mtime : -1;
}

static void read_current_book(char *title, size_t cap) {
    char path[512] = "";
    FILE *f = fopen(CURRENT_BOOK, "r");
    if (f) { if (!fgets(path, sizeof path, f)) path[0] = 0; fclose(f); }
    path[strcspn(path, "\r\n")] = 0;
    title_from_path(title, cap, path);
}

/* The foreground app from the firmware's task manager, which answers only a registered
 * InkView task: that is why the agent stays resident as one (in the background) instead of
 * forking a plain daemon. The book is the one the active subtask registered, or else
 * /tmp/.current (written by the firmware on open) while the task that was in front when it
 * changed stays in front. */
static void current_state(State *out) {
    memset(out, 0, sizeof *out);
    strcpy(out->kind, KIND_APP);
#ifndef NO_INKVIEW
    static int64_t seen_current = -1;
    static int reader_task = -1;
    static int warned;
    int task = -1, subtask = -1;
    GetActiveTask(&task, &subtask);
    taskinfo *ti = task >= 0 ? GetTaskInfo(task) : NULL;
    if (!ti) {
        if (!warned++) logf_("task manager gave no active task (%d)", task);
        strcpy(out->bundle_id, "unknown");
        strcpy(out->app_name, "unknown");
        return;
    }
    warned = 0;
    /* appname is the executable's path: /ebrmain/bin/eink-reader.app -> eink-reader */
    app_from_path(out->bundle_id, sizeof out->bundle_id, ti->appname && ti->appname[0] ? ti->appname : "unknown");
    snprintf(out->app_name, sizeof out->app_name, "%s", out->bundle_id);
    for (int i = 0; ti->subtasks && i < ti->nsubtasks; i++) {
        subtaskinfo *st = &ti->subtasks[i];
        if (st->id == subtask && st->book && st->book[0]) {
            title_from_path(out->title, sizeof out->title, st->book);
            return;
        }
    }
    int64_t m = mtime_of(CURRENT_BOOK);
    if (m != seen_current) {
        /* At the first look we can't tell who opened it; after that, whoever is in front. */
        static int first = 1;
        reader_task = first ? -1 : task;
        first = 0;
        seen_current = m;
    }
    if (reader_task >= 0 && task == reader_task) read_current_book(out->title, sizeof out->title);
#else
    strcpy(out->bundle_id, "unknown");
    strcpy(out->app_name, "unknown");
#endif
}

static void spool(const Block *b) {
    if (b->stop_at <= b->start_at) return;
    char json[1536];
    if (block_json(json, sizeof json, &device, b) < 0) return;
    FILE *f = fopen(SPOOL_PATH, "a");
    if (!f) { logf_("spool: %s", strerror(errno)); return; }
    fprintf(f, "%s\n", json);
    fclose(f);
}

static size_t discard(char *p, size_t n, size_t m, void *u) { (void)p; (void)u; return n * m; }

static int post(const char *body) {
    CURL *c = curl_easy_init();
    if (!c) return 0;
    char url[300], auth[160];
    snprintf(url, sizeof url, "%s/api/usage", server_url);
    struct curl_slist *h = curl_slist_append(NULL, "Content-Type: application/json");
    if (token[0]) {
        snprintf(auth, sizeof auth, "Authorization: Bearer %s", token);
        h = curl_slist_append(h, auth);
    }
    curl_easy_setopt(c, CURLOPT_URL, url);
    curl_easy_setopt(c, CURLOPT_HTTPHEADER, h);
    curl_easy_setopt(c, CURLOPT_POSTFIELDS, body);
    curl_easy_setopt(c, CURLOPT_TIMEOUT, 20L);
    curl_easy_setopt(c, CURLOPT_CONNECTTIMEOUT, 5L);
    curl_easy_setopt(c, CURLOPT_WRITEFUNCTION, discard);
    /* Use the system CA bundle only where it exists; otherwise keep libcurl's built-in path. */
    if (access("/etc/ssl/certs/ca-certificates.crt", R_OK) == 0)
        curl_easy_setopt(c, CURLOPT_CAINFO, "/etc/ssl/certs/ca-certificates.crt");
    CURLcode rc = curl_easy_perform(c);
    long status = 0;
    curl_easy_getinfo(c, CURLINFO_RESPONSE_CODE, &status);
    curl_slist_free_all(h);
    curl_easy_cleanup(c);
    if (rc != CURLE_OK) {
        /* Logged once per distinct error, so being offline for hours doesn't fill the log. */
        static CURLcode last_rc = CURLE_OK;
        if (rc != last_rc) logf_("POST %s: %s", url, curl_easy_strerror(rc));
        last_rc = rc;
        return 0;
    }
    if (status / 100 != 2) logf_("POST %s: HTTP %ld", url, status);
    /* 422 = a block the server will never take; drop it rather than retry forever. */
    return status / 100 == 2 || status == 422;
}

/* Sends up to MAX_BATCH spooled blocks plus the open one; keeps the rest. */
static int flush(const Block *open_block) {
    FILE *f = fopen(SPOOL_PATH, "r");
    size_t cap = 4096, len = 0;
    char *body = malloc(cap);
    if (!body) { if (f) fclose(f); return 0; }
    len += (size_t)sprintf(body, "{\"blocks\":[");
    int count = 0;
    char line[1600];
    long rest = 0;
    while (f && count < MAX_BATCH && fgets(line, sizeof line, f)) {
        size_t l = strcspn(line, "\n");
        if (!l) continue;
        if (len + l + 4 > cap) { cap = (cap + l) * 2; char *nb = realloc(body, cap); if (!nb) break; body = nb; }
        if (count++) body[len++] = ',';
        memcpy(body + len, line, l);
        len += l;
        rest = ftell(f);
    }
    char open_json[1536];
    if (open_block->stop_at > open_block->start_at &&
        block_json(open_json, sizeof open_json, &device, open_block) > 0) {
        size_t l = strlen(open_json);
        if (len + l + 4 > cap) { cap = (cap + l) * 2; char *nb = realloc(body, cap); if (nb) body = nb; else l = 0; }
        if (l) { if (count) body[len++] = ','; memcpy(body + len, open_json, l); len += l; count++; }
    }
    memcpy(body + len, "]}", 3);
    int ok = !count || post(body);
    if (count && ok) {
        static int first = 1;
        if (first) logf_("sent %d blocks", count);
        first = 0;
    }
    if (count && ok && f) {
        /* Drop what was sent: copy the unsent tail over the file. */
        FILE *tmp = fopen(SPOOL_PATH ".tmp", "w");
        if (tmp) {
            fseek(f, rest, SEEK_SET);
            size_t n;
            char buf[4096];
            while ((n = fread(buf, 1, sizeof buf, f))) fwrite(buf, 1, n, tmp);
            fclose(tmp);
            rename(SPOOL_PATH ".tmp", SPOOL_PATH);
        }
    }
    if (f) fclose(f);
    free(body);
    return ok;
}

/* Every input device (touch screen, page buttons): apps run as root, so the agent sees each
 * tap system-wide, though InkView sends events only to the app in front. */
#define MAX_INPUTS 8
static struct pollfd inputs[MAX_INPUTS];
static int n_inputs;

static void open_inputs(void) {
    DIR *d = opendir("/dev/input");
    if (!d) logf_("/dev/input: %s (uid %d)", strerror(errno), (int)getuid());
    struct dirent *e;
    while (d && (e = readdir(d)) && n_inputs < MAX_INPUTS) {
        if (e->d_name[0] == '.') continue;
        if (strncmp(e->d_name, "event", 5)) { logf_("/dev/input/%s: skipped", e->d_name); continue; }
        char p[300];
        snprintf(p, sizeof p, "/dev/input/%s", e->d_name);
        int fd = open(p, O_RDONLY | O_NONBLOCK);
        if (fd < 0) { logf_("%s: %s (uid %d)", p, strerror(errno), (int)getuid()); continue; }
        inputs[n_inputs].fd = fd;
        inputs[n_inputs].events = POLLIN;
        n_inputs++;
    }
    if (d) closedir(d);
    logf_("listening on %d input devices", n_inputs);
}

/* Waits up to timeout_ms for input; returns the presses (touch-downs and button presses). */
static int wait_presses(int timeout_ms) {
    if (!n_inputs) { usleep((useconds_t)timeout_ms * 1000); return 0; }
    if (poll(inputs, (nfds_t)n_inputs, timeout_ms) <= 0) return 0;
    int presses = 0;
    for (int i = 0; i < n_inputs; i++) {
        if (!(inputs[i].revents & POLLIN)) continue;
        struct input_event ev[32];
        ssize_t n;
        while ((n = read(inputs[i].fd, ev, sizeof ev)) > 0)
            for (size_t j = 0; j < (size_t)n / sizeof ev[0]; j++)
                if (ev[j].type == EV_KEY && ev[j].value == 1) presses++;
    }
    return presses;
}

static void daemon_loop(void) {
    curl_global_init(CURL_GLOBAL_DEFAULT);
    logf_("started as %s, sending to %s", device.device_id, server_url);
    open_inputs();
    Block cur = {0};
    int64_t last_wall = now_ms(CLOCK_REALTIME), last_mono = now_ms(CLOCK_MONOTONIC);
    int64_t last_send = 0, last_press = 0;
    int failures = 0, presses = 0;
    for (;;) {
        int64_t wall = now_ms(CLOCK_REALTIME), mono = now_ms(CLOCK_MONOTONIC);
        int64_t slept = suspended_ms(wall - last_wall, mono - last_mono, SUSPEND_THRESHOLD_MS);
        int woke = slept != 0;
        /* The firmware suspends the device a few seconds after each page turn, so while a
         * book is open a sleep shorter than reading_gap_s is reading: the block goes on. */
        if (slept && cur.state.title[0] && slept <= reading_gap_s * 1000) slept = 0;
        if (slept && cur.state.bundle_id[0]) {
            /* The block ended at the last tap before the sleep (or the last look, without one). */
            int64_t end = last_press > cur.start_at ? last_press : last_wall;
            cur.stop_at = end;
            spool(&cur);
            Block s = {{KIND_AWAY, AWAY_SLEEP, AWAY_SLEEP, ""}, end, wall - (mono - last_mono), 0};
            spool(&s);
            memset(&cur, 0, sizeof cur);
        }
        State st;
        current_state(&st);
        if (!state_eq(&st, &cur.state)) logf_("now: %s \"%s\"", st.bundle_id, st.title);
        if (!cur.state.bundle_id[0]) {
            cur.state = st;
            cur.start_at = wall;
        } else if (!state_eq(&st, &cur.state)) {
            cur.stop_at = wall;
            spool(&cur);
            memset(&cur, 0, sizeof cur);
            cur.state = st;
            cur.start_at = wall;
        }
        cur.page_turns += presses;
        cur.stop_at = wall;
        /* Every 5 minutes, at start and on waking; after a failure (Wi-Fi takes a while to
         * come back after sleep) again every 30 s for the next 5 minutes. */
        if (woke) failures = 0;
        if (woke || !last_send || mono - last_send >= SEND_EVERY_S * 1000 ||
            (failures && failures < SEND_EVERY_S / TICK_S && mono - last_send >= TICK_S * 1000)) {
            if (flush(&cur)) failures = 0;
            else failures++;
            last_send = mono;
        }
        last_wall = wall;
        last_mono = mono;
        /* Next look: at the next tap (the app or book may have changed) or after TICK_S. */
        presses = 0;
        while (!presses) {
            int64_t left = TICK_S * 1000 - (now_ms(CLOCK_MONOTONIC) - mono);
            if (left <= 0) break;
            presses = wait_presses((int)left);
        }
        if (presses) last_press = now_ms(CLOCK_REALTIME);
    }
}

static char status_text[512];

static int count_spooled(void) {
    FILE *f = fopen(SPOOL_PATH, "r");
    if (!f) return 0;
    int n = 0, c;
    while ((c = fgetc(f)) != EOF) n += c == '\n';
    fclose(f);
    return n;
}

static void *loop_thread(void *arg) {
    (void)arg;
    daemon_loop();
    return NULL;
}

#ifndef NO_INKVIEW
static int configured, started;

/* Tapping the app brings this same resident task to the front: show the status, then go
 * back to the background (CloseApp would end the agent). */
static int handler(int type, int p1, int p2) {
    (void)p1; (void)p2;
    if (type == EVT_SHOW) {
        if (!configured)
            snprintf(status_text, sizeof status_text,
                     "No server_url in %s.\nCreate it with server_url=, token=, device_id= lines.", CONFIG_PATH);
        else
            snprintf(status_text, sizeof status_text, "%s as %s.\n%d blocks waiting to be sent.\nLog: %s",
                     started++ ? "Running" : "Started", device.device_id, count_spooled(), LOG_PATH);
        Message(ICON_INFORMATION, "Usage agent", status_text, 4000);
        if (configured) GoToBackground();
        else CloseApp();
    }
    return 0;
}
#endif

int main(int argc, char **argv) {
    (void)argc; (void)argv;
    mkdir(DIR_PATH, 0755);
    signal(SIGPIPE, SIG_IGN);
#ifndef NO_INKVIEW
    configured = load_config();
    if (configured) {
        pthread_t t;
        pthread_create(&t, NULL, loop_thread, NULL);
    }
    InkViewMain(handler);
#else
    /* Host build: no task manager; for a syntax check and a dry run of the loop. */
    if (load_config()) loop_thread(NULL);
#endif
    return 0;
}
