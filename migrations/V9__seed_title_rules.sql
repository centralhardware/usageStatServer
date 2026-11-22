-- Starting rules for the apps seen so far; edit the table from here on (new rules apply within
-- 5 minutes and stored titles are re-parsed). Groups other than subject/context land in
-- mac_usage.fields. Lower priority is tried first. ClickHouse string literals: a regex backslash is \\.
INSERT INTO title_rules (bundle_id, pattern, priority) VALUES
    -- dotclaude – mac_usage [default@10.168.0.32] → project, file/tab, database connection
    ('com.jetbrains.intellij', '^(?P<context>.+?) – (?P<subject>.+?)(?: \\[(?P<db_user>[^@\\]]+)@(?P<db_host>[^\\]]+)\\])?$', 100),
    -- root — [coolify] lazydocker ~ — ssh root@10.168.0.77 → shell user — tab/dir — ssh target
    ('com.apple.Terminal', '^(?P<user>[^—]+?) — (?P<context>.+?) — (?P<subject>(?P<command>ssh)(?: -\\S+)* (?:(?P<ssh_user>[^@\\s]+)@)?(?P<ssh_host>[^\\s—]+)[^—]*)$', 50),
    -- alex — ~ — -fish → shell user — tab/dir — command (first word in fields)
    ('com.apple.Terminal', '^(?P<user>[^—]+?) — (?P<context>.+?) — (?P<subject>-?(?P<command>[^\\s—]+)[^—]*)$', 100),
    -- Прочиточная @ central hardware → chat, account
    ('com.tdesktop.Telegram', '^(?P<subject>.+?) @ (?P<context>.+)$', 100),
    ('ru.keepcoder.Telegram', '^(?P<subject>.+?) @ (?P<context>.+)$', 100),
    -- GitHub: <title> by <author> · Pull Request #5 · owner/repo → title, repo; author, kind, number
    ('com.apple.Safari', '^(?P<subject>.+?)(?: by (?P<author>[^·]+?))? · (?P<kind>Pull Request|Issue|Discussion) #(?P<number>\\d+) · (?P<context>[\\w.-]+/[\\w.-]+)$', 50),
    ('com.google.Chrome', '^(?P<subject>.+?)(?: by (?P<author>[^·]+?))? · (?P<kind>Pull Request|Issue|Discussion) #(?P<number>\\d+) · (?P<context>[\\w.-]+/[\\w.-]+)$', 50),
    -- GitHub: <message> · owner/repo@2b5d60b → commit message, repo; commit
    ('com.apple.Safari', '^(?P<subject>.+) · (?P<context>[\\w.-]+/[\\w.-]+)@(?P<commit>[0-9a-f]{7,40})$', 51),
    ('com.google.Chrome', '^(?P<subject>.+) · (?P<context>[\\w.-]+/[\\w.-]+)@(?P<commit>[0-9a-f]{7,40})$', 51),
    -- GitHub: Commits · owner/repo, usageStatServer/Casks at main · owner/repo → page, repo
    ('com.apple.Safari', '^(?P<subject>.+) · (?P<context>[\\w.-]+/[\\w.-]+)$', 52),
    ('com.google.Chrome', '^(?P<subject>.+) · (?P<context>[\\w.-]+/[\\w.-]+)$', 52),
    -- <page> - YouTube, <page> | Pangear, <page> — PeakHour, <post> : r/macsysadmin → page, site (the last separator)
    ('com.apple.Safari', '^(?P<subject>.+)(?: - | \\| | — | : )(?P<context>[^|—]+?)$', 100),
    ('com.google.Chrome', '^(?P<subject>.+)(?: - | \\| | — | : )(?P<context>[^|—]+?)$', 100),
    -- processed — iCloud – 0 messages → mailbox, account; message count
    ('com.apple.mail', '^(?P<subject>.+?) — (?P<context>.+?)(?: – (?P<messages>\\d+) messages?)?$', 100);
