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
    let idx = ((timings.len() as f64 * p / 100.0) - 1.0)
        .max(0.0) as usize;
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

    let server = AbixioServer::builder().volume_count(disks).no_auth(false).start();
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
            .put_object("bench", &format!("warmup/{}", i), payload_1k.clone(), "application/octet-stream")
            .await
            .unwrap();
    }

    // -- PUT 4KB (small object hot path) --
    let iters = 500;
    let mut timings = Vec::with_capacity(iters);
    for i in 0..iters {
        let t = Instant::now();
        client
            .put_object("bench", &format!("tiny/{}", i), payload_4k.clone(), "application/octet-stream")
            .await
            .unwrap();
        timings.push(t.elapsed());
    }
    results.push(BenchResult { op: "PUT", size: "4KB", size_bytes: 4096, iters, timings });

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
    results.push(BenchResult { op: "GET", size: "4KB", size_bytes: 4096, iters, timings });

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
    results.push(BenchResult { op: "HEAD", size: "4KB", size_bytes: 0, iters, timings });

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
    results.push(BenchResult { op: "DELETE", size: "4KB", size_bytes: 0, iters, timings });

    // -- PUT 1KB --
    let iters = 100;
    let mut timings = Vec::with_capacity(iters);
    for i in 0..iters {
        let t = Instant::now();
        client
            .put_object("bench", &format!("small/{}", i), payload_1k.clone(), "application/octet-stream")
            .await
            .unwrap();
        timings.push(t.elapsed());
    }
    results.push(BenchResult { op: "PUT", size: "1KB", size_bytes: 1024, iters, timings });

    // -- PUT 1MB --
    let iters = 20;
    let mut timings = Vec::with_capacity(iters);
    for i in 0..iters {
        let t = Instant::now();
        client
            .put_object("bench", &format!("medium/{}", i), payload_1m.clone(), "application/octet-stream")
            .await
            .unwrap();
        timings.push(t.elapsed());
    }
    results.push(BenchResult { op: "PUT", size: "1MB", size_bytes: 1024 * 1024, iters, timings });

    // -- PUT 10MB --
    let iters = 5;
    let mut timings = Vec::with_capacity(iters);
    for i in 0..iters {
        let t = Instant::now();
        client
            .put_object("bench", &format!("large/{}", i), payload_10m.clone(), "application/octet-stream")
            .await
            .unwrap();
        timings.push(t.elapsed());
    }
    results.push(BenchResult { op: "PUT", size: "10MB", size_bytes: 10 * 1024 * 1024, iters, timings });

    // -- PUT 10MB UNSIGNED (skip client-side SHA256) --
    let iters = 5;
    let mut timings = Vec::with_capacity(iters);
    for i in 0..iters {
        let t = Instant::now();
        client
            .put_object_unsigned("bench", &format!("unsigned/{}", i), payload_10m.clone(), "application/octet-stream")
            .await
            .unwrap();
        timings.push(t.elapsed());
    }
    results.push(BenchResult { op: "PUT*", size: "10MB", size_bytes: 10 * 1024 * 1024, iters, timings });

    // -- GET 1KB --
    let iters = 100;
    let mut timings = Vec::with_capacity(iters);
    for i in 0..iters {
        let t = Instant::now();
        let _ = client.get_object("bench", &format!("small/{}", i)).await.unwrap();
        timings.push(t.elapsed());
    }
    results.push(BenchResult { op: "GET", size: "1KB", size_bytes: 1024, iters, timings });

    // -- GET 1MB --
    let iters = 20;
    let mut timings = Vec::with_capacity(iters);
    for i in 0..iters {
        let t = Instant::now();
        let _ = client.get_object("bench", &format!("medium/{}", i)).await.unwrap();
        timings.push(t.elapsed());
    }
    results.push(BenchResult { op: "GET", size: "1MB", size_bytes: 1024 * 1024, iters, timings });

    // -- GET 10MB --
    let iters = 5;
    let mut timings = Vec::with_capacity(iters);
    for i in 0..iters {
        let t = Instant::now();
        let _ = client.get_object("bench", &format!("large/{}", i)).await.unwrap();
        timings.push(t.elapsed());
    }
    results.push(BenchResult { op: "GET", size: "10MB", size_bytes: 10 * 1024 * 1024, iters, timings });

    // -- HEAD --
    let iters = 100;
    let mut timings = Vec::with_capacity(iters);
    for i in 0..iters {
        let t = Instant::now();
        let _ = client.head_object("bench", &format!("small/{}", i)).await.unwrap();
        timings.push(t.elapsed());
    }
    results.push(BenchResult { op: "HEAD", size: "-", size_bytes: 0, iters, timings });

    // -- LIST (100 objects already in small/) --
    let iters = 50;
    let mut timings = Vec::with_capacity(iters);
    for _ in 0..iters {
        let t = Instant::now();
        let _ = client.list_objects("bench", "small/", "").await.unwrap();
        timings.push(t.elapsed());
    }
    results.push(BenchResult { op: "LIST", size: "100obj", size_bytes: 0, iters, timings });

    // -- DELETE --
    let iters = 100;
    let mut timings = Vec::with_capacity(iters);
    for i in 0..iters {
        let t = Instant::now();
        client.delete_object("bench", &format!("small/{}", i)).await.unwrap();
        timings.push(t.elapsed());
    }
    results.push(BenchResult { op: "DELETE", size: "1KB", size_bytes: 0, iters, timings });

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
    let mut r = BenchResult { op: "WRITE", size: "1KB", size_bytes: 1024, iters, timings };
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
    let mut r = BenchResult { op: "WRITE", size: "1MB", size_bytes: 1024 * 1024, iters, timings };
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
    let mut r = BenchResult { op: "WRITE", size: "10MB", size_bytes: 10 * 1024 * 1024, iters, timings };
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
    let mut r = BenchResult { op: "FSYNC", size: "10MB", size_bytes: 10 * 1024 * 1024, iters, timings };
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
    let mut r = BenchResult { op: "READ", size: "1KB", size_bytes: 1024, iters, timings };
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
    let mut r = BenchResult { op: "READ", size: "1MB", size_bytes: 1024 * 1024, iters, timings };
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
    let mut r = BenchResult { op: "READ", size: "10MB", size_bytes: 10 * 1024 * 1024, iters, timings };
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
        r.op, r.size, r.iters,
        format_duration(a), format_duration(p50), format_duration(p99), tp,
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

