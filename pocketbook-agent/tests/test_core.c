#include "../src/core.h"

#include <stdio.h>
#include <string.h>

static int failures;
#define CHECK(cond) do { if (!(cond)) { printf("FAIL %s:%d: %s\n", __FILE__, __LINE__, #cond); failures++; } } while (0)

static void test_json_string(void) {
    char buf[64];
    CHECK(json_string(buf, sizeof buf, "a\"b\\c\n\x01") > 0);
    CHECK(!strcmp(buf, "\"a\\\"b\\\\c\\n\\u0001\""));
    CHECK(json_string(buf, 4, "toolong") == -1);
    CHECK(json_string(buf, sizeof buf, "Война и мир") > 0);
    CHECK(!strcmp(buf, "\"Война и мир\""));
}

static void test_cmdline(void) {
    State s;
    const char reader[] = "/ebrmain/bin/reader.app\0-x\0/mnt/ext1/Books/War and Peace.fb2\0";
    CHECK(state_from_cmdline(&s, reader, sizeof reader - 1));
    CHECK(!strcmp(s.kind, "app"));
    CHECK(!strcmp(s.bundle_id, "reader"));
    CHECK(!strcmp(s.title, "War and Peace"));

    const char sd[] = "/ebrmain/bin/pdfviewer\0/mnt/ext2/a.b.pdf\0";
    CHECK(state_from_cmdline(&s, sd, sizeof sd - 1));
    CHECK(!strcmp(s.bundle_id, "pdfviewer"));
    CHECK(!strcmp(s.title, "a.b"));

    const char shell[] = "/ebrmain/bin/bookshelf.app\0";
    CHECK(!state_from_cmdline(&s, shell, sizeof shell - 1));
    const char app[] = "/ebrmain/bin/x\0/mnt/ext1/applications/usage-agent.app\0";
    CHECK(!state_from_cmdline(&s, app, sizeof app - 1));
    const char sys[] = "/ebrmain/bin/x\0/mnt/ext1/system/config/a.cfg\0";
    CHECK(!state_from_cmdline(&s, sys, sizeof sys - 1));
    const char dir[] = "/ebrmain/bin/x\0/mnt/ext1/Books\0";
    CHECK(!state_from_cmdline(&s, dir, sizeof dir - 1));
}

static void test_block_json(void) {
    Device d = {"pocketbook-619", "PocketBook"};
    Block b = {{"app", "reader", "reader", "Dune"}, 1000, 2000};
    char buf[512];
    CHECK(block_json(buf, sizeof buf, &d, &b) > 0);
    CHECK(!strcmp(buf, "{\"device_id\":\"pocketbook-619\",\"device_name\":\"PocketBook\",\"kind\":\"app\","
                       "\"bundle_id\":\"reader\",\"app_name\":\"reader\",\"title\":\"Dune\","
                       "\"start_at\":1000,\"stop_at\":2000}"));
    CHECK(block_json(buf, 10, &d, &b) == -1);
}

static void test_suspend(void) {
    CHECK(suspended_ms(30000, 30000, 10000) == 0);
    CHECK(suspended_ms(3600000, 30000, 10000) == 3570000);
    CHECK(suspended_ms(30000, 31000, 10000) == 0);
}

int main(void) {
    test_json_string();
    test_cmdline();
    test_block_json();
    test_suspend();
    if (failures) return 1;
    puts("all tests passed");
    return 0;
}
