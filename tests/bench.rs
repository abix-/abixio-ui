//! Benchmark suite for AbixIO storage operations.
//!
//! Launches real abixio server instances with 1-4 disks and measures
//! PUT, GET, HEAD, LIST, DELETE latency and throughput via aws-sdk-s3.
//!
//! Run with: `cargo test --test bench -- --ignored --nocapture`
//! Single config: `cargo test --test bench -- --ignored --nocapture bench_4_disks`

#[path = "support/mod.rs"]
mod support;

use std::sync::Arc;
use std::time::{Duration, Instant};

use support::server::AbixioServer;
use support::tls::TlsMaterial;

// -- result types --

struct BenchResult {
    op: &'static str,
    size: &'static str,
    size_bytes: usize,
    iters: usize,
    timings: Vec<Duration>,
}

// -- statistics --

fn percentile(timings: &mut [Duration], p: f64) -> Duration {
    timings.sort();
    let idx = ((timings.len() as f64 * p / 100.0) - 1.0).max(0.0) as usize;
    timings[idx.min(timings.len() - 1)]
}

fn avg(timings: &[Duration]) -> Duration {
    let total: Duration = timings.iter().sum();
    total / timings.len() as u32
}

fn format_duration(d: Duration) -> String {
    let ms = d.as_secs_f64() * 1000.0;
    if ms < 1.0 {
        format!("{:.1}us", ms * 1000.0)
    } else if ms < 1000.0 {
        format!("{:.1}ms", ms)
    } else {
        format!("{:.2}s", ms / 1000.0)
    }
}

fn format_throughput(size_bytes: usize, iters: usize, total: Duration) -> String {
    if size_bytes == 0 {
        return "-".to_string();
    }
    let total_bytes = size_bytes as f64 * iters as f64;
    let secs = total.as_secs_f64();
    if secs == 0.0 {
        return "-".to_string();
    }
    let bytes_per_sec = total_bytes / secs;
    if bytes_per_sec > 1024.0 * 1024.0 {
        format!("{:.1} MB/s", bytes_per_sec / (1024.0 * 1024.0))
    } else {
        format!("{:.1} KB/s", bytes_per_sec / 1024.0)
    }
}

fn print_results(config_name: &str, results: &mut [BenchResult]) {
    eprintln!();
    eprintln!("--- {} ---", config_name);
    eprintln!(
        "{:<8} {:<6} {:>6} ops  {:>10}  {:>10}  {:>10}  {:>12}  {:>10}",
        "OP", "SIZE", "", "avg", "p50", "p99", "MB/s", "obj/sec"
    );

    for r in results.iter_mut() {
        let total: Duration = r.timings.iter().sum();
        let a = avg(&r.timings);
        let p50 = percentile(&mut r.timings, 50.0);
        let p99 = percentile(&mut r.timings, 99.0);
        let tp = format_throughput(r.size_bytes, r.iters, total);
        let ops = if total.as_secs_f64() > 0.0 {
            format!("{:.0}", r.iters as f64 / total.as_secs_f64())
        } else {
            "-".to_string()
        };

        eprintln!(
            "{:<8} {:<6} {:>6} ops  {:>10}  {:>10}  {:>10}  {:>12}  {:>10}",
            r.op,
            r.size,
            r.iters,
            format_duration(a),
            format_duration(p50),
            format_duration(p99),
            tp,
            ops,
        );
    }
}

// -- benchmark runner --

