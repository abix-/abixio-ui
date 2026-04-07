//! Benchmark suite for AbixIO storage operations.
//!
//! Launches real abixio server instances with 1-4 disks and measures
//! PUT, GET, HEAD, LIST, DELETE latency and throughput via aws-sdk-s3.
//!
//! Run with: `cargo test --test bench -- --ignored --nocapture`
//! Single config: `cargo test --test bench -- --ignored --nocapture bench_4_disks`

#[path = "support/mod.rs"]
mod support;

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
        "{:<8} {:<6} {:>6} ops  {:>10}  {:>10}  {:>10}  {:>12}",
        "OP", "SIZE", "", "avg", "p50", "p99", "throughput"
    );

    for r in results.iter_mut() {
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
