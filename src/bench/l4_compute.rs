//! L4: Hashing + erasure coding (isolated)
//!
//! Measures ONLY the CPU cost of hashing and reed-solomon encoding.
//! No I/O, no HTTP, no storage pipeline.
//!
//! How it works:
//! - Calls blake3_hex, md5_hex, sha256_hex directly on in-memory buffers
//! - Calls reed-solomon encode 3+1 directly on in-memory shards
//! - Pure compute, no disk, no network
//!
//! What this number means: the per-byte CPU cost of integrity checks
//! and erasure coding. These run inline during every PUT. At large
//! sizes, hashing (especially MD5 at 703 MB/s) can become a
//! measurable fraction of total PUT time.

use std::time::Instant;

use super::stats::{human_size, iters_for_size, BenchResult};

pub async fn run(sizes: &[usize], iters_override: Option<usize>) -> Vec<BenchResult> {
    let mut results = Vec::new();

    eprintln!("--- L4: Hashing + erasure coding ---");

    for &size in sizes {
        let data = vec![0x42u8; size];
        let iters = iters_override.unwrap_or_else(|| iters_for_size(size));
        let label = human_size(size);

        // blake3
        let mut timings = Vec::with_capacity(iters);
        for _ in 0..iters {
            let t = Instant::now();
            let _ = abixio::storage::bitrot::blake3_hex(&data);
            timings.push(t.elapsed());
        }
        results.push(BenchResult {
            layer: "L4".into(),
            op: "blake3".into(),
            size,
            iters,
            write_path: None,
            write_cache: None,
            read_cache: None,
            server: None,
            client: None,
            timings,
        });

        // md5
        let mut timings = Vec::with_capacity(iters);
        for _ in 0..iters {
            let t = Instant::now();
            let _ = abixio::storage::bitrot::md5_hex(&data);
            timings.push(t.elapsed());
        }
        results.push(BenchResult {
            layer: "L4".into(),
            op: "md5".into(),
            size,
            iters,
            write_path: None,
            write_cache: None,
            read_cache: None,
            server: None,
            client: None,
            timings,
        });

        // sha256
        let mut timings = Vec::with_capacity(iters);
        for _ in 0..iters {
            let t = Instant::now();
            let _ = abixio::storage::bitrot::sha256_hex(&data);
            timings.push(t.elapsed());
        }
        results.push(BenchResult {
            layer: "L4".into(),
            op: "sha256".into(),
            size,
            iters,
            write_path: None,
            write_cache: None,
            read_cache: None,
            server: None,
            client: None,
            timings,
        });

        // reed-solomon encode 3+1
        let rs = reed_solomon_erasure::galois_8::ReedSolomon::new(3, 1).unwrap();
        let mut timings = Vec::with_capacity(iters);
        for _ in 0..iters {
            let mut shards = abixio::storage::erasure_encode::split_data(&data, 3);
            let shard_size = shards[0].len();
            shards.push(vec![0u8; shard_size]);
            let t = Instant::now();
            rs.encode(&mut shards).unwrap();
            timings.push(t.elapsed());
        }
        results.push(BenchResult {
            layer: "L4".into(),
            op: "rs_encode_3+1".into(),
            size,
            iters,
            write_path: None,
            write_cache: None,
            read_cache: None,
            server: None,
            client: None,
            timings,
        });

        eprintln!("  {} done ({} iters)", label, iters);
    }

    results
}
