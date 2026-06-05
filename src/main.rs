mod normalizer;
mod redis_pubsub;
mod settings;
mod signature;

use std::sync::Arc;

use axum::{
    body::Bytes,
    extract::{Request, State},
    http::{HeaderMap, StatusCode},
    middleware::{self, Next},
    response::{IntoResponse, Response},
    routing::{get, post},
    Json, Router,
};
use serde_json::{json, Map, Value};
use tokio::net::TcpListener;
use tower_http::trace::TraceLayer;
use tracing::{info};

use normalizer::{build_event_id, normalize_gowa_payload, safe_key};
use redis_pubsub::RedisPubSub;
use settings::Settings;
use signature::verify_gowa_signature;

#[derive(Clone)]
struct AppState {
    settings: Arc<Settings>,
    redis: RedisPubSub,
}

#[derive(Debug)]
struct ApiError {
    status: StatusCode,
    detail: String,
}

impl ApiError {
    fn new(status: StatusCode, detail: impl Into<String>) -> Self {
        Self {
            status,
            detail: detail.into(),
        }
    }
}

impl IntoResponse for ApiError {
    fn into_response(self) -> Response {
        (self.status, Json(json!({ "detail": self.detail }))).into_response()
    }
}

#[tokio::main]
async fn main() -> Result<(), Box<dyn std::error::Error>> {
    tracing_subscriber::fmt()
    .with_env_filter(
        std::env::var("RUST_LOG")
            .unwrap_or_else(|_| "info".to_string()),
    )
    .init();
    let settings = Arc::new(Settings::from_env());
    let redis = RedisPubSub::new(settings.clone()).await?;

    let state = AppState { settings, redis };

    let app = Router::new()
        .route("/health", get(health_check))
        .route("/webhooks/gowa", post(receive_gowa_webhook))
        .with_state(state)
        .layer(TraceLayer::new_for_http())
        .layer(middleware::from_fn(drop_unknown_routes));

    let listener = TcpListener::bind("0.0.0.0:8000").await?;
    info!("server running on http://0.0.0.0:8000");
    axum::serve(listener, app).await?;

    Ok(())
}

async fn drop_unknown_routes(request: Request, next: Next) -> Response {
    let method = request.method().as_str().to_uppercase();
    let path = request.uri().path();

    let allowed = matches!(
        (method.as_str(), path),
        ("GET", "/health") | ("POST", "/webhooks/gowa")
    );

    if !allowed {
        return StatusCode::NOT_FOUND.into_response();
    }

    next.run(request).await
}

async fn health_check(State(state): State<AppState>) -> Result<Json<Value>, ApiError> {
    let redis_ok = state.redis.ping_redis().await.map_err(|_| {
        ApiError::new(StatusCode::INTERNAL_SERVER_ERROR, "Failed to ping Redis")
    })?;

    Ok(Json(json!({
        "ok": true,
        "redis": redis_ok,
        "transport": "redis_pubsub",
    })))
}

async fn receive_gowa_webhook(
    State(state): State<AppState>,
    headers: HeaderMap,
    raw_body: Bytes,
) -> Result<Json<Value>, ApiError> {
    let x_hub_signature_256 = headers
        .get("X-Hub-Signature-256")
        .and_then(|value| value.to_str().ok());

    if !verify_gowa_signature(
        raw_body.as_ref(),
        x_hub_signature_256,
        &state.settings.gowa_webhook_secret,
    ) {
        return Err(ApiError::new(
            StatusCode::UNAUTHORIZED,
            "Invalid webhook signature",
        ));
    }

    let parsed_body: Value = serde_json::from_slice(raw_body.as_ref()).map_err(|_| {
        ApiError::new(StatusCode::BAD_REQUEST, "Invalid JSON payload")
    })?;

    let data = normalize_gowa_payload(&parsed_body)
        .map_err(|error| ApiError::new(StatusCode::BAD_REQUEST, error))?;

    let device_id = data
        .get("device_id")
        .and_then(Value::as_str)
        .map(safe_key)
        .unwrap_or_default();

    let allowed_devices = state.settings.allowed_device_set();
    if !allowed_devices.is_empty() && !allowed_devices.contains(&device_id) {
        return Ok(Json(json!({
            "ok": true,
            "published": false,
            "reason": "device_not_allowed",
            "event": data.get("event").cloned().unwrap_or(Value::Null),
            "device_id": device_id,
        })));
    }

    let event_id = build_event_id(&data, raw_body.as_ref());
    let channel_name = state.redis.get_channel_name(&device_id);

    let result = state
        .redis
        .publish_event_once(&channel_name, &data, &event_id)
        .await
        .map_err(|_| {
            ApiError::new(
                StatusCode::SERVICE_UNAVAILABLE,
                "Failed to publish event to Redis Pub/Sub",
            )
        })?;

    let mut response = Map::new();
    response.insert("ok".to_owned(), json!(true));
    response.insert(
        "event".to_owned(),
        data.get("event").cloned().unwrap_or(Value::Null),
    );
    response.insert("device_id".to_owned(), json!(device_id));
    response.insert("channel".to_owned(), json!(channel_name));
    response.insert("published".to_owned(), json!(result.published));
    response.insert("duplicate".to_owned(), json!(result.duplicate));
    response.insert("subscribers".to_owned(), json!(result.subscribers));

    Ok(Json(Value::Object(response)))
}