async fn run_bench(disks: usize) {
    let ec_desc = match disks {
        1 => "EC 1+0",
        2 => "EC 1+1",
        3 => "EC 2+1",
        4 => "EC 3+1",
        _ => "EC auto",
    };
    let config_name = format!("{} disk(s) ({})", disks, ec_desc);
    eprintln!("\nstarting abixio with {} disks...", disks);

    let server = AbixioServer::builder()
        .volume_count(disks)
        .no_auth(false)
        .start();
    let client = server.s3_client();

    eprintln!("server ready at {}", server.endpoint());
    if let Err(e) = client.create_bucket("bench").await {
        eprintln!("create_bucket failed: {}", e);
        panic!("create_bucket failed: {}", e);
    }

    // pre-generate payloads
    let payload_4k = vec![0x42u8; 4096];
    let payload_1k = vec![0x42u8; 1024];
    let payload_1m = vec![0x42u8; 1024 * 1024];
    let payload_10m = vec![0x42u8; 10 * 1024 * 1024];

    let mut results = Vec::new();

    // -- warmup (3 puts, not timed) --
    for i in 0..3 {
        client
            .put_object(
                "bench",
                &format!("warmup/{}", i),
                payload_1k.clone(),
                "application/octet-stream",
            )
            .await
            .unwrap();
    }

    // -- PUT 4KB (small object hot path) --
    let iters = 500;
    let mut timings = Vec::with_capacity(iters);
    for i in 0..iters {
        let t = Instant::now();
        client
            .put_object(
                "bench",
                &format!("tiny/{}", i),
                payload_4k.clone(),
                "application/octet-stream",
            )
            .await
            .unwrap();
        timings.push(t.elapsed());
    }
    results.push(BenchResult {
        op: "PUT",
        size: "4KB",
        size_bytes: 4096,
        iters,
        timings,
    });

    // -- GET 4KB --
    let iters = 500;
    let mut timings = Vec::with_capacity(iters);
    for i in 0..iters {
        let t = Instant::now();
        let _ = client
            .get_object("bench", &format!("tiny/{}", i))
            .await
            .unwrap();
        timings.push(t.elapsed());
    }
    results.push(BenchResult {
        op: "GET",
        size: "4KB",
        size_bytes: 4096,
        iters,
        timings,
    });

    // -- HEAD 4KB --
    let iters = 500;
    let mut timings = Vec::with_capacity(iters);
    for i in 0..iters {
        let t = Instant::now();
        let _ = client
            .head_object("bench", &format!("tiny/{}", i))
            .await
            .unwrap();
        timings.push(t.elapsed());
    }
    results.push(BenchResult {
        op: "HEAD",
        size: "4KB",
        size_bytes: 0,
        iters,
        timings,
    });

    // -- DELETE 4KB --
    let iters = 500;
    let mut timings = Vec::with_capacity(iters);
    for i in 0..iters {
        let t = Instant::now();
        client
            .delete_object("bench", &format!("tiny/{}", i))
            .await
            .unwrap();
        timings.push(t.elapsed());
    }
    results.push(BenchResult {
        op: "DELETE",
        size: "4KB",
        size_bytes: 0,
        iters,
        timings,
    });

    // -- PUT 1KB --
    let iters = 100;
    let mut timings = Vec::with_capacity(iters);
    for i in 0..iters {
        let t = Instant::now();
        client
            .put_object(
                "bench",
                &format!("small/{}", i),
                payload_1k.clone(),
                "application/octet-stream",
            )
            .await
            .unwrap();
        timings.push(t.elapsed());
    }
    results.push(BenchResult {
        op: "PUT",
        size: "1KB",
        size_bytes: 1024,
        iters,
        timings,
    });

    // -- PUT 1MB --
    let iters = 20;
    let mut timings = Vec::with_capacity(iters);
    for i in 0..iters {
        let t = Instant::now();
        client
            .put_object(
                "bench",
                &format!("medium/{}", i),
                payload_1m.clone(),
                "application/octet-stream",
            )
            .await
            .unwrap();
        timings.push(t.elapsed());
    }
    results.push(BenchResult {
        op: "PUT",
        size: "1MB",
        size_bytes: 1024 * 1024,
        iters,
        timings,
    });

    // -- PUT 10MB --
    let iters = 5;
    let mut timings = Vec::with_capacity(iters);
    for i in 0..iters {
        let t = Instant::now();
        client
            .put_object(
                "bench",
                &format!("large/{}", i),
                payload_10m.clone(),
                "application/octet-stream",
            )
            .await
            .unwrap();
        timings.push(t.elapsed());
    }
    results.push(BenchResult {
        op: "PUT",
        size: "10MB",
        size_bytes: 10 * 1024 * 1024,
        iters,
        timings,
    });

    // -- PUT 10MB UNSIGNED (skip client-side SHA256) --
    let iters = 5;
    let mut timings = Vec::with_capacity(iters);
    for i in 0..iters {
        let t = Instant::now();
        client
            .put_object_unsigned(
                "bench",
                &format!("unsigned/{}", i),
                payload_10m.clone(),
                "application/octet-stream",
            )
            .await
            .unwrap();
        timings.push(t.elapsed());
    }
    results.push(BenchResult {
        op: "PUT*",
        size: "10MB",
        size_bytes: 10 * 1024 * 1024,
        iters,
        timings,
    });

    // -- GET 1KB --
    let iters = 100;
    let mut timings = Vec::with_capacity(iters);
    for i in 0..iters {
        let t = Instant::now();
        let _ = client
            .get_object("bench", &format!("small/{}", i))
            .await
            .unwrap();
        timings.push(t.elapsed());
    }
    results.push(BenchResult {
        op: "GET",
        size: "1KB",
        size_bytes: 1024,
        iters,
        timings,
    });

    // -- GET 1MB --
    let iters = 20;
    let mut timings = Vec::with_capacity(iters);
    for i in 0..iters {
        let t = Instant::now();
        let _ = client
            .get_object("bench", &format!("medium/{}", i))
            .await
            .unwrap();
        timings.push(t.elapsed());
    }
    results.push(BenchResult {
        op: "GET",
        size: "1MB",
        size_bytes: 1024 * 1024,
        iters,
        timings,
    });

    // -- GET 10MB --
    let iters = 5;
    let mut timings = Vec::with_capacity(iters);
    for i in 0..iters {
        let t = Instant::now();
        let _ = client
            .get_object("bench", &format!("large/{}", i))
            .await
            .unwrap();
        timings.push(t.elapsed());
    }
    results.push(BenchResult {
        op: "GET",
        size: "10MB",
        size_bytes: 10 * 1024 * 1024,
        iters,
        timings,
    });

    // -- HEAD --
    let iters = 100;
    let mut timings = Vec::with_capacity(iters);
    for i in 0..iters {
        let t = Instant::now();
        let _ = client
            .head_object("bench", &format!("small/{}", i))
            .await
            .unwrap();
        timings.push(t.elapsed());
    }
    results.push(BenchResult {
        op: "HEAD",
        size: "-",
        size_bytes: 0,
        iters,
        timings,
    });

    // -- LIST (100 objects already in small/) --
    let iters = 50;
    let mut timings = Vec::with_capacity(iters);
    for _ in 0..iters {
        let t = Instant::now();
        let _ = client.list_objects("bench", "small/", "").await.unwrap();
        timings.push(t.elapsed());
    }
    results.push(BenchResult {
        op: "LIST",
        size: "100obj",
        size_bytes: 0,
        iters,
        timings,
    });

    // -- DELETE --
    let iters = 100;
    let mut timings = Vec::with_capacity(iters);
    for i in 0..iters {
        let t = Instant::now();
        client
            .delete_object("bench", &format!("small/{}", i))
            .await
            .unwrap();
        timings.push(t.elapsed());
    }
    results.push(BenchResult {
        op: "DELETE",
        size: "1KB",
        size_bytes: 0,
        iters,
        timings,
    });

    print_results(&config_name, &mut results);
}

// -- test entry points --

async fn run_raw_disk() {
    eprintln!("\n--- raw disk baseline (tokio::fs) ---");
    eprintln!(
        "{:<8} {:<6} {:>6} ops  {:>10}  {:>10}  {:>10}  {:>12}",
        "OP", "SIZE", "", "avg", "p50", "p99", "throughput"
    );

    let tmp = tempfile::TempDir::new().unwrap();
    let base = tmp.path().to_path_buf();

    let payload_1k = vec![0x42u8; 1024];
    let payload_1m = vec![0x42u8; 1024 * 1024];
    let payload_10m = vec![0x42u8; 10 * 1024 * 1024];

    // raw write 1KB
    let iters = 200;
    let mut timings = Vec::with_capacity(iters);
    for i in 0..iters {
        let path = base.join(format!("w1k_{}", i));
        let t = Instant::now();
        tokio::fs::write(&path, &payload_1k).await.unwrap();
        timings.push(t.elapsed());
    }
    let mut r = BenchResult {
        op: "WRITE",
        size: "1KB",
        size_bytes: 1024,
        iters,
        timings,
    };
    print_one(&mut r);

    // raw write 1MB
    let iters = 50;
    let mut timings = Vec::with_capacity(iters);
    for i in 0..iters {
        let path = base.join(format!("w1m_{}", i));
        let t = Instant::now();
        tokio::fs::write(&path, &payload_1m).await.unwrap();
        timings.push(t.elapsed());
    }
    let mut r = BenchResult {
        op: "WRITE",
        size: "1MB",
        size_bytes: 1024 * 1024,
        iters,
        timings,
    };
    print_one(&mut r);

    // raw write 10MB (cached)
    let iters = 10;
    let mut timings = Vec::with_capacity(iters);
    for i in 0..iters {
        let path = base.join(format!("w10m_{}", i));
        let t = Instant::now();
        tokio::fs::write(&path, &payload_10m).await.unwrap();
        timings.push(t.elapsed());
    }
    let mut r = BenchResult {
        op: "WRITE",
        size: "10MB",
        size_bytes: 10 * 1024 * 1024,
        iters,
        timings,
    };
    print_one(&mut r);

    // raw write 10MB + fsync (real disk speed)
    let iters = 10;
    let mut timings = Vec::with_capacity(iters);
    for i in 0..iters {
        let path = base.join(format!("s10m_{}", i));
        let t = Instant::now();
        {
            use tokio::io::AsyncWriteExt;
            let mut f = tokio::fs::File::create(&path).await.unwrap();
            f.write_all(&payload_10m).await.unwrap();
            f.sync_all().await.unwrap();
        }
        timings.push(t.elapsed());
    }
    let mut r = BenchResult {
        op: "FSYNC",
        size: "10MB",
        size_bytes: 10 * 1024 * 1024,
        iters,
        timings,
    };
    print_one(&mut r);

    // raw read 1KB
    let iters = 200;
    let mut timings = Vec::with_capacity(iters);
    for i in 0..iters {
        let path = base.join(format!("w1k_{}", i));
        let t = Instant::now();
        let _ = tokio::fs::read(&path).await.unwrap();
        timings.push(t.elapsed());
    }
    let mut r = BenchResult {
        op: "READ",
        size: "1KB",
        size_bytes: 1024,
        iters,
        timings,
    };
    print_one(&mut r);

    // raw read 1MB
    let iters = 50;
    let mut timings = Vec::with_capacity(iters);
    for i in 0..iters {
        let path = base.join(format!("w1m_{}", i));
        let t = Instant::now();
        let _ = tokio::fs::read(&path).await.unwrap();
        timings.push(t.elapsed());
    }
    let mut r = BenchResult {
        op: "READ",
        size: "1MB",
        size_bytes: 1024 * 1024,
        iters,
        timings,
    };
    print_one(&mut r);

    // raw read 10MB
    let iters = 10;
    let mut timings = Vec::with_capacity(iters);
    for i in 0..iters {
        let path = base.join(format!("w10m_{}", i));
        let t = Instant::now();
        let _ = tokio::fs::read(&path).await.unwrap();
        timings.push(t.elapsed());
    }
    let mut r = BenchResult {
        op: "READ",
        size: "10MB",
        size_bytes: 10 * 1024 * 1024,
        iters,
        timings,
    };
    print_one(&mut r);
}

