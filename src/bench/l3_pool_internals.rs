//! Pool write path internals: slot primitives, write strategies, JSON
//! serializers, rename worker drain, integrated write_shard, per-step
//! overhead breakdown, and inlined section timing.

use std::sync::Arc;
use std::time::{Duration, Instant};

use abixio::storage::local_volume::LocalVolume;
use abixio::storage::metadata::{ErasureMeta, ObjectMeta, ObjectMetaFile};
use abixio::storage::pathing;
use abixio::storage::write_slot_pool::{
    PendingEntry, RenameRequest, WriteSlot, WriteSlotPool,
    process_rename_request, run_rename_worker,
};
use abixio::storage::Backend;
use std::collections::HashMap;
use tokio::io::AsyncWriteExt;

use super::stats::BenchResult;

const MB: usize = 1024 * 1024;

fn human(size: usize) -> String {
    if size >= MB { format!("{}MB", size / MB) }
    else if size >= 1024 { format!("{}KB", size / 1024) }
    else { format!("{}B", size) }
}

fn make_meta() -> ObjectMeta {
    ObjectMeta {
        size: 4096,
        etag: "d41d8cd98f00b204e9800998ecf8427e".to_string(),
        content_type: "application/octet-stream".to_string(),
        created_at: 1700000000,
        erasure: ErasureMeta {
            ftt: 1, index: 0, epoch_id: 1,
            volume_ids: vec!["vol-0".into(), "vol-1".into(), "vol-2".into(), "vol-3".into()],
        },
        checksum: "abcdef0123456789abcdef0123456789abcdef0123456789abcdef0123456789".to_string(),
        user_metadata: HashMap::new(),
        tags: HashMap::new(),
        version_id: String::new(),
        is_latest: true,
        is_delete_marker: false,
        parts: Vec::new(),
        inline_data: None,
    }
}

fn make_meta_file() -> ObjectMetaFile {
    ObjectMetaFile {
        bucket: "bench".to_string(),
        key: "obj".to_string(),
        versions: vec![make_meta()],
    }
}

fn fmt_dur(d: Duration) -> String {
    let us = d.as_secs_f64() * 1_000_000.0;
    if us >= 1000.0 { format!("{:.2}ms", us / 1000.0) }
    else { format!("{:.1}us", us) }
}

fn report_row(size: usize, label: &str, iters: usize, timings: &mut Vec<Duration>) {
    timings.sort();
    let total: Duration = timings.iter().sum();
    let avg = total / iters as u32;
    let p50 = timings[iters / 2];
    let p99_idx = ((iters * 99) / 100).min(iters - 1);
    let p99 = timings[p99_idx];
    let mbps = (size * iters) as f64 / total.as_secs_f64() / MB as f64;
    eprintln!(
        "  {:<8} {:<26} {:>5}  {:>9}  {:>9}  {:>9}  {:>9.1} MB/s",
        human(size), label, iters, fmt_dur(avg), fmt_dur(p50), fmt_dur(p99), mbps,
    );
}

fn report_ns(label: &str, iters: usize, mut samples: Vec<Duration>) {
    samples.sort();
    let total: Duration = samples.iter().sum();
    let avg = total / iters as u32;
    let p50 = samples[iters / 2];
    let p99 = samples[(iters * 99) / 100];
    let p999_idx = ((iters * 999) / 1000).min(iters - 1);
    let p999 = samples[p999_idx];
    eprintln!(
        "  {:<32}  avg {:>6}ns  p50 {:>6}ns  p99 {:>6}ns  p999 {:>6}ns",
        label, avg.as_nanos(), p50.as_nanos(), p99.as_nanos(), p999.as_nanos(),
    );
}

fn report_breakdown(label: &str, mut samples: Vec<Duration>) {
    if samples.is_empty() { return; }
    samples.sort();
    let n = samples.len();
    let total: Duration = samples.iter().sum();
    let avg = total / n as u32;
    let p50 = samples[n / 2];
    let p99 = samples[((n * 99) / 100).min(n - 1)];
    eprintln!(
        "  {:<46}  avg {:>7}ns  p50 {:>7}ns  p99 {:>7}ns",
        label, avg.as_nanos(), p50.as_nanos(), p99.as_nanos(),
    );
}

pub async fn run(_iters_override: Option<usize>) -> Vec<BenchResult> {
    // Pool internals produce specialized output, not BenchResult rows.
    // They print directly to stderr. Return empty vec.
    bench_l0_primitive().await;
    bench_l1_slot_write().await;
    bench_l1_5_json_serializers().await;
    bench_l2_worker_drain().await;
    bench_l3_integrated_put().await;
    bench_l3_5_integration_breakdown().await;
    bench_l3_6_write_shard_breakdown().await;
    Vec::new()
}

