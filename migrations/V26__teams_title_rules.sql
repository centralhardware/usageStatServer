-- Microsoft Teams (new client): "<section> | <chat or meeting> | <tenant> | <account>" or, without a
-- section (meeting windows, Calendar), "<name> | <tenant> | <account>" → subject = chat/meeting
-- (or the section when it is the only name), context = tenant; section and account into fields.
INSERT INTO title_rules (bundle_id, pattern, priority) VALUES
    -- Chat | Jane Doe | contoso | a.b@example.com → chat, tenant; section, account
    ('com.microsoft.teams2', '^(?P<section>[^|]+?) \\| (?P<subject>.+?) \\| (?P<context>[^|]+?) \\| (?P<account>[^|\\s]+@[^|\\s]+)$', 50),
    -- Weekly Team Meeting | contoso | a.b@example.com → meeting, tenant; account
    ('com.microsoft.teams2', '^(?P<subject>[^|]+?) \\| (?P<context>[^|]+?) \\| (?P<account>[^|\\s]+@[^|\\s]+)$', 51);
