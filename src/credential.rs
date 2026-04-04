use serde::{Deserialize, Serialize};
use std::path::PathBuf;

use crate::keychain;

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct StoredCredential {
    pub name: String,
    pub access_key_id: String,
    pub region: String,
}

impl StoredCredential {
    /// Resolve the full credential by reading the secret key from the OS keychain.
    /// Returns (access_key_id, secret_key).
    pub fn resolve(&self) -> Result<(String, String), String> {
        let secret = keychain::get_secret(&self.name)?
            .ok_or_else(|| format!("no secret key found in keychain for '{}'", self.name))?;
        Ok((self.access_key_id.clone(), secret))
    }
}

fn config_dir() -> Result<PathBuf, String> {
    let home = dirs::home_dir().ok_or("could not determine home directory")?;
    Ok(home.join(".abixio-ui"))
}

fn credentials_path() -> Result<PathBuf, String> {
    Ok(config_dir()?.join("credentials.json"))
}

pub fn load() -> Result<Vec<StoredCredential>, String> {
    let path = credentials_path()?;
    if !path.exists() {
        return Ok(Vec::new());
    }
    let data = std::fs::read_to_string(&path).map_err(|e| e.to_string())?;
    serde_json::from_str(&data).map_err(|e| e.to_string())
}

pub fn save(credentials: &[StoredCredential]) -> Result<(), String> {
    let dir = config_dir()?;
    if !dir.exists() {
        std::fs::create_dir_all(&dir).map_err(|e| e.to_string())?;
    }
    let data = serde_json::to_string_pretty(credentials).map_err(|e| e.to_string())?;
    std::fs::write(credentials_path()?, data).map_err(|e| e.to_string())
}

pub fn add(credentials: &mut Vec<StoredCredential>, cred: StoredCredential, secret_key: &str) -> Result<(), String> {
    // store secret in keychain first
    keychain::store_secret(&cred.name, secret_key)?;
    // remove existing with same name
    credentials.retain(|c| c.name != cred.name);
    credentials.push(cred);
    save(credentials)
}

pub fn remove(credentials: &mut Vec<StoredCredential>, name: &str) -> Result<(), String> {
    keychain::delete_secret(name)?;
    credentials.retain(|c| c.name != name);
    save(credentials)
}

pub fn is_valid_name(name: &str) -> bool {
    !name.is_empty()
        && name.as_bytes()[0].is_ascii_alphabetic()
        && name.bytes().all(|b| b.is_ascii_alphanumeric() || b == b'_' || b == b'-')
}

pub fn is_valid_access_key(key: &str) -> bool {
    key.is_empty() || key.len() >= 3
}

pub fn is_valid_secret_key(key: &str) -> bool {
    key.is_empty() || key.len() >= 8
}
