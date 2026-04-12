use std::time::Instant;

use super::stats::{human_size, iters_for_size, BenchResult};

pub async fn run(sizes: &[usize]) -> Vec<BenchResult> {
    let mut results = Vec::new();

    eprintln!("--- L2: Hashing + erasure coding ---");

    for &size in sizes {
        let data = vec![0x42u8; size];
        let iters = iters_for_size(size);
        let label = human_size(size);

        // blake3
        let mut timings = Vec::with_capacity(iters);
        for _ in 0..iters {
            let t = Instant::now();
            let _ = abixio::storage::bitrot::blake3_hex(&data);
            timings.push(t.elapsed());
        }
        results.push(BenchResult {
            layer: "L2".into(),
            op: "blake3".into(),
            size,
            iters,
            write_path: None,
            write_cache: None,
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
            layer: "L2".into(),
            op: "md5".into(),
            size,
            iters,
            write_path: None,
            write_cache: None,
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
            layer: "L2".into(),
            op: "rs_encode_3+1".into(),
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