// ========================================================================
// L0: WriteSlotPool primitive (acquire/release, contention, starvation)
// ========================================================================

async fn bench_l0_primitive() {
    let tmp = super::stats::make_tmp_dir();
    let pool_dir = tmp.path().join("preopen");
    let depth = 32u32;

    eprintln!("\n=== Pool L0: WriteSlotPool primitive ===\n");

    let t = Instant::now();
    let pool = WriteSlotPool::new(&pool_dir, depth).await.unwrap();
    let init_elapsed = t.elapsed();
    eprintln!(
        "  pool init (depth={}): {:.2}ms  ({:.2}us per slot pair)",
        depth, init_elapsed.as_secs_f64() * 1000.0,
        init_elapsed.as_secs_f64() * 1_000_000.0 / depth as f64,
    );

    let warmup = 10_000;
    for _ in 0..warmup {
        let s = pool.try_pop().unwrap();
        pool.release(s).unwrap();
    }
    let iters = 100_000;
    let mut samples = Vec::with_capacity(iters);
    for _ in 0..iters {
        let t = Instant::now();
        let s = pool.try_pop().unwrap();
        pool.release(s).unwrap();
        samples.push(t.elapsed());
    }
    samples.sort();
    let total_ns: u128 = samples.iter().map(|d| d.as_nanos()).sum();
    let avg_ns = total_ns / iters as u128;
    let p50_ns = samples[iters / 2].as_nanos();
    let p99_ns = samples[(iters * 99) / 100].as_nanos();
    let p999_ns = samples[(iters * 999) / 1000].as_nanos();
    eprintln!(
        "  single-thread pop+release ({} iters):  avg {}ns  p50 {}ns  p99 {}ns  p999 {}ns",
        iters, avg_ns, p50_ns, p99_ns, p999_ns,
    );

    for workers in [2usize, 8, 32] {
        let per_worker = 10_000;
        let pool_clone = Arc::clone(&pool);
        let t = Instant::now();
        let mut handles = Vec::with_capacity(workers);
        for _ in 0..workers {
            let p = Arc::clone(&pool_clone);
            handles.push(tokio::spawn(async move {
                for _ in 0..per_worker {
                    let slot = loop {
                        if let Some(s) = p.try_pop() { break s; }
                        std::hint::spin_loop();
                    };
                    p.release(slot).unwrap();
                }
            }));
        }
        for h in handles { h.await.unwrap(); }
        let elapsed = t.elapsed();
        let total_ops = workers * per_worker;
        eprintln!(
            "  {:>2} workers x {} ops:  {:.2}ms total  {:>10.0} ops/sec  {}ns/op",
            workers, per_worker, elapsed.as_secs_f64() * 1000.0,
            total_ops as f64 / elapsed.as_secs_f64(),
            elapsed.as_nanos() / total_ops as u128,
        );
    }

    let mut held = Vec::with_capacity(depth as usize);
    for _ in 0..depth { held.push(pool.try_pop().unwrap()); }
    let t = Instant::now();
    let drained = pool.try_pop();
    let drained_elapsed = t.elapsed();
    assert!(drained.is_none());
    eprintln!("  empty try_pop:  {}ns", drained_elapsed.as_nanos());
    for s in held { pool.release(s).unwrap(); }
}

// ========================================================================
// L1: slot writes with real I/O (6 strategies)
// ========================================================================

