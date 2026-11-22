use clickhouse::Client;
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

    async fn save(&self, blocks: Vec<UsageBlock>, media: Vec<MediaBlock>) -> Result<(), String> {
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
}

impl UsageRepository {
    pub fn new(url: &str, database: &str, user: &str, password: &str) -> Self {
        let client = Client::default().with_url(url).with_database(database).with_user(user).with_password(password);
        Self { client }
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
