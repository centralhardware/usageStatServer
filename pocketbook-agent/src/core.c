#include "core.h"

#include <inttypes.h>
#include <stdio.h>
#include <string.h>

int json_string(char *out, size_t cap, const char *s) {
    size_t n = 0;
#define PUT(c) do { if (n + 1 >= cap) return -1; out[n++] = (c); } while (0)
    PUT('"');
    for (const unsigned char *p = (const unsigned char *)s; *p; p++) {
        unsigned char c = *p;
        if (c == '"' || c == '\\') { PUT('\\'); PUT(c); }
        else if (c == '\n') { PUT('\\'); PUT('n'); }
        else if (c == '\r') { PUT('\\'); PUT('r'); }
        else if (c == '\t') { PUT('\\'); PUT('t'); }
        else if (c < 0x20) {
            if (n + 7 >= cap) return -1;
            n += (size_t)snprintf(out + n, cap - n, "\\u%04x", c);
        } else PUT(c);
    }
    PUT('"');
#undef PUT
    out[n] = 0;
    return (int)n;
}

int block_json(char *out, size_t cap, const Device *d, const Block *b) {
    char id[160], name[160], kind[32], bundle[160], app[160], title[800];
    if (json_string(id, sizeof id, d->device_id) < 0 ||
        json_string(name, sizeof name, d->device_name) < 0 ||
        json_string(kind, sizeof kind, b->state.kind) < 0 ||
        json_string(bundle, sizeof bundle, b->state.bundle_id) < 0 ||
        json_string(app, sizeof app, b->state.app_name) < 0 ||
        json_string(title, sizeof title, b->state.title) < 0)
        return -1;
    int n = snprintf(out, cap,
                     "{\"device_id\":%s,\"device_name\":%s,\"kind\":%s,\"bundle_id\":%s,"
                     "\"app_name\":%s,\"title\":%s,\"start_at\":%" PRId64 ",\"stop_at\":%" PRId64 "}",
                     id, name, kind, bundle, app, title, b->start_at, b->stop_at);
    return (n < 0 || (size_t)n >= cap) ? -1 : n;
}

int state_eq(const State *a, const State *b) {
    return !strcmp(a->kind, b->kind) && !strcmp(a->bundle_id, b->bundle_id) &&
           !strcmp(a->title, b->title);
}

static void copy(char *dst, size_t cap, const char *src, size_t len) {
    if (len >= cap) len = cap - 1;
    memcpy(dst, src, len);
    dst[len] = 0;
}

static int on_storage(const char *path) {
    return !strncmp(path, "/mnt/ext1/", 10) || !strncmp(path, "/mnt/ext2/", 10);
}

int state_from_cmdline(State *out, const char *cmdline, size_t len) {
    if (len == 0) return 0;
    const char *argv0 = cmdline;
    const char *end = cmdline + len;
    /* The document is the last argument on device storage that has an extension and isn't
     * an application or a directory of the system area. */
    const char *doc = NULL;
    for (const char *a = argv0 + strlen(argv0) + 1; a < end; a += strlen(a) + 1) {
        const char *slash = strrchr(a, '/');
        const char *dot = slash ? strrchr(slash, '.') : NULL;
        if (on_storage(a) && dot && dot[1] && strcmp(dot, ".app") &&
            strncmp(a, "/mnt/ext1/system/", 17))
            doc = a;
    }
    if (!doc) return 0;

    memset(out, 0, sizeof *out);
    strcpy(out->kind, KIND_APP);
    const char *base = strrchr(argv0, '/');
    base = base ? base + 1 : argv0;
    size_t blen = strlen(base);
    if (blen > 4 && !strcmp(base + blen - 4, ".app")) blen -= 4;
    copy(out->bundle_id, sizeof out->bundle_id, base, blen);
    copy(out->app_name, sizeof out->app_name, base, blen);

    const char *file = strrchr(doc, '/') + 1;
    const char *dot = strrchr(file, '.');
    copy(out->title, sizeof out->title, file, (size_t)(dot - file));
    return 1;
}

int64_t suspended_ms(int64_t wall_delta_ms, int64_t mono_delta_ms, int64_t threshold_ms) {
    int64_t gap = wall_delta_ms - mono_delta_ms;
    return gap > threshold_ms ? gap : 0;
}
