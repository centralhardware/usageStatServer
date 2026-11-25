use crate::titles::{self, Rule};
use clickhouse::Client;
use std::sync::{Arc, RwLock};
use usage_model::{MediaBlock, UsageBatch, UsageBlock};

/// What the routes need from storage, so they can be tested against a fake.
pub trait Store: Send + Sync + 'static {
    fn ping(&self) -> impl Future<Output = Result<(), String>> + Send;
    fn save(&self, batch: UsageBatch) -> impl Future<Output = Result<(), String>> + Send;
}

impl Store for UsageRepository {
    async fn ping(&self) -> Result<(), String> {
        self.client.query("SELECT 1").execute().await.map_err(|e| e.to_string())
    }

    async fn save(&self, batch: UsageBatch) -> Result<(), String> {
        let UsageBatch { mut blocks, media, screen_time, events } = batch;
        self.fill_subjects(&mut blocks);
        if !blocks.is_empty() {
            self.insert("mac_usage", &blocks).await.map_err(|e| e.to_string())?;
        }
        if !media.is_empty() {
            self.insert("mac_media", &media).await.map_err(|e| e.to_string())?;
        }
        if !screen_time.is_empty() {
            self.insert("screen_time", &screen_time).await.map_err(|e| e.to_string())?;
        }
        if !events.is_empty() {
            self.insert("events", &events).await.map_err(|e| e.to_string())?;
        }
        Ok(())
    }
}

/// Plays on YouTube without a link: the source says so, or a YouTube tab had that very title (the
/// agent may have filed the play under another tab). Nothing else is ever searched for.
const UNLINKED: &str = "url = '' AND title != '' AND artist != '' AND stop_at < now() - INTERVAL 1 HOUR \
     AND (source = 'youtube.com' OR title IN (SELECT replaceRegexpOne(title, ' - YouTube$', '') FROM mac_usage \
     WHERE domain = 'youtube.com' AND endsWith(title, ' - YouTube')))";

/// A title and channel to look for.
#[derive(clickhouse::Row, serde::Deserialize)]
pub struct Unlinked {
    pub title: String,
    pub artist: String,
}

pub struct UsageRepository {
    pub client: Client,
    rules: Arc<RwLock<Vec<Rule>>>,
}

#[derive(clickhouse::Row, serde::Deserialize)]
struct RuleRow {
    bundle_id: String,
    pattern: String,
    priority: i16,
}

impl UsageRepository {
    pub fn new(url: &str, database: &str, user: &str, password: &str) -> Self {
        let client = Client::default().with_url(url).with_database(database).with_user(user).with_password(password);
        Self { client, rules: Default::default() }
    }

    fn fill_subjects(&self, blocks: &mut [UsageBlock]) {
        let rules = self.rules.read().unwrap();
        for b in blocks.iter_mut().filter(|b| !b.title.is_empty()) {
            let p = titles::parse(&rules, &b.bundle_id, &b.title);
            (b.subject, b.context, b.fields) = (p.subject, p.context, p.fields);
        }
    }

    /// Reloads `title_rules`; true when they changed. Rules that don't compile are skipped.
    pub async fn load_title_rules(&self) -> Result<bool, clickhouse::error::Error> {
        let rows: Vec<RuleRow> = self
            .client
            .query("SELECT bundle_id, pattern, priority FROM title_rules FINAL ORDER BY priority, bundle_id DESC, pattern")
            .fetch_all()
            .await?;
        let mut rules = Vec::new();
        for r in rows {
            match Rule::new(&r.bundle_id, &r.pattern, r.priority) {
                Ok(rule) => rules.push(rule),
                Err(e) => tracing::warn!("Skipping title rule {:?} for {:?}: {e}", r.pattern, r.bundle_id),
            }
        }
        let key = |v: &[Rule]| v.iter().map(|r| (r.bundle_id.clone(), r.pattern.clone(), r.priority)).collect::<Vec<_>>();
        let changed = key(&self.rules.read().unwrap()) != key(&rules);
        *self.rules.write().unwrap() = rules;
        Ok(changed)
    }

