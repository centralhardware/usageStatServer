-- Title rules for apps that started showing up after V11/V26 (V37/V38 are left to the media fixes).
-- ClickHouse string literals: a regex backslash is \\.
INSERT INTO title_rules (bundle_id, pattern, priority) VALUES
    -- report.pdf – Page 3 of 12 → file; page, pages (the page no longer splits one file into many contexts)
    ('com.apple.Preview', '^(?P<subject>.+?) – Page (?P<page>\\d+) of (?P<pages>\\d+)$', 50),
    -- any other Preview window: the whole title is the file, not split on " - "
    ('com.apple.Preview', '^(?P<subject>.+)$', 100),
    -- Searching “All” – 7 Items → group; search, items
    ('com.apple.Passwords', '^(?P<search>Searching) “(?P<subject>[^”]+)” – (?P<items>\\d+) Items?$', 50),
    -- All – 274 Items → group; items
    ('com.apple.Passwords', '^(?P<subject>.+?) – (?P<items>\\d+) Items?$', 100),
    -- (env-1) Client Application - 1.2.3 (user: jdoe) → application, environment; version, user
    ('net.java.openjdk.cmd', '^(?:\\((?P<context>[^)]+)\\) )?(?P<subject>.+?) - (?P<version>\\d+(?:\\.\\d+)+)(?: \\(user: (?P<user>[^)]+)\\))?$', 50),
    -- Pages / TextEdit: the title is the document name, even with " - " inside it
    ('com.apple.Pages', '^(?P<subject>.+)$', 100),
    ('com.apple.TextEdit', '^(?P<subject>.+)$', 100);
