use std::path::PathBuf;
use std::sync::OnceLock;
use std::time::Duration;

use serde::{Deserialize, Serialize};

const MB: f64 = 1024.0 * 1024.0;

static BENCH_TMP_DIR: OnceLock<PathBuf> = OnceLock::new();

pub fn set_tmp_dir(dir: &str) {
    let _ = BENCH_TMP_DIR.set(PathBuf::from(dir));
}

pub fn make_tmp_dir() -> tempfile::TempDir {
    match BENCH_TMP_DIR.get() {
        Some(dir) => tempfile::TempDir::new_in(dir).unwrap(),
        None => tempfile::TempDir::new().unwrap(),
    }
}

pub fn make_tmp_dir_opt() -> Option<tempfile::TempDir> {
    match BENCH_TMP_DIR.get() {
        Some(dir) => tempfile::TempDir::new_in(dir).ok(),
        None => tempfile::TempDir::new().ok(),
    }
}

pub struct BenchResult {
    pub layer: String,
    pub op: String,
    pub size: usize,
    pub iters: usize,
    pub write_path: Option<String>,
    pub write_cache: Option<bool>,
    pub server: Option<String>,
    pub client: Option<String>,
    pub timings: Vec<Duration>,
}

#[derive(Serialize, Deserialize, Clone)]
pub struct JsonResult {
    pub layer: String,
    pub op: String,
    pub size: usize,
    pub iters: usize,
    pub write_path: Option<String>,
    pub write_cache: Option<bool>,
    pub server: Option<String>,
    pub client: Option<String>,
    pub p50_us: f64,
    pub p95_us: f64,
    pub p99_us: f64,
    pub ops_per_sec: f64,
    pub mbps: f64,
}

#[derive(Serialize, Deserialize)]
pub struct BenchReport {
    pub timestamp: String,
    pub git_commit: String,
    pub results: Vec<JsonResult>,
}

impl BenchResult {
    pub fn to_json(&self) -> JsonResult {
        let mut timings = self.timings.clone();
        let stats = Stats::from(&mut timings, self.size);
        JsonResult {
            layer: self.layer.clone(),
            op: self.op.clone(),
            size: self.size,
            iters: self.iters,
            write_path: self.write_path.clone(),
            write_cache: self.write_cache,
            server: self.server.clone(),
            client: self.client.clone(),
            p50_us: stats.p50_us,
            p95_us: stats.p95_us,
            p99_us: stats.p99_us,
            ops_per_sec: stats.ops_per_sec,
            mbps: stats.mbps,
        }
    }
}

pub struct Stats {
    pub p50_us: f64,
    pub p95_us: f64,
    pub p99_us: f64,
    pub ops_per_sec: f64,
    pub mbps: f64,
}

impl Stats {
    pub fn from(timings: &mut [Duration], size: usize) -> Self {
        timings.sort();
        let n = timings.len();
        let total: Duration = timings.iter().sum();
        let total_s = total.as_secs_f64();

        let p50_us = timings[n / 2].as_nanos() as f64 / 1000.0;
        let p95_us = timings[(n * 95) / 100].as_nanos() as f64 / 1000.0;
        let p99_us = timings[((n * 99) / 100).min(n - 1)].as_nanos() as f64 / 1000.0;
        let ops_per_sec = if total_s > 0.0 { n as f64 / total_s } else { 0.0 };
        let mbps = if total_s > 0.0 { (size * n) as f64 / total_s / MB } else { 0.0 };

        Self { p50_us, p95_us, p99_us, ops_per_sec, mbps }
    }
}

pub fn parse_size(s: &str) -> usize {
    let s = s.trim().to_uppercase();
    if let Some(n) = s.strip_suffix("GB") {
        n.parse::<usize>().unwrap_or(0) * 1024 * 1024 * 1024
    } else if let Some(n) = s.strip_suffix("MB") {
        n.parse::<usize>().unwrap_or(0) * 1024 * 1024
    } else if let Some(n) = s.strip_suffix("KB") {
        n.parse::<usize>().unwrap_or(0) * 1024
    } else {
        s.parse::<usize>().unwrap_or(0)
    }
}

/// 10GB total data per size tier, capped at 10k iterations for small objects.
pub fn iters_for_size(size: usize) -> usize {
    const TARGET_BYTES: usize = 10 * 1024 * 1024 * 1024; // 10GB
    (TARGET_BYTES / size).min(10_000).max(1)
}

pub fn human_size(size: usize) -> String {
    if size >= 1024 * 1024 * 1024 {
        format!("{}GB", size / (1024 * 1024 * 1024))
    } else if size >= 1024 * 1024 {
        format!("{}MB", size / (1024 * 1024))
    } else if size >= 1024 {
        format!("{}KB", size / 1024)
    } else {
        format!("{}B", size)
    }
}

fn format_latency(us: f64) -> String {
    if us < 1000.0 {
        format!("{:.0}us", us)
    } else if us < 1_000_000.0 {
        format!("{:.1}ms", us / 1000.0)
    } else {
        format!("{:.2}s", us / 1_000_000.0)
    }
}

