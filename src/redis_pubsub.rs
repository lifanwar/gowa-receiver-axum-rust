use std::sync::Arc;

use redis::AsyncCommands;
use serde::Serialize;
use serde_json::Value;
use tokio::sync::Mutex;
use tokio::time::{timeout, Duration, Instant};
use tracing::warn;

use crate::{normalizer::NormalizedData, settings::Settings};

#[derive(Clone)]
pub struct RedisPubSub {
    conn: redis::aio::ConnectionManager,
    settings: Arc<Settings>,
    state: Arc<Mutex<RedisPubSubState>>,
}

#[derive(Debug)]
struct RedisPubSubState {
    failure_count: u32,
    next_retry_at: Option<Instant>,
}

#[derive(Debug, Serialize)]
pub struct PublishResult {
    pub published: bool,
    pub duplicate: bool,
    pub subscribers: i64,
}

impl RedisPubSub {
    pub async fn new(settings: Arc<Settings>) -> redis::RedisResult<Self> {
        let client = redis::Client::open(settings.redis_url.as_str())?;
        let conn = client.get_connection_manager().await?;

        Ok(Self {
            conn,
            settings,
            state: Arc::new(Mutex::new(RedisPubSubState {
                failure_count: 0,
                next_retry_at: None,
            })),
        })
    }

    fn redis_timeout(&self) -> Duration {
        Duration::from_secs_f64(self.settings.redis_command_timeout_seconds.max(0.2))
    }

    fn reconnect_delay_from_failure_count(failure_count: u32, base_seconds: f64) -> Duration {
        let base_seconds = base_seconds.max(1.0);
        let multiplier = 2_f64.powi(failure_count.min(6) as i32);
        let delay_seconds = (base_seconds * multiplier).min(60.0);

        Duration::from_secs_f64(delay_seconds)
    }

    async fn redis_available_for_try(&self) -> bool {
        let state = self.state.lock().await;

        match state.next_retry_at {
            Some(next_retry_at) => Instant::now() >= next_retry_at,
            None => true,
        }
    }

    async fn mark_redis_success(&self) {
        let mut state = self.state.lock().await;
        state.failure_count = 0;
        state.next_retry_at = None;
    }

    async fn mark_redis_failure(&self, reason: &'static str) {
        let mut state = self.state.lock().await;

        let delay = Self::reconnect_delay_from_failure_count(
            state.failure_count,
            self.settings.redis_reconnect_sleep_seconds,
        );

        state.failure_count = state.failure_count.saturating_add(1);
        state.next_retry_at = Some(Instant::now() + delay);

        warn!(
            reason = reason,
            delay_seconds = delay.as_secs_f64(),
            failure_count = state.failure_count,
            "redis_backoff_scheduled"
        );
    }

    pub async fn ping_redis(&self) -> redis::RedisResult<bool> {
        if !self.redis_available_for_try().await {
            return Ok(false);
        }

        let mut conn = self.conn.clone();

        let result = timeout(self.redis_timeout(), async {
            redis::cmd("PING").query_async::<String>(&mut conn).await
        })
        .await;

        match result {
            Ok(Ok(response)) => {
                self.mark_redis_success().await;
                Ok(response == "PONG")
            }
            Ok(Err(error)) => {
                warn!(error = %error, "redis_ping_failed");
                self.mark_redis_failure("redis_ping_failed").await;
                Ok(false)
            }
            Err(_) => {
                warn!("redis_ping_timeout");
                self.mark_redis_failure("redis_ping_timeout").await;
                Ok(false)
            }
        }
    }

    pub fn get_channel_name(&self, device_id: &str) -> String {
        format!("{}:{}", self.settings.pubsub_channel_prefix, device_id)
    }

    pub async fn publish_event(
        &self,
        channel_name: &str,
        data: &NormalizedData,
    ) -> redis::RedisResult<i64> {
        if !self.redis_available_for_try().await {
            return Ok(0);
        }

        let mut conn = self.conn.clone();
        let payload = serde_json::to_string(&Value::Object(data.clone())).unwrap_or_default();

        let result = timeout(self.redis_timeout(), async {
            conn.publish::<_, _, i64>(channel_name, payload).await
        })
        .await;

        match result {
            Ok(Ok(subscribers)) => {
                self.mark_redis_success().await;
                Ok(subscribers)
            }
            Ok(Err(error)) => {
                warn!(error = %error, "redis_publish_failed");
                self.mark_redis_failure("redis_publish_failed").await;
                Ok(0)
            }
            Err(_) => {
                warn!("redis_publish_timeout");
                self.mark_redis_failure("redis_publish_timeout").await;
                Ok(0)
            }
        }
    }

    pub fn get_dedup_key(&self, event_id: &str) -> String {
        let hashed = sha256_prefix(event_id.as_bytes(), 32);
        format!("{}:{}", self.settings.dedup_prefix, hashed)
    }

    pub async fn publish_event_once(
        &self,
        channel_name: &str,
        data: &NormalizedData,
        event_id: &str,
    ) -> redis::RedisResult<PublishResult> {
        if !self.redis_available_for_try().await {
            return Ok(PublishResult {
                published: false,
                duplicate: false,
                subscribers: 0,
            });
        }

        let dedup_key = self.get_dedup_key(event_id);
        let payload = serde_json::to_string(&Value::Object(data.clone())).unwrap_or_default();

        let mut conn = self.conn.clone();

        let result = timeout(self.redis_timeout(), async {
            let is_new: Option<String> = redis::cmd("SET")
                .arg(&dedup_key)
                .arg("1")
                .arg("EX")
                .arg(self.settings.dedup_ttl_seconds)
                .arg("NX")
                .query_async(&mut conn)
                .await?;

            if is_new.is_none() {
                return Ok::<PublishResult, redis::RedisError>(PublishResult {
                    published: false,
                    duplicate: true,
                    subscribers: 0,
                });
            }

            let subscribers: i64 = conn.publish(channel_name, payload).await?;

            Ok(PublishResult {
                published: true,
                duplicate: false,
                subscribers,
            })
        })
        .await;

        match result {
            Ok(Ok(result)) => {
                self.mark_redis_success().await;
                Ok(result)
            }
            Ok(Err(error)) => {
                warn!(error = %error, "redis_publish_once_failed");
                self.mark_redis_failure("redis_publish_once_failed").await;

                Ok(PublishResult {
                    published: false,
                    duplicate: false,
                    subscribers: 0,
                })
            }
            Err(_) => {
                warn!("redis_publish_once_timeout");
                self.mark_redis_failure("redis_publish_once_timeout").await;

                Ok(PublishResult {
                    published: false,
                    duplicate: false,
                    subscribers: 0,
                })
            }
        }
    }
}

fn sha256_prefix(input: &[u8], chars: usize) -> String {
    use sha2::{Digest, Sha256};

    let digest = Sha256::digest(input);
    hex::encode(digest).chars().take(chars).collect()
}