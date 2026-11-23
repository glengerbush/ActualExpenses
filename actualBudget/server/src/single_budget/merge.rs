use sha2::{Digest, Sha256};

pub(super) fn identity(parts: &[&str]) -> String {
    let hash = Sha256::digest(serde_json::to_vec(parts).expect("string array"));
    let mut bytes: [u8; 16] = hash[..16].try_into().unwrap();
    bytes[6] = (bytes[6] & 15) | 64;
    bytes[8] = (bytes[8] & 63) | 128;
    uuid::Uuid::from_bytes(bytes).to_string()
}