use std::process::{Command, Child, Stdio};

struct ExternalServer {
    child: Child,
    port: u16,
    _temp: tempfile::TempDir,
}

impl ExternalServer {
    fn start_rustfs(port: u16) -> Option<Self> {
        let bin = std::env::var("RUSTFS_BIN").unwrap_or_else(|_| r"C:\tools\rustfs.exe".to_string());
        if !std::path::Path::new(&bin).exists() { return None; }
        let tmp = tempfile::TempDir::new().ok()?;
        let console_port = port + 1;
        let child = Command::new(&bin)
            .args(["server", tmp.path().to_str().unwrap(),
                   "--address", &format!(":{}", port),
                   "--console-address", &format!(":{}", console_port)])
            .env("RUSTFS_ROOT_USER", "benchuser")
            .env("RUSTFS_ROOT_PASSWORD", "benchpass")
            .stdout(Stdio::null()).stderr(Stdio::null())
            .spawn().ok()?;
        std::thread::sleep(Duration::from_millis(1500));
        Some(Self { child, port, _temp: tmp })
    }

    fn start_minio(port: u16) -> Option<Self> {
        let bin = std::env::var("MINIO_BIN").unwrap_or_else(|_| r"C:\tools\minio.exe".to_string());
        if !std::path::Path::new(&bin).exists() { return None; }
        let tmp = tempfile::TempDir::new().ok()?;
        let console_port = port + 1;
        let child = Command::new(&bin)
            .args(["server", tmp.path().to_str().unwrap(),
                   "--address", &format!(":{}", port),
                   "--console-address", &format!(":{}", console_port)])
            .env("MINIO_ROOT_USER", "benchuser")
            .env("MINIO_ROOT_PASSWORD", "benchpass")
            .stdout(Stdio::null()).stderr(Stdio::null())
            .spawn().ok()?;
        std::thread::sleep(Duration::from_millis(1500));
        Some(Self { child, port, _temp: tmp })
    }

