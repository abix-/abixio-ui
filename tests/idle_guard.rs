//! Guard test: ensures no request_repaint() calls exist in the UI render path.
//!
//! The ONLY place request_repaint() should be called is inside async_op.rs,
//! where the background task wakes the UI after completing a network request.
//!
//! If this test fails, someone added a repaint call in the UI path, which
//! would cause continuous rendering and burn CPU while idle.

use std::fs;
use std::path::Path;

fn read_source(path: &str) -> String {
    let p = Path::new(env!("CARGO_MANIFEST_DIR")).join(path);
    fs::read_to_string(&p).unwrap_or_else(|e| panic!("failed to read {}: {}", p.display(), e))
}

fn count_repaint_calls(source: &str) -> usize {
    source
        .lines()
        .filter(|line| {
            let trimmed = line.trim();
            // skip comments
            if trimmed.starts_with("//") || trimmed.starts_with("///") {
                return false;
            }
            trimmed.contains("request_repaint")
        })
        .count()
}

#[test]
fn no_repaint_in_app_logic() {
    let source = read_source("src/app.rs");
    let count = count_repaint_calls(&source);
    assert_eq!(
        count, 0,
        "found {} request_repaint() calls in app.rs -- \
         background tasks handle repaint, UI must stay idle",
        count
    );
}

#[test]
fn no_repaint_in_views() {
    let views_dir = Path::new(env!("CARGO_MANIFEST_DIR")).join("src/views");
    for entry in fs::read_dir(&views_dir).unwrap() {
        let entry = entry.unwrap();
        let path = entry.path();
        if path.extension().is_some_and(|e| e == "rs") {
            let source = fs::read_to_string(&path).unwrap();
            let count = count_repaint_calls(&source);
            assert_eq!(
                count,
                0,
                "found {} request_repaint() calls in {} -- \
                 background tasks handle repaint, UI must stay idle",
                count,
                path.display()
            );
        }
    }
}

#[test]
fn async_op_has_exactly_one_repaint() {
    let source = read_source("src/async_op.rs");
    let count = count_repaint_calls(&source);
    assert_eq!(
        count, 1,
        "async_op.rs should have exactly 1 request_repaint() call \
         (in the background task completion handler), found {}",
        count
    );
}
