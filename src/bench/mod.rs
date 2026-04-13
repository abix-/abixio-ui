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

    /// Write paths to test (comma-separated)
    #[arg(long, default_value = "file,wal", value_delimiter = ',')]
    pub write_paths: Vec<String>,

    /// Write cache: on, off, or both
    #[arg(long, default_value = "both")]
    pub write_cache: String,

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

pub async fn run(args: BenchArgs) {
    if let Some(dir) = &args.tmp_dir {
        std::fs::create_dir_all(dir).ok();
        stats::set_tmp_dir(dir);
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
        for (wp, wc) in write_configs(&args.write_paths, &args.write_cache) {
            results.extend(l3_storage::run(&sizes, &wp, wc, args.iters, &args.disks).await);
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
