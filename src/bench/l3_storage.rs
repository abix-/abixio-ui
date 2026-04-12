use std::time::Instant;

use abixio::storage::local_volume::LocalVolume;
use abixio::storage::metadata::PutOptions;
use abixio::storage::volume_pool::VolumePool;
use abixio::storage::{Backend, Store};

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
    let tmp = tempfile::TempDir::new().unwrap();
    let disk_path = tmp.path().join("d0");
    std::fs::create_dir_all(&disk_path).unwrap();

    let wp_label = if write_cache {
        format!("{}+wc", write_path)
    } else {
        write_path.to_string()
    };
    eprintln!("--- L3: Storage pipeline ({}) ---", wp_label);

    // build LocalVolume with the requested tier
    let mut volume = LocalVolume::new(&disk_path).unwrap();
    match write_path {
        "log" => {
            volume.enable_log_store().unwrap();
        }
        "pool" => {
            volume.enable_write_pool(32).await.unwrap();
        }
        _ => {} // "file" = baseline
    }

    let backends: Vec<Box<dyn Backend>> = vec![Box::new(volume)];
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

        eprintln!("  {} done ({} iters)", label, iters);
    }

    results
}
