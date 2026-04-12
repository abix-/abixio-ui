use super::stats::BenchResult;

pub async fn run(
    _sizes: &[usize],
    _write_path: &str,
    _write_cache: bool,
) -> Vec<BenchResult> {
    eprintln!("--- L3: Storage pipeline (not yet implemented) ---");
    Vec::new()
}