pub fn print_results(results: &[BenchResult]) {
    if results.is_empty() {
        return;
    }

    // collect unique sizes in order of appearance
    let mut sizes: Vec<usize> = Vec::new();
    for r in results {
        if !sizes.contains(&r.size) {
            sizes.push(r.size);
        }
    }

    for &size in &sizes {
        let group: Vec<&BenchResult> = results.iter().filter(|r| r.size == size).collect();
        if group.is_empty() {
            continue;
        }

        eprintln!();
        let size_label = if size == 0 { "meta".to_string() } else { human_size(size) };
        eprintln!("--- {} ---", size_label);
        eprintln!(
            "{:<5} {:<20} {:>6} {:>10} {:>10} {:>10} {:>10} {:>10}",
            "LAYER", "OP", "ITERS", "p50", "p95", "p99", "ops/s", "MB/s"
        );
        eprintln!("{}", "-".repeat(91));

        for r in &group {
            let mut timings = r.timings.clone();
            let stats = Stats::from(&mut timings, r.size);

            let mut label = r.op.clone();
            if let Some(wp) = &r.write_path {
                label = format!("{} ({}{})", label, wp,
                    if let Some(wc) = r.write_cache {
                        if wc { "+wc" } else { "" }
                    } else { "" }
                );
            }
            if let Some(srv) = &r.server {
                label = format!("{} [{}]", label, srv);
            }
            if let Some(cli) = &r.client {
                label = format!("{} {}", label, cli);
            }

            eprintln!(
                "{:<5} {:<20} {:>6} {:>10} {:>10} {:>10} {:>10.0} {:>8.1}",
                r.layer,
                label,
                r.iters,
                format_latency(stats.p50_us),
                format_latency(stats.p95_us),
                format_latency(stats.p99_us),
                stats.ops_per_sec,
                stats.mbps,
            );
        }
    }
    eprintln!();
}

fn git_commit() -> String {
    std::process::Command::new("git")
        .args(["rev-parse", "--short", "HEAD"])
        .output()
        .ok()
        .and_then(|o| String::from_utf8(o.stdout).ok())
        .map(|s| s.trim().to_string())
        .unwrap_or_else(|| "unknown".to_string())
}

fn timestamp_now() -> String {
    let d = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .unwrap_or_default();
    let secs = d.as_secs();
    let days = secs / 86400;
    let rem = secs % 86400;
    let h = rem / 3600;
    let m = (rem % 3600) / 60;
    let s = rem % 60;
    let mut y = 1970u64;
    let mut remaining_days = days;
    loop {
        let ydays = if y % 4 == 0 && (y % 100 != 0 || y % 400 == 0) { 366 } else { 365 };
        if remaining_days < ydays { break; }
        remaining_days -= ydays;
        y += 1;
    }
    let leap = y % 4 == 0 && (y % 100 != 0 || y % 400 == 0);
    let mdays = [31, if leap {29} else {28}, 31, 30, 31, 30, 31, 31, 30, 31, 30, 31];
    let mut mo = 0u64;
    for md in mdays {
        if remaining_days < md { break; }
        remaining_days -= md;
        mo += 1;
    }
    format!("{:04}-{:02}-{:02}T{:02}-{:02}-{:02}Z", y, mo + 1, remaining_days + 1, h, m, s)
}

pub fn save_json(results: &[BenchResult], path: &str) {
    let json_results: Vec<JsonResult> = results.iter().map(|r| r.to_json()).collect();
    let report = BenchReport {
        timestamp: timestamp_now(),
        git_commit: git_commit(),
        results: json_results,
    };
    if let Ok(json) = serde_json::to_string_pretty(&report) {
        if let Err(e) = std::fs::write(path, &json) {
            eprintln!("failed to save JSON: {}", e);
        } else {
            eprintln!("saved: {}", path);
        }
    }
}

pub fn compare_baseline(results: &[BenchResult], baseline_path: &str) {
    let baseline_json = match std::fs::read_to_string(baseline_path) {
        Ok(s) => s,
        Err(e) => { eprintln!("failed to read baseline {}: {}", baseline_path, e); return; }
    };
    let baseline: BenchReport = match serde_json::from_str(&baseline_json) {
        Ok(b) => b,
        Err(e) => { eprintln!("failed to parse baseline: {}", e); return; }
    };

    eprintln!();
    eprintln!("comparing against: {} ({})", baseline_path, baseline.git_commit);
    eprintln!();
    eprintln!("  {:<5} {:<20} {:>6} {:>12} {:>12} {:>8}",
        "LAYER", "OP", "SIZE", "BASELINE", "CURRENT", "DELTA");
    eprintln!("  {}", "-".repeat(70));

    for r in results {
        let cur = r.to_json();
        if let Some(base) = baseline.results.iter().find(|b|
            b.layer == cur.layer && b.op == cur.op && b.size == cur.size
            && b.write_path == cur.write_path && b.write_cache == cur.write_cache
            && b.server == cur.server && b.client == cur.client
        ) {
            if base.mbps > 0.0 && cur.mbps > 0.0 {
                let delta = (cur.mbps - base.mbps) / base.mbps * 100.0;
                let flag = if delta < -5.0 { " REGRESSION" }
                    else if delta > 5.0 { " FASTER" }
                    else { "" };
                eprintln!("  {:<5} {:<20} {:>6} {:>9.1} MB/s {:>9.1} MB/s {:>+7.1}%{}",
                    cur.layer, cur.op, human_size(cur.size),
                    base.mbps, cur.mbps, delta, flag);
            }
        }
    }
    eprintln!();
}
