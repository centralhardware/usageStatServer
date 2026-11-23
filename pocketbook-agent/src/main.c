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

#ifndef NO_INKVIEW
#include <inkview.h>
#endif

#define DIR_PATH "/mnt/ext1/system/usage-agent"
#define CONFIG_PATH DIR_PATH "/config"
#define SPOOL_PATH DIR_PATH "/spool.jsonl"
#define PID_PATH DIR_PATH "/pid"
#define LOG_PATH DIR_PATH "/log"

#define TICK_S 30
#define SEND_EVERY_S 300
#define SUSPEND_THRESHOLD_MS 15000
#define MAX_BATCH 500
#define TASK_DIR "/var/run/task"
#define CURRENT_BOOK "/tmp/.current"
/* How close the reader's coming to the front must be to the book's open. */
#define BOOK_OPEN_WINDOW_S 15

static char server_url[256];
static char token[128];
static Device device = {"pocketbook", "PocketBook"};

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
        else if (!strcmp(k, "device_name")) snprintf(device.device_name, sizeof device.device_name, "%s", v);
    }
    fclose(f);
    return server_url[0] != 0;
}

static int64_t mtime_of(const char *path) {
    struct stat st;
    return stat(path, &st) == 0 ? (int64_t)st.st_mtime : -1;
}

/* The foreground task: the firmware's task manager touches /var/run/task/<pid> when a task
 * comes to the front, so the newest entry of a live process is the one on screen. */
/* A running process: zombies (an app that just exited) have an empty cmdline. */
static int alive(pid_t pid) {
    char p[64], c;
    snprintf(p, sizeof p, "/proc/%d/cmdline", (int)pid);
    int fd = open(p, O_RDONLY);
    if (fd < 0) return 0;
    ssize_t n = read(fd, &c, 1);
    close(fd);
    return n == 1;
}

/* Temporary: each task file's mtime and the strings in it, to learn how the task manager
 * marks the front task. Logged on each of the first 30 ticks (15 minutes). */
static void log_task_files(void) {
    static int left = 30;
    if (left-- <= 0) return;
    DIR *d = opendir(TASK_DIR);
    if (!d) return;
    struct dirent *e;
    while ((e = readdir(d))) {
        if (e->d_name[0] == '.') continue;
        char p[300], buf[256], out[512];
        snprintf(p, sizeof p, TASK_DIR "/%s", e->d_name);
        FILE *f = fopen(p, "rb");
        if (!f) continue;
        size_t n = fread(buf, 1, sizeof buf, f);
        fclose(f);
        size_t o = 0;
        for (size_t i = 0; i < n && o + 4 < sizeof out; i++) {
            unsigned char c = (unsigned char)buf[i];
            o += (size_t)snprintf(out + o, sizeof out - o, "%02x", c);
        }
        out[o] = 0;
        logf_("  task %s alive=%d mtime=%lld %s", e->d_name, alive(atoi(e->d_name)),
              (long long)mtime_of(p), out);
    }
    closedir(d);
}

static pid_t foreground(int64_t *since) {
    DIR *d = opendir(TASK_DIR);
    if (!d) return 0;
    pid_t best = 0;
    int64_t best_m = -1;
    struct dirent *e;
    while ((e = readdir(d))) {
        pid_t pid = (pid_t)atoi(e->d_name);
        if (pid <= 0 || !alive(pid)) continue;
        char p[300];
        snprintf(p, sizeof p, TASK_DIR "/%s", e->d_name);
        int64_t m = mtime_of(p);
        if (m > best_m) { best_m = m; best = pid; }
    }
    closedir(d);
    *since = best_m;
    return best;
}

/* The foreground app, with the open book when that app is the one that opened it: the
 * firmware writes the book's path to /tmp/.current on open, so the task that came to the
 * front at that moment is the reader. */
