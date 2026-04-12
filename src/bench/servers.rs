use std::net::TcpListener;
use std::path::{Path, PathBuf};
use std::process::{Child, Command, Stdio};
use std::sync::{Arc, LazyLock};
use std::time::{Duration, Instant};

use crate::s3::client::S3Client;

use super::tls::TlsMaterial;

static BENCH_RUNTIME: LazyLock<tokio::runtime::Runtime> =
    LazyLock::new(|| tokio::runtime::Runtime::new().expect("create bench runtime"));

pub fn find_abixio_binary() -> PathBuf {
    if let Ok(path) = std::env::var("ABIXIO_BIN") {
        let p = PathBuf::from(&path);
        assert!(p.exists(), "ABIXIO_BIN={} does not exist", path);
        return p;
    }
    for candidate in [
        r"C:\code\endless\rust\target\release\abixio.exe",
        r"C:\code\endless\rust\target\debug\abixio.exe",
        r"C:\code\abixio\abixio.exe",
    ] {
        let p = PathBuf::from(candidate);
        if p.exists() {
            return p;
        }
    }
    panic!(
        "abixio binary not found. Set ABIXIO_BIN or build abixio first."
    );
}

pub fn find_binary(env_var: &str, default: &str) -> Option<String> {
    if let Ok(p) = std::env::var(env_var) {
        if Path::new(&p).exists() {
            return Some(p);
        }
    }
    if Path::new(default).exists() {
        return Some(default.to_string());
    }
    if let Ok(output) = Command::new("where.exe")
        .arg(default.split('\\').last().unwrap_or(default))
        .output()
    {
        if output.status.success() {
            return Some(String::from_utf8_lossy(&output.stdout).trim().to_string());
        }
    }
    None
}

pub fn expect_binary(env_var: &str, default: &str, display: &str) -> String {
    find_binary(env_var, default).unwrap_or_else(|| {
        panic!(
            "{} binary not found. Set {} or install it in PATH.",
            display, env_var
        )
    })
}

pub fn free_port() -> u16 {
    let listener = TcpListener::bind("127.0.0.1:0").expect("bind to free port");
    listener.local_addr().expect("get local addr").port()
}

// -- AbixioServer --

pub struct AbixioServer {
    child: Child,
    port: u16,
    temp_dir: PathBuf,
    tls_ca_pem: Option<Vec<u8>>,
}

impl AbixioServer {
    pub fn builder() -> AbixioServerBuilder {
        AbixioServerBuilder {
            volume_count: 4,
            no_auth: true,
            scan_interval: "10m".to_string(),
            heal_interval: "24h".to_string(),
            mrf_workers: 2,
            tls: None,
            write_tier: None,
            write_cache: None,
        }
    }

    pub fn endpoint(&self) -> String {
        let scheme = if self.tls_ca_pem.is_some() { "https" } else { "http" };
        format!("{}://127.0.0.1:{}", scheme, self.port)
    }

    pub fn s3_client(&self) -> Arc<S3Client> {
        Arc::new(
            S3Client::new_with_ca_pem(
                &self.endpoint(),
                Some(("test", "testsecret")),
                "us-east-1",
                self.tls_ca_pem.as_deref(),
            )
            .expect("create S3 client"),
        )
    }

    pub fn s3_client_with_creds(&self, creds: (&str, &str)) -> Arc<S3Client> {
        Arc::new(
            S3Client::new_with_ca_pem(
                &self.endpoint(),
                Some(creds),
                "us-east-1",
                self.tls_ca_pem.as_deref(),
            )
            .expect("create S3 client"),
        )
    }

    pub fn ca_cert_pem(&self) -> Option<&[u8]> {
        self.tls_ca_pem.as_deref()
    }
}

impl Drop for AbixioServer {
    fn drop(&mut self) {
        let _ = self.child.kill();
        let _ = self.child.wait();
        let _ = std::fs::remove_dir_all(&self.temp_dir);
    }
}

