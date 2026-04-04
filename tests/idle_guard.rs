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

fn count_pattern(source: &str, pattern: &str) -> usize {
    source
        .lines()
        .filter(|line| {
            let trimmed = line.trim();
            if trimmed.starts_with("//") || trimmed.starts_with("///") {
                return false;
            }
            trimmed.contains(pattern)
        })
        .count()
}

#[test]
fn no_spinners_anywhere() {
    // ui.spinner() forces continuous 60fps repainting for animation.
    // use static "Loading..." text instead.
    let forbidden = ["spinner()"];

    for pattern in &forbidden {
        for dir in &["src/app.rs", "src/views"] {
            let base = Path::new(env!("CARGO_MANIFEST_DIR")).join(dir);
            let files: Vec<_> = if base.is_file() {
                vec![base.clone()]
            } else {
                fs::read_dir(&base)
                    .unwrap()
                    .filter_map(|e| e.ok())
                    .map(|e| e.path())
                    .filter(|p| p.extension().is_some_and(|e| e == "rs"))
                    .collect()
            };

            for path in &files {
                let source = fs::read_to_string(path).unwrap();
                let count = count_pattern(&source, pattern);
                assert_eq!(
                    count,
                    0,
                    "found '{}' {} times in {} -- spinners cause continuous repainting. use static label instead",
                    pattern,
                    count,
                    path.display()
                );
            }
        }
    }
}

#[test]
fn no_animation_widgets() {
    // these widgets all trigger continuous repainting:
    let forbidden = [
        "spinner()",
        "progress_bar(",
        "animate_bool",
        "animate_value",
    ];

    let views_dir = Path::new(env!("CARGO_MANIFEST_DIR")).join("src/views");
    for entry in fs::read_dir(&views_dir).unwrap() {
        let entry = entry.unwrap();
        let path = entry.path();
        if path.extension().is_some_and(|e| e == "rs") {
            let source = fs::read_to_string(&path).unwrap();
            for pattern in &forbidden {
                let count = count_pattern(&source, pattern);
                assert_eq!(
                    count,
                    0,
                    "found animation widget '{}' {} times in {} -- \
                     animations cause continuous repainting",
                    pattern,
                    count,
                    path.display()
                );
            }
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
