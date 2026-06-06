use std::{collections::HashSet, env};

#[derive(Clone, Debug)]
pub struct Settings {
    pub app_name: String,
    pub redis_url: String,
    pub redis_command_timeout_seconds: f64,
    pub redis_reconnect_sleep_seconds: f64,
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

            // Redis conf
            redis_url: env_string("REDIS_URL", "redis_url", "redis://localhost:6379/0"),
            redis_command_timeout_seconds: env_f64(
                "REDIS_COMMAND_TIMEOUT_SECONDS",
                "redis_command_timeout_seconds",
                2.0,
            ),
            redis_reconnect_sleep_seconds: env_f64(
                "REDIS_RECONNECT_SLEEP_SECONDS",
                "redis_reconnect_sleep_seconds",
                2.0,
            ),

            // Gowa conf
            gowa_webhook_secret: env_string("GOWA_WEBHOOK_SECRET", "gowa_webhook_secret", ""),
            pubsub_channel_prefix: env_string(
                "PUBSUB_CHANNEL_PREFIX",
                "pubsub_channel_prefix",
                "wa:incoming",
            ),
            dedup_prefix: env_string("DEDUP_PREFIX", "dedup_prefix", "dedup:gowa"),
            dedup_ttl_seconds: env_u64("DEDUP_TTL_SECONDS", "dedup_ttl_seconds", 600),
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

fn env_f64(upper_key: &str, lower_key: &str, default_value: f64) -> f64 {
    env::var(upper_key)
        .or_else(|_| env::var(lower_key))
        .ok()
        .and_then(|value| value.parse::<f64>().ok())
        .unwrap_or(default_value)
}

fn env_u64(upper_key: &str, lower_key: &str, default_value: u64) -> u64 {
    env::var(upper_key)
        .or_else(|_| env::var(lower_key))
        .ok()
        .and_then(|value| value.parse::<u64>().ok())
        .unwrap_or(default_value)
}