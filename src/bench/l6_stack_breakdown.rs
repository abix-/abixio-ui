//! 5-stage stack attribution: attributes PUT latency to each stack layer
//! by running progressively more complex stages and subtracting.

use std::convert::Infallible;
use std::sync::Arc;
use std::time::{Duration, Instant};

use abixio::cluster::{ClusterConfig, ClusterManager};
use abixio::s3_route::AbixioDispatch;
use abixio::storage::local_volume::LocalVolume;
use abixio::storage::metadata::{ErasureMeta, ObjectMeta};
use abixio::storage::volume_pool::VolumePool;
use abixio::storage::{Backend, Store};

use super::stats::BenchResult;

const MB: usize = 1024 * 1024;

struct SimpleStats {
    avg_us: f64,
    p50_us: f64,
    p99_us: f64,
}

impl SimpleStats {
    fn from(samples: &mut [Duration], _size: usize) -> Self {
        samples.sort();
        let n = samples.len();
        let total_ns: u128 = samples.iter().map(|d| d.as_nanos()).sum();
        Self {
            avg_us: (total_ns as f64 / n as f64) / 1000.0,
            p50_us: samples[n / 2].as_nanos() as f64 / 1000.0,
            p99_us: samples[((n * 99) / 100).min(n - 1)].as_nanos() as f64 / 1000.0,
        }
    }
}

#[derive(Default)]
struct Buckets { fast: usize, mid: usize, slow: usize }

fn bucket_samples(samples: &[Duration]) -> Buckets {
    let mut b = Buckets::default();
    for s in samples {
        let us = s.as_micros();
        if us < 300 { b.fast += 1; }
        else if us < 500 { b.mid += 1; }
        else { b.slow += 1; }
    }
    b
}

fn print_row(name: &str, samples: &[Duration], size: usize) -> SimpleStats {
    let mut s = samples.to_vec();
    let stats = SimpleStats::from(&mut s, size);
    let b = bucket_samples(samples);
    eprintln!(
        "  {:<36} avg {:>8.1}us  p50 {:>8.1}us  p99 {:>9.1}us   fast {:>4}  mid {:>3}  slow {:>4}",
        name, stats.avg_us, stats.p50_us, stats.p99_us, b.fast, b.mid, b.slow,
    );
    stats
}

fn setup(n: usize) -> (tempfile::TempDir, Vec<std::path::PathBuf>) {
    let base = tempfile::TempDir::new().unwrap();
    let mut paths = Vec::new();
    for i in 0..n {
        let p = base.path().join(format!("d{}", i));
        std::fs::create_dir_all(&p).unwrap();
        paths.push(p);
    }
    (base, paths)
}