pub struct AbixioServerBuilder {
    volume_count: usize,
    no_auth: bool,
    scan_interval: String,
    heal_interval: String,
    mrf_workers: usize,
    tls: Option<(String, String, Vec<u8>)>,
    write_tier: Option<String>,
    write_cache: Option<u64>,
}

impl AbixioServerBuilder {
    pub fn volume_count(mut self, n: usize) -> Self { self.volume_count = n; self }
    pub fn no_auth(mut self, v: bool) -> Self { self.no_auth = v; self }
    pub fn scan_interval(mut self, s: &str) -> Self { self.scan_interval = s.to_string(); self }
    pub fn heal_interval(mut self, s: &str) -> Self { self.heal_interval = s.to_string(); self }
    pub fn mrf_workers(mut self, n: usize) -> Self { self.mrf_workers = n; self }

    pub fn tls(mut self, tls: &TlsMaterial) -> Self {
        self.tls = Some((
            tls.leaf_cert_path.to_string_lossy().to_string(),
            tls.leaf_key_path.to_string_lossy().to_string(),
            tls.ca_cert_pem.clone(),
        ));
        self
    }

    pub fn write_tier(mut self, tier: &str) -> Self {
        self.write_tier = Some(tier.to_string());
        self
    }

    pub fn write_cache(mut self, mb: u64) -> Self {
        self.write_cache = Some(mb);
        self
    }

    pub fn start(self) -> AbixioServer {
        let binary = find_abixio_binary();
        let port = free_port();
        let temp_dir = std::env::temp_dir().join(format!("abixio-bench-{}", port));

        let mut volume_paths = Vec::new();
        for i in 1..=self.volume_count {
            let vol = temp_dir.join(format!("d{}", i));
            std::fs::create_dir_all(&vol).expect("create volume dir");
            volume_paths.push(vol.to_string_lossy().to_string());
        }

        let mut cmd = Command::new(&binary);
        cmd.arg("--listen").arg(format!("0.0.0.0:{}", port))
            .arg("--volumes").arg(volume_paths.join(","))
            .arg("--scan-interval").arg(&self.scan_interval)
            .arg("--heal-interval").arg(&self.heal_interval)
            .arg("--mrf-workers").arg(self.mrf_workers.to_string())
            .env("ABIXIO_ACCESS_KEY", "test")
            .env("ABIXIO_SECRET_KEY", "testsecret")
            .stdout(Stdio::piped())
            .stderr(Stdio::piped());

        if self.no_auth { cmd.arg("--no-auth"); }
        if let Some((cert, key, _)) = &self.tls {
            cmd.arg("--tls-cert").arg(cert).arg("--tls-key").arg(key);
        }
        if let Some(tier) = &self.write_tier {
            cmd.arg("--write-tier").arg(tier);
        }
        if let Some(mb) = self.write_cache {
            cmd.arg("--write-cache").arg(mb.to_string());
        }

        let child = cmd.spawn()
            .unwrap_or_else(|e| panic!("failed to spawn abixio at {}: {}", binary.display(), e));

        let mut server = AbixioServer {
            child,
            port,
            temp_dir,
            tls_ca_pem: self.tls.map(|(_, _, ca)| ca),
        };

        wait_for_ready(&mut server);
        server
    }
}

fn wait_for_ready(server: &mut AbixioServer) {
    let deadline = Instant::now() + Duration::from_secs(15);
    let url = format!("{}/_admin/status", server.endpoint());
    let mut client_builder = reqwest::Client::builder();
    if let Some(ca_pem) = server.tls_ca_pem.as_deref() {
        let cert = reqwest::Certificate::from_pem(ca_pem).expect("parse CA cert");
        client_builder = client_builder.add_root_certificate(cert);
    }
    let client = client_builder.build().expect("build readiness client");

    while Instant::now() < deadline {
        if let Some(status) = server.child.try_wait().ok().flatten() {
            panic!("abixio on port {} exited early: {}", server.port, status);
        }
        let client_ref = &client;
        let url_ref = &url;
        let ready = std::thread::scope(|s| {
            s.spawn(move || {
                BENCH_RUNTIME.block_on(async move {
                    match client_ref.get(url_ref).send().await {
                        Ok(resp) => resp.status().is_success(),
                        Err(_) => false,
                    }
                })
            })
            .join()
            .unwrap()
        });
        if ready { return; }
        std::thread::sleep(Duration::from_millis(100));
    }
    panic!("abixio on port {} did not become ready within 15s", server.port);
}

