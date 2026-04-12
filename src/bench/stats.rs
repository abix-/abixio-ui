use std::time::Duration;

const MB: f64 = 1024.0 * 1024.0;

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

pub fn iters_for_size(size: usize) -> usize {
    if size <= 4096 { 500 }
    else if size <= 65536 { 200 }
    else if size <= 10 * 1024 * 1024 { 50 }
    else if size <= 100 * 1024 * 1024 { 10 }
    else { 3 }
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
