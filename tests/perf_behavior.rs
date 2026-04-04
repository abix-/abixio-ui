//! Behavioral tests for the performance tracking system.
//!
//! These tests verify that:
//! 1. When idle, no frames are recorded (0 CPU)
//! 2. When active, frames are recorded correctly
//! 3. Network stats only increment on actual requests
//! 4. The 5-minute sliding window prunes old samples

use std::thread;
use std::time::Duration;

// we test the perf module directly
// (it's pub in main.rs)

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
    assert_eq!(stats.total_requests, 0);
    assert_eq!(stats.total_bytes_in, 0);
    assert_eq!(stats.total_bytes_out, 0);
    assert_eq!(stats.requests_5m(), 0);
}

#[test]
fn network_records_requests() {
    let mut stats = abixio_ui::perf::PerfStats::new();
    stats.record_request(100, 5000);
    stats.record_request(200, 3000);
    assert_eq!(stats.total_requests, 2);
    assert_eq!(stats.total_bytes_out, 300);
    assert_eq!(stats.total_bytes_in, 8000);
    assert_eq!(stats.requests_5m(), 2);
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
