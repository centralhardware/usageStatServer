# Migrations

ClickHouse schema history for the `mac_usage` table, applied by hand (in order) before
deploying code that needs them, as the writer user has no DDL rights:

    clickhouse-client --database timetagger --multiquery < migrations/V1__create_mac_usage_table.sql