    fn s3_client(&self, creds: (&str, &str)) -> Arc<abixio_ui::s3::client::S3Client> {
        Arc::new(
            abixio_ui::s3::client::S3Client::new(
                &format!("http://127.0.0.1:{}", self.port),
                Some(creds),
                "us-east-1",
            ).expect("create S3 client"),
        )
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
        let _ = client.put_object("bench4k", &format!("w{}", i), payload.clone(), "application/octet-stream").await;
    }

    // PUT 4KB
    let iters = 500;
    let mut timings = Vec::with_capacity(iters);
    for i in 0..iters {
        let t = Instant::now();
        let _ = client.put_object("bench4k", &format!("p{}", i), payload.clone(), "application/octet-stream").await;
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
        let _ = client.put_object("bench4k", &format!("big{}", i), payload_10m.clone(), "application/octet-stream").await;
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
        name, put_ops, put_avg.as_micros(), get_ops, get_avg.as_micros(), put10_mbps, get10_mbps,
    );
}

#[tokio::test(flavor = "multi_thread")]
#[ignore]
async fn bench_competitive() {
    eprintln!();
    eprintln!("=== competitive benchmark (aws-sdk-s3, release, keep-alive) ===");
    eprintln!("| {:12} | {:>22} | {:>22} | {:>10} | {:>10} |", "Server", "4KB PUT", "4KB GET", "10MB PUT", "10MB GET");
    eprintln!("|{:-<14}|{:-<24}|{:-<24}|{:-<12}|{:-<12}|", "", "", "", "", "");

    // AbixIO
    let abixio = AbixioServer::builder().volume_count(1).no_auth(false).start();
    run_competitive_4kb("AbixIO", &abixio.s3_client()).await;

    // RustFS
    if let Some(rustfs) = ExternalServer::start_rustfs(11501) {
        let client = rustfs.s3_client(("benchuser", "benchpass"));
        run_competitive_4kb("RustFS", &client).await;
    } else {
        eprintln!("| RustFS       | (binary not found)                                                           |");
    }

    // MinIO
    if let Some(minio) = ExternalServer::start_minio(11503) {
        let client = minio.s3_client(("benchuser", "benchpass"));
        run_competitive_4kb("MinIO", &client).await;
    } else {
        eprintln!("| MinIO        | (binary not found)                                                           |");
    }

    eprintln!();
}

// ============================================================================
// Client comparison: same server, different S3 clients
// ============================================================================

fn find_binary(env_var: &str, default: &str) -> Option<String> {
    if let Ok(p) = std::env::var(env_var) {
        if std::path::Path::new(&p).exists() { return Some(p); }
    }
    if std::path::Path::new(default).exists() { return Some(default.to_string()); }
    // check PATH
    if let Ok(output) = Command::new("which").arg(default.split('\\').last().unwrap_or(default)).output() {
        if output.status.success() {
            return Some(String::from_utf8_lossy(&output.stdout).trim().to_string());
        }
    }
    None
}

