mod l1_http;
mod l2_s3proto;
mod l3_storage;
mod l4_compute;
mod l5_disk;
mod l6_s3storage;
mod l7_e2e;
pub mod stats;
pub mod tls;
pub mod servers;
pub mod clients;

use clap::Parser;
use stats::{parse_size, print_results, save_json, compare_baseline};

#[derive(Parser)]
pub struct BenchArgs {
    /// Object sizes to test (comma-separated)
    #[arg(long, default_value = "4KB,64KB,10MB,100MB,1GB", value_delimiter = ',')]
    pub sizes: Vec<String>,

    /// Stack layers to test (comma-separated)
    #[arg(long, default_value = "L1,L2,L3,L4,L5,L6,L7", value_delimiter = ',')]
    pub layers: Vec<String>,

    /// Write paths to test (comma-separated). Default is `wal` --
    /// the production small-object path. Pass `file,wal` to run
    /// ablation rows for the file-only tier.
    #[arg(long, default_value = "wal", value_delimiter = ',')]
    pub write_paths: Vec<String>,

    /// Write cache: on, off, or both. Default `on` matches the
    /// production stack. Pass `both` for ablation runs.
    #[arg(long, default_value = "on")]
    pub write_cache: String,

    /// Read cache: on, off, or both. Applies to abixio only.
    /// Default `on` matches the production stack. Pass `both` for
    /// ablation runs. `off` spawns abixio with `--read-cache 0`.
    #[arg(long, default_value = "on")]
    pub read_cache: String,

    /// Servers to benchmark (comma-separated)
    #[arg(long, default_value = "abixio,rustfs,minio", value_delimiter = ',')]
    pub servers: Vec<String>,

    /// S3 clients to use (comma-separated)
    #[arg(long, default_value = "sdk,aws-cli,rclone", value_delimiter = ',')]
    pub clients: Vec<String>,

    /// Operations to test (comma-separated)
    #[arg(long, default_value = "PUT,GET,HEAD,LIST,DELETE", value_delimiter = ',')]
    pub ops: Vec<String>,

    /// Override iteration count (default: auto-scaled by size)
    #[arg(long)]
    pub iters: Option<usize>,

    /// TLS mode: on (HTTPS), off (plain HTTP), or both
    #[arg(long, default_value = "on")]
    pub tls: String,

    /// Directory for benchmark result JSON files
    #[arg(long, default_value = r"C:\code\abixio-ui\bench-results")]
    pub output_dir: String,

    /// Save results to a specific JSON file (overrides output-dir)
    #[arg(long)]
    pub output: Option<String>,

    /// Compare against a baseline JSON file
    #[arg(long)]
    pub baseline: Option<String>,

    /// Number of disks to test (comma-separated)
    #[arg(long, default_value = "1", value_delimiter = ',')]
    pub disks: Vec<usize>,

    /// Temp directory for benchmark files (must be Defender-excluded for accurate results)
    #[arg(long, default_value = r"C:\code\bench-tmp")]
    pub tmp_dir: Option<String>,
}

fn has(list: &[String], val: &str) -> bool {
    list.iter().any(|s| s.eq_ignore_ascii_case(val))
}

fn clean_tmp_dir(dir: &str) {
    let path = std::path::Path::new(dir);
    let Ok(entries) = std::fs::read_dir(path) else { return };
    let mut freed: u64 = 0;
    let mut removed = 0usize;
    for entry in entries.flatten() {
        let p = entry.path();
        let size = dir_size(&p).unwrap_or(0);
        let res = if p.is_dir() {
            std::fs::remove_dir_all(&p)
        } else {
            std::fs::remove_file(&p)
        };
        if res.is_ok() {
            freed += size;
            removed += 1;
        } else if let Err(e) = res {
            eprintln!("  warn: failed to remove {}: {}", p.display(), e);
        }
    }
    if removed > 0 {
        eprintln!("  cleaned {} leftover entries ({:.1} GB) from {}",
            removed, freed as f64 / (1024.0 * 1024.0 * 1024.0), dir);
    }
}

fn estimate_required_bytes(args: &BenchArgs) -> u64 {
    let max_size = args
        .sizes
        .iter()
        .map(|s| parse_size(s) as u64)
        .max()
        .unwrap_or(0);
    // Reserve 20x the largest object size, floored at 4 GB.
    // Rationale: L3 at 1GB x ~5 iters with 2 write-path configs both alive
    // intermittently, plus shard + meta overhead. L7 server processes also
    // keep their own temp trees alive. 20x covers the worst case we've seen.
    let reserve = max_size.saturating_mul(20);
    reserve.max(4 * 1024 * 1024 * 1024)
}

#[cfg(windows)]
fn free_bytes(path: &std::path::Path) -> std::io::Result<u64> {
    use std::os::windows::ffi::OsStrExt;
    use windows_sys::Win32::Storage::FileSystem::GetDiskFreeSpaceExW;

    let wide: Vec<u16> = path.as_os_str().encode_wide().chain(std::iter::once(0)).collect();
    let mut free_avail: u64 = 0;
    let mut total: u64 = 0;
    let mut free_total: u64 = 0;
    let ok = unsafe {
        GetDiskFreeSpaceExW(
            wide.as_ptr(),
            &mut free_avail,
            &mut total,
            &mut free_total,
        )
    };
    if ok == 0 {
        Err(std::io::Error::last_os_error())
    } else {
        Ok(free_avail)
    }
}

