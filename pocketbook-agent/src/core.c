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

int64_t suspended_ms(int64_t wall_delta_ms, int64_t mono_delta_ms, int64_t threshold_ms) {
    int64_t gap = wall_delta_ms - mono_delta_ms;
    return gap > threshold_ms ? gap : 0;
}

void app_from_path(char *out, size_t cap, const char *path) {
    const char *base = strrchr(path, '/');
    base = base ? base + 1 : path;
    size_t len = strlen(base);
    if (len > 4 && !strcmp(base + len - 4, ".app")) len -= 4;
    copy(out, cap, base, len);
}

void title_from_path(char *out, size_t cap, const char *path) {
    const char *base = strrchr(path, '/');
    base = base ? base + 1 : path;
    size_t len = strlen(base);
    /* Drop up to two extensions (Dune.fb2.zip), but never the whole name. */
    for (int i = 0; i < 2; i++) {
        const char *dot = NULL;
        for (const char *p = base; p < base + len; p++) if (*p == '.') dot = p;
        if (!dot || dot == base || base + len - dot > 6 || base + len - dot < 2) break;
        int alnum = 1;
        for (const char *p = dot + 1; p < base + len; p++)
            if (!((*p >= 'a' && *p <= 'z') || (*p >= 'A' && *p <= 'Z') || (*p >= '0' && *p <= '9'))) alnum = 0;
        if (!alnum) break;
        len = (size_t)(dot - base);
    }
    copy(out, cap, base, len);
}