async fn bench_l1_slot_write() {
    let meta_file = make_meta_file();
    let meta_pretty: Vec<u8> = serde_json::to_vec_pretty(&meta_file).unwrap();
    let meta_compact: Vec<u8> = serde_json::to_vec(&meta_file).unwrap();

    eprintln!("\n=== Pool L1: slot write strategies ===\n");
    eprintln!("  meta.json sizes:  pretty {} bytes,  compact {} bytes", meta_pretty.len(), meta_compact.len());
    eprintln!();
    eprintln!("  {:<8} {:<26} {:>5}  {:>9}  {:>9}  {:>9}  {:>14}", "SIZE", "STRATEGY", "ITERS", "AVG", "p50", "p99", "THROUGHPUT");

    let sizes: &[(usize, usize)] = &[(4096, 100), (65536, 60), (MB, 30), (10*MB, 15), (100*MB, 5)];

    for &(size, iters) in sizes {
        let data = vec![0x42u8; size];
        let base = super::stats::make_tmp_dir();
        eprintln!();

        // A: file_tier_full
        let dir_a = base.path().join("a");
        std::fs::create_dir_all(&dir_a).unwrap();
        let mut timings = Vec::with_capacity(iters);
        for i in 0..iters {
            let obj_dir = dir_a.join(format!("obj_{}", i));
            let t = Instant::now();
            tokio::fs::create_dir_all(&obj_dir).await.unwrap();
            tokio::fs::write(obj_dir.join("shard.dat"), &data).await.unwrap();
            tokio::fs::write(obj_dir.join("meta.json"), &meta_pretty).await.unwrap();
            timings.push(t.elapsed());
        }
        report_row(size, "A: file_tier_full", iters, &mut timings);

        // B: file_tier_no_mkdir
        let dir_b = base.path().join("b");
        std::fs::create_dir_all(&dir_b).unwrap();
        let obj_dirs_b: Vec<_> = (0..iters).map(|i| {
            let p = dir_b.join(format!("obj_{}", i));
            std::fs::create_dir_all(&p).unwrap();
            p
        }).collect();
        let mut timings = Vec::with_capacity(iters);
        for obj_dir in &obj_dirs_b {
            let t = Instant::now();
            tokio::fs::write(obj_dir.join("shard.dat"), &data).await.unwrap();
            tokio::fs::write(obj_dir.join("meta.json"), &meta_pretty).await.unwrap();
            timings.push(t.elapsed());
        }
        report_row(size, "B: file_tier_no_mkdir", iters, &mut timings);

        // C: pool_serial
        let dir_c = base.path().join("c");
        let pool = WriteSlotPool::new(&dir_c, iters as u32).await.unwrap();
        let mut timings = Vec::with_capacity(iters);
        for _ in 0..iters {
            let t = Instant::now();
            let mut slot = pool.try_pop().unwrap();
            slot.data_file.write_all(&data).await.unwrap();
            slot.meta_file.write_all(&meta_pretty).await.unwrap();
            drop(slot);
            timings.push(t.elapsed());
        }
        report_row(size, "C: pool_serial", iters, &mut timings);

        // D: pool_join
        let dir_d = base.path().join("d");
        let pool = WriteSlotPool::new(&dir_d, iters as u32).await.unwrap();
        let mut timings = Vec::with_capacity(iters);
        for _ in 0..iters {
            let t = Instant::now();
            let WriteSlot { mut data_file, mut meta_file, .. } = pool.try_pop().unwrap();
            tokio::try_join!(data_file.write_all(&data), meta_file.write_all(&meta_pretty)).unwrap();
            drop(data_file);
            drop(meta_file);
            timings.push(t.elapsed());
        }
        report_row(size, "D: pool_join", iters, &mut timings);

        // E: pool_join_compact
        let dir_e = base.path().join("e");
        let pool = WriteSlotPool::new(&dir_e, iters as u32).await.unwrap();
        let mut timings = Vec::with_capacity(iters);
        for _ in 0..iters {
            let t = Instant::now();
            let WriteSlot { mut data_file, mut meta_file, .. } = pool.try_pop().unwrap();
            tokio::try_join!(data_file.write_all(&data), meta_file.write_all(&meta_compact)).unwrap();
            drop(data_file);
            drop(meta_file);
            timings.push(t.elapsed());
        }
        report_row(size, "E: pool_join_compact", iters, &mut timings);

        // F: pool_sync_small (4KB only)
        if size <= 4096 {
            let dir_f = base.path().join("f");
            let pool = WriteSlotPool::new(&dir_f, iters as u32).await.unwrap();
            let mut std_pairs: Vec<(std::fs::File, std::fs::File)> = Vec::with_capacity(iters);
            for _ in 0..iters {
                let WriteSlot { data_file, meta_file, .. } = pool.try_pop().unwrap();
                std_pairs.push((data_file.into_std().await, meta_file.into_std().await));
            }
            let mut timings = Vec::with_capacity(iters);
            for (mut data_std, mut meta_std) in std_pairs {
                use std::io::Write;
                let t = Instant::now();
                data_std.write_all(&data).unwrap();
                meta_std.write_all(&meta_compact).unwrap();
                timings.push(t.elapsed());
            }
            report_row(size, "F: pool_sync_small", iters, &mut timings);
        } else {
            eprintln!("  {:<8} {:<26} {:>5}  {}", human(size), "F: pool_sync_small", "-", "(N/A above 4KB)");
        }
    }
    eprintln!();
}

// ========================================================================
// L1.5: JSON serializer comparison
// ========================================================================

