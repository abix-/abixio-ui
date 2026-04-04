use serde::{Deserialize, Serialize};
use std::path::PathBuf;

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Connection {
    pub name: String,
    pub endpoint: String,
    pub credential: Option<String>,
}

fn config_dir() -> Result<PathBuf, String> {
    let home = dirs::home_dir().ok_or("could not determine home directory")?;
    Ok(home.join(".abixio-ui"))
}

fn connections_path() -> Result<PathBuf, String> {
    Ok(config_dir()?.join("connections.json"))
}

pub fn load() -> Result<Vec<Connection>, String> {
    let path = connections_path()?;
    if !path.exists() {
        return Ok(Vec::new());
    }
    let data = std::fs::read_to_string(&path).map_err(|e| e.to_string())?;
    serde_json::from_str(&data).map_err(|e| e.to_string())
}

pub fn save(connections: &[Connection]) -> Result<(), String> {
    let dir = config_dir()?;
    if !dir.exists() {
        std::fs::create_dir_all(&dir).map_err(|e| e.to_string())?;
    }
    let data = serde_json::to_string_pretty(connections).map_err(|e| e.to_string())?;
    std::fs::write(connections_path()?, data).map_err(|e| e.to_string())
}

pub fn add(connections: &mut Vec<Connection>, conn: Connection) -> Result<(), String> {
    connections.retain(|c| c.name != conn.name);
    connections.push(conn);
    save(connections)
}

pub fn remove(connections: &mut Vec<Connection>, name: &str) -> Result<(), String> {
    connections.retain(|c| c.name != name);
    save(connections)
}

pub fn is_valid_endpoint(url: &str) -> bool {
    url.starts_with("http://") || url.starts_with("https://")
}