fn run_cli_bench(name: &str, setup_cmd: &[&str], put_cmd_template: &[&str], get_cmd_template: &[&str], iters: usize) {
    // run setup (alias, bucket create, etc)
    for cmd in setup_cmd.chunks(1) {
        let _ = Command::new("bash").arg("-c").arg(cmd[0]).stdout(Stdio::null()).stderr(Stdio::null()).status();
    }

    // PUT benchmark
    let start = Instant::now();
    for i in 0..iters {
        let cmd = put_cmd_template.join(" ").replace("{i}", &i.to_string());
        let _ = Command::new("bash").arg("-c").arg(&cmd).stdout(Stdio::null()).stderr(Stdio::null()).status();
    }
    let put_elapsed = start.elapsed();
    let put_ops = iters as f64 / put_elapsed.as_secs_f64();
    let put_avg_us = put_elapsed.as_micros() as f64 / iters as f64;

    // GET benchmark
    let start = Instant::now();
    for i in 0..iters {
        let cmd = get_cmd_template.join(" ").replace("{i}", &i.to_string());
        let _ = Command::new("bash").arg("-c").arg(&cmd).stdout(Stdio::null()).stderr(Stdio::null()).status();
    }
    let get_elapsed = start.elapsed();
    let get_ops = iters as f64 / get_elapsed.as_secs_f64();
    let get_avg_us = get_elapsed.as_micros() as f64 / iters as f64;

    eprintln!(
        "| {:<18} | {:>6.0} obj/s {:>8.0}us | {:>6.0} obj/s {:>8.0}us |",
        name, put_ops, put_avg_us, get_ops, get_avg_us,
    );
}