static void current_state(State *out) {
    static int64_t seen_current = -1;
    static pid_t reader = 0;
    memset(out, 0, sizeof *out);
    strcpy(out->kind, KIND_APP);

    int64_t since = -1;
    pid_t fg = foreground(&since);
    char cmd[512] = "";
    if (fg) {
        char p[64];
        snprintf(p, sizeof p, "/proc/%d/cmdline", (int)fg);
        int fd = open(p, O_RDONLY);
        if (fd >= 0) { ssize_t n = read(fd, cmd, sizeof cmd - 1); cmd[n > 0 ? n : 0] = 0; close(fd); }
    }
    if (!cmd[0]) {
        strcpy(out->bundle_id, "unknown");
        strcpy(out->app_name, "unknown");
        return;
    }
    app_from_path(out->bundle_id, sizeof out->bundle_id, cmd);
    snprintf(out->app_name, sizeof out->app_name, "%s", out->bundle_id);

    int64_t cur_m = mtime_of(CURRENT_BOOK);
    if (cur_m != seen_current) {
        seen_current = cur_m;
        reader = (cur_m >= 0 && since >= 0 && llabs(since - cur_m) <= BOOK_OPEN_WINDOW_S) ? fg : 0;
    }
    if (reader && fg == reader) {
        char path[512] = "";
        FILE *f = fopen(CURRENT_BOOK, "r");
        if (f) { if (!fgets(path, sizeof path, f)) path[0] = 0; fclose(f); }
        path[strcspn(path, "\r\n")] = 0;
        title_from_path(out->title, sizeof out->title, path);
    }
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

static void daemon_loop(void) {
    curl_global_init(CURL_GLOBAL_DEFAULT);
    logf_("started as %s, sending to %s", device.device_id, server_url);
    Block cur = {0};
    int64_t last_wall = now_ms(CLOCK_REALTIME), last_mono = now_ms(CLOCK_MONOTONIC);
    int64_t last_send = 0;
    int failures = 0;
    for (;;) {
        int64_t wall = now_ms(CLOCK_REALTIME), mono = now_ms(CLOCK_MONOTONIC);
        int64_t slept = suspended_ms(wall - last_wall, mono - last_mono, SUSPEND_THRESHOLD_MS);
        int woke = 0;
        if (slept && cur.state.bundle_id[0]) {
            /* The open block ended when the device went to sleep, roughly at the last tick. */
            cur.stop_at = last_wall;
            spool(&cur);
            Block s = {{KIND_AWAY, AWAY_SLEEP, AWAY_SLEEP, ""}, last_wall, wall - (mono - last_mono)};
            spool(&s);
            memset(&cur, 0, sizeof cur);
            woke = 1;
        }
        State st;
        current_state(&st);
        if (!state_eq(&st, &cur.state)) logf_("now: %s \"%s\"", st.bundle_id, st.title);
        log_task_files();
        if (!cur.state.bundle_id[0]) {
            cur.state = st;
            cur.start_at = wall;
        } else if (!state_eq(&st, &cur.state)) {
            cur.stop_at = wall;
            spool(&cur);
            cur.state = st;
            cur.start_at = wall;
        }
        cur.stop_at = wall;
        /* Every 5 minutes, at start and on waking; after a failure (Wi-Fi takes a while to
         * come back after sleep) again on each tick for the next 5 minutes. */
        if (woke) failures = 0;
        if (woke || !last_send || mono - last_send >= SEND_EVERY_S * 1000 ||
            (failures && failures < SEND_EVERY_S / TICK_S)) {
            if (flush(&cur)) failures = 0;
            else failures++;
            last_send = mono;
        }
        last_wall = wall;
        last_mono = mono;
        sleep(TICK_S);
    }
}

static pid_t running_pid(void) {
    FILE *f = fopen(PID_PATH, "r");
    if (!f) return 0;
    int pid = 0;
    if (fscanf(f, "%d", &pid) != 1) pid = 0;
    fclose(f);
    return (pid > 0 && kill(pid, 0) == 0) ? pid : 0;
}

/* Detaches a daemon child; returns its pid in the parent. */
static pid_t start_daemon(void) {
    pid_t pid = fork();
    if (pid != 0) return pid;
    setsid();
    if (fork() != 0) _exit(0);
    signal(SIGHUP, SIG_IGN);
    int null = open("/dev/null", O_RDWR);
    dup2(null, 0); dup2(null, 1); dup2(null, 2);
    FILE *f = fopen(PID_PATH, "w");
    if (f) { fprintf(f, "%d\n", getpid()); fclose(f); }
    daemon_loop();
    _exit(0);
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

#ifndef NO_INKVIEW
static int handler(int type, int p1, int p2) {
    (void)p1; (void)p2;
    if (type == EVT_SHOW) {
        Message(ICON_INFORMATION, "Usage agent", status_text, 5000);
        CloseApp();
    }
    return 0;
}
#endif

int main(int argc, char **argv) {
    mkdir(DIR_PATH, 0755);
    int foreground = argc > 1 && !strcmp(argv[1], "--foreground");
    if (!load_config()) {
        snprintf(status_text, sizeof status_text,
                 "No server_url in %s.\nCreate it with server_url=, token=, device_id= lines.", CONFIG_PATH);
    } else if (foreground) {
        daemon_loop();
    } else if (running_pid()) {
        snprintf(status_text, sizeof status_text, "Running as %s.\n%d blocks waiting to be sent.",
                 device.device_id, count_spooled());
    } else {
        start_daemon();
        snprintf(status_text, sizeof status_text, "Started as %s.\nSending to %s.\nLog: %s",
                 device.device_id, server_url, LOG_PATH);
    }
#ifndef NO_INKVIEW
    InkViewMain(handler);
#else
    puts(status_text);
#endif
    return 0;
}