pub async fn run(_iters_override: Option<usize>) -> Vec<BenchResult> {
    let iters = 1000usize;
    let size = 4 * 1024;

    eprintln!("\n=== Stack breakdown at 4KB PUT ({} iters, sequential) ===\n", iters);
    eprintln!("  client -> reqwest -> hyper -> [increasing stack layers]");
    eprintln!("  1 disk, ftt=0, 127.0.0.1 TCP loopback");
    eprintln!();
    eprintln!("  Bimodal buckets: fast = samples <300us, slow = samples >500us");
    eprintln!();

    let sa = stage_a(size, iters).await;
    let sta = print_row("Stage A  hyper_bare", &sa, size);

    let sb = stage_b(size, iters).await;
    let stb = print_row("Stage B  hyper_manual_handler", &sb, size);

    let sc = stage_c(size, iters).await;
    let stc = print_row("Stage C  abixio_null_backend", &sc, size);

    let sd = stage_d(size, iters).await;
    let std_ = print_row("Stage D  file_tier", &sd, size);

    let se = stage_e(size, iters, 32).await;
    let ste = print_row("Stage E  pool_tier (depth 32)", &se, size);

    let se100 = stage_e(size, iters, 100).await;
    let ste100 = print_row("Stage E* pool_tier (depth 100)", &se100, size);

    let sep = stage_e_drained(size, iters).await;
    let step = print_row("Stage E' pool_tier_drained(32)", &sep, size);

    let seb = stage_e(size, iters, 1024).await;
    let steb = print_row("Stage E'' pool_tier_big(1024)", &seb, size);

    let seu = stage_e_channel(size, iters, 32, 100_000).await;
    let _steu = print_row("Stage E+ pool_tier(d32,ch100k)", &seu, size);

    let sef = stage_e_channel(size, iters, 1024, 100_000).await;
    let _stef = print_row("Stage E# pool_tier(d1024,ch100k)", &sef, size);

    eprintln!();
    eprintln!("=== Layer attribution (p50 subtraction) ===");
    eprintln!();
    eprintln!("  hyper + TCP + reqwest floor                (A)           {:>8.1}us", sta.p50_us);
    eprintln!("  + body read + minimal write_shard          (B - A)       {:>8.1}us", stb.p50_us - sta.p50_us);
    eprintln!("  + s3s + AbixioS3 + VolumePool dispatch     (C - A)       {:>8.1}us", stc.p50_us - sta.p50_us);
    eprintln!("  + real file-tier storage work              (D - C)       {:>8.1}us", std_.p50_us - stc.p50_us);
    eprintln!();
    eprintln!("=== Pool starvation analysis ===");
    eprintln!();
    eprintln!("  Pool tier, no drain (depth 32)      (E)                  {:>8.1}us", ste.p50_us);
    eprintln!("  Pool tier, no drain (depth 100)     (E*)                 {:>8.1}us", ste100.p50_us);
    eprintln!("  Pool tier, drained every 32 iters   (E', depth 32)       {:>8.1}us", step.p50_us);
    eprintln!("  Pool tier, depth 1024, no drain     (E'', never empty)   {:>8.1}us", steb.p50_us);
    eprintln!();
    eprintln!("  Pool starvation cost                (E - E')             {:>8.1}us", ste.p50_us - step.p50_us);
    eprintln!("  Fast path cost at HTTP layer        (E'' - C)            {:>8.1}us", steb.p50_us - stc.p50_us);
    eprintln!("  Pool savings vs file tier at HTTP   (D - E'')            {:>8.1}us", std_.p50_us - steb.p50_us);
    eprintln!();

    Vec::new()
}

// -- Stage A: bare hyper --

async fn stage_a(size: usize, iters: usize) -> Vec<Duration> {
    let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
    let addr = listener.local_addr().unwrap();
    let server = tokio::spawn(async move {
        loop {
            let (stream, _) = match listener.accept().await { Ok(v) => v, Err(_) => return };
            stream.set_nodelay(true).ok();
            let io = hyper_util::rt::TokioIo::new(stream);
            tokio::spawn(async move {
                let svc = hyper::service::service_fn(|req: hyper::Request<hyper::body::Incoming>| async move {
                    use http_body_util::BodyExt;
                    let _ = req.into_body().collect().await;
                    Ok::<_, Infallible>(hyper::Response::builder().status(200)
                        .body(http_body_util::Full::new(bytes::Bytes::new())).unwrap())
                });
                let _ = hyper::server::conn::http1::Builder::new().serve_connection(io, svc).await;
            });
        }
    });
    let samples = client_loop(addr, size, iters).await;
    server.abort();
    samples
}

// -- Stage B: hyper + body read + write_shard --