async fn bench_l1_5_json_serializers() {
    let meta = make_meta_file();
    let iters = 100_000;

    eprintln!("\n=== Pool L1.5: JSON serializer comparison ===\n");
    eprintln!("  payload: ObjectMetaFile with 1 version");
    eprintln!("  iterations: {}", iters);
    eprintln!();

    let json_serde_pretty = serde_json::to_vec_pretty(&meta).unwrap();
    let json_serde = serde_json::to_vec(&meta).unwrap();
    let json_simd = simd_json::serde::to_vec(&meta).unwrap();
    let json_sonic = sonic_rs::to_vec(&meta).unwrap();

    for (label, bytes) in [("serde_pretty", &json_serde_pretty), ("serde", &json_serde), ("simd-json", &json_simd), ("sonic-rs", &json_sonic)] {
        let parsed: ObjectMetaFile = serde_json::from_slice(bytes).unwrap_or_else(|e| panic!("{} parse failed: {}", label, e));
        assert_eq!(parsed, meta, "{} round-trip changed struct", label);
    }

    eprintln!("  output sizes:");
    eprintln!("    A serde_json::to_vec_pretty:  {}", json_serde_pretty.len());
    eprintln!("    B serde_json::to_vec:         {}", json_serde.len());
    eprintln!("    C simd-json::serde::to_vec:   {}", json_simd.len());
    eprintln!("    D sonic-rs::to_vec:           {}", json_sonic.len());
    eprintln!();

    for _ in 0..10_000 { let _ = serde_json::to_vec(&meta).unwrap(); }

    let mut s = Vec::with_capacity(iters);
    for _ in 0..iters { let t = Instant::now(); let _ = serde_json::to_vec_pretty(&meta).unwrap(); s.push(t.elapsed()); }
    report_ns("A: serde_json::to_vec_pretty", iters, s);

    let mut s = Vec::with_capacity(iters);
    for _ in 0..iters { let t = Instant::now(); let _ = serde_json::to_vec(&meta).unwrap(); s.push(t.elapsed()); }
    report_ns("B: serde_json::to_vec", iters, s);

    let mut s = Vec::with_capacity(iters);
    for _ in 0..iters { let t = Instant::now(); let _ = simd_json::serde::to_vec(&meta).unwrap(); s.push(t.elapsed()); }
    report_ns("C: simd-json::serde::to_vec", iters, s);

    let mut s = Vec::with_capacity(iters);
    for _ in 0..iters { let t = Instant::now(); let _ = sonic_rs::to_vec(&meta).unwrap(); s.push(t.elapsed()); }
    report_ns("D: sonic-rs::to_vec", iters, s);

    eprintln!();
}

// ========================================================================
// L2: rename worker drain rate
// ========================================================================

