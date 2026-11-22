mod migrations;
mod repository;
mod routes;

use crate::repository::UsageRepository;
use crate::routes::usage_routes;
use std::env;
use std::sync::Arc;

#[tokio::main]
async fn main() {
    tracing_subscriber::fmt()
        .with_env_filter(
            tracing_subscriber::EnvFilter::try_from_default_env()
                .unwrap_or_else(|_| "info".into()),
        )
        .init();

    let clickhouse_url =
        env::var("CH_URL").unwrap_or_else(|_| "http://localhost:8123".into());
    let clickhouse_db =
        env::var("CH_DB").unwrap_or_else(|_| "default".into());
    let clickhouse_user =
        env::var("CH_USER").unwrap_or_else(|_| "default".into());
    let clickhouse_password =
        env::var("CH_PASSWORD").unwrap_or_default();
    let port: u16 = env::var("PORT")
        .ok()
        .and_then(|p| p.parse().ok())
        .unwrap_or(8080);

    tracing::info!("Starting Usage Stats Server on port {port}");
    tracing::info!("ClickHouse URL: {clickhouse_url}, DB: {clickhouse_db}");

    let repository = UsageRepository::new(&clickhouse_url, &clickhouse_db, &clickhouse_user, &clickhouse_password);

    // Retried until ClickHouse answers, so a deploy that races it still comes up migrated.
    loop {
        match migrations::migrate(&repository.client).await {
            Ok(()) => break,
            Err(e) => {
                tracing::error!("Migrations failed, retrying in 10 s: {e}");
                tokio::time::sleep(std::time::Duration::from_secs(10)).await;
            }
        }
    }

    let token = env::var("API_TOKEN").ok().filter(|t| !t.is_empty());
    if token.is_none() {
        tracing::warn!("API_TOKEN is not set: POST /api/usage accepts anyone");
    }

    let app = usage_routes(Arc::new(repository), token);

    let listener = tokio::net::TcpListener::bind(("0.0.0.0", port))
        .await
        .expect("failed to bind");

    tracing::info!("Listening on 0.0.0.0:{port}");

    axum::serve(listener, app).await.expect("server error");
}
