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
    let source = read_source("src/app/mod.rs");
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
        for dir in &["src/app", "src/views"] {
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
    // audited egui 0.34.1 source: these widgets call request_repaint()
    // continuously (every frame), not just during interaction:
    //
    // widgets/spinner.rs:40      -- always animates
    // widgets/progress_bar.rs:138 -- animates fill
    // context.rs:3236,3262       -- animate_bool/animate_value
    //
    // conditional repainters (ok to use -- only repaint during interaction):
    // collapsing_header -- open/close transition only
    // scroll_area      -- scroll deceleration only
    // tooltip          -- show delay timer only
    // grid             -- one extra frame for layout stabilization
    // menu             -- open/close transition only
    let forbidden = [
        "spinner()",
        ".spinner()",
        "progress_bar(",
        "animate_bool",
        "animate_value",
        "request_repaint_after_secs", // timed repaints = hidden polling
        "request_repaint_after(",     // same
    ];

    let src_dir = Path::new(env!("CARGO_MANIFEST_DIR")).join("src");
    let all_rs_files = collect_rs_files(&src_dir);

    for path in &all_rs_files {
        let source = fs::read_to_string(path).unwrap();
        for pattern in &forbidden {
            let count = count_pattern(&source, pattern);
            assert_eq!(
                count,
                0,
                "found '{}' {} times in {} -- \
                 this widget/call causes continuous or timed repainting. \
                 see tests/idle_guard.rs for the full audit list",
                pattern,
                count,
                path.display()
            );
        }
    }
}

fn collect_rs_files(dir: &Path) -> Vec<std::path::PathBuf> {
    let mut files = Vec::new();
    for entry in fs::read_dir(dir).unwrap() {
        let entry = entry.unwrap();
        let path = entry.path();
        if path.is_dir() {
            files.extend(collect_rs_files(&path));
        } else if path.extension().is_some_and(|e| e == "rs") {
            files.push(path);
        }
    }
    files
}

#[test]
fn no_repaint_anywhere() {
    // after migrating to iced, there should be ZERO request_repaint calls
    // in the entire codebase. iced handles all repaints via reactive rendering.
    let src_dir = Path::new(env!("CARGO_MANIFEST_DIR")).join("src");
    let all_rs_files = collect_rs_files(&src_dir);
    for path in &all_rs_files {
        let source = fs::read_to_string(path).unwrap();
        let count = count_repaint_calls(&source);
        assert_eq!(
            count,
            0,
            "found {} request_repaint() calls in {} -- \
             iced handles repaints via reactive rendering, no manual repaint needed",
            count,
            path.display()
        );
    }
}
