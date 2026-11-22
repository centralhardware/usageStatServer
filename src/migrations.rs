//! Applies migrations/V<n>__*.sql at startup, each once, recorded in `schema_migrations`
//! (the Flyway convention; Flyway itself has no ClickHouse support in current versions).
//! One statement per file. The files are compiled into the binary.

use clickhouse::Client;

/// Every migration, in version order. Add new files here.
pub const MIGRATIONS: &[(u32, &str, &str)] = &[
    (1, "create_mac_usage_table", include_str!("../migrations/V1__create_mac_usage_table.sql")),
    (2, "add_device_name", include_str!("../migrations/V2__add_device_name.sql")),
    (3, "create_mac_media_table", include_str!("../migrations/V3__create_mac_media_table.sql")),
    (4, "add_domain_and_app_category", include_str!("../migrations/V4__add_domain_and_app_category.sql")),
    (5, "create_activity_rules_table", include_str!("../migrations/V5__create_activity_rules_table.sql")),
    (6, "add_media_title", include_str!("../migrations/V6__add_media_title.sql")),
    (7, "add_title_subject_context", include_str!("../migrations/V7__add_title_subject_context.sql")),
    (8, "create_title_rules_table", include_str!("../migrations/V8__create_title_rules_table.sql")),
    (9, "seed_title_rules", include_str!("../migrations/V9__seed_title_rules.sql")),
];

#[derive(clickhouse::Row, serde::Deserialize)]
struct Applied {
    version: u32,
}

#[derive(clickhouse::Row, serde::Serialize)]
struct Record<'a> {
    version: u32,
    name: &'a str,
}

/// The statement without comment lines or a trailing ';'.
pub fn statement(sql: &str) -> String {
    sql.lines().filter(|l| !l.trim_start().starts_with("--")).collect::<Vec<_>>().join("\n").trim().trim_end_matches(';').trim().to_owned()
}

pub async fn migrate(client: &Client) -> Result<(), clickhouse::error::Error> {
    client
        .query(
            "CREATE TABLE IF NOT EXISTS schema_migrations (version UInt32, name String, \
             applied_at DateTime64(3, 'UTC') DEFAULT now64(3)) ENGINE = MergeTree ORDER BY version",
        )
        .execute()
        .await?;
    let applied: Vec<u32> =
        client.query("SELECT version FROM schema_migrations").fetch_all::<Applied>().await?.into_iter().map(|a| a.version).collect();
    for &(version, name, sql) in MIGRATIONS {
        if applied.contains(&version) {
            continue;
        }
        tracing::info!("Applying migration V{version}__{name}");
        // Raw: migration text may contain `?` (regexes), which query() would read as bind placeholders.
        client.query_raw(&statement(sql)).execute().await?;
        let mut insert = client.insert::<Record>("schema_migrations").await?;
        insert.write(&Record { version, name }).await?;
        insert.end().await?;
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn migrations_are_numbered_and_single_statements() {
        for (i, (v, _, sql)) in MIGRATIONS.iter().enumerate() {
            assert_eq!(*v as usize, i + 1);
            let s = statement(sql);
            assert!(!s.is_empty() && !s.contains(';'), "V{v} must hold one statement");
        }
    }

    #[test]
    fn every_file_is_listed() {
        let files = std::fs::read_dir(concat!(env!("CARGO_MANIFEST_DIR"), "/migrations"))
            .unwrap()
            .filter_map(|e| e.ok()?.file_name().into_string().ok())
            .filter(|n| n.ends_with(".sql"))
            .count();
        assert_eq!(files, MIGRATIONS.len());
    }
}
