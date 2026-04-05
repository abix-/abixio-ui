use std::collections::VecDeque;
use std::sync::Arc;
use std::sync::atomic::Ordering;
use std::time::{Duration, Instant};

use crate::s3::client::S3Stats;

const HISTORY_SECS: u64 = 300; // 5 minutes

/// A single timestamped sample.
#[derive(Clone)]
struct Sample {
    time: Instant,
    value: f64,
}

/// Ring buffer of samples over the last 5 minutes.
struct TimeSeries {
    samples: VecDeque<Sample>,
}

impl TimeSeries {
    fn new() -> Self {
        Self {
            samples: VecDeque::new(),
        }
    }

    fn push(&mut self, value: f64) {
        let now = Instant::now();
        self.samples.push_back(Sample { time: now, value });
        self.prune(now);
    }

    fn prune(&mut self, now: Instant) {
        let cutoff = now - Duration::from_secs(HISTORY_SECS);
        while let Some(front) = self.samples.front() {
            if front.time < cutoff {
                self.samples.pop_front();
            } else {
                break;
            }
        }
    }

    fn last(&self) -> f64 {
        self.samples.back().map(|s| s.value).unwrap_or(0.0)
    }

    fn avg(&self) -> f64 {
        if self.samples.is_empty() {
            return 0.0;
        }
        let sum: f64 = self.samples.iter().map(|s| s.value).sum();
        sum / self.samples.len() as f64
    }

    fn count(&self) -> usize {
        self.samples.len()
    }
}

/// Tracks UI performance metrics over the last 5 minutes.
pub struct PerfStats {
    // rendering
    frame_times_ms: TimeSeries,
    fps: TimeSeries,
    last_frame: Instant,
    frame_count: u64,
    fps_update: Instant,
    fps_accumulator: u32,

    // repaints
    repaints: TimeSeries,

    // network (read from shared S3 client counters)
    s3_stats: Option<Arc<S3Stats>>,
}

impl Default for PerfStats {
    fn default() -> Self {
        Self::new()
    }
}

impl PerfStats {
    pub fn new() -> Self {
        let now = Instant::now();
        Self {
            frame_times_ms: TimeSeries::new(),
            fps: TimeSeries::new(),
            last_frame: now,
            frame_count: 0,
            fps_update: now,
            fps_accumulator: 0,
            repaints: TimeSeries::new(),
            s3_stats: None,
        }
    }

    /// Link to the S3 client's shared atomic counters.
    pub fn set_s3_stats(&mut self, stats: Arc<S3Stats>) {
        self.s3_stats = Some(stats);
    }

    /// Call once per frame from update().
    pub fn record_frame(&mut self) {
        let now = Instant::now();
        let dt = now.duration_since(self.last_frame);
        self.last_frame = now;
        self.frame_count += 1;

        self.frame_times_ms.push(dt.as_secs_f64() * 1000.0);
        self.repaints.push(1.0);

        // update fps once per second
        self.fps_accumulator += 1;
        if now.duration_since(self.fps_update) >= Duration::from_secs(1) {
            self.fps.push(self.fps_accumulator as f64);
            self.fps_accumulator = 0;
            self.fps_update = now;
        }
    }

    // -- accessors --

    pub fn current_fps(&self) -> f64 {
        self.fps.last()
    }

    pub fn avg_fps(&self) -> f64 {
        self.fps.avg()
    }

    pub fn current_frame_ms(&self) -> f64 {
        self.frame_times_ms.last()
    }

    pub fn total_frames(&self) -> u64 {
        self.frame_count
    }

    pub fn repaints_5m(&self) -> usize {
        self.repaints.count()
    }

    // -- network (live from S3 client atomics) --

    pub fn total_requests(&self) -> u64 {
        self.s3_stats
            .as_ref()
            .map(|s| s.requests.load(Ordering::Relaxed))
            .unwrap_or(0)
    }

    pub fn total_bytes_in(&self) -> u64 {
        self.s3_stats
            .as_ref()
            .map(|s| s.bytes_in.load(Ordering::Relaxed))
            .unwrap_or(0)
    }

    pub fn total_bytes_out(&self) -> u64 {
        self.s3_stats
            .as_ref()
            .map(|s| s.bytes_out.load(Ordering::Relaxed))
            .unwrap_or(0)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn new_starts_at_zero() {
        let stats = PerfStats::new();
        assert_eq!(stats.total_frames(), 0);
        assert_eq!(stats.current_fps(), 0.0);
        assert_eq!(stats.avg_fps(), 0.0);
        assert_eq!(stats.current_frame_ms(), 0.0);
        assert_eq!(stats.repaints_5m(), 0);
    }

    #[test]
    fn network_zero_without_s3_stats() {
        let stats = PerfStats::new();
        assert_eq!(stats.total_requests(), 0);
        assert_eq!(stats.total_bytes_in(), 0);
        assert_eq!(stats.total_bytes_out(), 0);
    }

    #[test]
    fn network_reads_from_s3_stats() {
        let mut stats = PerfStats::new();
        let s3 = Arc::new(S3Stats::default());
        stats.set_s3_stats(s3.clone());

        s3.requests.fetch_add(10, Ordering::Relaxed);
        s3.bytes_in.fetch_add(5000, Ordering::Relaxed);
        s3.bytes_out.fetch_add(200, Ordering::Relaxed);

        assert_eq!(stats.total_requests(), 10);
        assert_eq!(stats.total_bytes_in(), 5000);
        assert_eq!(stats.total_bytes_out(), 200);
    }

    #[test]
    fn record_frame_increments() {
        let mut stats = PerfStats::new();
        stats.record_frame();
        stats.record_frame();
        stats.record_frame();
        assert_eq!(stats.total_frames(), 3);
        assert_eq!(stats.repaints_5m(), 3);
    }

    #[test]
    fn frame_time_positive_after_record() {
        let mut stats = PerfStats::new();
        stats.record_frame();
        std::thread::sleep(std::time::Duration::from_millis(5));
        stats.record_frame();
        assert!(stats.current_frame_ms() > 0.0);
    }

    #[test]
    fn set_s3_stats_replaces_previous() {
        let mut stats = PerfStats::new();
        let s3a = Arc::new(S3Stats::default());
        let s3b = Arc::new(S3Stats::default());

        stats.set_s3_stats(s3a.clone());
        s3a.requests.fetch_add(5, Ordering::Relaxed);
        assert_eq!(stats.total_requests(), 5);

        stats.set_s3_stats(s3b.clone());
        assert_eq!(stats.total_requests(), 0);
        s3b.requests.fetch_add(3, Ordering::Relaxed);
        assert_eq!(stats.total_requests(), 3);
    }
}
