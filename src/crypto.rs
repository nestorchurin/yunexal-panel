use aes_gcm::{
    aead::{Aead, AeadCore, KeyInit, OsRng},
    Aes256Gcm, Key,
};
use base64::{engine::general_purpose::STANDARD as BASE64, Engine};
use sha2::{Digest, Sha256};

const PREFIX: &str = "enc:v1:";

pub fn derive_db_key(cookie_secret: &str) -> [u8; 32] {
    Sha256::digest(cookie_secret.as_bytes()).into()
}

pub fn encrypt(plaintext: &str, key: &[u8; 32]) -> String {
    let cipher = Aes256Gcm::new(Key::<Aes256Gcm>::from_slice(key));
    let nonce = Aes256Gcm::generate_nonce(&mut OsRng);
    let ciphertext = cipher
        .encrypt(&nonce, plaintext.as_bytes())
        .expect("AES-GCM encrypt failed");
    let mut combined = nonce.to_vec();
    combined.extend_from_slice(&ciphertext);
    format!("{}{}", PREFIX, BASE64.encode(combined))
}

pub fn decrypt(ciphertext: &str, key: &[u8; 32]) -> anyhow::Result<String> {
    let data = ciphertext
        .strip_prefix(PREFIX)
        .ok_or_else(|| anyhow::anyhow!("not an encrypted value"))?;
    let bytes = BASE64.decode(data)?;
    if bytes.len() < 12 {
        anyhow::bail!("ciphertext too short");
    }
    let (nonce_bytes, ct) = bytes.split_at(12);
    let cipher = Aes256Gcm::new(Key::<Aes256Gcm>::from_slice(key));
    let nonce = aes_gcm::Nonce::from_slice(nonce_bytes);
    let plaintext = cipher
        .decrypt(nonce, ct)
        .map_err(|_| anyhow::anyhow!("AES-GCM decrypt failed — wrong key"))?;
    Ok(String::from_utf8(plaintext)?)
}

pub fn is_encrypted(value: &str) -> bool {
    value.starts_with(PREFIX)
}

/// Decrypts if value carries the enc:v1: prefix, returns as-is otherwise.
/// Handles transparent migration of pre-encryption plaintext rows.
pub fn decrypt_if_encrypted(value: &str, key: &[u8; 32]) -> anyhow::Result<String> {
    if is_encrypted(value) {
        decrypt(value, key)
    } else {
        Ok(value.to_string())
    }
}
