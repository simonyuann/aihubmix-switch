use crate::types::{Result, Settings};
use chacha20poly1305::{
    aead::{Aead, AeadCore, KeyInit, OsRng},
    ChaCha20Poly1305, Nonce,
};
use std::{
    collections::BTreeMap,
    fs,
    io::Write,
    path::{Path, PathBuf},
};

pub fn ensure_dir(path: &Path) -> Result<()> {
    fs::create_dir_all(path).map_err(|e| format!("无法创建目录：{e}"))?;
    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt;
        fs::set_permissions(path, fs::Permissions::from_mode(0o700)).map_err(|e| e.to_string())?;
    }
    Ok(())
}

pub fn atomic_write(path: &Path, bytes: &[u8]) -> Result<()> {
    let parent = path.parent().ok_or("无效文件路径")?;
    ensure_dir(parent)?;
    let mut tmp = tempfile::NamedTempFile::new_in(parent).map_err(|e| e.to_string())?;
    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt;
        tmp.as_file()
            .set_permissions(fs::Permissions::from_mode(0o600))
            .map_err(|e| e.to_string())?;
    }
    tmp.write_all(bytes).map_err(|e| e.to_string())?;
    tmp.as_file().sync_all().map_err(|e| e.to_string())?;
    tmp.persist(path)
        .map_err(|e| format!("无法写入 {}：{e}", path.display()))?;
    Ok(())
}

pub struct Store {
    pub settings: Settings,
    pub dir: PathBuf,
    pub secure_error: Option<String>,
    secrets: BTreeMap<String, String>,
    master: Option<Vec<u8>>,
}

impl Store {
    pub fn load(dir: PathBuf) -> Result<Self> {
        ensure_dir(&dir)?;
        let file = dir.join("state-v2.json");
        let settings = if file.exists() {
            serde_json::from_slice(&fs::read(&file).map_err(|e| e.to_string())?)
                .map_err(|e| format!("设置文件无法解析，原文件已保留：{e}"))?
        } else {
            Settings::default()
        };
        let mut result = Self {
            settings,
            dir,
            secure_error: None,
            secrets: BTreeMap::new(),
            master: None,
        };
        if result.dir.join("vault-v2.bin").exists() && result.load_vault().is_err() {
            result.secure_error = Some(
                "无法解锁系统凭据存储。请解锁钥匙串或凭据管理器后重启应用；已有数据未被覆盖。"
                    .into(),
            );
        }
        // An account is usable only when its session can be decrypted.
        if result.secret("access").is_empty() && result.secret("refresh").is_empty() {
            result.settings.account.logged_in = false;
        }
        Ok(result)
    }

    #[cfg(test)]
    pub fn for_test(dir: PathBuf) -> Self {
        let mut store = Self::load(dir).expect("temporary test store");
        store.master = Some(vec![9; 32]);
        store
    }

    #[cfg(test)]
    pub fn memory_secret(&mut self, name: &str, value: &str) {
        self.secrets.insert(name.into(), value.into());
    }

    fn credential() -> Result<keyring::Entry> {
        keyring::Entry::new("fun.csmy.aihubmix-switch", "vault-v2-master")
            .map_err(|_| "系统安全存储不可用".into())
    }

    fn load_vault(&mut self) -> Result<()> {
        let password = Self::credential()?
            .get_password()
            .map_err(|_| "无法读取钥匙串")?;
        let key = hex::decode(password).map_err(|_| "钥匙串密钥格式不正确")?;
        let cipher = ChaCha20Poly1305::new_from_slice(&key).map_err(|_| "无效密钥")?;
        let bytes = fs::read(self.dir.join("vault-v2.bin")).map_err(|e| e.to_string())?;
        if bytes.len() < 12 {
            return Err("加密存储已损坏".into());
        }
        let plain = cipher
            .decrypt(Nonce::from_slice(&bytes[..12]), &bytes[12..])
            .map_err(|_| "无法解密存储")?;
        self.secrets = serde_json::from_slice(&plain).map_err(|_| "无法解析加密存储")?;
        self.master = Some(key);
        Ok(())
    }

