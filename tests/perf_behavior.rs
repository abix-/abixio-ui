//! Behavioral tests for the performance tracking system.
//!
//! These tests verify that:
//! 1. When idle, no frames are recorded (0 CPU)
//! 2. When active, frames are recorded correctly
//! 3. Network stats only increment on actual S3 client calls
//! 4. Frame timing is reasonable

use std::sync::Arc;
use std::sync::atomic::Ordering;
use std::thread;
use std::time::Duration;

#[test]
fn idle_records_zero_frames() {
    let stats = abixio_ui::perf::PerfStats::new();
    // don't call record_frame -- simulates idle
    assert_eq!(stats.total_frames(), 0);
    assert_eq!(stats.current_fps(), 0.0);
    assert_eq!(stats.repaints_5m(), 0);
}

#[test]
fn active_records_frames() {
    let mut stats = abixio_ui::perf::PerfStats::new();
    for _ in 0..10 {
        stats.record_frame();
    }
    assert_eq!(stats.total_frames(), 10);
    assert_eq!(stats.repaints_5m(), 10);
}

#[test]
fn network_starts_at_zero() {
    let stats = abixio_ui::perf::PerfStats::new();
    assert_eq!(stats.total_requests(), 0);
    assert_eq!(stats.total_bytes_in(), 0);
    assert_eq!(stats.total_bytes_out(), 0);
}

#[test]
fn network_reads_from_s3_stats() {
    let mut stats = abixio_ui::perf::PerfStats::new();
    let s3_stats = Arc::new(abixio_ui::s3::client::S3Stats::default());
    stats.set_s3_stats(s3_stats.clone());

    s3_stats.requests.fetch_add(3, Ordering::Relaxed);
    s3_stats.bytes_out.fetch_add(100, Ordering::Relaxed);
    s3_stats.bytes_in.fetch_add(5000, Ordering::Relaxed);

    assert_eq!(stats.total_requests(), 3);
    assert_eq!(stats.total_bytes_out(), 100);
    assert_eq!(stats.total_bytes_in(), 5000);
}

#[test]
fn frame_time_is_reasonable() {
    let mut stats = abixio_ui::perf::PerfStats::new();
    stats.record_frame();
    thread::sleep(Duration::from_millis(10));
    stats.record_frame();

    // frame time should be roughly 10ms (allow 5-100ms for CI variance)
    let ft = stats.current_frame_ms();
    assert!(
        ft > 5.0 && ft < 100.0,
        "frame time {} ms not in expected range",
        ft
    );
}

#[test]
fn no_frames_while_sleeping() {
    let mut stats = abixio_ui::perf::PerfStats::new();
    stats.record_frame(); // initial frame
    let frames_before = stats.total_frames();

    // simulate "idle" -- just sleep, don't call record_frame
    thread::sleep(Duration::from_millis(50));

    // frames should not have increased
    assert_eq!(stats.total_frames(), frames_before);
}
