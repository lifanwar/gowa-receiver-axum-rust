use once_cell::sync::Lazy;
use regex::Regex;
use serde_json::{json, Map, Value};
use sha2::{Digest, Sha256};

pub type NormalizedData = Map<String, Value>;

static SAFE_KEY_RE: Lazy<Regex> = Lazy::new(|| Regex::new(r"[^A-Za-z0-9_.@-]").unwrap());

pub fn safe_key(value: &str) -> String {
    let cleaned = SAFE_KEY_RE.replace_all(value.trim(), "_").to_string();
    cleaned.chars().take(250).collect()
}

fn is_truthy(value: &Value) -> bool {
    match value {
        Value::Null => false,
        Value::Bool(value) => *value,
        Value::Number(value) => {
            if let Some(value) = value.as_i64() {
                value != 0
            } else if let Some(value) = value.as_u64() {
                value != 0
            } else if let Some(value) = value.as_f64() {
                value != 0.0
            } else {
                false
            }
        }
        Value::String(value) => !value.is_empty(),
        Value::Array(value) => !value.is_empty(),
        Value::Object(value) => !value.is_empty(),
    }
}

fn py_like_str(value: &Value) -> String {
    match value {
        Value::Null => "None".to_owned(),
        Value::Bool(true) => "True".to_owned(),
        Value::Bool(false) => "False".to_owned(),
        Value::Number(value) => value.to_string(),
        Value::String(value) => value.to_owned(),
        Value::Array(_) | Value::Object(_) => serde_json::to_string(value).unwrap_or_default(),
    }
}

fn optional_py_string(value: Option<&Value>) -> Value {
    match value.filter(|value| is_truthy(value)) {
        Some(value) => json!(py_like_str(value)),
        None => Value::Null,
    }
}

fn string_or_empty(value: Option<&Value>) -> String {
    value
        .filter(|value| is_truthy(value))
        .map(py_like_str)
        .unwrap_or_default()
}

pub fn detect_media(payload: &Map<String, Value>) -> (Value, Value) {
    // Deteksi media berdasarkan field yang dikirim GOWA.
    // Contoh payload Anda memakai field: image.
    // Fungsi ini tetap disiapkan untuk media lain jika nanti dipakai.
    let media_fields = ["image", "video", "audio", "document", "sticker"];

    for media_type in media_fields {
        if let Some(media_path) = payload.get(media_type) {
            if is_truthy(media_path) {
                return (json!(media_type), json!(py_like_str(media_path)));
            }
        }
    }

    (Value::Null, Value::Null)
}

pub fn normalize_gowa_payload(parsed_body: &Value) -> Result<NormalizedData, String> {
    // Normalizer khusus untuk payload webhook GOWA
    let payload = parsed_body
        .get("payload")
        .and_then(Value::as_object)
        .ok_or_else(|| "payload not found or invalid in webhook body".to_owned())?;

    let device_id = parsed_body.get("device_id");
    let event = parsed_body
        .get("event")
        .cloned()
        .unwrap_or_else(|| json!("message"));

    if !device_id.map(is_truthy).unwrap_or(false) {
        return Err("device_id not found in webhook payload".to_owned());
    }

    let device_id = device_id.expect("device_id already checked");
    let message_id = payload.get("id");
    let chat_id = payload.get("chat_id");
    let sender = payload.get("from");
    let is_from_me = payload.get("is_from_me").map(is_truthy).unwrap_or(false);
    let (media_type, media_path) = detect_media(payload);

    let chat_id_string = string_or_empty(chat_id);

    let mut data = Map::new();

    data.insert("event".to_owned(), json!(py_like_str(&event)));
    data.insert("device_id".to_owned(), json!(py_like_str(device_id)));

    data.insert("message_id".to_owned(), optional_py_string(message_id));

    // chat_id adalah ID percakapan/kontak tujuan.
    // Untuk pesan keluar, ini biasanya nomor customer.
    data.insert("chat_id".to_owned(), optional_py_string(chat_id));
    data.insert("chat_lid".to_owned(), optional_py_string(payload.get("chat_lid")));

    // sender adalah pengirim aktual dari payload.
    // Untuk pesan keluar, sender biasanya device_id sendiri.
    data.insert("sender".to_owned(), optional_py_string(sender));
    data.insert("sender_lid".to_owned(), optional_py_string(payload.get("from_lid")));
    data.insert("sender_name".to_owned(), optional_py_string(payload.get("from_name")));

    // contact_id dibuat agar lebih mudah mengambil lawan bicara/customer.
    data.insert("contact_id".to_owned(), optional_py_string(chat_id));

    data.insert("text".to_owned(), json!(string_or_empty(payload.get("body"))));
    data.insert("is_group".to_owned(), json!(chat_id_string.ends_with("@g.us")));
    data.insert("is_from_me".to_owned(), json!(is_from_me));
    data.insert(
        "direction".to_owned(),
        json!(if is_from_me { "outgoing" } else { "incoming" }),
    );

    data.insert(
        "replied_to_id".to_owned(),
        optional_py_string(payload.get("replied_to_id")),
    );

    data.insert("timestamp".to_owned(), optional_py_string(payload.get("timestamp")));

    data.insert("media_type".to_owned(), media_type);
    data.insert("media_path".to_owned(), media_path);

    data.insert("raw".to_owned(), parsed_body.clone());

    Ok(data)
}

pub fn build_event_id(data: &NormalizedData, raw_body: &[u8]) -> String {
    let event = data
        .get("event")
        .filter(|value| is_truthy(value))
        .map(py_like_str)
        .unwrap_or_else(|| "unknown".to_owned());

    let device_id = data
        .get("device_id")
        .filter(|value| is_truthy(value))
        .map(py_like_str)
        .unwrap_or_else(|| "unknown".to_owned());

    let payload_id = data
        .get("message_id")
        .filter(|value| is_truthy(value))
        .map(py_like_str)
        .unwrap_or_else(|| {
            let digest = Sha256::digest(raw_body);
            hex::encode(digest)[..32].to_owned()
        });

    safe_key(&format!("{device_id}_{event}_{payload_id}"))
}