async fn stage_b(size: usize, iters: usize) -> Vec<Duration> {
    use http_body_util::BodyExt;
    let (_base, paths) = setup(1);
    let volume = Arc::new(LocalVolume::new(&paths[0]).unwrap());
    volume.make_bucket("bench").await.unwrap();
    let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
    let addr = listener.local_addr().unwrap();
    let vc = Arc::clone(&volume);
    let server = tokio::spawn(async move {
        loop {
            let (stream, _) = match listener.accept().await { Ok(v) => v, Err(_) => return };
            stream.set_nodelay(true).ok();
            let io = hyper_util::rt::TokioIo::new(stream);
            let v = Arc::clone(&vc);
            tokio::spawn(async move {
                let svc = hyper::service::service_fn(move |req: hyper::Request<hyper::body::Incoming>| {
                    let v = Arc::clone(&v);
                    async move {
                        let (parts, body) = req.into_parts();
                        let path = parts.uri.path().trim_start_matches('/').to_string();
                        let data = body.collect().await.unwrap().to_bytes();
                        let (bucket, key) = path.split_once('/').unwrap_or(("bench", "default"));
                        let meta = ObjectMeta {
                            size: data.len() as u64, etag: "deadbeef".into(),
                            content_type: "application/octet-stream".into(),
                            erasure: ErasureMeta::default(), ..Default::default()
                        };
                        v.write_shard(bucket, key, &data, &meta).await.unwrap();
                        Ok::<_, Infallible>(hyper::Response::builder().status(200)
                            .body(http_body_util::Full::new(bytes::Bytes::new())).unwrap())
                    }
                });
                let _ = hyper::server::conn::http1::Builder::new().serve_connection(io, svc).await;
            });
        }
    });
    let samples = client_loop(addr, size, iters).await;
    server.abort();
    samples
}

// -- Stage C: full s3s stack + NullBackend --

async fn stage_c(size: usize, iters: usize) -> Vec<Duration> {
    let backends: Vec<Box<dyn Backend>> = vec![Box::new(super::l2_s3proto::NullBackend::new())];
    let pool = Arc::new(VolumePool::new(backends).unwrap());
    pool.make_bucket("bench").await.unwrap();
    let (addr, handle) = spawn_stack(Arc::clone(&pool)).await;
    let samples = client_loop(addr, size, iters).await;
    handle.abort();
    samples
}

// -- Stage D: full s3s stack + file tier --

async fn stage_d(size: usize, iters: usize) -> Vec<Duration> {
    let (_base, paths) = setup(1);
    let volume = LocalVolume::new(&paths[0]).unwrap();
    let backends: Vec<Box<dyn Backend>> = vec![Box::new(volume)];
    let pool = Arc::new(VolumePool::new(backends).unwrap());
    pool.make_bucket("bench").await.unwrap();
    let (addr, handle) = spawn_stack(Arc::clone(&pool)).await;
    let samples = client_loop(addr, size, iters).await;
    handle.abort();
    samples
}

// -- Stage E: pool tier (configurable depth) --

async fn stage_e(size: usize, iters: usize, depth: u32) -> Vec<Duration> {
    let (_base, paths) = setup(1);
    let mut volume = LocalVolume::new(&paths[0]).unwrap();
    volume.enable_write_pool(depth).await.unwrap();
    let backends: Vec<Box<dyn Backend>> = vec![Box::new(volume)];
    let pool = Arc::new(VolumePool::new(backends).unwrap());
    pool.make_bucket("bench").await.unwrap();
    let (addr, handle) = spawn_stack(Arc::clone(&pool)).await;
    let samples = client_loop(addr, size, iters).await;
    for b in pool.disks() { b.drain_pending_writes().await; }
    handle.abort();
    samples
}

// -- Stage E': pool drained every 32 iters --

async fn stage_e_drained(size: usize, iters: usize) -> Vec<Duration> {
    let (_base, paths) = setup(1);
    let mut volume = LocalVolume::new(&paths[0]).unwrap();
    volume.enable_write_pool(32).await.unwrap();
    let backends: Vec<Box<dyn Backend>> = vec![Box::new(volume)];
    let pool = Arc::new(VolumePool::new(backends).unwrap());
    pool.make_bucket("bench").await.unwrap();
    let (addr, handle) = spawn_stack(Arc::clone(&pool)).await;

    let client = reqwest::Client::builder()
        .pool_idle_timeout(Duration::from_secs(60))
        .pool_max_idle_per_host(4)
        .build().unwrap();

    let warmup = vec![0x42u8; size];
    for i in 0..5 {
        let _ = client.put(&format!("http://{}/bench/w_{}", addr, i)).body(warmup.clone()).send().await;
    }
    for b in pool.disks() { b.drain_pending_writes().await; }

    let data = vec![0xA5u8; size];
    let mut samples = Vec::with_capacity(iters);
    for i in 0..iters {
        let t = Instant::now();
        let resp = client.put(&format!("http://{}/bench/put_{}", addr, i)).body(data.clone()).send().await.unwrap();
        let _ = resp.bytes().await.unwrap();
        samples.push(t.elapsed());
        if (i + 1) % 32 == 0 {
            for b in pool.disks() { b.drain_pending_writes().await; }
        }
    }
    for b in pool.disks() { b.drain_pending_writes().await; }
    handle.abort();
    samples
}

