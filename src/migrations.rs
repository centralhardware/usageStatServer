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
    (10, "clear_title_rules", include_str!("../migrations/V10__clear_title_rules.sql")),
    (11, "reseed_title_rules", include_str!("../migrations/V11__reseed_title_rules.sql")),
    (12, "create_screen_time_table", include_str!("../migrations/V12__create_screen_time_table.sql")),
    (13, "create_events_table", include_str!("../migrations/V13__create_events_table.sql")),
    (14, "create_view_web_visits", include_str!("../migrations/V14__create_view_web_visits.sql")),
    (15, "create_view_safari_navigations", include_str!("../migrations/V15__create_view_safari_navigations.sql")),
    (16, "create_view_notifications_biome", include_str!("../migrations/V16__create_view_notifications_biome.sql")),
    (17, "create_view_notification_deliveries", include_str!("../migrations/V17__create_view_notification_deliveries.sql")),
    (18, "create_view_app_intents", include_str!("../migrations/V18__create_view_app_intents.sql")),
    (19, "create_view_bluetooth_events", include_str!("../migrations/V19__create_view_bluetooth_events.sql")),
    (20, "create_view_menu_items", include_str!("../migrations/V20__create_view_menu_items.sql")),
    (21, "create_view_documents_opened", include_str!("../migrations/V21__create_view_documents_opened.sql")),
    (22, "create_view_screenshots", include_str!("../migrations/V22__create_view_screenshots.sql")),
    (23, "create_view_web_apps_focus", include_str!("../migrations/V23__create_view_web_apps_focus.sql")),
    (24, "create_view_media_usage_biome", include_str!("../migrations/V24__create_view_media_usage_biome.sql")),
    (25, "create_view_screentime_app_usage", include_str!("../migrations/V25__create_view_screentime_app_usage.sql")),
    (26, "teams_title_rules", include_str!("../migrations/V26__teams_title_rules.sql")),
    (27, "add_user_name_to_mac_usage", include_str!("../migrations/V27__add_user_name_to_mac_usage.sql")),
    (28, "add_user_name_to_mac_media", include_str!("../migrations/V28__add_user_name_to_mac_media.sql")),
    (29, "add_url_to_mac_media", include_str!("../migrations/V29__add_url_to_mac_media.sql")),
    (30, "create_youtube_videos_table", include_str!("../migrations/V30__create_youtube_videos_table.sql")),
    (31, "create_youtube_channels_table", include_str!("../migrations/V31__create_youtube_channels_table.sql")),
    (32, "create_view_youtube_watched", include_str!("../migrations/V32__create_view_youtube_watched.sql")),
    (33, "youtube_watched_youtube_only", include_str!("../migrations/V33__youtube_watched_youtube_only.sql")),
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
