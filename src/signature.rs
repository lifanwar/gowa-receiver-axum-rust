use hmac::{Hmac, Mac};
use sha2::Sha256;
use subtle::ConstantTimeEq;

type HmacSha256 = Hmac<Sha256>;

pub fn normalize_signature(signature: Option<&str>) -> Option<String> {
    let signature = signature?;
    let signature = signature.trim();

    if let Some(stripped) = signature.strip_prefix("sha256=") {
        return Some(stripped.to_owned());
    }

    Some(signature.to_owned())
}

pub fn verify_gowa_signature(
    raw_body: &[u8],
    signature_header: Option<&str>,
    secret: &str,
) -> bool {
    // Jika secret kosong, validasi signature dilewati.
    // Untuk production, selalu isi GOWA_WEBHOOK_SECRET.
    if secret.is_empty() {
        return true;
    }

    let Some(received_signature) = normalize_signature(signature_header) else {
        return false;
    };

    if received_signature.is_empty() {
        return false;
    }

    let Ok(mut mac) = HmacSha256::new_from_slice(secret.as_bytes()) else {
        return false;
    };

    mac.update(raw_body);
    let expected_signature = hex::encode(mac.finalize().into_bytes());

    expected_signature
        .as_bytes()
        .ct_eq(received_signature.as_bytes())
        .into()
}