async fn bench_l2_worker_drain() {
    async fn fill_pool_and_build_requests(
        pool: &Arc<WriteSlotPool>, n: usize, dest_root: &std::path::Path, precreate: bool,
    ) -> Vec<RenameRequest> {
        let mut requests = Vec::with_capacity(n);
        for i in 0..n {
            let mut slot = pool.try_pop().expect("pool depth must match n");
            slot.data_file.write_all(b"x").await.unwrap();
            slot.meta_file.write_all(b"y").await.unwrap();
            let dest_dir = dest_root.join(format!("obj_{}", i));
            if precreate { tokio::fs::create_dir_all(&dest_dir).await.unwrap(); }
            requests.push(RenameRequest {
                slot, bucket: "bench".into(), key: format!("obj_{}", i),
                dest_dir: dest_dir.clone(), data_dest: dest_dir.join("shard.dat"), meta_dest: dest_dir.join("meta.json"),
            });
        }
        requests
    }

    eprintln!("\n=== Pool L2: rename worker drain rate ===\n");

    // Scenario 1: cold drain
    eprintln!("  -- Scenario 1: cold drain --");
    for &n in &[32usize, 256, 1024] {
        let tmp = super::stats::make_tmp_dir();
        let pool = WriteSlotPool::new(&tmp.path().join("pool"), n as u32).await.unwrap();
        let requests = fill_pool_and_build_requests(&pool, n, &tmp.path().join("dest"), false).await;
        let (tx, rx) = tokio::sync::mpsc::channel::<RenameRequest>(n + 16);
        let (_stx, srx) = tokio::sync::watch::channel(false);
        let pc = Arc::clone(&pool);
        let worker = tokio::spawn(async move { run_rename_worker(pc, None, rx, srx).await; });
        for req in requests { tx.send(req).await.unwrap(); }
        drop(tx);
        let t = Instant::now();
        worker.await.unwrap();
        let elapsed = t.elapsed();
        eprintln!("  N={:<5}  workers=1   drain {:>9.2}ms   {:>7.0} ops/sec   {:>5.0}us/op",
            n, elapsed.as_secs_f64()*1000.0, n as f64/elapsed.as_secs_f64(), elapsed.as_secs_f64()*1_000_000.0/n as f64);
    }

    // Scenario 2: pre-created dest dirs
    eprintln!();
    eprintln!("  -- Scenario 2: pre-created dest dirs --");
    {
        let n = 256usize;
        let tmp = super::stats::make_tmp_dir();
        let pool = WriteSlotPool::new(&tmp.path().join("pool"), n as u32).await.unwrap();
        let requests = fill_pool_and_build_requests(&pool, n, &tmp.path().join("dest"), true).await;
        let (tx, rx) = tokio::sync::mpsc::channel::<RenameRequest>(n + 16);
        let (_stx, srx) = tokio::sync::watch::channel(false);
        let pc = Arc::clone(&pool);
        let worker = tokio::spawn(async move { run_rename_worker(pc, None, rx, srx).await; });
        for req in requests { tx.send(req).await.unwrap(); }
        drop(tx);
        let t = Instant::now();
        worker.await.unwrap();
        let elapsed = t.elapsed();
        eprintln!("  N={:<5}  workers=1   drain {:>9.2}ms   {:>7.0} ops/sec   {:>5.0}us/op",
            n, elapsed.as_secs_f64()*1000.0, n as f64/elapsed.as_secs_f64(), elapsed.as_secs_f64()*1_000_000.0/n as f64);
    }

    // Scenario 3: steady-state
    eprintln!();
    eprintln!("  -- Scenario 3: steady state --");
    for &target_rate in &[500u64, 1000, 1500, 5000] {
        let n_total = 1000usize;
        let tmp = super::stats::make_tmp_dir();
        let pool = WriteSlotPool::new(&tmp.path().join("pool"), 64).await.unwrap();
        let (tx, rx) = tokio::sync::mpsc::channel::<RenameRequest>(2048);
        let (_stx, srx) = tokio::sync::watch::channel(false);
        let pw = Arc::clone(&pool);
        let worker = tokio::spawn(async move { run_rename_worker(pw, None, rx, srx).await; });
        let pp = Arc::clone(&pool);
        let dest_root = tmp.path().join("dest");
        let interval_ns = 1_000_000_000u64 / target_rate;
        let producer = tokio::spawn(async move {
            let mut next_send = Instant::now();
            for i in 0..n_total {
                let slot = loop { if let Some(s) = pp.try_pop() { break s; } tokio::task::yield_now().await; };
                let mut slot = slot;
                slot.data_file.write_all(b"x").await.unwrap();
                slot.meta_file.write_all(b"y").await.unwrap();
                let dest_dir = dest_root.join(format!("obj_{}", i));
                let req = RenameRequest { slot, bucket: "bench".into(), key: format!("obj_{}", i),
                    dest_dir: dest_dir.clone(), data_dest: dest_dir.join("shard.dat"), meta_dest: dest_dir.join("meta.json") };
                tx.send(req).await.unwrap();
                next_send += Duration::from_nanos(interval_ns);
                let now = Instant::now();
                if now < next_send { tokio::time::sleep(next_send - now).await; } else { next_send = now; }
            }
            drop(tx);
        });
        let t = Instant::now();
        producer.await.unwrap();
        worker.await.unwrap();
        let elapsed = t.elapsed();
        let actual_rate = n_total as f64 / elapsed.as_secs_f64();
        let saturated = actual_rate < target_rate as f64 * 0.9;
        eprintln!("  target {:>5}/sec   actual {:>5.0}/sec   total {:>6}ms   {}",
            target_rate, actual_rate, elapsed.as_secs_f64()*1000.0, if saturated { "SATURATED" } else { "ok" });
    }

    // Scenario 4: parallel workers
    eprintln!();
    eprintln!("  -- Scenario 4: parallel workers --");
    for &n_workers in &[1usize, 2, 4] {
        let n = 256usize;
        let tmp = super::stats::make_tmp_dir();
        let pool = WriteSlotPool::new(&tmp.path().join("pool"), n as u32).await.unwrap();
        let requests = fill_pool_and_build_requests(&pool, n, &tmp.path().join("dest"), false).await;
        let (tx, rx) = tokio::sync::mpsc::channel::<RenameRequest>(n + 16);
        let rx = Arc::new(tokio::sync::Mutex::new(rx));
        for req in requests { tx.send(req).await.unwrap(); }
        drop(tx);
        let pa = Arc::clone(&pool);
        let mut handles = Vec::with_capacity(n_workers);
        let t = Instant::now();
        for _ in 0..n_workers {
            let p = Arc::clone(&pa);
            let r = Arc::clone(&rx);
            handles.push(tokio::spawn(async move {
                loop {
                    let msg = { let mut guard = r.lock().await; guard.recv().await };
                    let Some(req) = msg else { break; };
                    if let Err(e) = process_rename_request(&p, req).await { eprintln!("rename failed: {}", e); }
                }
            }));
        }
        for h in handles { h.await.unwrap(); }
        let elapsed = t.elapsed();
        eprintln!("  workers={}   drain {:>8.2}ms   {:>7.0} ops/sec", n_workers, elapsed.as_secs_f64()*1000.0, n as f64/elapsed.as_secs_f64());
    }
    eprintln!();
}

