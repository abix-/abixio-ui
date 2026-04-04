use std::collections::VecDeque;
use std::time::{Duration, Instant};

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

    fn max(&self) -> f64 {
        self.samples.iter().map(|s| s.value).fold(0.0_f64, f64::max)
    }

    fn count(&self) -> usize {
        self.samples.len()
    }

    fn recent_values(&self, max_points: usize) -> Vec<f64> {
        let len = self.samples.len();
        let skip = if len > max_points {
            len - max_points
        } else {
            0
        };
        self.samples.iter().skip(skip).map(|s| s.value).collect()
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

    // network
    network_requests: TimeSeries, // 1.0 per request
    network_bytes_out: TimeSeries,
    network_bytes_in: TimeSeries,
    pub total_requests: u64,
    pub total_bytes_out: u64,
    pub total_bytes_in: u64,

    // repaints
    repaints: TimeSeries,
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
            network_requests: TimeSeries::new(),
            network_bytes_out: TimeSeries::new(),
            network_bytes_in: TimeSeries::new(),
            total_requests: 0,
            total_bytes_out: 0,
            total_bytes_in: 0,
            repaints: TimeSeries::new(),
        }
    }

    /// Call once per frame from logic().
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

    /// Call when a network request completes.
    pub fn record_request(&mut self, bytes_out: u64, bytes_in: u64) {
        self.network_requests.push(1.0);
        self.network_bytes_out.push(bytes_out as f64);
        self.network_bytes_in.push(bytes_in as f64);
        self.total_requests += 1;
        self.total_bytes_out += bytes_out;
        self.total_bytes_in += bytes_in;
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

    pub fn avg_frame_ms(&self) -> f64 {
        self.frame_times_ms.avg()
    }

    pub fn max_frame_ms(&self) -> f64 {
        self.frame_times_ms.max()
    }

    pub fn total_frames(&self) -> u64 {
        self.frame_count
    }

    pub fn repaints_5m(&self) -> usize {
        self.repaints.count()
    }

    pub fn requests_5m(&self) -> usize {
        self.network_requests.count()
    }

    pub fn bytes_in_5m(&self) -> f64 {
        self.network_bytes_in.samples.iter().map(|s| s.value).sum()
    }

    pub fn bytes_out_5m(&self) -> f64 {
        self.network_bytes_out.samples.iter().map(|s| s.value).sum()
    }

    pub fn fps_history(&self, points: usize) -> Vec<f64> {
        self.fps.recent_values(points)
    }
}
