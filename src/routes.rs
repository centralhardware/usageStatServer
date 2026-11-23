use usage_model::{MediaBlock, ScreenTimeBlock, UsageBatch, UsageBlock};
use crate::repository::Store;
use axum::extract::{Request, State};
use axum::http::{StatusCode, header};
use axum::middleware::{self, Next};
use axum::response::{IntoResponse, Response};
use axum::routing::{get, post};
use axum::{Json, Router};
use serde_json::{json, Value};
use std::sync::Arc;

/// `token`: when set, POST /api/usage requires `Authorization: Bearer <token>`.
pub fn usage_routes<S: Store>(repository: Arc<S>, token: Option<String>) -> Router {
    let token = Arc::new(token);
    let api = Router::new()
        .route("/api/usage", post(post_usage::<S>))
        .route_layer(middleware::from_fn(move |req, next| require_token(token.clone(), req, next)));
    Router::new()
        .merge(api)
        .route("/ping", get(ping::<S>))
        .with_state(repository)
}

async fn require_token(token: Arc<Option<String>>, req: Request, next: Next) -> Response {
    let Some(expected) = token.as_deref() else {
        return next.run(req).await;
    };
    // The scheme is case-insensitive (RFC 7235); the token itself is not.
    let given = req
        .headers()
        .get(header::AUTHORIZATION)
        .and_then(|v| v.to_str().ok())
        .and_then(|v| v.split_once(' '))
        .filter(|(scheme, _)| scheme.eq_ignore_ascii_case("bearer"))
        .map(|(_, t)| t.trim());
    if given.is_some_and(|g| constant_time_eq(g.as_bytes(), expected.as_bytes())) {
        next.run(req).await
    } else {
        (StatusCode::UNAUTHORIZED, Json(json!({"status": "error", "message": "unauthorized"}))).into_response()
    }
}

/// Equality that takes as long however early the inputs differ. Only the
/// length can leak, and the length of a random token tells nothing.
fn constant_time_eq(a: &[u8], b: &[u8]) -> bool {
    a.len() == b.len() && a.iter().zip(b).fold(0u8, |acc, (x, y)| acc | (x ^ y)) == 0
}

/// The database's own error stays in the server log: `/ping` is open to anyone,
/// and the message can name hosts, tables and users.
fn db_error(status: StatusCode) -> (StatusCode, Json<Value>) {
    (status, Json(json!({"status": "error", "message": "database unavailable"})))
}

async fn ping<S: Store>(
    State(repo): State<Arc<S>>,
) -> Result<(StatusCode, Json<Value>), (StatusCode, Json<Value>)> {
    repo.ping().await.map_err(|e| {
        tracing::error!("ClickHouse ping failed: {e}");
        db_error(StatusCode::SERVICE_UNAVAILABLE)
    })?;

    Ok((StatusCode::OK, Json(json!({"status": "ok"}))))
}

/// Most blocks one request may carry; the agent sends at most 500.
const MAX_BLOCKS: usize = 1000;

async fn post_usage<S: Store>(
    State(repo): State<Arc<S>>,
    Json(batch): Json<UsageBatch>,
) -> Result<(StatusCode, Json<Value>), (StatusCode, Json<Value>)> {
    let invalid = |m: String| (StatusCode::UNPROCESSABLE_ENTITY, Json(json!({"status": "error", "message": m})));
    let n = batch.blocks.len() + batch.media.len() + batch.screen_time.len();
    if n > MAX_BLOCKS {
        return Err(invalid(format!("at most {MAX_BLOCKS} blocks per request")));
    }
    if let Some(p) = batch.blocks.iter().find_map(UsageBlock::problem)
        .or_else(|| batch.media.iter().find_map(MediaBlock::problem))
        .or_else(|| batch.screen_time.iter().find_map(ScreenTimeBlock::problem))
    {
        return Err(invalid(p));
    }
    if n == 0 {
        return Ok((StatusCode::CREATED, Json(json!({"status": "success", "saved": 0}))));
    }
    repo.save(batch.blocks, batch.media, batch.screen_time).await.map_err(|e| {
        tracing::error!("Failed to save usage blocks: {e}");
        db_error(StatusCode::INTERNAL_SERVER_ERROR)
    })?;

    Ok((StatusCode::CREATED, Json(json!({"status": "success", "saved": n}))))
}

#[cfg(test)]
mod tests {
    use super::*;
    use axum::body::Body;
    use axum::http::Request;
    use std::sync::Mutex;
    use tower::ServiceExt;

    #[derive(Default)]
    struct Fake {
        saved: Mutex<Vec<UsageBlock>>,
        down: bool,
    }