// -- ExternalServer (RustFS, MinIO) --

pub struct ExternalServer {
    child: Child,
    port: u16,
    _temp: tempfile::TempDir,
    ca_cert_pem: Vec<u8>,
}

impl ExternalServer {
    pub fn start_rustfs_tls(bin: &str, port: u16, tls: &TlsMaterial) -> Option<Self> {
        if !Path::new(bin).exists() { return None; }
        let tmp = tempfile::TempDir::new().ok()?;
        let console_port = port + 1;
        let child = Command::new(bin)
            .args([
                "server", tmp.path().to_str().unwrap(),
                "--address", &format!(":{}", port),
                "--console-address", &format!(":{}", console_port),
                "--tls-path", tls.rustfs_tls_dir.to_str().unwrap(),
            ])
            .env("RUSTFS_ROOT_USER", "benchuser")
            .env("RUSTFS_ROOT_PASSWORD", "benchpass")
            .stdout(Stdio::null())
            .stderr(Stdio::null())
            .spawn().ok()?;
        let mut server = Self { child, port, _temp: tmp, ca_cert_pem: tls.ca_cert_pem.clone() };
        server.wait_for_ready();
        Some(server)
    }

    pub fn start_minio_tls(bin: &str, port: u16, tls: &TlsMaterial) -> Option<Self> {
        if !Path::new(bin).exists() { return None; }
        let tmp = tempfile::TempDir::new().ok()?;
        let console_port = port + 1;
        let child = Command::new(bin)
            .args([
                "server", tmp.path().to_str().unwrap(),
                "--address", &format!(":{}", port),
                "--console-address", &format!(":{}", console_port),
                "--certs-dir", tls.minio_certs_dir.to_str().unwrap(),
            ])
            .env("MINIO_ROOT_USER", "benchuser")
            .env("MINIO_ROOT_PASSWORD", "benchpass")
            .stdout(Stdio::null())
            .stderr(Stdio::null())
            .spawn().ok()?;
        let mut server = Self { child, port, _temp: tmp, ca_cert_pem: tls.ca_cert_pem.clone() };
        server.wait_for_ready();
        Some(server)
    }

    pub fn endpoint(&self) -> String {
        format!("https://127.0.0.1:{}", self.port)
    }

    pub fn s3_client(&self, creds: (&str, &str)) -> Arc<S3Client> {
        Arc::new(
            S3Client::new_with_ca_pem(
                &self.endpoint(),
                Some(creds),
                "us-east-1",
                Some(&self.ca_cert_pem),
            )
            .expect("create S3 client"),
        )
    }

    pub fn ca_cert_pem(&self) -> &[u8] {
        &self.ca_cert_pem
    }

    fn wait_for_ready(&mut self) {
        let deadline = Instant::now() + Duration::from_secs(20);
        let cert = reqwest::Certificate::from_pem(&self.ca_cert_pem).expect("parse benchmark CA");
        let client = reqwest::Client::builder()
            .add_root_certificate(cert)
            .build()
            .expect("build external readiness client");
        let url = self.endpoint();

        while Instant::now() < deadline {
            if let Some(status) = self.child.try_wait().ok().flatten() {
                panic!("external server on port {} exited early: {}", self.port, status);
            }
            let client_ref = &client;
            let url_ref = &url;
            let ready = std::thread::scope(|s| {
                s.spawn(move || {
                    BENCH_RUNTIME.block_on(async move {
                        client_ref.get(url_ref).send().await.is_ok()
                    })
                })
                .join()
                .unwrap()
            });
            if ready { return; }
            std::thread::sleep(Duration::from_millis(100));
        }
        panic!("external server on port {} did not become ready", self.port);
    }
}

impl Drop for ExternalServer {
    fn drop(&mut self) {
        let _ = self.child.kill();
        let _ = self.child.wait();
    }
}
