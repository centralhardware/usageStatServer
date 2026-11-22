use crate::titles::{self, Rule};
use clickhouse::Client;
use std::sync::{Arc, RwLock};
use usage_model::{MediaBlock, UsageBlock};

/// What the routes need from storage, so they can be tested against a fake.
pub trait Store: Send + Sync + 'static {
    fn ping(&self) -> impl Future<Output = Result<(), String>> + Send;
    fn save(&self, blocks: Vec<UsageBlock>, media: Vec<MediaBlock>) -> impl Future<Output = Result<(), String>> + Send;
}

impl Store for UsageRepository {
    async fn ping(&self) -> Result<(), String> {
        self.client.query("SELECT 1").execute().await.map_err(|e| e.to_string())
    }

    async fn save(&self, mut blocks: Vec<UsageBlock>, media: Vec<MediaBlock>) -> Result<(), String> {
        self.fill_subjects(&mut blocks);
        if !blocks.is_empty() {
            self.insert("mac_usage", &blocks).await.map_err(|e| e.to_string())?;
        }
        if !media.is_empty() {
            self.insert("mac_media", &media).await.map_err(|e| e.to_string())?;
        }
        Ok(())
    }
}

pub struct UsageRepository {
    pub client: Client,
    rules: Arc<RwLock<Vec<Rule>>>,
}

#[derive(clickhouse::Row, serde::Deserialize, serde::Serialize)]
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
            (b.subject, b.context) = titles::parse(&rules, &b.bundle_id, &b.title);
        }
    }

    /// Writes the seed rules into an empty `title_rules`.
    pub async fn seed_title_rules(&self) -> Result<(), clickhouse::error::Error> {
        let n: u64 = self.client.query("SELECT count() FROM title_rules").fetch_one().await?;
        if n > 0 {
            return Ok(());
        }
        let rows: Vec<RuleRow> =
            titles::SEED.iter().map(|(b, p)| RuleRow { bundle_id: (*b).into(), pattern: (*p).into(), priority: 100 }).collect();
        self.insert("title_rules", &rows).await
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
        let before: Vec<(String, String)> = blocks.iter().map(|b| (b.subject.clone(), b.context.clone())).collect();
        self.fill_subjects(&mut blocks);
        let changed: Vec<UsageBlock> =
            blocks.into_iter().zip(before).filter(|(b, old)| (&b.subject, &b.context) != (&old.0, &old.1)).map(|(b, _)| b).collect();
        if !changed.is_empty() {
            self.insert("mac_usage", &changed).await?;
        }
        Ok(changed.len())
    }

    async fn insert<T: clickhouse::Row + serde::Serialize>(&self, table: &str, rows: &[T]) -> Result<(), clickhouse::error::Error>
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
