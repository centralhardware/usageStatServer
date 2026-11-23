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

#ifndef NO_INKVIEW
/* The foreground task from the firmware's task manager: its app name, and the book when the
 * active subtask has one (readers register it with SetSubtaskInfo). Returns 0 when the task
 * manager can't be asked. */
static int task_state(State *out) {
    int task = -1, subtask = -1;
    GetActiveTask(&task, &subtask);
    taskinfo *ti = task >= 0 ? GetTaskInfo(task) : NULL;
    if (!ti) return 0;
    memset(out, 0, sizeof *out);
    strcpy(out->kind, KIND_APP);
    snprintf(out->bundle_id, sizeof out->bundle_id, "%s", ti->appname ? ti->appname : "unknown");
    snprintf(out->app_name, sizeof out->app_name, "%s", out->bundle_id);
    for (int i = 0; ti->subtasks && i < ti->nsubtasks; i++) {
        subtaskinfo *s = &ti->subtasks[i];
        if (s->id != subtask) continue;
        if (s->book && s->book[0]) {
            /* name is the title the reader shows; fall back to the file name. */
            if (s->name && s->name[0]) snprintf(out->title, sizeof out->title, "%s", s->name);
            else {
                const char *b = strrchr(s->book, '/');
                snprintf(out->title, sizeof out->title, "%s", b ? b + 1 : s->book);
            }
        }
        break;
    }
    return 1;
}

/* Once at startup: every task the firmware knows, to see what the readers register. */
static void log_tasks(void) {
    int list[64];
    int n = GetTaskList(list, 64);
    int at = -1, as = -1;
    GetActiveTask(&at, &as);
    logf_("tasks: %d, active %d/%d", n, at, as);
    for (int i = 0; i < n && i < 64; i++) {
        taskinfo *ti = GetTaskInfo(list[i]);
        if (!ti) { logf_("  task %d: no info", list[i]); continue; }
        logf_("  task %d app=%s pid=%d", list[i], ti->appname ? ti->appname : "-", (int)ti->mainpid);
        for (int j = 0; ti->subtasks && j < ti->nsubtasks; j++)
            logf_("    sub %d name=%s book=%s", ti->subtasks[j].id,
                  ti->subtasks[j].name ? ti->subtasks[j].name : "-",
                  ti->subtasks[j].book ? ti->subtasks[j].book : "-");
    }
}
#endif

/* The foreground app and its book; without the task manager, a book open in any process,
 * else the library (home screen, settings…). */
static void current_state(State *out) {
#ifndef NO_INKVIEW
    static int warned;
    if (task_state(out)) return;
    if (!warned++) logf_("task manager unavailable, falling back to /proc");
#endif
    DIR *proc = opendir("/proc");
    struct dirent *e;
    while (proc && (e = readdir(proc))) {
        if (e->d_name[0] < '0' || e->d_name[0] > '9') continue;
        char path[64], buf[2048];
        snprintf(path, sizeof path, "/proc/%s/cmdline", e->d_name);
        int fd = open(path, O_RDONLY);
        if (fd < 0) continue;
        ssize_t n = read(fd, buf, sizeof buf - 1);
        close(fd);
        if (n > 0 && buf[n - 1] != 0) buf[n++] = 0;
        if (n > 0 && state_from_cmdline(out, buf, (size_t)n)) {
            closedir(proc);
            return;
        }
    }
    if (proc) closedir(proc);
    memset(out, 0, sizeof *out);
    strcpy(out->kind, KIND_APP);
    strcpy(out->bundle_id, "library");
    strcpy(out->app_name, "Library");
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
static void flush(const Block *open_block) {
    FILE *f = fopen(SPOOL_PATH, "r");
    size_t cap = 4096, len = 0;
    char *body = malloc(cap);
    if (!body) { if (f) fclose(f); return; }
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
    if (count && post(body) && f) {
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
}

static void log_file(const char *path) {
    struct stat st;
    if (stat(path, &st) != 0) { logf_("  %s: missing", path); return; }
    char buf[512] = "";
    FILE *f = fopen(path, "r");
    if (f && S_ISREG(st.st_mode)) {
        size_t n = fread(buf, 1, sizeof buf - 1, f);
        buf[n] = 0;
        for (size_t i = 0; i < n; i++) if (buf[i] == '\n' || buf[i] == 0) buf[i] = '|';
    }
    if (f) fclose(f);
    logf_("  %s: mtime %ld size %ld: %s", path, (long)st.st_mtime, (long)st.st_size, buf);
}

static void log_dir(const char *path) {
    DIR *d = opendir(path);
    if (!d) { logf_("  %s/: missing", path); return; }
    struct dirent *e;
    while ((e = readdir(d))) {
        if (e->d_name[0] == '.' && (!e->d_name[1] || (e->d_name[1] == '.' && !e->d_name[2]))) continue;
        char p[512];
        snprintf(p, sizeof p, "%s/%s", path, e->d_name);
        log_file(p);
    }
    closedir(d);
}

/* Diagnostic dump (temporary): where the firmware might say which book is open. */
static void log_diagnostics(void) {
    logf_("diag: files");
    log_file("/tmp/.current");
    log_file("/tmp/history.txt");
    log_dir("/var/run/task");
    log_dir("/mnt/ext1/system/state");
    log_dir("/mnt/ext1/system/explorer-3");
    logf_("diag: processes");
    DIR *proc = opendir("/proc");
    struct dirent *e;
    while (proc && (e = readdir(proc))) {
        if (e->d_name[0] < '0' || e->d_name[0] > '9') continue;
        char path[64], buf[600];
        snprintf(path, sizeof path, "/proc/%s/cmdline", e->d_name);
        int fd = open(path, O_RDONLY);
        if (fd < 0) continue;
        ssize_t n = read(fd, buf, sizeof buf - 1);
        close(fd);
        if (n <= 0) continue; /* kernel thread */
        for (ssize_t i = 0; i < n; i++) if (!buf[i]) buf[i] = ' ';
        buf[n] = 0;
        logf_("  %s: %s", e->d_name, buf);
    }
    if (proc) closedir(proc);
}

static void daemon_loop(void) {
    curl_global_init(CURL_GLOBAL_DEFAULT);
    logf_("started as %s, sending to %s", device.device_id, server_url);
#ifndef NO_INKVIEW
    log_tasks();
#endif
    Block cur = {0};
    int64_t last_wall = now_ms(CLOCK_REALTIME), last_mono = now_ms(CLOCK_MONOTONIC);
    int64_t last_send = 0;
    int tick = 0;
    for (;;) {
        /* 1 and 3 minutes after start: open a book in between to see what changes. */
        if (tick == 2 || tick == 6) log_diagnostics();
        tick++;
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
        if (woke || mono - last_send >= SEND_EVERY_S * 1000) {
            flush(&cur);
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