#[tokio::test(flavor = "multi_thread")]
#[ignore]
async fn bench_clients() {
    eprintln!();
    eprintln!("=== client comparison (4KB, same AbixIO server) ===");
    eprintln!("| {:18} | {:>28} | {:>28} |", "Client", "4KB PUT", "4KB GET");
    eprintln!("|{:-<20}|{:-<30}|{:-<30}|", "", "", "");

    let server = AbixioServer::builder().volume_count(1).no_auth(false).start();
    let port = server.endpoint().split(':').last().unwrap().to_string();
    let endpoint = server.endpoint();

    // 1. aws-sdk-s3 (Rust, keep-alive, SigV4)
    {
        let client = server.s3_client();
        client.create_bucket("clientbench").await.unwrap();
        let payload = vec![0x42u8; 4096];

        // warmup
        for i in 0..20 {
            let _ = client.put_object("clientbench", &format!("w{}", i), payload.clone(), "application/octet-stream").await;
        }

        let iters = 200;
        let start = Instant::now();
        for i in 0..iters {
            let _ = client.put_object("clientbench", &format!("sdk{}", i), payload.clone(), "application/octet-stream").await;
        }
        let put_elapsed = start.elapsed();

        let start = Instant::now();
        for i in 0..iters {
            let _ = client.get_object("clientbench", &format!("sdk{}", i)).await;
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

    // 2. mc (MinIO client, per-process)
    let mc_bin = find_binary("MC", r"C:\tools\mc.exe");
    if let Some(ref mc) = mc_bin {
        let _ = Command::new(mc).args(["alias", "set", "benchmc", &endpoint, "test", "testsecret", "--api", "S3v4"])
            .stdout(Stdio::null()).stderr(Stdio::null()).status();
        let _ = Command::new(mc).args(["mb", "benchmc/mcbench"])
            .stdout(Stdio::null()).stderr(Stdio::null()).status();

        // create test file
        let tmp = tempfile::NamedTempFile::new().unwrap();
        std::fs::write(tmp.path(), &[0x42u8; 4096]).unwrap();
        let tmppath = tmp.path().to_str().unwrap().to_string();
        let sink = tempfile::NamedTempFile::new().unwrap();
        let sinkpath = sink.path().to_str().unwrap().to_string();

        let iters = 50; // mc is slow, fewer iters

        let start = Instant::now();
        for i in 0..iters {
            let _ = Command::new(mc).args(["cp", &tmppath, &format!("benchmc/mcbench/mc{}", i)])
                .stdout(Stdio::null()).stderr(Stdio::null()).status();
        }
        let put_elapsed = start.elapsed();

        let start = Instant::now();
        for i in 0..iters {
            let _ = Command::new(mc).args(["cp", &format!("benchmc/mcbench/mc{}", i), &sinkpath])
                .stdout(Stdio::null()).stderr(Stdio::null()).status();
        }
        let get_elapsed = start.elapsed();

        let _ = Command::new(mc).args(["alias", "rm", "benchmc"]).stdout(Stdio::null()).stderr(Stdio::null()).status();

        eprintln!(
            "| {:<18} | {:>6.0} obj/s {:>8.0}us | {:>6.0} obj/s {:>8.0}us |",
            "mc (per-process)",
            iters as f64 / put_elapsed.as_secs_f64(),
            put_elapsed.as_micros() as f64 / iters as f64,
            iters as f64 / get_elapsed.as_secs_f64(),
            get_elapsed.as_micros() as f64 / iters as f64,
        );
    } else {
        eprintln!("| mc (per-process)   | (binary not found)                                           |");
    }

    // 3. rclone
    let rclone_bin = find_binary("RCLONE", "rclone");
    if let Some(ref rclone) = rclone_bin {
        // rclone uses env vars for config
        let iters = 50;

        let tmp = tempfile::NamedTempFile::new().unwrap();
        std::fs::write(tmp.path(), &[0x42u8; 4096]).unwrap();
        let tmppath = tmp.path().to_str().unwrap().to_string();
        let tmpdir = tempfile::TempDir::new().unwrap();
        let sinkdir = tmpdir.path().to_str().unwrap().to_string();

        let start = Instant::now();
        for i in 0..iters {
            let _ = Command::new(rclone)
                .args(["copyto", &tmppath, &format!(":s3:rclonebench/rc{}", i),
                       "--s3-provider", "Other",
                       "--s3-endpoint", &endpoint,
                       "--s3-access-key-id", "test",
                       "--s3-secret-access-key", "testsecret",
                       "--s3-force-path-style"])
                .stdout(Stdio::null()).stderr(Stdio::null()).status();
        }
        let put_elapsed = start.elapsed();

        let start = Instant::now();
        for i in 0..iters {
            let _ = Command::new(rclone)
                .args(["copyto", &format!(":s3:rclonebench/rc{}", i),
                       &format!("{}/rc{}", sinkdir, i),
                       "--s3-provider", "Other",
                       "--s3-endpoint", &endpoint,
                       "--s3-access-key-id", "test",
                       "--s3-secret-access-key", "testsecret",
                       "--s3-force-path-style"])
                .stdout(Stdio::null()).stderr(Stdio::null()).status();
        }
        let get_elapsed = start.elapsed();

        eprintln!(
            "| {:<18} | {:>6.0} obj/s {:>8.0}us | {:>6.0} obj/s {:>8.0}us |",
            "rclone (per-proc)",
            iters as f64 / put_elapsed.as_secs_f64(),
            put_elapsed.as_micros() as f64 / iters as f64,
            iters as f64 / get_elapsed.as_secs_f64(),
            get_elapsed.as_micros() as f64 / iters as f64,
        );
    } else {
        eprintln!("| rclone (per-proc)  | (binary not found)                                           |");
    }

    // 4. curl (unsigned, per-process baseline)
    {
        let iters = 100;
        let payload = "x".repeat(4096);

        let start = Instant::now();
        for i in 0..iters {
            let _ = Command::new("curl")
                .args(["-s", "-X", "PUT", "-H", "Content-Length: 4096",
                       "-d", &payload, &format!("{}/clientbench/curl{}", endpoint, i)])
                .stdout(Stdio::null()).stderr(Stdio::null()).status();
        }
        let put_elapsed = start.elapsed();

        let start = Instant::now();
        for i in 0..iters {
            let _ = Command::new("curl")
                .args(["-s", "-o", "/dev/null", &format!("{}/clientbench/curl{}", endpoint, i)])
                .stdout(Stdio::null()).stderr(Stdio::null()).status();
        }
        let get_elapsed = start.elapsed();

        eprintln!(
            "| {:<18} | {:>6.0} obj/s {:>8.0}us | {:>6.0} obj/s {:>8.0}us |",
            "curl (unsigned)",
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

async fn matrix_sdk(
    name: &str,
    client: &abixio_ui::s3::client::S3Client,
    bucket: &str,
    sizes: &[(&str, usize, usize)],
) -> Vec<MatrixResult> {
    let _ = client.create_bucket(bucket).await;
    let mut results = Vec::new();

    for &(label, size_bytes, iters) in sizes {
        let payload = vec![0x42u8; size_bytes];

        // warmup (unsigned)
        for i in 0..3.min(iters) {
            let _ = client.put_object_unsigned(bucket, &format!("w_{}_{}", label, i), payload.clone(), "application/octet-stream").await;
        }

        // PUT (unsigned -- same as mc, rclone, AWS CLI over HTTPS)
        let start = Instant::now();
        for i in 0..iters {
            let _ = client.put_object_unsigned(bucket, &format!("{}/{}", label, i), payload.clone(), "application/octet-stream").await;
        }
        let put_elapsed = start.elapsed();

        // GET
        let start = Instant::now();
        for i in 0..iters {
            let _ = client.get_object(bucket, &format!("{}/{}", label, i)).await;
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

fn matrix_mc(
    server_name: &str,
    mc: &str,
    sizes: &[(&str, usize, usize)],
) -> Vec<MatrixResult> {
    let mut results = Vec::new();
    let bucket = server_name.to_lowercase(); // S3 bucket names must be lowercase

    for &(label, size_bytes, iters) in sizes {
        // write payload to a persistent temp file (NamedTempFile can get deleted too early on Windows)
        let tmpdir = tempfile::TempDir::new().unwrap();
        let tmppath = tmpdir.path().join("payload.dat");
        std::fs::write(&tmppath, &vec![0x42u8; size_bytes]).unwrap();
        let tmppath = tmppath.to_str().unwrap().to_string();

        // verify file exists and has right size
        let meta = std::fs::metadata(&tmppath).unwrap();
        assert_eq!(meta.len(), size_bytes as u64, "temp file wrong size");
        let sink = tempfile::NamedTempFile::new().unwrap();
        let sinkpath = sink.path().to_str().unwrap().to_string();

        let iters = if size_bytes >= 1024 * 1024 * 1024 { iters.min(3) }
                    else if size_bytes >= 10 * 1024 * 1024 { iters.min(5) }
                    else { iters.min(30) };

        // test first PUT with error capture
        let test = Command::new(mc)
            .args(["cp", &tmppath, &format!("mx/bench{}/{}/test", bucket, label)])
            .output();
        if let Ok(output) = &test {
            if !output.status.success() {
                let err = String::from_utf8_lossy(&output.stderr);
                eprintln!("    mc {} {} error: {}", server_name, label, err.trim());
            }
        }

        let start = Instant::now();
        for i in 0..iters {
            let _ = Command::new(mc)
                .args(["cp", &tmppath, &format!("mx/bench{}/{}/{}", bucket, label, i)])
                .stdout(Stdio::null()).stderr(Stdio::null()).status();
        }
        let put_elapsed = start.elapsed();

        let sinkdir = tempfile::TempDir::new().unwrap();
        let sinkpath = sinkdir.path().join("out.dat");
        let sinkpath = sinkpath.to_str().unwrap().to_string();

        let start = Instant::now();
        for i in 0..iters {
            let _ = Command::new(mc)
                .args(["cp", &format!("mx/bench{}/{}/{}", bucket, label, i), &sinkpath])
                .stdout(Stdio::null()).stderr(Stdio::null()).status();
        }
        let get_elapsed = start.elapsed();

        results.push(MatrixResult {
            server: server_name.to_string(),
            client: "mc".to_string(),
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
    sizes: &[(&str, usize, usize)],
) -> Vec<MatrixResult> {
    let mut results = Vec::new();
    let bucket = server_name.to_lowercase();

    for &(label, size_bytes, iters) in sizes {
        let tmpdir = tempfile::TempDir::new().unwrap();
        let tmppath = tmpdir.path().join("payload.dat");
        std::fs::write(&tmppath, &vec![0x42u8; size_bytes]).unwrap();
        let tmppath = tmppath.to_str().unwrap().to_string();

        let iters = if size_bytes >= 1024 * 1024 * 1024 { iters.min(3) }
                    else if size_bytes >= 10 * 1024 * 1024 { iters.min(5) }
                    else { iters.min(30) };

        // test first PUT
        let test = Command::new(rclone)
            .args(["copyto", &tmppath, &format!("mx:bench{}/{}/test", bucket, label)])
            .output();
        if let Ok(output) = &test {
            if !output.status.success() {
                let err = String::from_utf8_lossy(&output.stderr);
                eprintln!("    rclone {} {} error: {}", server_name, label, err.trim());
            }
        }

        let start = Instant::now();
        for i in 0..iters {
            let _ = Command::new(rclone)
                .args(["copyto", &tmppath, &format!("mx:bench{}/{}/{}", bucket, label, i)])
                .stdout(Stdio::null()).stderr(Stdio::null()).status();
        }
        let put_elapsed = start.elapsed();

        let sinkdir = tempfile::TempDir::new().unwrap();
        let sinkpath = sinkdir.path().join("out.dat");
        let sinkpath = sinkpath.to_str().unwrap().to_string();

        let start = Instant::now();
        for i in 0..iters {
            let _ = Command::new(rclone)
                .args(["copyto", &format!("mx:bench{}/{}/{}", bucket, label, i), &sinkpath])
                .stdout(Stdio::null()).stderr(Stdio::null()).status();
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
        eprintln!("| {:12} | {:10} | {:>24} | {:>24} |", "Server", "Client", "PUT", "GET");
        eprintln!("|{:-<14}|{:-<12}|{:-<26}|{:-<26}|", "", "", "", "");

        for r in results.iter().filter(|r| r.size == *size_label) {
            eprintln!("| {:<12} | {:<10} | {} | {} |",
                r.server, r.client,
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
    eprintln!("  3 servers x 2 clients x 3 sizes x 2 ops");
    eprintln!("=============================================================");

    let sizes: Vec<(&str, usize, usize)> = vec![
        ("4KB", 4096, 200),
        ("10MB", 10 * 1024 * 1024, 5),
        ("1GB", 1024 * 1024 * 1024, 3),
    ];

    let mc_bin = find_binary("MC", r"C:\tools\mc.exe");
    let rclone_bin = find_binary("RCLONE", r"C:\tools\rclone.exe");
    let mut all_results: Vec<MatrixResult> = Vec::new();

    // --- AbixIO ---
    eprintln!("\nstarting AbixIO...");
    let abixio = AbixioServer::builder().volume_count(1).no_auth(false).start();
    let abixio_endpoint = abixio.endpoint();

    eprintln!("  aws-sdk-s3...");
    all_results.extend(matrix_sdk("AbixIO", &abixio.s3_client(), "matrix", &sizes).await);

    if let Some(ref mc) = mc_bin {
        eprintln!("  mc...");
        let _ = Command::new(mc)
            .args(["alias", "set", "mx", &abixio_endpoint, "test", "testsecret", "--api", "S3v4"])
            .stdout(Stdio::null()).stderr(Stdio::null()).status();
        let _ = Command::new(mc).args(["mb", "mx/benchabixio"]).stdout(Stdio::null()).stderr(Stdio::null()).status();
        all_results.extend(matrix_mc("AbixIO", mc, &sizes));
    }

    if let Some(ref rclone) = rclone_bin {
        eprintln!("  rclone...");
        let _ = Command::new(rclone)
            .args(["config", "create", "mx", "s3", "provider", "Other",
                   "endpoint", &abixio_endpoint, "access_key_id", "test",
                   "secret_access_key", "testsecret", "env_auth", "false"])
            .stdout(Stdio::null()).stderr(Stdio::null()).status();
        all_results.extend(matrix_rclone("AbixIO", rclone, &sizes));
    }

    // --- RustFS ---
    if let Some(rustfs) = ExternalServer::start_rustfs(11701) {
        eprintln!("\nstarting RustFS...");
        let rustfs_endpoint = format!("http://127.0.0.1:{}", rustfs.port);
        let client = rustfs.s3_client(("benchuser", "benchpass"));

        eprintln!("  aws-sdk-s3...");
        all_results.extend(matrix_sdk("RustFS", &client, "matrix", &sizes).await);

        if let Some(ref mc) = mc_bin {
            eprintln!("  mc...");
            let _ = Command::new(mc)
                .args(["alias", "set", "mx", &rustfs_endpoint, "benchuser", "benchpass", "--api", "S3v4"])
                .stdout(Stdio::null()).stderr(Stdio::null()).status();
            let _ = Command::new(mc).args(["mb", "mx/benchrustfs"]).stdout(Stdio::null()).stderr(Stdio::null()).status();
            all_results.extend(matrix_mc("RustFS", mc, &sizes));
        }

        if let Some(ref rclone) = rclone_bin {
            eprintln!("  rclone...");
            let _ = Command::new(rclone)
                .args(["config", "create", "mx", "s3", "provider", "Other",
                       "endpoint", &rustfs_endpoint, "access_key_id", "benchuser",
                       "secret_access_key", "benchpass", "env_auth", "false"])
                .stdout(Stdio::null()).stderr(Stdio::null()).status();
            all_results.extend(matrix_rclone("RustFS", rclone, &sizes));
        }
    } else {
        eprintln!("\nRustFS not found, skipping");
    }

    // --- MinIO ---
    if let Some(minio) = ExternalServer::start_minio(11703) {
        eprintln!("\nstarting MinIO...");
        let minio_endpoint = format!("http://127.0.0.1:{}", minio.port);
        let client = minio.s3_client(("benchuser", "benchpass"));

        eprintln!("  aws-sdk-s3...");
        all_results.extend(matrix_sdk("MinIO", &client, "matrix", &sizes).await);

        if let Some(ref mc) = mc_bin {
            eprintln!("  mc...");
            let _ = Command::new(mc)
                .args(["alias", "set", "mx", &minio_endpoint, "benchuser", "benchpass", "--api", "S3v4"])
                .stdout(Stdio::null()).stderr(Stdio::null()).status();
            let _ = Command::new(mc).args(["mb", "mx/benchminio"]).stdout(Stdio::null()).stderr(Stdio::null()).status();
            all_results.extend(matrix_mc("MinIO", mc, &sizes));
        }

        if let Some(ref rclone) = rclone_bin {
            eprintln!("  rclone...");
            let _ = Command::new(rclone)
                .args(["config", "create", "mx", "s3", "provider", "Other",
                       "endpoint", &minio_endpoint, "access_key_id", "benchuser",
                       "secret_access_key", "benchpass", "env_auth", "false"])
                .stdout(Stdio::null()).stderr(Stdio::null()).status();
            all_results.extend(matrix_rclone("MinIO", rclone, &sizes));
        }
    } else {
        eprintln!("\nMinIO not found, skipping");
    }

    if let Some(ref mc) = mc_bin {
        let _ = Command::new(mc).args(["alias", "rm", "mx"]).stdout(Stdio::null()).stderr(Stdio::null()).status();
    }
    if let Some(ref rclone) = rclone_bin {
        let _ = Command::new(rclone).args(["config", "delete", "mx"]).stdout(Stdio::null()).stderr(Stdio::null()).status();
    }

    eprintln!();
    eprintln!("=============================================================");
    eprintln!("  RESULTS");
    eprintln!("=============================================================");
    print_matrix(&all_results);
    eprintln!();
}
