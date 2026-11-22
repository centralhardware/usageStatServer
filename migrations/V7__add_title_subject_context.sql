-- The window title split into what it names, by title_rules (else a generic split on separators):
-- subject = the thing you're on (file, chat, page, command), context = where (project, account,
-- site, host). Filled by the server on insert; the raw title stays as sent.
ALTER TABLE mac_usage
    ADD COLUMN IF NOT EXISTS subject LowCardinality(String) DEFAULT '' AFTER title,
    ADD COLUMN IF NOT EXISTS context LowCardinality(String) DEFAULT '' AFTER subject;