// ========================================================================
// L3: integrated PUT (write_shard with pool vs file tier)
// ========================================================================

async fn bench_l3_integrated_put() {
    let meta = make_meta();

    eprintln!("\n=== Pool L3: integrated PUT (LocalVolume::write_shard) ===\n");
    eprintln!("  {:<8} {:<26} {:>5}  {:>9}  {:>9}  {:>9}  {:>14}", "SIZE", "STRATEGY", "ITERS", "AVG", "p50", "p99", "THROUGHPUT");

    let sizes: &[(usize, usize)] = &[(4096, 10_000), (65536, 1_000), (MB, 1_000), (10*MB, 100), (100*MB, 100)];

    for &(size, iters) in sizes {
        let data = vec![0x42u8; size];
        eprintln!();

        // file tier baseline
        {
            let tmp = super::stats::make_tmp_dir();
            let disk = LocalVolume::new(tmp.path()).unwrap();
            let mut timings = Vec::with_capacity(iters);
            for i in 0..iters {
                let t = Instant::now();
                disk.write_shard("bench", &format!("obj_{}", i), &data, &meta).await.unwrap();
                timings.push(t.elapsed());
            }
            report_row(size, "file_tier (baseline)", iters, timings.as_mut());
        }

        // pool
        {
            let tmp = super::stats::make_tmp_dir();
            let mut disk = LocalVolume::new(tmp.path()).unwrap();
            disk.enable_write_pool(64).await.unwrap();
            for i in 0..4 { disk.write_shard("bench", &format!("warm_{}", i), &data, &meta).await.unwrap(); }
            disk.drain_pending().await;
            let mut timings = Vec::with_capacity(iters);
            for i in 0..iters {
                let t = Instant::now();
                disk.write_shard("bench", &format!("obj_{}", i), &data, &meta).await.unwrap();
                timings.push(t.elapsed());
                if (i + 1) % 32 == 0 { disk.drain_pending().await; }
            }
            disk.drain_pending().await;
            report_row(size, "pool (Phase 4)", iters, timings.as_mut());
        }
    }
    eprintln!();
}

// ========================================================================
// L3.5: 4KB integration overhead breakdown
// ========================================================================

