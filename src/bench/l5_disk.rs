//! L5: Raw disk I/O (isolated)
//!
//! Measures ONLY the cost of filesystem read/write. No storage
//! pipeline, no HTTP, no hashing, no erasure coding.
//!
//! How it works:
//! - tokio::fs::write to a temp file (page cache, no fsync)
//! - tokio::fs::write + sync_all to a temp file (forces to physical disk)
//! - tokio::fs::read from the files written above
//!
//! What this number means: the ceiling for any storage tier. No
//! write path in AbixIO can be faster than the raw filesystem.
//! The gap between L5 and L3 is the overhead of the storage pipeline
//! (hashing, EC, metadata, placement).

use std::time::Instant;

use super::stats::{human_size, iters_for_size, BenchResult};

pub async fn run(sizes: &[usize], iters_override: Option<usize>) -> Vec<BenchResult> {
    let mut results = Vec::new();
    let tmp = tempfile::TempDir::new().unwrap();

    eprintln!("--- L5: Raw disk I/O ---");

    for &size in sizes {
        let data = vec![0x42u8; size];
        let iters = iters_override.unwrap_or_else(|| iters_for_size(size));
        let label = human_size(size);

        // write (page cache)
        let mut timings = Vec::with_capacity(iters);
        for i in 0..iters {
            let path = tmp.path().join(format!("w_{}_{}", label, i));
            let t = Instant::now();
            tokio::fs::write(&path, &data).await.unwrap();
            timings.push(t.elapsed());
        }
        results.push(BenchResult {
            layer: "L5".into(),
            op: "disk_write".into(),
            size,
            iters,
            write_path: None,
            write_cache: None,
            server: None,
            client: None,
            timings,
        });

        // write + fsync (physical disk)
        let mut timings = Vec::with_capacity(iters);
        for i in 0..iters {
            let path = tmp.path().join(format!("ws_{}_{}", label, i));
            let t = Instant::now();
            {
                use tokio::io::AsyncWriteExt;
                let mut file = tokio::fs::File::create(&path).await.unwrap();
                file.write_all(&data).await.unwrap();
                file.sync_all().await.unwrap();
            }
            timings.push(t.elapsed());
        }
        results.push(BenchResult {
            layer: "L5".into(),
            op: "disk_write_fsync".into(),
            size,
            iters,
            write_path: None,
            write_cache: None,
            server: None,
            client: None,
            timings,
        });

        // read (from files written above, likely cached)
        let mut timings = Vec::with_capacity(iters);
        for i in 0..iters {
            let path = tmp.path().join(format!("w_{}_{}", label, i));
            let t = Instant::now();
            let _ = tokio::fs::read(&path).await.unwrap();
            timings.push(t.elapsed());
        }
        results.push(BenchResult {
            layer: "L5".into(),
            op: "disk_read".into(),
            size,
            iters,
            write_path: None,
            write_cache: None,
            server: None,
            client: None,
            timings,
        });

        eprintln!("  {} done ({} iters)", label, iters);
    }

    results
}