fn print_one(r: &mut BenchResult) {
    let total: Duration = r.timings.iter().sum();
    let a = avg(&r.timings);
    let p50 = percentile(&mut r.timings, 50.0);
    let p99 = percentile(&mut r.timings, 99.0);
    let tp = format_throughput(r.size_bytes, r.iters, total);
    eprintln!(
        "{:<8} {:<6} {:>6} ops  {:>10}  {:>10}  {:>10}  {:>12}",
        r.op,
        r.size,
        r.iters,
        format_duration(a),
        format_duration(p50),
        format_duration(p99),
        tp,
    );
}

#[tokio::test(flavor = "multi_thread")]
#[ignore]
async fn bench_0_raw_disk() {
    eprintln!("\nabixio benchmark\n");
    run_raw_disk().await;
}

#[tokio::test(flavor = "multi_thread")]
#[ignore]
async fn bench_1_disk() {
    run_bench(1).await;
}

#[tokio::test(flavor = "multi_thread")]
#[ignore]
async fn bench_2_disks() {
    run_bench(2).await;
}

#[tokio::test(flavor = "multi_thread")]
#[ignore]
async fn bench_3_disks() {
    run_bench(3).await;
}

#[tokio::test(flavor = "multi_thread")]
#[ignore]
async fn bench_4_disks() {
    run_bench(4).await;
}

// ============================================================================
// Competitive benchmark: AbixIO vs RustFS vs MinIO (4KB + 10MB + 1GB)
// ============================================================================

use std::path::{Path, PathBuf};
use std::process::{Child, Command, Stdio};

struct ExternalServer {
    child: Child,
    port: u16,
    _temp: tempfile::TempDir,
    ca_cert_pem: Vec<u8>,
}

impl ExternalServer {
    fn start_rustfs_tls(bin: &str, port: u16, tls: &TlsMaterial) -> Option<Self> {
        if !std::path::Path::new(bin).exists() {
            return None;
        }
        let tmp = tempfile::TempDir::new().ok()?;
        let console_port = port + 1;
        let child = Command::new(bin)
            .args([
                "server",
                tmp.path().to_str().unwrap(),
                "--address",
                &format!(":{}", port),
                "--console-address",
                &format!(":{}", console_port),
                "--tls-path",
                tls.rustfs_tls_dir.to_str().unwrap(),
            ])
            .env("RUSTFS_ROOT_USER", "benchuser")
            .env("RUSTFS_ROOT_PASSWORD", "benchpass")
            .stdout(Stdio::null())
            .stderr(Stdio::null())
            .spawn()
            .ok()?;
        let mut server = Self {
            child,
            port,
            _temp: tmp,
            ca_cert_pem: tls.ca_cert_pem.clone(),
        };
        server.wait_for_ready();
        Some(server)
    }

    fn start_minio_tls(bin: &str, port: u16, tls: &TlsMaterial) -> Option<Self> {
        if !std::path::Path::new(bin).exists() {
            return None;
        }
        let tmp = tempfile::TempDir::new().ok()?;
        let console_port = port + 1;
        let child = Command::new(bin)
            .args([
                "server",
                tmp.path().to_str().unwrap(),
                "--address",
                &format!(":{}", port),
                "--console-address",
                &format!(":{}", console_port),
                "--certs-dir",
                tls.minio_certs_dir.to_str().unwrap(),
            ])
            .env("MINIO_ROOT_USER", "benchuser")
            .env("MINIO_ROOT_PASSWORD", "benchpass")
            .stdout(Stdio::null())
            .stderr(Stdio::null())
            .spawn()
            .ok()?;
        let mut server = Self {
            child,
            port,
            _temp: tmp,
            ca_cert_pem: tls.ca_cert_pem.clone(),
        };
        server.wait_for_ready();
        Some(server)
    }

    fn endpoint(&self) -> String {
        format!("https://127.0.0.1:{}", self.port)
    }

    fn s3_client(&self, creds: (&str, &str)) -> Arc<abixio_ui::s3::client::S3Client> {
        Arc::new(
            abixio_ui::s3::client::S3Client::new_with_ca_pem(
                &self.endpoint(),
                Some(creds),
                "us-east-1",
                Some(&self.ca_cert_pem),
            )
            .expect("create S3 client"),
        )
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
                panic!(
                    "external server on port {} exited early: {}",
                    self.port, status
                );
            }

