/* Device-independent logic of the PocketBook agent, unit-tested on the host. */
#ifndef USAGE_CORE_H
#define USAGE_CORE_H

#include <stddef.h>
#include <stdint.h>

#define KIND_APP "app"
#define KIND_AWAY "away"
#define AWAY_SLEEP "sleep"

/* What the device is doing right now. Empty bundle_id = nothing seen yet. */
typedef struct {
    char kind[8];
    char bundle_id[64];  /* reader process name, or "sleep" */
    char app_name[64];
    char title[256];     /* book file name without extension */
} State;

typedef struct {
    State state;
    int64_t start_at;    /* unix ms */
    int64_t stop_at;
} Block;

typedef struct {
    char device_id[64];
    char device_name[64];
} Device;

/* Writes `s` as a JSON string literal (quoted, escaped) into out. Returns the length,
 * or -1 when it does not fit. */
int json_string(char *out, size_t cap, const char *s);

/* One element of the blocks array of POST /api/usage. Returns the length or -1. */
int block_json(char *out, size_t cap, const Device *d, const Block *b);

int state_eq(const State *a, const State *b);

/* From a process's argv (NUL-separated cmdline, len bytes): when it opened a document on
 * the device storage, fills an app State (process name, title from the file name) and
 * returns 1; else 0. */
int state_from_cmdline(State *out, const char *cmdline, size_t len);

/* Wall-clock time beyond monotonic time between two ticks means the device was suspended
 * that long. Returns the suspended milliseconds when over threshold_ms, else 0. */
int64_t suspended_ms(int64_t wall_delta_ms, int64_t mono_delta_ms, int64_t threshold_ms);

#endif