async fn bench_l3_5_integration_breakdown() {
    let meta = make_meta();
    let iters = 100_000;
    let bucket = "bench";
    let key = "obj.bin";
    let data_4kb = vec![0x42u8; 4096];

    eprintln!("\n=== Pool L3.5: 4KB integration overhead breakdown ===\n");
    eprintln!("  iterations: {}", iters);
    eprintln!();

    let mut samples = Vec::with_capacity(iters);
    for _ in 0..iters {
        let t = Instant::now();
        pathing::validate_bucket_name(bucket).unwrap();
        pathing::validate_object_key(key).unwrap();
        samples.push(t.elapsed());
    }
    report_breakdown("1. validate_bucket_name + validate_object_key", samples);

    let bucket_str = bucket.to_string();
    let key_str = key.to_string();
    let mut samples = Vec::with_capacity(iters);
    for _ in 0..iters {
        let t = Instant::now();
        let mut version = meta.clone();
        version.is_latest = true;
        let _mf = ObjectMetaFile { bucket: bucket_str.clone(), key: key_str.clone(), versions: vec![version] };
        samples.push(t.elapsed());
    }
    report_breakdown("2. ObjectMetaFile construction (clone+alloc)", samples);

    let mut version = meta.clone();
    version.is_latest = true;
    let mf = ObjectMetaFile { bucket: bucket_str.clone(), key: key_str.clone(), versions: vec![version] };
    let mut samples = Vec::with_capacity(iters);
    for _ in 0..iters {
        let t = Instant::now();
        let _bytes = simd_json::serde::to_vec(&mf).unwrap();
        samples.push(t.elapsed());
    }
    report_breakdown("3. simd_json::serde::to_vec(&mf)", samples);

    let tmp = super::stats::make_tmp_dir();
    let pool_dir = tmp.path().join("pool_step4");
    let pool = WriteSlotPool::new(&pool_dir, 32).await.unwrap();
    let mut samples = Vec::with_capacity(iters);
    for _ in 0..iters {
        let t = Instant::now();
        let slot = pool.try_pop().unwrap();
        samples.push(t.elapsed());
        let slot_id = slot.slot_id;
        drop(slot);
        pool.replenish_slot(slot_id).await.unwrap();
    }
    report_breakdown("4. pool.try_pop()", samples);

    let pool_dir = tmp.path().join("pool_step5");
    let pool = WriteSlotPool::new(&pool_dir, 32).await.unwrap();
    let mut samples = Vec::with_capacity(iters);
    for _ in 0..iters {
        let slot = pool.try_pop().unwrap();
        let t = Instant::now();
        let _slot_id = slot.slot_id;
        let _data_src = slot.data_path.clone();
        let _meta_src = slot.meta_path.clone();
        let WriteSlot { data_file: _, meta_file: _, .. } = slot;
        samples.push(t.elapsed());
        pool.replenish_slot(_slot_id).await.unwrap();
    }
    report_breakdown("5. WriteSlot destructure + 2x PathBuf clone", samples);

    let pool_dir = tmp.path().join("pool_step6");
    let pool = WriteSlotPool::new(&pool_dir, 32).await.unwrap();
    let meta_json = simd_json::serde::to_vec(&mf).unwrap();
    let mut samples = Vec::with_capacity(iters);
    for _ in 0..iters {
        let WriteSlot { slot_id, mut data_file, mut meta_file, .. } = pool.try_pop().unwrap();
        let t = Instant::now();
        tokio::try_join!(data_file.write_all(&data_4kb), meta_file.write_all(&meta_json)).unwrap();
        samples.push(t.elapsed());
        drop(data_file);
        drop(meta_file);
        pool.replenish_slot(slot_id).await.unwrap();
    }
    report_breakdown("6. tokio::try_join!(data_write, meta_write)", samples);

    let root = tmp.path();
    let mut samples = Vec::with_capacity(iters);
    for _ in 0..iters {
        let t = Instant::now();
        let _dest_dir = pathing::object_dir(root, bucket, key).unwrap();
        let _data_dest = pathing::object_shard_path(root, bucket, key).unwrap();
        let _meta_dest = pathing::object_meta_path(root, bucket, key).unwrap();
        samples.push(t.elapsed());
    }
    report_breakdown("7. object_dir + shard_path + meta_path", samples);
    eprintln!();
}

// ========================================================================
// L3.6: write_shard pool-branch in-context section timing
// ========================================================================

