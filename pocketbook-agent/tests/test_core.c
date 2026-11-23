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

static void test_paths(void) {
    char buf[256];
    app_from_path(buf, sizeof buf, "/ebrmain/bin/eink-reader.app");
    CHECK(!strcmp(buf, "eink-reader"));
    app_from_path(buf, sizeof buf, "./pocketbook");
    CHECK(!strcmp(buf, "pocketbook"));
    title_from_path(buf, sizeof buf, "/mnt/ext1/Dropbox PocketBook/devid_mitchell-cloud_atlas-61f974f6337ce.fb2.zip");
    CHECK(!strcmp(buf, "devid_mitchell-cloud_atlas-61f974f6337ce"));
    title_from_path(buf, sizeof buf, "/mnt/ext1/Books/Harry-Potter-and-the-Prisoner-of-Azkaban_RuLit_Me_840767.epub");
    CHECK(!strcmp(buf, "Harry-Potter-and-the-Prisoner-of-Azkaban_RuLit_Me_840767"));
    title_from_path(buf, sizeof buf, "/mnt/ext1/Books/Dr. No.pdf");
    CHECK(!strcmp(buf, "Dr. No"));
    title_from_path(buf, sizeof buf, "/mnt/ext1/.hidden");
    CHECK(!strcmp(buf, ".hidden"));
    title_from_path(buf, sizeof buf, "");
    CHECK(!strcmp(buf, ""));
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
    test_paths();
    test_block_json();
    test_suspend();
    if (failures) return 1;
    puts("all tests passed");
    return 0;
}
