mod migrations;
mod repository;
mod routes;
mod titles;
mod youtube;

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

    let repository = Arc::new(repository);
    // Title rules: loaded now and every 5 minutes; stored titles are re-parsed when they change.
    {
        let repo = repository.clone();
        tokio::spawn(async move {
            loop {
                match repo.load_title_rules().await {
                    Ok(true) => match repo.reparse_titles().await {
                        Ok(n) => tracing::info!("Title rules changed: re-parsed {n} rows"),
                        Err(e) => tracing::error!("Re-parsing titles failed: {e}"),
                    },
                    Ok(false) => {}
                    Err(e) => tracing::error!("Loading title_rules failed: {e}"),
                }
                tokio::time::sleep(std::time::Duration::from_secs(300)).await;
            }
        });
    }

    // YouTube videos and channels behind the media blocks, looked up every minute.
    match env::var("YOUTUBE_API_KEY").ok().filter(|k| !k.is_empty()) {
        None => tracing::warn!("YOUTUBE_API_KEY is not set: YouTube videos and channels are not looked up"),
        Some(key) => {
            let repo = repository.clone();
            let youtube = youtube::YouTube::new(key).expect("failed to build the HTTP client");
            tokio::spawn(async move {
                loop {
                    let wait = match youtube.enrich(&repo).await {
                        Ok(_) => 60,
                        Err(e) => {
                            tracing::error!("YouTube lookup failed: {e}");
                            600
                        }
                    };
                    tokio::time::sleep(std::time::Duration::from_secs(wait)).await;
                }
            });
        }
    }

    let token = env::var("API_TOKEN").ok().filter(|t| !t.is_empty());
    if token.is_none() {
        tracing::warn!("API_TOKEN is not set: POST /api/usage accepts anyone");
    }

    let app = usage_routes(repository, token);

    let listener = tokio::net::TcpListener::bind(("0.0.0.0", port))
        .await
        .expect("failed to bind");

    tracing::info!("Listening on 0.0.0.0:{port}");

    axum::serve(listener, app).await.expect("server error");
}
