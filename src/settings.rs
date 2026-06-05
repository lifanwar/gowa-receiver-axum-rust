use std::{collections::HashSet, env};

#[derive(Clone, Debug)]
pub struct Settings {
    pub app_name: String,
    pub redis_url: String,
    pub gowa_webhook_secret: String,
    pub pubsub_channel_prefix: String,
    pub dedup_prefix: String,
    pub dedup_ttl_seconds: u64,
    pub allowed_devices: String,
}

impl Settings {
    pub fn from_env() -> Self {
        dotenvy::dotenv().ok();

        Self {
            app_name: env_string("APP_NAME", "app_name", "gowa-webhook-api"),
            redis_url: env_string("REDIS_URL", "redis_url", "redis://localhost:6379/0"),
            gowa_webhook_secret: env_string("GOWA_WEBHOOK_SECRET", "gowa_webhook_secret", ""),
            pubsub_channel_prefix: env_string(
                "PUBSUB_CHANNEL_PREFIX",
                "pubsub_channel_prefix",
                "wa:incoming",
            ),
            dedup_prefix: env_string("DEDUP_PREFIX", "dedup_prefix", "dedup:gowa"),
            dedup_ttl_seconds: env_string("DEDUP_TTL_SECONDS", "dedup_ttl_seconds", "600")
                .parse::<u64>()
                .unwrap_or(600),
            allowed_devices: env_string("ALLOWED_DEVICES", "allowed_devices", ""),
        }
    }

    pub fn allowed_device_set(&self) -> HashSet<String> {
        if self.allowed_devices.trim().is_empty() {
            return HashSet::new();
        }

        self.allowed_devices
            .split(',')
            .map(str::trim)
            .filter(|item| !item.is_empty())
            .map(ToOwned::to_owned)
            .collect()
    }
}

fn env_string(upper_key: &str, lower_key: &str, default_value: &str) -> String {
    env::var(upper_key)
        .or_else(|_| env::var(lower_key))
        .unwrap_or_else(|_| default_value.to_owned())
}
