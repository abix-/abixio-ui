use serde::{Deserialize, Serialize};
use std::path::PathBuf;

use crate::keychain;

#[derive(Debug, Clone, Serialize, Deserialize, Default)]
pub struct Settings {
    #[serde(default)]
    pub connections: Vec<Connection>,
}

/// A connection profile. Name, endpoint, and region are stored on disk.
/// Access key and secret key are stored in the OS keychain under the
/// connection name. If no keychain entry exists, the connection is anonymous.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Connection {
    pub name: String,
    pub endpoint: String,
    pub region: String,
}

impl Connection {
    /// Resolve credentials from the OS keychain.
    /// Returns Some((access_key, secret_key)) if keys exist, None for anonymous.
    pub fn resolve_keys(&self) -> Result<Option<(String, String)>, String> {
        keychain::get_keys(&self.name)
    }
}

// -- persistence --

fn config_dir() -> Result<PathBuf, String> {
    let home = dirs::home_dir().ok_or("could not determine home directory")?;
    Ok(home.join(".abixio-ui"))
}

fn settings_path() -> Result<PathBuf, String> {
    Ok(config_dir()?.join("settings.json"))
}

pub fn load() -> Result<Settings, String> {
    let path = settings_path()?;
    if !path.exists() {
        return Ok(Settings::default());
    }
    let data = std::fs::read_to_string(&path).map_err(|e| e.to_string())?;
    serde_json::from_str(&data).map_err(|e| e.to_string())
}

pub fn save(settings: &Settings) -> Result<(), String> {
    let dir = config_dir()?;
    if !dir.exists() {
        std::fs::create_dir_all(&dir).map_err(|e| e.to_string())?;
    }
    let data = serde_json::to_string_pretty(settings).map_err(|e| e.to_string())?;
    std::fs::write(settings_path()?, data).map_err(|e| e.to_string())
}

/// Add or update a connection. If access_key and secret_key are provided,
/// store them in the OS keychain. If both are empty, the connection is anonymous.
pub fn add_connection(
    settings: &mut Settings,
    conn: Connection,
    access_key: &str,
    secret_key: &str,
) -> Result<(), String> {
    if !access_key.is_empty() && !secret_key.is_empty() {
        keychain::store_keys(&conn.name, access_key, secret_key)?;
    }
    settings.connections.retain(|c| c.name != conn.name);
    settings.connections.push(conn);
    save(settings)
}

pub fn remove_connection(settings: &mut Settings, name: &str) -> Result<(), String> {
    // best-effort keychain cleanup
    let _ = keychain::delete_keys(name);
    settings.connections.retain(|c| c.name != name);
    save(settings)
}

// -- validation --

pub fn is_valid_name(name: &str) -> bool {
    !name.is_empty()
        && name.as_bytes()[0].is_ascii_alphabetic()
        && name
            .bytes()
            .all(|b| b.is_ascii_alphanumeric() || b == b'_' || b == b'-')
}

pub fn is_valid_endpoint(url: &str) -> bool {
    url.starts_with("http://") || url.starts_with("https://")
}

pub fn is_valid_access_key(key: &str) -> bool {
    key.is_empty() || key.len() >= 3
}

pub fn is_valid_secret_key(key: &str) -> bool {
    key.is_empty() || key.len() >= 8
}
