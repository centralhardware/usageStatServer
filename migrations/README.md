# Migrations

ClickHouse schema history, applied automatically at server startup (see `src/migrations.rs`):
each `V<n>__<name>.sql` runs once, in order, and is recorded in `schema_migrations`.
One statement per file; list a new file in `MIGRATIONS` (a test checks none is missing).
The writer user needs CREATE TABLE (and ALTER for later migrations) on the database.
