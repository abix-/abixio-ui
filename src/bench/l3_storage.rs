//! L3: Storage pipeline (isolated)
//!
//! Measures ONLY the cost of VolumePool put/get. No HTTP, no s3s,
//! no TCP. Calls the VolumePool API directly in-process.
//!
//! How it works:
//! - Creates a VolumePool with real LocalVolume backends on tmpdir
//! - Calls put_object / get_object / put_object_stream / get_object_stream
//!   directly as Rust function calls
//! - Tests each write path (file/log/pool) and cache state (on/off)
//!
//! What this number means: the cost of storage routing, EC encoding,
//! hashing, placement, and the actual disk write/read. This is the
//! storage work that sits underneath the S3 protocol layer.

use std::time::Instant;

use abixio::storage::local_volume::LocalVolume;
use abixio::storage::metadata::PutOptions;
use abixio::storage::volume_pool::VolumePool;
use abixio::storage::{Backend, Store};
use futures::StreamExt;

use super::stats::{human_size, iters_for_size, BenchResult};

fn opts() -> PutOptions {
    PutOptions {
        content_type: "application/octet-stream".to_string(),
        ..Default::default()
    }
}

pub async fn run(
    sizes: &[usize],
    write_path: &str,
    write_cache: bool,
    iters_override: Option<usize>,
) -> Vec<BenchResult> {
    let mut results = Vec::new();

    for disk_count in [1, 4] {
        results.extend(
            run_disks(sizes, write_path, write_cache, iters_override, disk_count).await,
        );
    }

    results
}

async fn run_disks(
    sizes: &[usize],
    write_path: &str,
    write_cache: bool,
    iters_override: Option<usize>,
    disk_count: usize,
) -> Vec<BenchResult> {
    let mut results = Vec::new();
    let tmp = super::stats::make_tmp_dir();
    let mut disk_paths = Vec::new();
    for i in 0..disk_count {
        let p = tmp.path().join(format!("d{}", i));
        std::fs::create_dir_all(&p).unwrap();
        disk_paths.push(p);
    }

    let wp_label = if write_cache {
        format!("{}+wc {}disk", write_path, disk_count)
    } else {
        format!("{} {}disk", write_path, disk_count)
    };
    eprintln!("--- L3: Storage pipeline ({}) ---", wp_label);

    // build LocalVolumes with the requested tier
    let mut backends: Vec<Box<dyn Backend>> = Vec::new();
    for path in &disk_paths {
        let mut volume = LocalVolume::new(path).unwrap();
        match write_path {
            "log" => { volume.enable_log_store().unwrap(); }
            "pool" => { volume.enable_write_pool(32).await.unwrap(); }
            _ => {}
        }
        backends.push(Box::new(volume));
    }
    let mut pool = VolumePool::new(backends).unwrap();
    if write_cache {
        pool.enable_write_cache(256 * 1024 * 1024);
    }
    pool.make_bucket("bench").await.unwrap();

    for &size in sizes {
        let data = vec![0x42u8; size];
        let iters = iters_override.unwrap_or_else(|| iters_for_size(size));
        let label = human_size(size);

        // warmup
        for i in 0..3 {
            pool.put_object("bench", &format!("w/{}/{}", label, i), &data, opts())
                .await
                .unwrap();
        }

        // PUT
        let mut timings = Vec::with_capacity(iters);
        for i in 0..iters {
            let t = Instant::now();
            pool.put_object("bench", &format!("p/{}/{}", label, i), &data, opts())
                .await
                .unwrap();
            timings.push(t.elapsed());
        }
        results.push(BenchResult {
            layer: "L3".into(),
            op: "put".into(),
            size,
            iters,
            write_path: Some(write_path.into()),
            write_cache: Some(write_cache),
            server: None,
            client: None,
            timings,
        });

        // drain pending writes for pool tier before GET
        if write_path == "pool" {
            for backend in pool.disks() {
                backend.drain_pending_writes().await;
            }
        }

        // flush write cache before GET so reads come from disk
        if write_cache {
            let _ = pool.flush_write_cache().await;
        }

        // GET
        let mut timings = Vec::with_capacity(iters);
        for i in 0..iters {
            let t = Instant::now();
            let _ = pool
                .get_object("bench", &format!("p/{}/{}", label, i))
                .await
                .unwrap();
            timings.push(t.elapsed());
        }
        results.push(BenchResult {
            layer: "L3".into(),
            op: "get".into(),
            size,
            iters,
            write_path: Some(write_path.into()),
            write_cache: Some(write_cache),
            server: None,
            client: None,
            timings,
        });

        // streaming PUT
        let mut timings = Vec::with_capacity(iters);
        for i in 0..iters {
            let chunks: Vec<Result<bytes::Bytes, std::io::Error>> = data
                .chunks(64 * 1024)
                .map(|c| Ok(bytes::Bytes::copy_from_slice(c)))
                .collect();
            let stream = futures::stream::iter(chunks);
            let t = Instant::now();
            pool.put_object_stream("bench", &format!("s/{}/{}", label, i), stream, opts(), None, None)
                .await
                .unwrap();
            timings.push(t.elapsed());
        }
        results.push(BenchResult {
            layer: "L3".into(),
            op: "put_stream".into(),
            size,
            iters,
            write_path: Some(write_path.into()),
            write_cache: Some(write_cache),
            server: None,
            client: None,
            timings,
        });

        // drain + flush before streaming GET
        if write_path == "pool" {
            for backend in pool.disks() {
                backend.drain_pending_writes().await;
            }
        }
        if write_cache {
            let _ = pool.flush_write_cache().await;
        }

        // streaming GET
        let mut timings = Vec::with_capacity(iters);
        for i in 0..iters {
            let t = Instant::now();
            let (_info, stream) = pool.get_object_stream("bench", &format!("s/{}/{}", label, i))
                .await
                .unwrap();
            let mut stream = std::pin::pin!(stream);
            while let Some(chunk) = stream.next().await {
                let _ = chunk.unwrap();
            }
            timings.push(t.elapsed());
        }
        results.push(BenchResult {
            layer: "L3".into(),
            op: "get_stream".into(),
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

    results
}
