use serde::{Deserialize, Serialize};
use std::path::PathBuf;

use crate::keychain;

#[derive(Debug, Clone, Serialize, Deserialize, Default, PartialEq)]
pub struct Settings {
    #[serde(default)]
    pub connections: Vec<Connection>,
}

/// A connection profile. Name, endpoint, and region are stored on disk.
/// Access key and secret key are stored in the OS keychain under the
/// connection name. If no keychain entry exists, the connection is anonymous.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
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

#[cfg(test)]
mod tests {
    use super::*;

    // -- is_valid_name --

    #[test]
    fn valid_name_simple() {
        assert!(is_valid_name("myconn"));
    }

    #[test]
    fn valid_name_with_dash_and_underscore() {
        assert!(is_valid_name("a-b_c"));
    }

    #[test]
    fn valid_name_single_letter() {
        assert!(is_valid_name("a"));
    }

    #[test]
    fn invalid_name_empty() {
        assert!(!is_valid_name(""));
    }

    #[test]
    fn invalid_name_starts_with_digit() {
        assert!(!is_valid_name("9start"));
    }

    #[test]
    fn invalid_name_has_space() {
        assert!(!is_valid_name("has space"));
    }

    #[test]
    fn invalid_name_has_dot() {
        assert!(!is_valid_name("has.dot"));
    }

    #[test]
    fn invalid_name_starts_with_dash() {
        assert!(!is_valid_name("-start"));
    }

    #[test]
    fn invalid_name_starts_with_underscore() {
        assert!(!is_valid_name("_start"));
    }

    // -- is_valid_endpoint --

    #[test]
    fn valid_endpoint_http() {
        assert!(is_valid_endpoint("http://localhost:10000"));
    }

    #[test]
    fn valid_endpoint_https() {
        assert!(is_valid_endpoint("https://s3.amazonaws.com"));
    }

    #[test]
    fn invalid_endpoint_ftp() {
        assert!(!is_valid_endpoint("ftp://example.com"));
    }

    #[test]
    fn invalid_endpoint_empty() {
        assert!(!is_valid_endpoint(""));
    }

    #[test]
    fn invalid_endpoint_no_scheme() {
        assert!(!is_valid_endpoint("localhost:10000"));
    }

    // -- is_valid_access_key --

    #[test]
    fn valid_access_key_empty() {
        assert!(is_valid_access_key(""));
    }

    #[test]
    fn valid_access_key_minimum() {
        assert!(is_valid_access_key("ABC"));
    }

    #[test]
    fn valid_access_key_aws_style() {
        assert!(is_valid_access_key("AKIAIOSFODNN7EXAMPLE"));
    }

    #[test]
    fn invalid_access_key_too_short() {
        assert!(!is_valid_access_key("AB"));
    }

    #[test]
    fn invalid_access_key_one_char() {
        assert!(!is_valid_access_key("X"));
    }

    // -- is_valid_secret_key --

    #[test]
    fn valid_secret_key_empty() {
        assert!(is_valid_secret_key(""));
    }

    #[test]
    fn valid_secret_key_minimum() {
        assert!(is_valid_secret_key("12345678"));
    }

    #[test]
    fn invalid_secret_key_too_short() {
        assert!(!is_valid_secret_key("1234567"));
    }

    #[test]
    fn invalid_secret_key_one_char() {
        assert!(!is_valid_secret_key("x"));
    }

    // -- Settings serde --

    #[test]
    fn settings_round_trip() {
        let settings = Settings {
            connections: vec![
                Connection {
                    name: "local".to_string(),
                    endpoint: "http://localhost:10000".to_string(),
                    region: "us-east-1".to_string(),
                },
                Connection {
                    name: "aws".to_string(),
                    endpoint: "https://s3.us-west-2.amazonaws.com".to_string(),
                    region: "us-west-2".to_string(),
                },
            ],
        };
        let json = serde_json::to_string(&settings).unwrap();
        let parsed: Settings = serde_json::from_str(&json).unwrap();
        assert_eq!(settings, parsed);
    }

    #[test]
    fn settings_empty_json_is_default() {
        let parsed: Settings = serde_json::from_str("{}").unwrap();
        assert_eq!(parsed, Settings::default());
        assert!(parsed.connections.is_empty());
    }

    #[test]
    fn settings_extra_fields_ignored() {
        let json = r#"{"connections": [], "unknown_field": 42}"#;
        let parsed: Settings = serde_json::from_str(json).unwrap();
        assert!(parsed.connections.is_empty());
    }

    #[test]
    fn settings_missing_connections_defaults_to_empty() {
        let json = r#"{}"#;
        let parsed: Settings = serde_json::from_str(json).unwrap();
        assert!(parsed.connections.is_empty());
    }

    #[test]
    fn connection_round_trip() {
        let conn = Connection {
            name: "test".to_string(),
            endpoint: "http://localhost:10000".to_string(),
            region: "us-east-1".to_string(),
        };
        let json = serde_json::to_string(&conn).unwrap();
        let parsed: Connection = serde_json::from_str(&json).unwrap();
        assert_eq!(conn, parsed);
    }

    #[test]
    fn connection_extra_fields_ignored() {
        let json =
            r#"{"name":"x","endpoint":"http://localhost","region":"us-east-1","extra":true}"#;
        let parsed: Connection = serde_json::from_str(json).unwrap();
        assert_eq!(parsed.name, "x");
    }
}
