use directories::ProjectDirs;
use serde::{Deserialize, Serialize};
use std::fs;
use std::path::{Path, PathBuf};
use uuid::Uuid;

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub struct ProfileColor {
    pub r: u8,
    pub g: u8,
    pub b: u8,
}

impl ProfileColor {
    pub const fn new(r: u8, g: u8, b: u8) -> Self {
        Self { r, g, b }
    }

    pub fn to_hex(self) -> String {
        format!("#{:02X}{:02X}{:02X}", self.r, self.g, self.b)
    }

    pub fn palette() -> &'static [(&'static str, ProfileColor)] {
        const PALETTE: &[(&str, ProfileColor)] = &[
            ("Sky", ProfileColor::new(56, 189, 248)),
            ("Emerald", ProfileColor::new(52, 211, 153)),
            ("Amber", ProfileColor::new(251, 191, 36)),
            ("Rose", ProfileColor::new(251, 113, 133)),
            ("Violet", ProfileColor::new(167, 139, 250)),
            ("Teal", ProfileColor::new(45, 212, 191)),
            ("Coral", ProfileColor::new(251, 146, 60)),
            ("Indigo", ProfileColor::new(129, 140, 248)),
        ];
        PALETTE
    }
}

impl Default for ProfileColor {
    fn default() -> Self {
        Self::new(56, 189, 248)
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub enum AuthMethod {
    Password {
        password: String,
    },
    PrivateKey {
        key_path: String,
        passphrase: Option<String>,
    },
}

impl Default for AuthMethod {
    fn default() -> Self {
        Self::Password {
            password: String::new(),
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct PortForwardRule {
    pub id: Uuid,
    pub enabled: bool,
    pub label: String,
    pub bind_address: String,
    pub local_port: u16,
    pub remote_host: String,
    pub remote_port: u16,
}

impl PortForwardRule {
    pub fn new(
        label: impl Into<String>,
        bind_address: impl Into<String>,
        local_port: u16,
        remote_host: impl Into<String>,
        remote_port: u16,
    ) -> Self {
        Self {
            id: Uuid::new_v4(),
            enabled: true,
            label: label.into(),
            bind_address: bind_address.into(),
            local_port,
            remote_host: remote_host.into(),
            remote_port,
        }
    }

    pub fn normalized_bind(&self) -> String {
        let trimmed = self.bind_address.trim();
        if trimmed.is_empty() {
            "127.0.0.1".to_string()
        } else {
            trimmed.to_string()
        }
    }

    pub fn summary(&self) -> String {
        format!(
            "{}:{} -> {}:{}",
            self.normalized_bind(),
            self.local_port,
            self.remote_host.trim(),
            self.remote_port
        )
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct SshProfile {
    pub id: Uuid,
    pub name: String,
    pub env_badge: String,
    pub color: ProfileColor,
    pub host: String,
    pub port: u16,
    pub username: String,
    pub auth: AuthMethod,
    pub initial_remote_path: String,
    pub port_forwards: Vec<PortForwardRule>,
    pub notes: String,
}

impl SshProfile {
    pub fn new(name: impl Into<String>, host: impl Into<String>, username: impl Into<String>) -> Self {
        Self {
            id: Uuid::new_v4(),
            name: name.into(),
            env_badge: "DEV".to_string(),
            color: ProfileColor::default(),
            host: host.into(),
            port: 22,
            username: username.into(),
            auth: AuthMethod::default(),
            initial_remote_path: ".".to_string(),
            port_forwards: Vec::new(),
            notes: String::new(),
        }
    }

    pub fn monogram(&self) -> String {
        let chars: Vec<char> = self
            .name
            .split_whitespace()
            .filter_map(|w| w.chars().next())
            .take(2)
            .collect();
        if chars.is_empty() {
            "SS".to_string()
        } else {
            chars.into_iter().collect::<String>().to_uppercase()
        }
    }

    pub fn endpoint_label(&self) -> String {
        format!("{}@{}:{}", self.username.trim(), self.host.trim(), self.port)
    }

    pub fn enabled_port_forwards(&self) -> Vec<&PortForwardRule> {
        self.port_forwards.iter().filter(|r| r.enabled).collect()
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub enum TerminalAppKind {
    Auto,
    Alacritty,
    WindowsTerminal,
    PowerShell,
    Custom(String),
}

impl Default for TerminalAppKind {
    fn default() -> Self {
        Self::Auto
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub enum SftpAppKind {
    Auto,
    WinSCP,
    FileZilla,
    OpenSshSftp,
    Custom(String),
}

impl Default for SftpAppKind {
    fn default() -> Self {
        Self::Auto
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct AppSettings {
    pub theme_light: bool,
    pub terminal_app: TerminalAppKind,
    pub sftp_app: SftpAppKind,
}

impl Default for AppSettings {
    fn default() -> Self {
        Self {
            theme_light: true,
            terminal_app: TerminalAppKind::Auto,
            sftp_app: SftpAppKind::Auto,
        }
    }
}

impl AppSettings {
    pub fn default_path() -> PathBuf {
        if let Some(proj) = ProjectDirs::from("com", "rust-multi-ssh", "RustMultiSshConnect") {
            proj.config_dir().join("settings.json")
        } else {
            PathBuf::from("settings.json")
        }
    }

    pub fn load_default() -> Self {
        let path = Self::default_path();
        if path.exists() {
            if let Ok(raw) = fs::read_to_string(&path) {
                if let Ok(s) = serde_json::from_str::<AppSettings>(&raw) {
                    return s;
                }
            }
        }
        Self::default()
    }

    pub fn save(&self) -> anyhow::Result<()> {
        let path = Self::default_path();
        if let Some(parent) = path.parent() {
            fs::create_dir_all(parent)?;
        }
        let json = serde_json::to_string_pretty(self)?;
        fs::write(path, json)?;
        Ok(())
    }
}

#[derive(Debug, Clone, Serialize, Deserialize, Default)]
pub struct ProfileStoreData {
    pub profiles: Vec<SshProfile>,
}

pub struct ProfileStore {
    path: PathBuf,
    pub profiles: Vec<SshProfile>,
}

impl ProfileStore {
    pub fn default_path() -> PathBuf {
        if let Some(proj) = ProjectDirs::from("com", "rust-multi-ssh", "RustMultiSshConnect") {
            proj.config_dir().join("profiles.json")
        } else {
            PathBuf::from("profiles.json")
        }
    }

    pub fn load_default() -> Self {
        Self::load_from_path(Self::default_path())
    }

    pub fn load_from_path(path: PathBuf) -> Self {
        let profiles = if path.exists() {
            match fs::read_to_string(&path) {
                Ok(raw) => serde_json::from_str::<ProfileStoreData>(&raw)
                    .map(|d| d.profiles)
                    .unwrap_or_default(),
                Err(_) => Vec::new(),
            }
        } else {
            Vec::new()
        };

        Self { path, profiles }
    }

    pub fn path(&self) -> &Path {
        &self.path
    }

    pub fn save(&self) -> anyhow::Result<()> {
        if let Some(parent) = self.path.parent() {
            fs::create_dir_all(parent)?;
        }
        let data = ProfileStoreData {
            profiles: self.profiles.clone(),
        };
        let json = serde_json::to_string_pretty(&data)?;
        fs::write(&self.path, json)?;
        Ok(())
    }

    pub fn upsert(&mut self, profile: SshProfile) -> anyhow::Result<()> {
        if let Some(existing) = self.profiles.iter_mut().find(|p| p.id == profile.id) {
            *existing = profile;
        } else {
            self.profiles.push(profile);
        }
        self.save()
    }

    pub fn delete(&mut self, id: Uuid) -> anyhow::Result<()> {
        self.profiles.retain(|p| p.id != id);
        self.save()
    }

    pub fn duplicate(&mut self, id: Uuid) -> anyhow::Result<Option<SshProfile>> {
        if let Some(orig) = self.profiles.iter().find(|p| p.id == id).cloned() {
            let mut copy = orig;
            copy.id = Uuid::new_v4();
            copy.name = format!("{} (Copy)", copy.name);
            for rule in &mut copy.port_forwards {
                rule.id = Uuid::new_v4();
            }
            self.profiles.push(copy.clone());
            self.save()?;
            Ok(Some(copy))
        } else {
            Ok(None)
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_profile_store_roundtrip_and_operations() {
        let temp_dir = std::env::temp_dir().join(format!("multi_ssh_test_{}", Uuid::new_v4()));
        let store_path = temp_dir.join("profiles.json");
        let mut store = ProfileStore::load_from_path(store_path.clone());
        assert!(store.profiles.is_empty());
        assert_eq!(store.path(), store_path.as_path());

        let mut p = SshProfile::new("Main Production", "192.168.10.5", "deploy");
        p.env_badge = "PROD".to_string();
        p.color = ProfileColor::palette()[3].1;
        p.auth = AuthMethod::PrivateKey {
            key_path: "C:\\keys\\id_ed25519".to_string(),
            passphrase: Some("secret".to_string()),
        };
        p.port_forwards
            .push(PortForwardRule::new("DB", "127.0.0.1", 15432, "127.0.0.1", 5432));

        store.upsert(p.clone()).expect("upsert");

        let reloaded = ProfileStore::load_from_path(store_path.clone());
        assert_eq!(reloaded.profiles.len(), 1);
        assert_eq!(reloaded.profiles[0].name, "Main Production");
        assert_eq!(reloaded.profiles[0].monogram(), "MP");
        assert_eq!(reloaded.profiles[0].port_forwards.len(), 1);

        let dup = store.duplicate(p.id).expect("dup").expect("some");
        assert_ne!(dup.id, p.id);
        assert_ne!(dup.port_forwards[0].id, p.port_forwards[0].id);
        assert_eq!(store.profiles.len(), 2);

        store.delete(p.id).expect("delete");
        assert_eq!(store.profiles.len(), 1);

        let _ = fs::remove_dir_all(temp_dir);
    }
}
