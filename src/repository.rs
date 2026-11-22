use clickhouse::Client;
use usage_model::UsageBlock;

/// What the routes need from storage, so they can be tested against a fake.
pub trait Store: Send + Sync + 'static {
    fn ping(&self) -> impl Future<Output = Result<(), String>> + Send;
    fn save(&self, blocks: Vec<UsageBlock>) -> impl Future<Output = Result<(), String>> + Send;
}

impl Store for UsageRepository {
    async fn ping(&self) -> Result<(), String> {
        self.client.query("SELECT 1").execute().await.map_err(|e| e.to_string())
    }

    async fn save(&self, blocks: Vec<UsageBlock>) -> Result<(), String> {
        self.insert(&blocks).await.map_err(|e| e.to_string())
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

    async fn insert(&self, blocks: &[UsageBlock]) -> Result<(), clickhouse::error::Error> {
        let mut insert = self.client.insert::<UsageBlock>("mac_usage").await?;
        for b in blocks {
            insert.write(b).await?;
        }
        insert.end().await?;
        tracing::info!("Saved {} usage blocks from {}", blocks.len(), blocks.first().map_or("-", |b| b.device_id.as_str()));
        Ok(())
    }
}