    /// Re-parses every stored title with the current rules and writes back the rows that
    /// change (same stop_at, so the rewrite replaces the row).
    pub async fn reparse_titles(&self) -> Result<usize, clickhouse::error::Error> {
        let mut blocks: Vec<UsageBlock> = self
            .client
            .query("SELECT ?fields FROM mac_usage FINAL WHERE title != ''")
            .fetch_all()
            .await?;
        let before: Vec<UsageBlock> = blocks.clone();
        self.fill_subjects(&mut blocks);
        let changed: Vec<UsageBlock> = blocks.into_iter().zip(before).filter(|(b, old)| b != old).map(|(b, _)| b).collect();
        if !changed.is_empty() {
            self.insert("mac_usage", &changed).await?;
        }
        Ok(changed.len())
    }

    /// Videos played but not looked up yet.
    pub async fn new_youtube_videos(&self) -> Result<Vec<String>, clickhouse::error::Error> {
        self.client
            .query(
                "SELECT DISTINCT extract(url, 'v=([A-Za-z0-9_-]{11})') AS id FROM mac_media \
                 WHERE id != '' AND id NOT IN (SELECT video_id FROM youtube_videos)",
            )
            .fetch_all()
            .await
    }

    /// Channels never looked up, or not for a week.
    pub async fn due_youtube_channels(&self) -> Result<Vec<String>, clickhouse::error::Error> {
        self.client
            .query(
                "SELECT DISTINCT channel_id FROM youtube_videos WHERE channel_id != '' AND channel_id NOT IN \
                 (SELECT channel_id FROM youtube_channels WHERE fetched_at > now() - INTERVAL 7 DAY)",
            )
            .fetch_all()
            .await
    }

    /// Plays to search for, newest first; at most `limit`, and `per_day` in any 24 hours.
    pub async fn unlinked_plays(&self, limit: u64, per_day: u64) -> Result<Vec<Unlinked>, clickhouse::error::Error> {
        let done: u64 =
            self.client.query("SELECT count() FROM youtube_searches WHERE searched_at > now() - INTERVAL 1 DAY").fetch_one().await?;
        let limit = limit.min(per_day.saturating_sub(done));
        if limit == 0 {
            return Ok(Vec::new());
        }
        self.client
            .query(&format!(
                "SELECT title, artist FROM mac_media FINAL WHERE {UNLINKED} \
                 AND (title, artist) NOT IN (SELECT title, artist FROM youtube_searches) \
                 GROUP BY title, artist ORDER BY max(start_at) DESC LIMIT {limit}"
            ))
            .fetch_all()
            .await
    }

    /// Gives every unlinked play of this title and channel its link (same stop_at, so the rewrite
    /// replaces the row).
    pub async fn link_plays(&self, p: &Unlinked, url: &str) -> Result<(), clickhouse::error::Error> {
        let mut rows: Vec<MediaBlock> = self
            .client
            .query(&format!("SELECT ?fields FROM mac_media FINAL WHERE title = ? AND artist = ? AND {UNLINKED}"))
            .bind(&p.title)
            .bind(&p.artist)
            .fetch_all()
            .await?;
        for r in &mut rows {
            r.url = url.to_owned();
            r.source = "youtube.com".into();
        }
        if rows.is_empty() { Ok(()) } else { self.insert("mac_media", &rows).await }
    }

    pub async fn insert<T: clickhouse::Row + serde::Serialize>(&self, table: &str, rows: &[T]) -> Result<(), clickhouse::error::Error>
    where
        for<'a> T: clickhouse::RowWrite<Value<'a> = T>,
    {
        let mut insert = self.client.insert::<T>(table).await?;
        for r in rows {
            insert.write(r).await?;
        }
        insert.end().await?;
        tracing::info!("Saved {} rows into {table}", rows.len());
        Ok(())
    }
}
