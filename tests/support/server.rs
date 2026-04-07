use std::net::TcpListener;
use std::path::PathBuf;
use std::process::{Child, Command, Stdio};
use std::sync::Arc;
use std::time::{Duration, Instant};

use abixio_ui::abixio::client::AdminClient;
use abixio_ui::s3::client::S3Client;

pub fn find_abixio_binary() -> PathBuf {
    if let Ok(path) = std::env::var("ABIXIO_BIN") {
        let p = PathBuf::from(&path);
        assert!(p.exists(), "ABIXIO_BIN={} does not exist", path);
        return p;
    }
    for candidate in [
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
}

impl AbixioServer {
    pub fn builder() -> AbixioServerBuilder {
        AbixioServerBuilder {
            volume_count: 4,
            no_auth: true,
            scan_interval: "10m".to_string(),
            heal_interval: "24h".to_string(),
            mrf_workers: 2,
        }
    }

    pub fn endpoint(&self) -> String {
        format!("http://127.0.0.1:{}", self.port)
    }

    pub fn s3_client(&self) -> Arc<S3Client> {
        Arc::new(
            S3Client::new(&self.endpoint(), Some(("test", "testsecret")), "us-east-1")
                .expect("create S3 client"),
        )
    }

    pub fn admin_client(&self) -> Arc<AdminClient> {
        Arc::new(AdminClient::new(&self.endpoint(), None, "us-east-1"))
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

        let child = cmd.spawn().unwrap_or_else(|e| {
            panic!("failed to spawn abixio at {}: {}", binary.display(), e)
        });

        let mut server = AbixioServer {
            child,
            port,
            temp_dir,
        };

        wait_for_ready(port, &mut server);
        server
    }
}

fn wait_for_ready(port: u16, server: &mut AbixioServer) {
    let deadline = Instant::now() + Duration::from_secs(15);
    let addr = format!("127.0.0.1:{}", port);
    let request = format!(
        "GET /_admin/status HTTP/1.1\r\nHost: {}\r\nConnection: close\r\n\r\n",
        addr
    );

    while Instant::now() < deadline {
        if let Some(status) = server.child.try_wait().ok().flatten() {
            panic!(
                "abixio on port {} exited early with status: {}",
                port, status
            );
        }

        if let Ok(mut stream) =
            std::net::TcpStream::connect_timeout(&addr.parse().unwrap(), Duration::from_secs(1))
        {
            use std::io::{Read, Write};
            let _ = stream.set_read_timeout(Some(Duration::from_secs(2)));
            if stream.write_all(request.as_bytes()).is_ok() {
                let mut buf = [0u8; 256];
                if let Ok(n) = stream.read(&mut buf) {
                    let response = String::from_utf8_lossy(&buf[..n]);
                    if response.contains("200") {
                        return;
                    }
                }
            }
        }
        std::thread::sleep(Duration::from_millis(100));
    }

    panic!(
        "abixio on port {} did not become ready within 15 seconds",
        port
    );
}