    impl Store for Fake {
        async fn ping(&self) -> Result<(), String> {
            if self.down { Err("secret-host:9000 auth failed for user x".into()) } else { Ok(()) }
        }
        async fn save(&self, blocks: Vec<UsageBlock>, _media: Vec<MediaBlock>, _screen_time: Vec<ScreenTimeBlock>) -> Result<(), String> {
            if self.down {
                return Err("secret-host:9000 auth failed for user x".into());
            }
            self.saved.lock().unwrap().extend(blocks);
            Ok(())
        }
    }

    fn block() -> UsageBlock {
        UsageBlock {
            device_id: "D".into(), device_name: "Mac".into(), kind: "app".into(), bundle_id: "com.apple.Safari".into(),
            app_name: "Safari".into(), title: "x".into(), domain: String::new(), app_category: String::new(), start_at: 1_000, stop_at: 2_000, ..Default::default()
        }
    }

    fn body() -> Body {
        Body::from(serde_json::to_string(&UsageBatch { blocks: vec![block()], media: vec![], ..Default::default() }).unwrap())
    }

    async fn post(fake: Arc<Fake>, token: Option<&str>, auth: Option<&str>) -> (StatusCode, String) {
        let mut req = Request::post("/api/usage").header("content-type", "application/json");
        if let Some(a) = auth {
            req = req.header("authorization", a);
        }
        let resp = usage_routes(fake, token.map(str::to_owned))
            .oneshot(req.body(body()).unwrap())
            .await
            .unwrap();
        let status = resp.status();
        let bytes = axum::body::to_bytes(resp.into_body(), usize::MAX).await.unwrap();
        (status, String::from_utf8(bytes.to_vec()).unwrap())
    }

    #[tokio::test]
    async fn token_checks() {
        let fake = Arc::new(Fake::default());
        assert_eq!(post(fake.clone(), Some("t0k"), None).await.0, StatusCode::UNAUTHORIZED);
        assert_eq!(post(fake.clone(), Some("t0k"), Some("Bearer nope")).await.0, StatusCode::UNAUTHORIZED);
        assert_eq!(post(fake.clone(), Some("t0k"), Some("Basic t0k")).await.0, StatusCode::UNAUTHORIZED);
        assert_eq!(post(fake.clone(), Some("t0k"), Some("Bearer t0k")).await.0, StatusCode::CREATED);
        assert_eq!(post(fake.clone(), Some("t0k"), Some("bearer t0k")).await.0, StatusCode::CREATED);
        assert_eq!(post(fake.clone(), None, None).await.0, StatusCode::CREATED);
        assert_eq!(fake.saved.lock().unwrap().len(), 3);
        assert_eq!(fake.saved.lock().unwrap()[0].device_id, "D");
    }

    #[tokio::test]
    async fn ping_is_open_and_hides_db_errors() {
        let ok = usage_routes(Arc::new(Fake::default()), Some("t".into()))
            .oneshot(Request::get("/ping").body(Body::empty()).unwrap())
            .await
            .unwrap();
        assert_eq!(ok.status(), StatusCode::OK);

        let down = usage_routes(Arc::new(Fake { down: true, ..Default::default() }), None)
            .oneshot(Request::get("/ping").body(Body::empty()).unwrap())
            .await
            .unwrap();
        assert_eq!(down.status(), StatusCode::SERVICE_UNAVAILABLE);
        let text = axum::body::to_bytes(down.into_body(), usize::MAX).await.unwrap();
        assert!(!String::from_utf8_lossy(&text).contains("secret-host"));
    }

    #[tokio::test]
    async fn save_error_is_generic() {
        let (status, text) = post(Arc::new(Fake { down: true, ..Default::default() }), None, None).await;
        assert_eq!(status, StatusCode::INTERNAL_SERVER_ERROR);
        assert!(!text.contains("secret-host"));
    }

    #[tokio::test]
    async fn rejects_bad_blocks() {
        let fake = Arc::new(Fake::default());
        let bad = UsageBatch { blocks: vec![UsageBlock { kind: "nope".into(), ..block() }], media: vec![], ..Default::default() };
        let resp = usage_routes(fake.clone(), None)
            .oneshot(
                Request::post("/api/usage")
                    .header("content-type", "application/json")
                    .body(Body::from(serde_json::to_string(&bad).unwrap()))
                    .unwrap(),
            )
            .await
            .unwrap();
        assert_eq!(resp.status(), StatusCode::UNPROCESSABLE_ENTITY);
        assert!(fake.saved.lock().unwrap().is_empty());
    }

    #[test]
    fn constant_time_eq_works() {
        assert!(constant_time_eq(b"abc", b"abc"));
        assert!(!constant_time_eq(b"abc", b"abd"));
        assert!(!constant_time_eq(b"abc", b"abcd"));
    }
}
