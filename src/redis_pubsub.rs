use std::sync::Arc;

use redis::AsyncCommands;
use serde::Serialize;
use serde_json::Value;

use crate::{normalizer::NormalizedData, settings::Settings};

#[derive(Clone)]
pub struct RedisPubSub {
    conn: redis::aio::ConnectionManager,
    settings: Arc<Settings>,
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

        Ok(Self { conn, settings })
    }

    pub async fn ping_redis(&self) -> redis::RedisResult<bool> {
        let mut conn = self.conn.clone();
        let response: String = redis::cmd("PING").query_async(&mut conn).await?;

        Ok(response == "PONG")
    }

    pub fn get_channel_name(&self, device_id: &str) -> String {
        format!("{}:{}", self.settings.pubsub_channel_prefix, device_id)
    }

    pub async fn publish_event(
        &self,
        channel_name: &str,
        data: &NormalizedData,
    ) -> redis::RedisResult<i64> {
        let mut conn = self.conn.clone();
        let payload = serde_json::to_string(&Value::Object(data.clone())).unwrap_or_default();
        let subscribers: i64 = conn.publish(channel_name, payload).await?;

        Ok(subscribers)
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
        let dedup_key = self.get_dedup_key(event_id);
        let mut conn = self.conn.clone();

        let is_new: Option<String> = redis::cmd("SET")
            .arg(&dedup_key)
            .arg("1")
            .arg("EX")
            .arg(self.settings.dedup_ttl_seconds)
            .arg("NX")
            .query_async(&mut conn)
            .await?;

        if is_new.is_none() {
            return Ok(PublishResult {
                published: false,
                duplicate: true,
                subscribers: 0,
            });
        }

        let subscribers = self.publish_event(channel_name, data).await?;

        Ok(PublishResult {
            published: true,
            duplicate: false,
            subscribers,
        })
    }
}

fn sha256_prefix(input: &[u8], chars: usize) -> String {
    use sha2::{Digest, Sha256};

    let digest = Sha256::digest(input);
    hex::encode(digest).chars().take(chars).collect()
}