            // see note in support::server::wait_for_ready — run block_on from
            // an OS thread so this works inside `#[tokio::test]` workers too.
            let client_ref = &client;
            let url_ref = &url;
            let ready = std::thread::scope(|s| {
                s.spawn(move || {
                    support::RUNTIME.block_on(async move {
                        client_ref.get(url_ref).send().await.is_ok()
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
            "external server on port {} did not become ready over TLS",
            self.port
        );
    }
}

impl Drop for ExternalServer {
    fn drop(&mut self) {
        let _ = self.child.kill();
        let _ = self.child.wait();
    }
}

async fn run_competitive_4kb(name: &str, client: &abixio_ui::s3::client::S3Client) {
    let payload = vec![0x42u8; 4096];

    if let Err(e) = client.create_bucket("bench4k").await {
        eprintln!("  {} create_bucket: {}", name, e);
        return;
    }

    // warmup
    for i in 0..20 {
        let _ = client
            .put_object(
                "bench4k",
                &format!("w{}", i),
                payload.clone(),
                "application/octet-stream",
            )
            .await;
    }

    // PUT 4KB
    let iters = 500;
    let mut timings = Vec::with_capacity(iters);
    for i in 0..iters {
        let t = Instant::now();
        let _ = client
            .put_object_unsigned(
                "bench4k",
                &format!("p{}", i),
                payload.clone(),
                "application/octet-stream",
            )
            .await;
        timings.push(t.elapsed());
    }
    let total: Duration = timings.iter().sum();
    let put_ops = iters as f64 / total.as_secs_f64();
    let put_avg = total / iters as u32;

    // GET 4KB
    let mut timings = Vec::with_capacity(iters);
    for i in 0..iters {
        let t = Instant::now();
        let _ = client.get_object("bench4k", &format!("p{}", i)).await;
        timings.push(t.elapsed());
    }
    let total: Duration = timings.iter().sum();
    let get_ops = iters as f64 / total.as_secs_f64();
    let get_avg = total / iters as u32;

    // PUT 10MB
    let payload_10m = vec![0x42u8; 10 * 1024 * 1024];
    let iters_10m = 5;
    let mut timings = Vec::with_capacity(iters_10m);
    for i in 0..iters_10m {
        let t = Instant::now();
        let _ = client
            .put_object_unsigned(
                "bench4k",
                &format!("big{}", i),
                payload_10m.clone(),
                "application/octet-stream",
            )
            .await;
        timings.push(t.elapsed());
    }
    let total: Duration = timings.iter().sum();
    let put10_mbps = (10.0 * iters_10m as f64) / total.as_secs_f64();

    // GET 10MB
    let mut timings = Vec::with_capacity(iters_10m);
    for i in 0..iters_10m {
        let t = Instant::now();
        let _ = client.get_object("bench4k", &format!("big{}", i)).await;
        timings.push(t.elapsed());
    }
    let total: Duration = timings.iter().sum();
    let get10_mbps = (10.0 * iters_10m as f64) / total.as_secs_f64();

    eprintln!(
        "| {:<12} | {:>6.0} obj/s {:>6.0}us | {:>6.0} obj/s {:>6.0}us | {:>6.1} MB/s | {:>6.1} MB/s |",
        name,
        put_ops,
        put_avg.as_micros(),
        get_ops,
        get_avg.as_micros(),
        put10_mbps,
        get10_mbps,
    );
}

#[tokio::test(flavor = "multi_thread")]
#[ignore]
async fn bench_competitive() {
    eprintln!();
    eprintln!("=== competitive benchmark (aws-sdk-s3, release, keep-alive) ===");
    eprintln!(
        "| {:12} | {:>22} | {:>22} | {:>10} | {:>10} |",
        "Server", "4KB PUT", "4KB GET", "10MB PUT", "10MB GET"
    );
    eprintln!(
        "|{:-<14}|{:-<24}|{:-<24}|{:-<12}|{:-<12}|",
        "", "", "", "", ""
    );

    // AbixIO
    let tls = TlsMaterial::generate();
    let abixio = AbixioServer::builder()
        .volume_count(1)
        .no_auth(false)
        .tls(&tls)
        .start();
    run_competitive_4kb("AbixIO", &abixio.s3_client()).await;

    // RustFS
    if let Some(rustfs) = ExternalServer::start_rustfs_tls(
        &expect_binary("RUSTFS_BIN", r"C:\tools\rustfs.exe", "RustFS"),
        11501,
        &tls,
    ) {
        let client = rustfs.s3_client(("benchuser", "benchpass"));
        run_competitive_4kb("RustFS", &client).await;
    } else {
        eprintln!(
            "| RustFS       | (binary not found)                                                           |"
        );
    }

    // MinIO
    if let Some(minio) = ExternalServer::start_minio_tls(
        &expect_binary("MINIO_BIN", r"C:\tools\minio.exe", "MinIO"),
        11503,
        &tls,
    ) {
        let client = minio.s3_client(("benchuser", "benchpass"));
        run_competitive_4kb("MinIO", &client).await;
    } else {
        eprintln!(
            "| MinIO        | (binary not found)                                                           |"
        );
    }

    eprintln!();
}

// ============================================================================
// Client comparison: same server, different S3 clients
// ============================================================================

fn measure_cli_overhead(bin: &str, args: &[&str], n: usize) -> Duration {
    for _ in 0..3 {
        let _ = Command::new(bin)
            .args(args)
            .stdout(Stdio::null())
            .stderr(Stdio::null())
            .status();
    }
    let start = Instant::now();
    for _ in 0..n {
        let _ = Command::new(bin)
            .args(args)
            .stdout(Stdio::null())
            .stderr(Stdio::null())
            .status();
    }
    start.elapsed() / n as u32
}

fn find_binary(env_var: &str, default: &str) -> Option<String> {
    if let Ok(p) = std::env::var(env_var) {
        if std::path::Path::new(&p).exists() {
            return Some(p);
        }
    }
    if std::path::Path::new(default).exists() {
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

fn expect_binary(env_var: &str, default: &str, display: &str) -> String {
    find_binary(env_var, default).unwrap_or_else(|| {
        panic!(
            "{} binary not found. Set {} or install it in PATH.",
            display, env_var
        )
    })
}

fn run_status(mut cmd: Command, purpose: &str) {
    let status = cmd
        .status()
        .unwrap_or_else(|e| panic!("failed to run {}: {}", purpose, e));
    assert!(
        status.success(),
        "{} failed with status {}",
        purpose,
        status
    );
}

struct AwsCliHarness {
    aws: String,
    _temp: tempfile::TempDir,
    config_path: PathBuf,
    credentials_path: PathBuf,
    ca_bundle_path: PathBuf,
}

impl AwsCliHarness {
    fn new(aws: String, ca_bundle_path: &Path, access_key: &str, secret_key: &str) -> Self {
        let temp = tempfile::TempDir::new().expect("create aws cli tempdir");
        let config_path = temp.path().join("config");
        let credentials_path = temp.path().join("credentials");
        let ca_bundle_copy = temp.path().join("ca.pem");
        std::fs::copy(ca_bundle_path, &ca_bundle_copy).expect("copy CA bundle");

        let config = format!(
            "[profile bench]\nregion = us-east-1\nca_bundle = {}\ns3 =\n    addressing_style = path\n    payload_signing_enabled = false\n",
            ca_bundle_copy.display()
        );
        let credentials = format!(
            "[bench]\naws_access_key_id = {}\naws_secret_access_key = {}\n",
            access_key, secret_key
        );
        std::fs::write(&config_path, config).expect("write aws config");
        std::fs::write(&credentials_path, credentials).expect("write aws credentials");

        Self {
            aws,
            _temp: temp,
            config_path,
            credentials_path,
            ca_bundle_path: ca_bundle_copy,
        }
    }

    fn command(&self) -> Command {
        let mut cmd = Command::new(&self.aws);
        cmd.env("AWS_CONFIG_FILE", &self.config_path)
            .env("AWS_SHARED_CREDENTIALS_FILE", &self.credentials_path)
            .env("AWS_PROFILE", "bench")
            .env("AWS_EC2_METADATA_DISABLED", "true")
            .arg("--no-cli-pager");
        cmd
    }

    fn create_bucket(&self, endpoint: &str, bucket: &str) {
        let mut cmd = self.command();
        cmd.args(["--endpoint-url", endpoint, "--ca-bundle"])
            .arg(&self.ca_bundle_path)
            .args(["s3api", "create-bucket", "--bucket", bucket]);
        run_status(cmd, "aws create-bucket");
    }

    fn measure_overhead(&self, endpoint: &str, n: usize) -> Duration {
        measure_cli_overhead(
            &self.aws,
            &[
                "--no-cli-pager",
                "--endpoint-url",
                endpoint,
                "--ca-bundle",
                self.ca_bundle_path.to_str().expect("ca path utf8"),
                "s3api",
                "list-buckets",
            ],
            n,
        )
    }

    fn put_object(&self, endpoint: &str, bucket: &str, key: &str, body_path: &Path) {
        let mut cmd = self.command();
        cmd.args(["--endpoint-url", endpoint, "--ca-bundle"])
            .arg(&self.ca_bundle_path)
            .args([
                "s3api",
                "put-object",
                "--bucket",
                bucket,
                "--key",
                key,
                "--body",
            ])
            .arg(body_path);
        run_status(cmd, "aws put-object");
    }

    fn get_object(&self, endpoint: &str, bucket: &str, key: &str, out_path: &Path) {
        let mut cmd = self.command();
        cmd.args(["--endpoint-url", endpoint, "--ca-bundle"])
            .arg(&self.ca_bundle_path)
            .args(["s3api", "get-object", "--bucket", bucket, "--key", key])
            .arg(out_path);
        run_status(cmd, "aws get-object");
    }
}

fn rclone_args(
    endpoint: &str,
    ca_bundle_path: &Path,
    access_key: &str,
    secret_key: &str,
) -> Vec<String> {
    vec![
        "--s3-provider".to_string(),
        "Other".to_string(),
        "--s3-endpoint".to_string(),
        endpoint.to_string(),
        "--s3-access-key-id".to_string(),
        access_key.to_string(),
        "--s3-secret-access-key".to_string(),
        secret_key.to_string(),
        "--s3-force-path-style".to_string(),
        "--s3-use-unsigned-payload".to_string(),
        "true".to_string(),
        "--ca-cert".to_string(),
        ca_bundle_path.to_string_lossy().to_string(),
    ]
}

fn rclone_mkdir(
    rclone: &str,
    endpoint: &str,
    ca_bundle_path: &Path,
    access_key: &str,
    secret_key: &str,
    bucket: &str,
) {
    let mut cmd = Command::new(rclone);
    cmd.arg("mkdir")
        .arg(format!(":s3:{}", bucket))
        .args(rclone_args(
            endpoint,
            ca_bundle_path,
            access_key,
            secret_key,
        ))
        .stdout(Stdio::null())
        .stderr(Stdio::null());
    run_status(cmd, "rclone mkdir");
}

#[tokio::test(flavor = "multi_thread")]
#[ignore]
async fn bench_clients() {
    eprintln!();
    eprintln!("=== client comparison (4KB, HTTPS + SigV4 + UNSIGNED-PAYLOAD) ===");
    eprintln!(
        "| {:18} | {:>28} | {:>28} |",
        "Client", "4KB PUT", "4KB GET"
    );
    eprintln!("|{:-<20}|{:-<30}|{:-<30}|", "", "", "");

    let tls = TlsMaterial::generate();
    let server = AbixioServer::builder()
        .volume_count(1)
        .no_auth(false)
        .tls(&tls)
        .start();
    let endpoint = server.endpoint();
    let aws = AwsCliHarness::new(
        expect_binary(
            "AWS",
            r"C:\Program Files\Amazon\AWSCLIV2\aws.exe",
            "AWS CLI",
        ),
        &tls.ca_cert_path,
        "test",
        "testsecret",
    );
    let rclone = expect_binary("RCLONE", r"C:\tools\rclone.exe", "rclone");

    // 1. aws-sdk-s3 (Rust, keep-alive, unsigned payload)
    {
        let client = server.s3_client();
        client.create_bucket("clientbench").await.unwrap();
        let tmpdir = tempfile::TempDir::new().unwrap();
        let srcpath = tmpdir.path().join("payload.dat");
        let sinkpath = tmpdir.path().join("sink.dat");
        std::fs::write(&srcpath, vec![0x42u8; 4096]).unwrap();

        // warmup
        for i in 0..20 {
            let data = tokio::fs::read(&srcpath).await.unwrap();
            let _ = client
                .put_object_unsigned(
                    "clientbench",
                    &format!("w{}", i),
                    data,
                    "application/octet-stream",
                )
                .await;
        }

        let iters = 200;
        let start = Instant::now();
        for i in 0..iters {
            let data = tokio::fs::read(&srcpath).await.unwrap();
            let _ = client
                .put_object_unsigned(
                    "clientbench",
                    &format!("sdk{}", i),
                    data,
                    "application/octet-stream",
                )
                .await;
        }
        let put_elapsed = start.elapsed();

        let start = Instant::now();
        for i in 0..iters {
            let _ = client
                .download_object_to_file("clientbench", &format!("sdk{}", i), &sinkpath)
                .await;
        }
        let get_elapsed = start.elapsed();

        eprintln!(
            "| {:<18} | {:>6.0} obj/s {:>8.0}us | {:>6.0} obj/s {:>8.0}us |",
            "aws-sdk-s3 (Rust)",
            iters as f64 / put_elapsed.as_secs_f64(),
            put_elapsed.as_micros() as f64 / iters as f64,
            iters as f64 / get_elapsed.as_secs_f64(),
            get_elapsed.as_micros() as f64 / iters as f64,
        );
    }

    // 2. AWS CLI
    {
        aws.create_bucket(&endpoint, "clientbench-aws");
        let tmpdir = tempfile::TempDir::new().unwrap();
        let srcpath = tmpdir.path().join("payload.dat");
        let sinkdir = tempfile::TempDir::new().unwrap();
        std::fs::write(&srcpath, vec![0x42u8; 4096]).unwrap();

        let iters = 50;
        let start = Instant::now();
        for i in 0..iters {
            aws.put_object(&endpoint, "clientbench-aws", &format!("aws{}", i), &srcpath);
        }
        let put_elapsed = start.elapsed();

        let start = Instant::now();
        for i in 0..iters {
            let sinkpath = sinkdir.path().join(format!("aws{}.dat", i));
            aws.get_object(
                &endpoint,
                "clientbench-aws",
                &format!("aws{}", i),
                &sinkpath,
            );
        }
        let get_elapsed = start.elapsed();

        eprintln!(
            "| {:<18} | {:>6.0} obj/s {:>8.0}us | {:>6.0} obj/s {:>8.0}us |",
            "aws cli",
            iters as f64 / put_elapsed.as_secs_f64(),
            put_elapsed.as_micros() as f64 / iters as f64,
            iters as f64 / get_elapsed.as_secs_f64(),
            get_elapsed.as_micros() as f64 / iters as f64,
        );
    }

    // 3. rclone
    {
        rclone_mkdir(
            &rclone,
            &endpoint,
            &tls.ca_cert_path,
            "test",
            "testsecret",
            "rclonebench",
        );
        let iters = 50;
        let tmpdir = tempfile::TempDir::new().unwrap();
        let srcpath = tmpdir.path().join("payload.dat");
        let sinkdir = tempfile::TempDir::new().unwrap();
        std::fs::write(&srcpath, vec![0x42u8; 4096]).unwrap();

        let start = Instant::now();
        for i in 0..iters {
            let mut cmd = Command::new(&rclone);
            cmd.arg("copyto")
                .arg(&srcpath)
                .arg(format!(":s3:rclonebench/rc{}", i))
                .args(rclone_args(
                    &endpoint,
                    &tls.ca_cert_path,
                    "test",
                    "testsecret",
                ))
                .stdout(Stdio::null())
                .stderr(Stdio::null());
            run_status(cmd, "rclone put");
        }
        let put_elapsed = start.elapsed();

        let start = Instant::now();
        for i in 0..iters {
            let sinkpath = sinkdir.path().join(format!("rc{}.dat", i));
            let mut cmd = Command::new(&rclone);
            cmd.arg("copyto")
                .arg(format!(":s3:rclonebench/rc{}", i))
                .arg(&sinkpath)
                .args(rclone_args(
                    &endpoint,
                    &tls.ca_cert_path,
                    "test",
                    "testsecret",
                ))
                .stdout(Stdio::null())
                .stderr(Stdio::null());
            run_status(cmd, "rclone get");
        }
        let get_elapsed = start.elapsed();

        eprintln!(
            "| {:<18} | {:>6.0} obj/s {:>8.0}us | {:>6.0} obj/s {:>8.0}us |",
            "rclone",
            iters as f64 / put_elapsed.as_secs_f64(),
            put_elapsed.as_micros() as f64 / iters as f64,
            iters as f64 / get_elapsed.as_secs_f64(),
            get_elapsed.as_micros() as f64 / iters as f64,
        );
    }

    eprintln!();
}

// ============================================================================
// Comprehensive matrix: 3 servers x 3 clients x 3 sizes x 2 ops
// ============================================================================

struct MatrixResult {
    server: String,
    client: String,
    size: String,
    size_bytes: usize,
    put_ops: f64,
    put_avg_us: f64,
    get_ops: f64,
    get_avg_us: f64,
}

struct MetaResult {
    server: String,
    op: &'static str,
    iters: usize,
    avg_us: f64,
    p50_us: f64,
    p99_us: f64,
    ops_per_sec: f64,
}

async fn matrix_sdk_meta(
    name: &str,
    client: &abixio_ui::s3::client::S3Client,
    bucket: &str,
) -> Vec<MetaResult> {
    let _ = client.create_bucket(bucket).await;
    let mut results = Vec::new();

    // seed 100 objects for HEAD/LIST/DELETE
    let payload = vec![0x42u8; 4096];
    for i in 0..100 {
        let _ = client
            .put_object_unsigned(
                bucket,
                &format!("meta/{}", i),
                payload.clone(),
                "application/octet-stream",
            )
            .await;
    }

    // warmup
    for i in 0..3 {
        let _ = client.head_object(bucket, &format!("meta/{}", i)).await;
        let _ = client.list_objects(bucket, "meta/", "").await;
    }

    // HEAD (100 iters)
    let iters = 100;
    let mut timings = Vec::with_capacity(iters);
    for i in 0..iters {
        let t = Instant::now();
        let _ = client
            .head_object(bucket, &format!("meta/{}", i))
            .await;
        timings.push(t.elapsed());
    }
    timings.sort();
    let total: Duration = timings.iter().sum();
    results.push(MetaResult {
        server: name.to_string(),
        op: "HEAD",
        iters,
        avg_us: total.as_micros() as f64 / iters as f64,
        p50_us: timings[iters / 2].as_micros() as f64,
        p99_us: timings[(iters * 99) / 100].as_micros() as f64,
        ops_per_sec: iters as f64 / total.as_secs_f64(),
    });

    // LIST 100 objects (50 iters)
    let iters = 50;
    let mut timings = Vec::with_capacity(iters);
    for _ in 0..iters {
        let t = Instant::now();
        let _ = client.list_objects(bucket, "meta/", "").await;
        timings.push(t.elapsed());
    }
    timings.sort();
    let total: Duration = timings.iter().sum();
    results.push(MetaResult {
        server: name.to_string(),
        op: "LIST",
        iters,
        avg_us: total.as_micros() as f64 / iters as f64,
        p50_us: timings[iters / 2].as_micros() as f64,
        p99_us: timings[(iters * 99) / 100].as_micros() as f64,
        ops_per_sec: iters as f64 / total.as_secs_f64(),
    });

    // DELETE (100 iters)
    let iters = 100;
    let mut timings = Vec::with_capacity(iters);
    for i in 0..iters {
        let t = Instant::now();
        let _ = client
            .delete_object(bucket, &format!("meta/{}", i))
            .await;
        timings.push(t.elapsed());
    }
    timings.sort();
    let total: Duration = timings.iter().sum();
    results.push(MetaResult {
        server: name.to_string(),
        op: "DELETE",
        iters,
        avg_us: total.as_micros() as f64 / iters as f64,
        p50_us: timings[iters / 2].as_micros() as f64,
        p99_us: timings[(iters * 99) / 100].as_micros() as f64,
        ops_per_sec: iters as f64 / total.as_secs_f64(),
    });

    results
}

fn print_meta_matrix(results: &[MetaResult]) {
    eprintln!();
    eprintln!("--- Metadata ops (aws-sdk-s3, 4KB objects) ---");
    eprintln!(
        "| {:12} | {:6} | {:>6} ops | {:>10} | {:>10} | {:>10} | {:>10} |",
        "Server", "OP", "", "avg", "p50", "p99", "obj/sec"
    );
    eprintln!(
        "|{:-<14}|{:-<8}|{:-<10}|{:-<12}|{:-<12}|{:-<12}|{:-<12}|",
        "", "", "", "", "", "", ""
    );

    for r in results {
        eprintln!(
            "| {:<12} | {:<6} | {:>6} ops | {:>8.0}us | {:>8.0}us | {:>8.0}us | {:>8.0}    |",
            r.server, r.op, r.iters, r.avg_us, r.p50_us, r.p99_us, r.ops_per_sec,
        );
    }
}

async fn matrix_sdk(
    name: &str,
    client: &abixio_ui::s3::client::S3Client,
    bucket: &str,
    sizes: &[(&str, usize, usize)],
) -> Vec<MatrixResult> {
    let _ = client.create_bucket(bucket).await;
    let mut results = Vec::new();

    for &(label, size_bytes, iters) in sizes {
        // write payload to disk (same as mc/rclone -- fairness: all clients read from disk)
        let tmpdir = tempfile::TempDir::new().unwrap();
        let srcpath = tmpdir.path().join("payload.dat");
        std::fs::write(&srcpath, &vec![0x42u8; size_bytes]).unwrap();
        let sinkpath = tmpdir.path().join("out.dat");

        // warmup: 3 PUT + 3 GET (fairness: same warmup for all clients)
        for i in 0..3 {
            let data = tokio::fs::read(&srcpath).await.unwrap();
            let _ = client
                .put_object_unsigned(
                    bucket,
                    &format!("w_{}_{}", label, i),
                    data,
                    "application/octet-stream",
                )
                .await;
        }
        for i in 0..3 {
            let _ = client
                .download_object_to_file(bucket, &format!("w_{}_{}", label, i), &sinkpath)
                .await;
        }

        // PUT: read from disk each iteration (fairness: same as mc/rclone reading temp file)
        let start = Instant::now();
        for i in 0..iters {
            let data = tokio::fs::read(&srcpath).await.unwrap();
            let _ = client
                .put_object_unsigned(
                    bucket,
                    &format!("{}/{}", label, i),
                    data,
                    "application/octet-stream",
                )
                .await;
        }
        let put_elapsed = start.elapsed();

        // GET: write to disk each iteration (fairness: same as mc/rclone writing to sink file)
        let start = Instant::now();
        for i in 0..iters {
            let _ = client
                .download_object_to_file(bucket, &format!("{}/{}", label, i), &sinkpath)
                .await;
        }
        let get_elapsed = start.elapsed();

        results.push(MatrixResult {
            server: name.to_string(),
            client: "aws-sdk-s3".to_string(),
            size: label.to_string(),
            size_bytes,
            put_ops: iters as f64 / put_elapsed.as_secs_f64(),
            put_avg_us: put_elapsed.as_micros() as f64 / iters as f64,
            get_ops: iters as f64 / get_elapsed.as_secs_f64(),
            get_avg_us: get_elapsed.as_micros() as f64 / iters as f64,
        });
    }
    results
}

fn matrix_aws_cli(
    server_name: &str,
    aws: &AwsCliHarness,
    endpoint: &str,
    bucket: &str,
    sizes: &[(&str, usize, usize)],
) -> Vec<MatrixResult> {
    let mut results = Vec::new();
    aws.create_bucket(endpoint, bucket);

    for &(label, size_bytes, iters) in sizes {
        let tmpdir = tempfile::TempDir::new().unwrap();
        let srcpath = tmpdir.path().join("payload.dat");
        std::fs::write(&srcpath, vec![0x42u8; size_bytes]).unwrap();
        let sinkdir = tempfile::TempDir::new().unwrap();

        let iters = if size_bytes >= 1024 * 1024 * 1024 {
            iters.min(3)
        } else if size_bytes >= 10 * 1024 * 1024 {
            iters.min(5)
        } else {
            iters.min(30)
        };

        for i in 0..3 {
            aws.put_object(endpoint, bucket, &format!("{}/w{}", label, i), &srcpath);
        }
        for i in 0..3 {
            let sinkpath = sinkdir.path().join(format!("warmup-{}.dat", i));
            aws.get_object(endpoint, bucket, &format!("{}/w{}", label, i), &sinkpath);
        }

        let start = Instant::now();
        for i in 0..iters {
            aws.put_object(endpoint, bucket, &format!("{}/{}", label, i), &srcpath);
        }
        let put_elapsed = start.elapsed();

        let start = Instant::now();
        for i in 0..iters {
            let sinkpath = sinkdir.path().join(format!("{}.dat", i));
            aws.get_object(endpoint, bucket, &format!("{}/{}", label, i), &sinkpath);
        }
        let get_elapsed = start.elapsed();

        results.push(MatrixResult {
            server: server_name.to_string(),
            client: "aws-cli".to_string(),
            size: label.to_string(),
            size_bytes,
            put_ops: iters as f64 / put_elapsed.as_secs_f64(),
            put_avg_us: put_elapsed.as_micros() as f64 / iters as f64,
            get_ops: iters as f64 / get_elapsed.as_secs_f64(),
            get_avg_us: get_elapsed.as_micros() as f64 / iters as f64,
        });
    }
    results
}

fn matrix_rclone(
    server_name: &str,
    rclone: &str,
    endpoint: &str,
    ca_bundle_path: &Path,
    access_key: &str,
    secret_key: &str,
    bucket: &str,
    sizes: &[(&str, usize, usize)],
) -> Vec<MatrixResult> {
    let mut results = Vec::new();
    rclone_mkdir(
        rclone,
        endpoint,
        ca_bundle_path,
        access_key,
        secret_key,
        bucket,
    );

    for &(label, size_bytes, iters) in sizes {
        let tmpdir = tempfile::TempDir::new().unwrap();
        let srcpath = tmpdir.path().join("payload.dat");
        std::fs::write(&srcpath, vec![0x42u8; size_bytes]).unwrap();

        let iters = if size_bytes >= 1024 * 1024 * 1024 {
            iters.min(3)
        } else if size_bytes >= 10 * 1024 * 1024 {
            iters.min(5)
        } else {
            iters.min(30)
        };

        let sinkdir = tempfile::TempDir::new().unwrap();

        for i in 0..3 {
            let mut cmd = Command::new(rclone);
            cmd.arg("copyto")
                .arg(&srcpath)
                .arg(format!(":s3:{}/{}/w{}", bucket, label, i))
                .args(rclone_args(
                    endpoint,
                    ca_bundle_path,
                    access_key,
                    secret_key,
                ))
                .stdout(Stdio::null())
                .stderr(Stdio::null());
            run_status(cmd, "rclone warmup put");
        }
        for i in 0..3 {
            let sinkpath = sinkdir.path().join(format!("warmup-{}.dat", i));
            let mut cmd = Command::new(rclone);
            cmd.arg("copyto")
                .arg(format!(":s3:{}/{}/w{}", bucket, label, i))
                .arg(&sinkpath)
                .args(rclone_args(
                    endpoint,
                    ca_bundle_path,
                    access_key,
                    secret_key,
                ))
                .stdout(Stdio::null())
                .stderr(Stdio::null());
            run_status(cmd, "rclone warmup get");
        }

        let start = Instant::now();
        for i in 0..iters {
            let mut cmd = Command::new(rclone);
            cmd.arg("copyto")
                .arg(&srcpath)
                .arg(format!(":s3:{}/{}/{}", bucket, label, i))
                .args(rclone_args(
                    endpoint,
                    ca_bundle_path,
                    access_key,
                    secret_key,
                ))
                .stdout(Stdio::null())
                .stderr(Stdio::null());
            run_status(cmd, "rclone put");
        }
        let put_elapsed = start.elapsed();

        let start = Instant::now();
        for i in 0..iters {
            let sinkpath = sinkdir.path().join(format!("{}.dat", i));
            let mut cmd = Command::new(rclone);
            cmd.arg("copyto")
                .arg(format!(":s3:{}/{}/{}", bucket, label, i))
                .arg(&sinkpath)
                .args(rclone_args(
                    endpoint,
                    ca_bundle_path,
                    access_key,
                    secret_key,
                ))
                .stdout(Stdio::null())
                .stderr(Stdio::null());
            run_status(cmd, "rclone get");
        }
        let get_elapsed = start.elapsed();

        results.push(MatrixResult {
            server: server_name.to_string(),
            client: "rclone".to_string(),
            size: label.to_string(),
            size_bytes,
            put_ops: iters as f64 / put_elapsed.as_secs_f64(),
            put_avg_us: put_elapsed.as_micros() as f64 / iters as f64,
            get_ops: iters as f64 / get_elapsed.as_secs_f64(),
            get_avg_us: get_elapsed.as_micros() as f64 / iters as f64,
        });
    }
    results
}

fn format_cell(ops: f64, avg_us: f64, size_bytes: usize) -> String {
    let mbps = ops * size_bytes as f64 / 1024.0 / 1024.0;
    if size_bytes <= 65536 {
        format!("{:>5.0} obj/s {:>7.0}us", ops, avg_us)
    } else {
        format!("{:>6.1} MB/s {:>8.0}ms", mbps, avg_us / 1000.0)
    }
}

fn print_matrix(results: &[MatrixResult]) {
    for size_label in &["4KB", "10MB", "1GB"] {
        eprintln!();
        eprintln!("--- {} ---", size_label);
        eprintln!(
            "| {:12} | {:10} | {:>24} | {:>24} |",
            "Server", "Client", "PUT", "GET"
        );
        eprintln!("|{:-<14}|{:-<12}|{:-<26}|{:-<26}|", "", "", "", "");

        for r in results.iter().filter(|r| r.size == *size_label) {
            eprintln!(
                "| {:<12} | {:<10} | {} | {} |",
                r.server,
                r.client,
                format_cell(r.put_ops, r.put_avg_us, r.size_bytes),
                format_cell(r.get_ops, r.get_avg_us, r.size_bytes),
            );
        }
    }
}

#[tokio::test(flavor = "multi_thread")]
#[ignore]
async fn bench_matrix() {
    eprintln!();
    eprintln!("=============================================================");
    eprintln!("  COMPREHENSIVE BENCHMARK MATRIX");
    eprintln!("  3 servers x 3 clients x 3 sizes x 2 ops");
    eprintln!("  Canonical mode: HTTPS + SigV4 + UNSIGNED-PAYLOAD");
    eprintln!("=============================================================");

    let sizes: Vec<(&str, usize, usize)> = vec![
        ("4KB", 4096, 200),
        ("10MB", 10 * 1024 * 1024, 5),
        ("1GB", 1024 * 1024 * 1024, 3),
    ];

    let tls = TlsMaterial::generate();
    let aws_path = expect_binary(
        "AWS",
        r"C:\Program Files\Amazon\AWSCLIV2\aws.exe",
        "AWS CLI",
    );
    let rclone_bin = expect_binary("RCLONE", r"C:\tools\rclone.exe", "rclone");
    let rustfs_bin = expect_binary("RUSTFS_BIN", r"C:\tools\rustfs.exe", "RustFS");
    let minio_bin = expect_binary("MINIO_BIN", r"C:\tools\minio.exe", "MinIO");
    let aws = AwsCliHarness::new(aws_path.clone(), &tls.ca_cert_path, "test", "testsecret");
    let mut all_results: Vec<MatrixResult> = Vec::new();
    let mut all_meta: Vec<MetaResult> = Vec::new();

    // --- AbixIO (file, log, pool tiers) ---
    // Each server is wrapped in its own block so the previous one is dropped
    // (child killed, TempDir reclaimed) before the next one starts. Without
    // this, three servers' worth of 1GB PUTs (~55GB) live on disk at once and
    // the run hits ENOSPC during the MinIO 1GB stage. Sequential anyway, so
    // dropping early is purely a disk-budget fix and changes no measurements.
    //
    // AbixIO is benched three times -- once per write tier -- because the
    // matrix should show what each storage path actually delivers end-to-end.
    // RustFS and MinIO are benched once each, after.
    for tier in ["file", "log", "pool"] {
        let label_owned = format!("AbixIO-{}", tier);
        let label: &str = &label_owned;
        eprintln!("\nstarting {} ({})...", label, tier);
        let abixio = AbixioServer::builder()
            .volume_count(1)
            .no_auth(false)
            .tls(&tls)
            .write_tier(tier)
            .start();
        let abixio_endpoint = abixio.endpoint();

        eprintln!("  aws-sdk-s3...");
        all_results.extend(matrix_sdk(label, &abixio.s3_client(), "matrix", &sizes).await);
        eprintln!("  aws-cli...");
        eprintln!(
            "    overhead: {:.1}ms",
            aws.measure_overhead(&abixio_endpoint, 10).as_secs_f64() * 1000.0
        );
        all_results.extend(matrix_aws_cli(
            label,
            &aws,
            &abixio_endpoint,
            "matrix-aws",
            &sizes,
        ));
        eprintln!("  rclone...");
        let rclone_overhead = measure_cli_overhead(
            &rclone_bin,
            &[
                "lsd",
                ":s3:",
                "--s3-provider",
                "Other",
                "--s3-endpoint",
                &abixio_endpoint,
                "--s3-access-key-id",
                "test",
                "--s3-secret-access-key",
                "testsecret",
                "--s3-force-path-style",
                "--s3-use-unsigned-payload",
                "true",
                "--ca-cert",
                tls.ca_cert_path.to_str().unwrap(),
            ],
            10,
        );
        eprintln!(
            "    overhead: {:.1}ms",
            rclone_overhead.as_secs_f64() * 1000.0
        );
        all_results.extend(matrix_rclone(
            label,
            &rclone_bin,
            &abixio_endpoint,
            &tls.ca_cert_path,
            "test",
            "testsecret",
            "matrix-rclone",
            &sizes,
        ));
        eprintln!("  metadata ops...");
        all_meta.extend(matrix_sdk_meta(label, &abixio.s3_client(), "matrix-meta").await);
    } // <-- each abixio dropped here at end of iteration, TempDir removed

    // --- RustFS ---
    {
        eprintln!("\nstarting RustFS...");
        let rustfs = ExternalServer::start_rustfs_tls(&rustfs_bin, 11701, &tls)
            .unwrap_or_else(|| panic!("failed to start RustFS with TLS"));
        let rustfs_endpoint = rustfs.endpoint();
        let rustfs_client = rustfs.s3_client(("benchuser", "benchpass"));
        let rustfs_aws = AwsCliHarness::new(
            aws_path.clone(),
            &tls.ca_cert_path,
            "benchuser",
            "benchpass",
        );

        eprintln!("  aws-sdk-s3...");
        all_results.extend(matrix_sdk("RustFS", &rustfs_client, "matrix", &sizes).await);
        eprintln!("  aws-cli...");
        eprintln!(
            "    overhead: {:.1}ms",
            rustfs_aws
                .measure_overhead(&rustfs_endpoint, 10)
                .as_secs_f64()
                * 1000.0
        );
        all_results.extend(matrix_aws_cli(
            "RustFS",
            &rustfs_aws,
            &rustfs_endpoint,
            "matrix-aws",
            &sizes,
        ));
        eprintln!("  rclone...");
        all_results.extend(matrix_rclone(
            "RustFS",
            &rclone_bin,
            &rustfs_endpoint,
            &tls.ca_cert_path,
            "benchuser",
            "benchpass",
            "matrix-rclone",
            &sizes,
        ));
        eprintln!("  metadata ops...");
        all_meta.extend(matrix_sdk_meta("RustFS", &rustfs_client, "matrix-meta").await);
    } // <-- rustfs dropped here

    // --- MinIO ---
    {
        eprintln!("\nstarting MinIO...");
        let minio = ExternalServer::start_minio_tls(&minio_bin, 11703, &tls)
            .unwrap_or_else(|| panic!("failed to start MinIO with TLS"));
        let minio_endpoint = minio.endpoint();
        let minio_client = minio.s3_client(("benchuser", "benchpass"));
        let minio_aws = AwsCliHarness::new(aws_path, &tls.ca_cert_path, "benchuser", "benchpass");

        eprintln!("  aws-sdk-s3...");
        all_results.extend(matrix_sdk("MinIO", &minio_client, "matrix", &sizes).await);
        eprintln!("  aws-cli...");
        eprintln!(
            "    overhead: {:.1}ms",
            minio_aws
                .measure_overhead(&minio_endpoint, 10)
                .as_secs_f64()
                * 1000.0
        );
        all_results.extend(matrix_aws_cli(
            "MinIO",
            &minio_aws,
            &minio_endpoint,
            "matrix-aws",
            &sizes,
        ));
        eprintln!("  rclone...");
        all_results.extend(matrix_rclone(
            "MinIO",
            &rclone_bin,
            &minio_endpoint,
            &tls.ca_cert_path,
            "benchuser",
            "benchpass",
            "matrix-rclone",
            &sizes,
        ));
        eprintln!("  metadata ops...");
        all_meta.extend(matrix_sdk_meta("MinIO", &minio_client, "matrix-meta").await);
    } // <-- minio dropped here

    eprintln!();
    eprintln!("=============================================================");
    eprintln!("  RESULTS");
    eprintln!("=============================================================");
    print_matrix(&all_results);
    print_meta_matrix(&all_meta);
    eprintln!();
}
