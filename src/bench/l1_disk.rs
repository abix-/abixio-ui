use std::time::Instant;

use super::stats::{human_size, iters_for_size, BenchResult};

pub async fn run(sizes: &[usize]) -> Vec<BenchResult> {
    let mut results = Vec::new();
    let tmp = tempfile::TempDir::new().unwrap();

    eprintln!("--- L1: Raw disk I/O ---");

    for &size in sizes {
        let data = vec![0x42u8; size];
        let iters = iters_for_size(size);
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
            layer: "L1".into(),
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
            layer: "L1".into(),
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
            layer: "L1".into(),
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
