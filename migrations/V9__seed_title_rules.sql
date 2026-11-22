-- Starting rules for the apps seen so far; edit the table from here on (new rules apply within
-- 5 minutes and stored titles are re-parsed). ClickHouse string literals: a regex backslash is \\.
INSERT INTO title_rules (bundle_id, pattern) VALUES
    -- dotclaude – mac_usage [default@10.168.0.32] → project, file/tab (the [connection] dropped)
    ('com.jetbrains.intellij', '^(?P<context>.+?) – (?P<subject>.+?)(?: \\[[^\\]]*\\])?$'),
    -- root — [coolify] lazydocker ~ — ssh root@10.168.0.77 → user — tab/dir — command
    ('com.apple.Terminal', '^[^—]+ — (?P<context>.+?) — (?P<subject>[^—]+)$'),
    -- Прочиточная @ central hardware → chat, account
    ('com.tdesktop.Telegram', '^(?P<subject>.+?) @ (?P<context>.+)$'),
    ('ru.keepcoder.Telegram', '^(?P<subject>.+?) @ (?P<context>.+)$'),
    -- <page> - YouTube → page, site (the last " - " part)
    ('com.apple.Safari', '^(?P<subject>.+) - (?P<context>[^-]+)$'),
    ('com.google.Chrome', '^(?P<subject>.+) - (?P<context>[^-]+)$'),
    -- processed — iCloud – 0 messages → mailbox, account
    ('com.apple.mail', '^(?P<subject>.+?) — (?P<context>.+?)(?: – \\d+ messages?)?$');