async fn bench_l3_6_write_shard_breakdown() {
    let tmp = super::stats::make_tmp_dir();
    let mut disk_owned = LocalVolume::new(tmp.path()).unwrap();
    disk_owned.enable_write_pool(64).await.unwrap();
    disk_owned.make_bucket("bench").await.unwrap();

    let pool_dir = tmp.path().join(".bench_l3_6_pool");
    let pool = WriteSlotPool::new(&pool_dir, 64).await.unwrap();
    let pending: abixio::storage::write_slot_pool::PendingRenames = Arc::new(dashmap::DashMap::new());
    let (tx, rx) = tokio::sync::mpsc::channel::<RenameRequest>(256);
    let (_stx, srx) = tokio::sync::watch::channel(false);
    let pfw = Arc::clone(&pool);
    let pfw2 = Arc::clone(&pending);
    tokio::spawn(async move { run_rename_worker(pfw, Some(pfw2), rx, srx).await; });

    tokio::fs::create_dir_all(tmp.path().join("bench")).await.unwrap();

    let bucket = "bench";
    let meta = make_meta();
    let data = vec![0x42u8; 4096];
    let iters = 100_000;
    let warmup = 1_000;

    let mut s_validate = Vec::with_capacity(iters);
    let mut s_try_pop = Vec::with_capacity(iters);
    let mut s_meta_build = Vec::with_capacity(iters);
    let mut s_writes = Vec::with_capacity(iters);
    let mut s_object_dir = Vec::with_capacity(iters);
    let mut s_join_paths = Vec::with_capacity(iters);
    let mut s_pending_insert = Vec::with_capacity(iters);
    let mut s_rename_req = Vec::with_capacity(iters);
    let mut s_send = Vec::with_capacity(iters);
    let mut s_total = Vec::with_capacity(iters);

    eprintln!("\n=== Pool L3.6: write_shard pool-branch breakdown ===\n");
    eprintln!("  iterations: {} (after {} warmup)", iters, warmup);
    eprintln!();

    let drain = |pool: &Arc<WriteSlotPool>| {
        let pool = Arc::clone(pool);
        async move { while pool.available() < 32 { tokio::task::yield_now().await; } }
    };

    for i in 0..(iters + warmup) {
        let key = format!("obj_{}", i);
        if pool.available() < 4 { drain(&pool).await; }

        let t_total = Instant::now();

        let t = Instant::now();
        pathing::validate_bucket_name(bucket).unwrap();
        pathing::validate_object_key(&key).unwrap();
        let d_validate = t.elapsed();

        let t = Instant::now();
        let slot = pool.try_pop().unwrap();
        let d_try_pop = t.elapsed();

        let t = Instant::now();
        let mut version = meta.clone();
        version.is_latest = true;
        let mf = ObjectMetaFile { bucket: bucket.into(), key: key.clone(), versions: vec![version] };
        let meta_json = simd_json::serde::to_vec(&mf).unwrap();
        let d_meta_build = t.elapsed();

        let mut slot = slot;
        let t = Instant::now();
        tokio::try_join!(slot.data_file.write_all(&data), slot.meta_file.write_all(&meta_json)).unwrap();
        let d_writes = t.elapsed();

        let t = Instant::now();
        let dest_dir = pathing::object_dir(tmp.path(), bucket, &key).unwrap();
        let d_object_dir = t.elapsed();

        let t = Instant::now();
        let data_dest = dest_dir.join("shard.dat");
        let meta_dest = dest_dir.join("meta.json");
        let d_join_paths = t.elapsed();

        let t = Instant::now();
        let entry = PendingEntry { slot_id: slot.slot_id, data_path: slot.data_path.clone(), meta_path: slot.meta_path.clone(), data_len: data.len() as u64 };
        pending.insert((Arc::<str>::from(bucket), Arc::<str>::from(key.as_str())), entry);
        let d_pending_insert = t.elapsed();

        let t = Instant::now();
        let req = RenameRequest { slot, bucket: bucket.into(), key: key.clone(), dest_dir, data_dest, meta_dest };
        let d_rename_req = t.elapsed();

        let t = Instant::now();
        tx.send(req).await.unwrap();
        let d_send = t.elapsed();

        let d_total = t_total.elapsed();

        if i >= warmup {
            s_validate.push(d_validate);
            s_try_pop.push(d_try_pop);
            s_meta_build.push(d_meta_build);
            s_writes.push(d_writes);
            s_object_dir.push(d_object_dir);
            s_join_paths.push(d_join_paths);
            s_pending_insert.push(d_pending_insert);
            s_rename_req.push(d_rename_req);
            s_send.push(d_send);
            s_total.push(d_total);
        }
    }

    report_breakdown("1. validate (bucket + key)", s_validate.clone());
    report_breakdown("2. pool.try_pop()", s_try_pop.clone());
    report_breakdown("3. meta clone + ObjectMetaFile + simd-json", s_meta_build.clone());
    report_breakdown("4. tokio::try_join!(data_write, meta_write)", s_writes.clone());
    report_breakdown("5. pathing::object_dir(...)", s_object_dir.clone());
    report_breakdown("6. data_dest + meta_dest joins", s_join_paths.clone());
    report_breakdown("7. PendingEntry build + DashMap insert", s_pending_insert.clone());
    report_breakdown("8. RenameRequest construction (move slot in)", s_rename_req.clone());
    report_breakdown("9. tx.send(req).await", s_send.clone());
    eprintln!();
    report_breakdown("--- TOTAL (sum of timings)", s_total.clone());

    let sum_of_medians: u128 = {
        let mut all: [&mut Vec<Duration>; 9] = [
            &mut s_validate, &mut s_try_pop, &mut s_meta_build, &mut s_writes,
            &mut s_object_dir, &mut s_join_paths, &mut s_pending_insert, &mut s_rename_req, &mut s_send,
        ];
        let mut sum = 0u128;
        for v in all.iter_mut() { v.sort(); sum += v[v.len() / 2].as_nanos(); }
        sum
    };
    s_total.sort();
    let total_p50 = s_total[s_total.len() / 2].as_nanos();
    eprintln!();
    eprintln!("  sum of step medians: {} ns", sum_of_medians);
    eprintln!("  measured total p50:  {} ns", total_p50);
    eprintln!("  unaccounted at p50:  {} ns ({:.0}%)",
        total_p50.saturating_sub(sum_of_medians),
        (total_p50.saturating_sub(sum_of_medians) as f64 / total_p50 as f64) * 100.0);
    eprintln!();
}