#[cfg(not(windows))]
fn free_bytes(_path: &std::path::Path) -> std::io::Result<u64> {
    Ok(u64::MAX)
}

fn check_free_space(dir: &str, args: &BenchArgs) {
    let path = std::path::Path::new(dir);
    let required = estimate_required_bytes(args);
    let available = match free_bytes(path) {
        Ok(b) => b,
        Err(e) => {
            eprintln!("  warn: could not query free space on {}: {}", dir, e);
            return;
        }
    };
    let gb = 1024.0 * 1024.0 * 1024.0;
    eprintln!(
        "  free space:  {:.1} GB available, {:.1} GB required (max-size x 20, floor 4 GB)",
        available as f64 / gb,
        required as f64 / gb,
    );
    if available < required {
        eprintln!(
            "\nabort: not enough free space on {} for requested size set.\n\
             available {:.1} GB < required {:.1} GB.\n\
             free up space, or drop the largest --sizes entry.",
            dir,
            available as f64 / gb,
            required as f64 / gb,
        );
        std::process::exit(1);
    }
}

fn dir_size(path: &std::path::Path) -> std::io::Result<u64> {
    let md = std::fs::symlink_metadata(path)?;
    if md.is_file() {
        return Ok(md.len());
    }
    let mut total = 0u64;
    let mut stack = vec![path.to_path_buf()];
    while let Some(p) = stack.pop() {
        let Ok(entries) = std::fs::read_dir(&p) else { continue };
        for entry in entries.flatten() {
            let Ok(md) = entry.metadata() else { continue };
            if md.is_dir() {
                stack.push(entry.path());
            } else {
                total = total.saturating_add(md.len());
            }
        }
    }
    Ok(total)
}

/// Generate write path x write cache combinations from CLI flags.
fn write_configs(write_paths: &[String], write_cache: &str) -> Vec<(String, bool)> {
    let cache_states: Vec<bool> = match write_cache.to_lowercase().as_str() {
        "on" => vec![true],
        "off" => vec![false],
        _ => vec![false, true], // "both" or default
    };

    let mut configs = Vec::new();
    for wp in write_paths {
        for &wc in &cache_states {
            configs.push((wp.clone(), wc));
        }
    }
    configs
}

/// Parse the `--read-cache` flag into the concrete states to run.
pub fn read_cache_states(flag: &str) -> Vec<bool> {
    match flag.to_lowercase().as_str() {
        "on" => vec![true],
        "off" => vec![false],
        _ => vec![false, true], // "both" or default
    }
}

pub async fn run(args: BenchArgs) {
    if let Some(dir) = &args.tmp_dir {
        clean_tmp_dir(dir);
        std::fs::create_dir_all(dir).ok();
        stats::set_tmp_dir(dir);
        check_free_space(dir, &args);
    }

    let sizes: Vec<usize> = args.sizes.iter().map(|s| parse_size(s)).collect();
    let mut results = Vec::new();

    eprintln!("abixio-ui bench");
    if let Some(dir) = &args.tmp_dir {
        eprintln!("  tmp-dir:     {}", dir);
    }
    eprintln!("  disks:       {:?}", args.disks);
    eprintln!("  sizes:       {:?}", args.sizes);
    eprintln!("  layers:      {:?}", args.layers);
    eprintln!("  write-paths: {:?}", args.write_paths);
    eprintln!("  write-cache: {}", args.write_cache);
    eprintln!("  read-cache:  {}", args.read_cache);
    eprintln!("  servers:     {:?}", args.servers);
    eprintln!("  clients:     {:?}", args.clients);
    eprintln!("  ops:         {:?}", args.ops);
    eprintln!();

    if has(&args.layers, "L1") {
        results.extend(l1_http::run(&sizes, args.iters).await);
    }

    if has(&args.layers, "L2") {
        results.extend(l2_s3proto::run(&sizes, args.iters).await);
    }

    if has(&args.layers, "L3") {
        let rc_states = read_cache_states(&args.read_cache);
        for (wp, wc) in write_configs(&args.write_paths, &args.write_cache) {
            for &rc in &rc_states {
                results.extend(l3_storage::run(&sizes, &wp, wc, rc, args.iters, &args.disks).await);
            }
        }
    }

    if has(&args.layers, "L4") {
        results.extend(l4_compute::run(&sizes, args.iters).await);
    }

    if has(&args.layers, "L5") {
        results.extend(l5_disk::run(&sizes, args.iters).await);
    }

    if has(&args.layers, "L6") {
        for (wp, wc) in write_configs(&args.write_paths, &args.write_cache) {
            results.extend(l6_s3storage::run(&sizes, &wp, wc, args.iters).await);
        }
    }

    if has(&args.layers, "L7") {
        results.extend(l7_e2e::run(&sizes, &args).await);
    }

    print_results(&results);

    // always save results -- use --output if given, otherwise auto-generate in --output-dir
    let output_path = args.output.unwrap_or_else(|| {
        let secs = std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .unwrap_or_default()
            .as_secs();
        std::fs::create_dir_all(&args.output_dir).ok();
        format!("{}/{}.json", args.output_dir, secs)
    });
    save_json(&results, &output_path);

    if let Some(path) = &args.baseline {
        compare_baseline(&results, path);
    }
}