    fn persist_vault(&mut self, next: &BTreeMap<String, String>) -> Result<()> {
        if let Some(error) = &self.secure_error {
            return Err(error.clone());
        }
        if self.master.is_none() {
            let entry = Self::credential()?;
            let key = match entry.get_password() {
                Ok(password) => hex::decode(password).map_err(|_| "钥匙串密钥格式不正确")?,
                Err(keyring::Error::NoEntry) => {
                    let key = ChaCha20Poly1305::generate_key(&mut OsRng).to_vec();
                    entry
                        .set_password(&hex::encode(&key))
                        .map_err(|_| "无法保存到系统钥匙串，请允许应用访问安全存储")?;
                    key
                }
                Err(_) => return Err("无法访问系统钥匙串，请解锁安全存储后重试".into()),
            };
            self.master = Some(key);
        }
        let cipher = ChaCha20Poly1305::new_from_slice(self.master.as_ref().ok_or("缺少加密密钥")?)
            .map_err(|_| "无效密钥")?;
        let nonce = ChaCha20Poly1305::generate_nonce(&mut OsRng);
        let plain = serde_json::to_vec(next).map_err(|e| e.to_string())?;
        let encrypted = cipher
            .encrypt(&nonce, plain.as_ref())
            .map_err(|_| "加密失败")?;
        let mut bytes = nonce.to_vec();
        bytes.extend(encrypted);
        atomic_write(&self.dir.join("vault-v2.bin"), &bytes)
    }

    pub fn secret(&self, name: &str) -> String {
        self.secrets.get(name).cloned().unwrap_or_default()
    }

    pub fn set_secrets(&mut self, entries: &[(&str, &str)]) -> Result<()> {
        let mut next = self.secrets.clone();
        for (key, value) in entries {
            if value.is_empty() {
                next.remove(*key);
            } else {
                next.insert((*key).into(), (*value).into());
            }
        }
        self.persist_vault(&next)?;
        self.secrets = next;
        Ok(())
    }

    pub fn save(&self) -> Result<()> {
        atomic_write(
            &self.dir.join("state-v2.json"),
            &serde_json::to_vec_pretty(&self.settings).map_err(|e| e.to_string())?,
        )
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn credentials_are_authenticated_encrypted_and_absent_from_settings() {
        let dir = tempfile::tempdir().unwrap();
        let mut store = Store::load(dir.path().into()).unwrap();
        store.master = Some(vec![7; 32]);
        store
            .set_secrets(&[("local:-1", "test-private-key")])
            .unwrap();
        store.save().unwrap();
        let encrypted = fs::read(dir.path().join("vault-v2.bin")).unwrap();
        assert!(!String::from_utf8_lossy(&encrypted).contains("test-private-key"));
        assert!(!fs::read_to_string(dir.path().join("state-v2.json"))
            .unwrap()
            .contains("test-private-key"));
        let cipher = ChaCha20Poly1305::new_from_slice(&[7; 32]).unwrap();
        let plain = cipher
            .decrypt(Nonce::from_slice(&encrypted[..12]), &encrypted[12..])
            .unwrap();
        let secrets: BTreeMap<String, String> = serde_json::from_slice(&plain).unwrap();
        assert_eq!(secrets["local:-1"], "test-private-key");
        let mut tampered = encrypted;
        tampered[13] ^= 1;
        assert!(cipher
            .decrypt(Nonce::from_slice(&tampered[..12]), &tampered[12..])
            .is_err());
    }
    #[test]
    fn locked_storage_does_not_overwrite_vault_or_fall_back_to_plaintext() {
        let dir = tempfile::tempdir().unwrap();
        let mut store = Store::load(dir.path().into()).unwrap();
        store.secure_error = Some("locked".into());
        assert!(store.set_secrets(&[("access", "private-session")]).is_err());
        assert!(store.secret("access").is_empty());
        assert!(!dir.path().join("vault-v2.bin").exists());
    }
    #[test]
    fn corrupted_settings_are_not_silently_reset() {
        let dir = tempfile::tempdir().unwrap();
        fs::write(dir.path().join("state-v2.json"), "{bad-json").unwrap();
        assert!(Store::load(dir.path().into()).is_err());
        assert_eq!(
            fs::read_to_string(dir.path().join("state-v2.json")).unwrap(),
            "{bad-json"
        );
    }
}
