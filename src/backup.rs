use crate::models::{AppSettings, SshProfile};
use aes_gcm::aead::{Aead, KeyInit};
use aes_gcm::{Aes256Gcm, Key, Nonce};
use data_encoding::BASE64;
use hmac::Hmac;
use pbkdf2::pbkdf2;
use rand::RngCore;
use serde::{Deserialize, Serialize};
use sha2::Sha256;
use std::fs;
use std::path::Path;

const FORMAT_VERSION: &str = "multi-ssh-connect-v1";
const KDF_NAME: &str = "pbkdf2-sha256";
const KDF_ITERATIONS: u32 = 100_000;

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ExportPayload {
    pub profiles: Vec<SshProfile>,
    pub settings: AppSettings,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct EncryptedBundle {
    pub format: String,
    pub kdf: String,
    pub iterations: u32,
    pub salt_b64: String,
    pub nonce_b64: String,
    pub ciphertext_b64: String,
}

fn derive_key(password: &str, salt: &[u8], iterations: u32) -> [u8; 32] {
    let mut key = [0u8; 32];
    let _ = pbkdf2::<Hmac<Sha256>>(password.as_bytes(), salt, iterations, &mut key);
    key
}

pub fn encrypt_payload(payload: &ExportPayload, password: &str) -> anyhow::Result<String> {
    if password.is_empty() {
        anyhow::bail!("Mật khẩu mã hoá không được để trống.");
    }

    let plaintext = serde_json::to_vec(payload)?;

    let mut salt = [0u8; 16];
    let mut nonce_bytes = [0u8; 12];
    rand::thread_rng().fill_bytes(&mut salt);
    rand::thread_rng().fill_bytes(&mut nonce_bytes);

    let key_bytes = derive_key(password, &salt, KDF_ITERATIONS);
    let key = Key::<Aes256Gcm>::from_slice(&key_bytes);
    let cipher = Aes256Gcm::new(key);
    let nonce = Nonce::from_slice(&nonce_bytes);

    let ciphertext = cipher
        .encrypt(nonce, plaintext.as_ref())
        .map_err(|_| anyhow::anyhow!("Lỗi mã hoá dữ liệu."))?;

    let bundle = EncryptedBundle {
        format: FORMAT_VERSION.to_string(),
        kdf: KDF_NAME.to_string(),
        iterations: KDF_ITERATIONS,
        salt_b64: BASE64.encode(&salt),
        nonce_b64: BASE64.encode(&nonce_bytes),
        ciphertext_b64: BASE64.encode(&ciphertext),
    };

    Ok(serde_json::to_string_pretty(&bundle)?)
}

pub fn decrypt_payload(raw_json: &str, password: &str) -> anyhow::Result<ExportPayload> {
    if password.is_empty() {
        anyhow::bail!("Vui lòng nhập mật khẩu để giải mã.");
    }

    let bundle: EncryptedBundle = serde_json::from_str(raw_json)
        .map_err(|_| anyhow::anyhow!("File backup không đúng định dạng."))?;

    if bundle.format != FORMAT_VERSION {
        anyhow::bail!("Phiên bản file backup không được hỗ trợ: {}", bundle.format);
    }

    let salt = BASE64
        .decode(bundle.salt_b64.as_bytes())
        .map_err(|_| anyhow::anyhow!("Dữ liệu salt không hợp lệ."))?;
    let nonce_bytes = BASE64
        .decode(bundle.nonce_b64.as_bytes())
        .map_err(|_| anyhow::anyhow!("Dữ liệu nonce không hợp lệ."))?;
    let ciphertext = BASE64
        .decode(bundle.ciphertext_b64.as_bytes())
        .map_err(|_| anyhow::anyhow!("Dữ liệu mã hoá không hợp lệ."))?;

    if nonce_bytes.len() != 12 {
        anyhow::bail!("Độ dài nonce không hợp lệ.");
    }

    let key_bytes = derive_key(password, &salt, bundle.iterations.max(10_000));
    let key = Key::<Aes256Gcm>::from_slice(&key_bytes);
    let cipher = Aes256Gcm::new(key);
    let nonce = Nonce::from_slice(&nonce_bytes);

    let plaintext = cipher
        .decrypt(nonce, ciphertext.as_ref())
        .map_err(|_| anyhow::anyhow!("Sai mật khẩu hoặc file backup đã bị hỏng."))?;

    let payload: ExportPayload = serde_json::from_slice(&plaintext)
        .map_err(|_| anyhow::anyhow!("Không đọc được nội dung bên trong file backup."))?;

    Ok(payload)
}

pub fn export_to_file(
    path: &Path,
    profiles: &[SshProfile],
    settings: &AppSettings,
    password: &str,
) -> anyhow::Result<()> {
    let payload = ExportPayload {
        profiles: profiles.to_vec(),
        settings: settings.clone(),
    };
    let encrypted = encrypt_payload(&payload, password)?;
    fs::write(path, encrypted)?;
    Ok(())
}

pub fn import_from_file(path: &Path, password: &str) -> anyhow::Result<ExportPayload> {
    let raw = fs::read_to_string(path)?;
    decrypt_payload(&raw, password)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::models::{AuthMethod, PortForwardRule, ProfileColor};

    #[test]
    fn test_encrypted_export_import_roundtrip() {
        let mut p = SshProfile::new("Prod API", "10.0.0.10", "deploy");
        p.color = ProfileColor::new(251, 113, 133);
        p.auth = AuthMethod::Password {
            password: "SuperSecretPassword!".to_string(),
        };
        p.port_forwards
            .push(PortForwardRule::new("DB", "127.0.0.1", 15432, "127.0.0.1", 5432));

        let payload = ExportPayload {
            profiles: vec![p.clone()],
            settings: AppSettings::default(),
        };

        let encrypted = encrypt_payload(&payload, "my-backup-pass").expect("encrypt");
        assert!(!encrypted.contains("SuperSecretPassword!"));

        let decrypted = decrypt_payload(&encrypted, "my-backup-pass").expect("decrypt");
        assert_eq!(decrypted.profiles.len(), 1);
        assert_eq!(decrypted.profiles[0].name, "Prod API");
        assert_eq!(decrypted.profiles[0].auth, p.auth);
        assert_eq!(decrypted.profiles[0].port_forwards.len(), 1);
    }

    #[test]
    fn test_wrong_password_rejected() {
        let payload = ExportPayload {
            profiles: vec![SshProfile::new("Test", "127.0.0.1", "root")],
            settings: AppSettings::default(),
        };
        let encrypted = encrypt_payload(&payload, "correct-pass").unwrap();
        let err = decrypt_payload(&encrypted, "wrong-pass").unwrap_err();
        assert!(err.to_string().contains("Sai mật khẩu"));
    }
}
