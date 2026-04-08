use std::path::PathBuf;
use std::process::Stdio;

use tokio::io::{AsyncBufReadExt, BufReader};
use tokio::process::{Child, Command};
use tokio::sync::mpsc;

use crate::config::ServerConfig;

/// Auto-detect the abixio binary by searching common locations.
pub fn find_binary(configured: &str) -> Option<PathBuf> {
    if !configured.is_empty() {
        let p = PathBuf::from(configured);
        if p.exists() {
            return Some(p);
        }
    }

    // known locations
    let candidates = [
        "C:\\code\\abixio\\abixio.exe",
        "C:\\code\\endless\\rust\\target\\release\\abixio.exe",
        "C:\\code\\endless\\rust\\target\\debug\\abixio.exe",
    ];
    for path in &candidates {
        let p = PathBuf::from(path);
        if p.exists() {
            return Some(p);
        }
    }

    // try PATH
    if let Ok(output) = std::process::Command::new("where")
        .arg("abixio.exe")
        .output()
        && output.status.success()
    {
        let stdout = String::from_utf8_lossy(&output.stdout);
        if let Some(line) = stdout.lines().next() {
            let p = PathBuf::from(line.trim());
            if p.exists() {
                return Some(p);
            }
        }
    }

    None
}

/// Build the command-line arguments for the abixio server.
fn build_args(config: &ServerConfig) -> Vec<String> {
    let mut args = Vec::new();

    if !config.listen.is_empty() {
        args.push("--listen".to_string());
        args.push(config.listen.clone());
    }

    for vol in &config.volumes {
        let trimmed = vol.trim();
        if !trimmed.is_empty() {
            args.push("--volumes".to_string());
            args.push(trimmed.to_string());
        }
    }

    if config.no_auth {
        args.push("--no-auth".to_string());
    }

    if !config.scan_interval.is_empty() {
        args.push("--scan-interval".to_string());
        args.push(config.scan_interval.clone());
    }

    if !config.heal_interval.is_empty() {
        args.push("--heal-interval".to_string());
        args.push(config.heal_interval.clone());
    }

    if config.mrf_workers > 0 {
        args.push("--mrf-workers".to_string());
        args.push(config.mrf_workers.to_string());
    }

    args
}

pub enum ServerEvent {
    Line(String),
    Exited(Option<i32>),
}

/// Spawn the abixio server and return the child + a channel of log lines.
pub fn spawn(
    config: &ServerConfig,
) -> Result<(Child, mpsc::UnboundedReceiver<ServerEvent>), String> {
    let binary = find_binary(&config.binary_path)
        .ok_or_else(|| "abixio binary not found. set the path in server settings.".to_string())?;

    let args = build_args(config);

    let mut child = Command::new(&binary)
        .args(&args)
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .kill_on_drop(true)
        .spawn()
        .map_err(|e| format!("failed to spawn {}: {}", binary.display(), e))?;

    let (tx, rx) = mpsc::unbounded_channel();

    // stream stdout
    if let Some(stdout) = child.stdout.take() {
        let tx_out = tx.clone();
        tokio::spawn(async move {
            let mut reader = BufReader::new(stdout).lines();
            while let Ok(Some(line)) = reader.next_line().await {
                if tx_out.send(ServerEvent::Line(line)).is_err() {
                    break;
                }
            }
        });
    }

    // stream stderr
    if let Some(stderr) = child.stderr.take() {
        let tx_err = tx.clone();
        tokio::spawn(async move {
            let mut reader = BufReader::new(stderr).lines();
            while let Ok(Some(line)) = reader.next_line().await {
                if tx_err.send(ServerEvent::Line(line)).is_err() {
                    break;
                }
            }
        });
    }

    // keep tx alive until streams end; drop signals EOF
    drop(tx);

    Ok((child, rx))
}

/// Resolve the listen address to a connectable endpoint.
/// ":10000" -> "http://127.0.0.1:10000"
/// "0.0.0.0:10000" -> "http://127.0.0.1:10000"
pub fn listen_to_endpoint(listen: &str) -> String {
    let addr = if listen.starts_with(':') {
        format!("127.0.0.1{}", listen)
    } else if listen.starts_with("0.0.0.0") {
        listen.replacen("0.0.0.0", "127.0.0.1", 1)
    } else {
        listen.to_string()
    };
    format!("http://{}", addr)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn build_args_default() {
        let config = ServerConfig::default();
        let args = build_args(&config);
        assert!(args.contains(&"--listen".to_string()));
        assert!(args.contains(&":10000".to_string()));
        assert!(!args.contains(&"--no-auth".to_string()));
    }

    #[test]
    fn build_args_with_volumes() {
        let config = ServerConfig {
            volumes: vec!["/mnt/d1".to_string(), "/mnt/d2".to_string()],
            ..Default::default()
        };
        let args = build_args(&config);
        let vol_count = args.iter().filter(|a| *a == "--volumes").count();
        assert_eq!(vol_count, 2);
    }

    #[test]
    fn build_args_no_auth() {
        let config = ServerConfig {
            no_auth: true,
            ..Default::default()
        };
        let args = build_args(&config);
        assert!(args.contains(&"--no-auth".to_string()));
    }

    #[test]
    fn build_args_empty_volumes_skipped() {
        let config = ServerConfig {
            volumes: vec!["".to_string(), "  ".to_string(), "/mnt/d1".to_string()],
            ..Default::default()
        };
        let args = build_args(&config);
        let vol_count = args.iter().filter(|a| *a == "--volumes").count();
        assert_eq!(vol_count, 1);
    }

    #[test]
    fn listen_to_endpoint_colon_prefix() {
        assert_eq!(
            listen_to_endpoint(":10000"),
            "http://127.0.0.1:10000"
        );
    }

    #[test]
    fn listen_to_endpoint_wildcard() {
        assert_eq!(
            listen_to_endpoint("0.0.0.0:10000"),
            "http://127.0.0.1:10000"
        );
    }

    #[test]
    fn listen_to_endpoint_specific() {
        assert_eq!(
            listen_to_endpoint("192.168.1.5:10000"),
            "http://192.168.1.5:10000"
        );
    }

    #[test]
    fn find_binary_empty_string() {
        // just tests the code path, not that the binary exists
        let result = find_binary("");
        // result depends on whether abixio.exe exists on this machine
        let _ = result;
    }
}