// -- Stage E variants: configurable channel buffer --

async fn stage_e_channel(size: usize, iters: usize, depth: u32, channel: usize) -> Vec<Duration> {
    let (_base, paths) = setup(1);
    let mut volume = LocalVolume::new(&paths[0]).unwrap();
    volume.enable_write_pool_with_channel(depth, channel).await.unwrap();
    let backends: Vec<Box<dyn Backend>> = vec![Box::new(volume)];
    let pool = Arc::new(VolumePool::new(backends).unwrap());
    pool.make_bucket("bench").await.unwrap();
    let (addr, handle) = spawn_stack(Arc::clone(&pool)).await;
    let samples = client_loop(addr, size, iters).await;
    for b in pool.disks() { b.drain_pending_writes().await; }
    handle.abort();
    samples
}

// -- Shared: spawn full abixio stack --

async fn spawn_stack(pool: Arc<VolumePool>) -> (std::net::SocketAddr, tokio::task::JoinHandle<()>) {
    let cluster = Arc::new(ClusterManager::new(ClusterConfig {
        node_id: "bench".into(), advertise_s3: "http://127.0.0.1:0".into(),
        advertise_cluster: "http://127.0.0.1:0".into(), nodes: Vec::new(),
        access_key: String::new(), secret_key: String::new(), no_auth: true,
        disk_paths: Vec::new(),
    }).unwrap());

    let s3 = abixio::s3_service::AbixioS3::new(Arc::clone(&pool), Arc::clone(&cluster));
    let mut builder = s3s::service::S3ServiceBuilder::new(s3);
    builder.set_validation(abixio::s3_service::RelaxedNameValidation);
    let s3_service = builder.build();
    let dispatch = Arc::new(AbixioDispatch::new(s3_service, None, None));

    let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
    let addr = listener.local_addr().unwrap();
    let d = dispatch.clone();
    let handle = tokio::spawn(async move {
        loop {
            let (stream, _) = match listener.accept().await { Ok(v) => v, Err(_) => return };
            stream.set_nodelay(true).ok();
            let io = hyper_util::rt::TokioIo::new(stream);
            let d = d.clone();
            tokio::spawn(async move {
                let svc = hyper::service::service_fn(move |req| {
                    let d = d.clone();
                    async move { Ok::<_, hyper::Error>(d.dispatch(req).await) }
                });
                let _ = hyper::server::conn::http1::Builder::new().serve_connection(io, svc).await;
            });
        }
    });
    (addr, handle)
}

// -- Shared: client loop --

async fn client_loop(addr: std::net::SocketAddr, size: usize, iters: usize) -> Vec<Duration> {
    let client = reqwest::Client::builder()
        .pool_idle_timeout(Duration::from_secs(60))
        .pool_max_idle_per_host(4)
        .build().unwrap();

    let warmup = vec![0x42u8; size];
    for i in 0..5 {
        let _ = client.put(&format!("http://{}/bench/w_{}", addr, i)).body(warmup.clone()).send().await;
    }

    let data = vec![0xA5u8; size];
    let mut samples = Vec::with_capacity(iters);
    for i in 0..iters {
        let t = Instant::now();
        let resp = client.put(&format!("http://{}/bench/put_{}", addr, i)).body(data.clone()).send().await.unwrap();
        let _ = resp.bytes().await.unwrap();
        samples.push(t.elapsed());
    }
    samples
}
