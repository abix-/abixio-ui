//! L6: S3 + real storage (integration, NOT isolated)
//!
//! This is NOT an isolated layer test. It combines L1 (HTTP) + L2
//! (S3 protocol) + L3 (storage) into a single in-process stack.
//!
//! How it works:
//! - Creates a full s3s service with real VolumePool backends
//! - Spins up a hyper server on TCP loopback
//! - reqwest client sends real S3 PUT/GET requests
//! - Tests each write path (file/wal) and cache state (on/off)
//!
//! What this number means: the total in-process cost of an S3
//! request with real storage, but without the SDK client overhead
//! or TLS. This is what the server itself costs per request.

use std::sync::Arc;
use std::time::Instant;

use abixio::cluster::{ClusterConfig, ClusterManager};
use abixio::s3_route::AbixioDispatch;
use abixio::storage::local_volume::LocalVolume;
use abixio::storage::volume_pool::VolumePool;
use abixio::storage::{Backend, Store};

use super::stats::{human_size, iters_for_size, BenchResult};

pub async fn run(
    sizes: &[usize],
    write_path: &str,
    write_cache: bool,
    iters_override: Option<usize>,
) -> Vec<BenchResult> {
    let mut results = Vec::new();
    let tmp = super::stats::make_tmp_dir();
    let disk_path = tmp.path().join("d0");
    std::fs::create_dir_all(&disk_path).unwrap();

    let wp_label = if write_cache {
        format!("{}+wc", write_path)
    } else {
        write_path.to_string()
    };
    eprintln!("--- L6: S3 + real storage ({}) ---", wp_label);

    let mut volume = LocalVolume::new(&disk_path).unwrap();
    match write_path {
        "wal" => { volume.enable_wal().await.unwrap(); }
        _ => {} // "file" = no extra wiring
    }

    let backends: Vec<Box<dyn Backend>> = vec![Box::new(volume)];
    let mut pool = VolumePool::new(backends).unwrap();
    if write_cache {
        pool.enable_write_cache(256 * 1024 * 1024);
    }
    let pool = Arc::new(pool);
    pool.make_bucket("bench").await.unwrap();

    let cluster = Arc::new(
        ClusterManager::new(ClusterConfig {
            node_id: "bench".into(),
            advertise_s3: "http://127.0.0.1:0".into(),
            advertise_cluster: "http://127.0.0.1:0".into(),
            nodes: Vec::new(),
            access_key: String::new(),
            secret_key: String::new(),
            no_auth: true,
            disk_paths: vec![disk_path],
        })
        .unwrap(),
    );

    let s3 = abixio::s3_service::AbixioS3::new(Arc::clone(&pool), Arc::clone(&cluster));
    let mut builder = s3s::service::S3ServiceBuilder::new(s3);
    builder.set_validation(abixio::s3_service::RelaxedNameValidation);
    let s3_service = builder.build();
    let dispatch = Arc::new(AbixioDispatch::new(s3_service, None, None));

    let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
    let addr = listener.local_addr().unwrap();
    let d = dispatch.clone();
    let server_task = tokio::spawn(async move {
        loop {
            let (stream, _) = match listener.accept().await {
                Ok(v) => v,
                Err(_) => return,
            };
            stream.set_nodelay(true).ok();
            let io = hyper_util::rt::TokioIo::new(stream);
            let d = d.clone();
            tokio::spawn(async move {
                let svc = hyper::service::service_fn(move |req| {
                    let d = d.clone();
                    async move { Ok::<_, hyper::Error>(d.dispatch(req).await) }
                });
                let _ = hyper::server::conn::http1::Builder::new()
                    .serve_connection(io, svc)
                    .await;
            });
        }
    });

    let client = reqwest::Client::builder()
        .pool_idle_timeout(std::time::Duration::from_secs(60))
        .build()
        .unwrap();

    for &size in sizes {
        let data = vec![0x42u8; size];
        let iters = iters_override.unwrap_or_else(|| iters_for_size(size));
        let label = human_size(size);

        // warmup
        for i in 0..3 {
            let url = format!("http://{}/bench/warmup_{}", addr, i);
            client.put(&url).body(data.clone()).send().await.unwrap();
        }

        // PUT
        let mut timings = Vec::with_capacity(iters);
        for i in 0..iters {
            let url = format!("http://{}/bench/s6_{}_{}", addr, label, i);
            let t = Instant::now();
            let resp = client.put(&url).body(data.clone()).send().await.unwrap();
            timings.push(t.elapsed());
            assert!(resp.status().is_success(), "PUT failed: {}", resp.status());
        }
        results.push(BenchResult {
            layer: "L6".into(),
            op: "s3s_put".into(),
            size,
            iters,
            write_path: Some(write_path.into()),
            write_cache: Some(write_cache),
            server: None,
            client: None,
            timings,
        });

        // drain wal + flush cache before GET
        if write_path == "wal" {
            for backend in pool.disks() {
                backend.drain_pending_writes().await;
            }
        }
        if write_cache {
            let _ = pool.flush_write_cache().await;
        }

        // GET
        let mut timings = Vec::with_capacity(iters);
        for i in 0..iters {
            let url = format!("http://{}/bench/s6_{}_{}", addr, label, i);
            let t = Instant::now();
            let resp = client.get(&url).send().await.unwrap();
            let _ = resp.bytes().await.unwrap();
            timings.push(t.elapsed());
        }
        results.push(BenchResult {
            layer: "L6".into(),
            op: "s3s_get".into(),
            size,
            iters,
            write_path: Some(write_path.into()),
            write_cache: Some(write_cache),
            server: None,
            client: None,
            timings,
        });

        eprintln!("  {} done ({} iters)", label, iters);
    }

    server_task.abort();
    results
}
