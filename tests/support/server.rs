use std::net::TcpListener;
use std::path::PathBuf;
use std::process::{Child, Command, Stdio};
use std::sync::Arc;
use std::time::{Duration, Instant};

use abixio_ui::abixio::client::AdminClient;
use abixio_ui::s3::client::S3Client;

use super::RUNTIME;
use super::tls::TlsMaterial;

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
        "abixio binary not found. Set ABIXIO_BIN or build abixio first.\n\
         Checked: ABIXIO_BIN env, C:\\code\\endless\\rust\\target\\debug\\abixio.exe, \
         C:\\code\\abixio\\abixio.exe"
    );
}

pub fn free_port() -> u16 {
    let listener = TcpListener::bind("127.0.0.1:0").expect("bind to free port");
    listener.local_addr().expect("get local addr").port()
}

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
        }
    }

    pub fn endpoint(&self) -> String {
        let scheme = if self.tls_ca_pem.is_some() {
            "https"
        } else {
            "http"
        };
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

    pub fn s3_client_anonymous(&self) -> Arc<S3Client> {
        Arc::new(
            S3Client::new_with_ca_pem(
                &self.endpoint(),
                None,
                "us-east-1",
                self.tls_ca_pem.as_deref(),
            )
            .expect("create anonymous S3 client"),
        )
    }

    pub fn admin_client(&self) -> Arc<AdminClient> {
        Arc::new(AdminClient::new_with_ca_pem(
            &self.endpoint(),
            None,
            "us-east-1",
            self.tls_ca_pem.as_deref(),
        ))
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
}

impl AbixioServerBuilder {
    pub fn volume_count(mut self, n: usize) -> Self {
        self.volume_count = n;
        self
    }

    #[allow(dead_code)]
    pub fn no_auth(mut self, v: bool) -> Self {
        self.no_auth = v;
        self
    }

    #[allow(dead_code)]
    pub fn scan_interval(mut self, s: &str) -> Self {
        self.scan_interval = s.to_string();
        self
    }

    #[allow(dead_code)]
    pub fn heal_interval(mut self, s: &str) -> Self {
        self.heal_interval = s.to_string();
        self
    }

    #[allow(dead_code)]
    pub fn mrf_workers(mut self, n: usize) -> Self {
        self.mrf_workers = n;
        self
    }

    #[allow(dead_code)]
    pub fn tls(mut self, tls: &TlsMaterial) -> Self {
        self.tls = Some((
            tls.leaf_cert_path.to_string_lossy().to_string(),
            tls.leaf_key_path.to_string_lossy().to_string(),
            tls.ca_cert_pem.clone(),
        ));
        self
    }

    /// Set the write tier the abixio binary will use: `file` (default
    /// in the binary), `log`, or `pool`. Passes `--write-tier <tier>` to
    /// the abixio process.
    #[allow(dead_code)]
    pub fn write_tier(mut self, tier: &str) -> Self {
        self.write_tier = Some(tier.to_string());
        self
    }

    pub fn start(self) -> AbixioServer {
        let binary = find_abixio_binary();
        let port = free_port();
        let temp_dir = std::env::temp_dir().join(format!("abixio-test-{}", port));

        let mut volume_paths = Vec::new();
        for i in 1..=self.volume_count {
            let vol = temp_dir.join(format!("d{}", i));
            std::fs::create_dir_all(&vol).expect("create volume dir");
            volume_paths.push(vol.to_string_lossy().to_string());
        }

        let listen = format!("0.0.0.0:{}", port);
        let volumes = volume_paths.join(",");

        let mut cmd = Command::new(&binary);
        cmd.arg("--listen")
            .arg(&listen)
            .arg("--volumes")
            .arg(&volumes)
            .arg("--scan-interval")
            .arg(&self.scan_interval)
            .arg("--heal-interval")
            .arg(&self.heal_interval)
            .arg("--mrf-workers")
            .arg(self.mrf_workers.to_string())
            .env("ABIXIO_ACCESS_KEY", "test")
            .env("ABIXIO_SECRET_KEY", "testsecret")
            .stdout(Stdio::piped())
            .stderr(Stdio::piped());

        if self.no_auth {
            cmd.arg("--no-auth");
        }

        if let Some((cert, key, _)) = &self.tls {
            cmd.arg("--tls-cert").arg(cert).arg("--tls-key").arg(key);
        }

        if let Some(tier) = &self.write_tier {
            cmd.arg("--write-tier").arg(tier);
        }

        let child = cmd
            .spawn()
            .unwrap_or_else(|e| panic!("failed to spawn abixio at {}: {}", binary.display(), e));

        let mut server = AbixioServer {
            child,
            port,
            temp_dir,
            tls_ca_pem: self.tls.map(|(_, _, ca_pem)| ca_pem),
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
            panic!(
                "abixio on port {} exited early with status: {}",
                server.port, status
            );
        }

        // block_on on a fresh OS thread so this works from both sync callers
        // and `#[tokio::test]` workers (can't nest block_on inside a runtime).
        let client_ref = &client;
        let url_ref = &url;
        let ready = std::thread::scope(|s| {
            s.spawn(move || {
                RUNTIME.block_on(async move {
                    match client_ref.get(url_ref).send().await {
                        Ok(resp) => resp.status().is_success(),
                        Err(_) => false,
                    }
                })
            })
            .join()
            .unwrap()
        });
        if ready {
            return;
        }
        std::thread::sleep(Duration::from_millis(100));
    }

    panic!(
        "abixio on port {} did not become ready within 15 seconds",
        server.port
    );
}
