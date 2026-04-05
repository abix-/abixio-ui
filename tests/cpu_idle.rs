//! Real CPU usage tests using Windows GetProcessTimes.
//!
//! These tests measure actual process CPU time to verify that idle code paths
//! consume near-zero CPU. A 60fps render loop would burn ~100% of one core;
//! these tests catch that by asserting <2.5% CPU during idle periods.
//!
//! MUST run single-threaded (GetProcessTimes measures entire process):
//!   cargo test --test cpu_idle -- --ignored --test-threads=1

use std::thread;
use std::time::Duration;

use windows_sys::Win32::Foundation::FILETIME;
use windows_sys::Win32::System::Threading::{GetCurrentProcess, GetProcessTimes};

/// Get total CPU time (kernel + user) consumed by this process, in milliseconds.
fn process_cpu_ms() -> f64 {
    unsafe {
        let mut creation = std::mem::zeroed::<FILETIME>();
        let mut exit = std::mem::zeroed::<FILETIME>();
        let mut kernel = std::mem::zeroed::<FILETIME>();
        let mut user = std::mem::zeroed::<FILETIME>();
        GetProcessTimes(
            GetCurrentProcess(),
            &mut creation,
            &mut exit,
            &mut kernel,
            &mut user,
        );
        filetime_to_ms(&kernel) + filetime_to_ms(&user)
    }
}

fn filetime_to_ms(ft: &FILETIME) -> f64 {
    let ticks = (ft.dwHighDateTime as u64) << 32 | ft.dwLowDateTime as u64;
    ticks as f64 / 10_000.0
}

/// Measure CPU time consumed during a closure.
fn measure_cpu_ms(f: impl FnOnce()) -> f64 {
    let before = process_cpu_ms();
    f();
    let after = process_cpu_ms();
    after - before
}

const IDLE_SECS: u64 = 2;
// 50ms of CPU over 2 seconds = 2.5% -- generous for CI, catches render loops
const MAX_IDLE_CPU_MS: f64 = 50.0;

#[test]
#[ignore] // must run single-threaded: cargo test --test cpu_idle -- --ignored --test-threads=1
fn perf_stats_idle_near_zero_cpu() {
    let _stats = abixio_ui::perf::PerfStats::new();

    let cpu = measure_cpu_ms(|| {
        thread::sleep(Duration::from_secs(IDLE_SECS));
    });

    assert!(
        cpu < MAX_IDLE_CPU_MS,
        "PerfStats idle consumed {:.1}ms CPU over {}s (max {}ms) -- something is spinning",
        cpu,
        IDLE_SECS,
        MAX_IDLE_CPU_MS,
    );
}

#[test]
#[ignore]
fn tokio_runtime_idle_near_zero_cpu() {
    // force the lazy runtime to initialize
    let _ = &*abixio_ui::async_op::RUNTIME;

    // now measure idle CPU with runtime alive but no work
    let cpu = measure_cpu_ms(|| {
        thread::sleep(Duration::from_secs(IDLE_SECS));
    });

    assert!(
        cpu < MAX_IDLE_CPU_MS,
        "tokio runtime idle consumed {:.1}ms CPU over {}s (max {}ms) -- runtime is not sleeping",
        cpu,
        IDLE_SECS,
        MAX_IDLE_CPU_MS,
    );
}

#[test]
#[ignore]
fn async_op_idle_after_completion_near_zero_cpu() {
    let _ = &*abixio_ui::async_op::RUNTIME;

    // fire a quick async op and let it complete
    let mut op = abixio_ui::async_op::AsyncOp::<String>::new();
    op.request(async {
        tokio::time::sleep(Duration::from_millis(50)).await;
        Ok("done".to_string())
    });

    // wait for it to complete
    thread::sleep(Duration::from_millis(200));
    op.poll();
    assert!(!op.pending, "op should have completed by now");

    // NOW measure idle -- no pending work, runtime should sleep
    let cpu = measure_cpu_ms(|| {
        thread::sleep(Duration::from_secs(IDLE_SECS));
    });

    assert!(
        cpu < MAX_IDLE_CPU_MS,
        "post-completion idle consumed {:.1}ms CPU over {}s (max {}ms) -- \
         something kept running after async op finished",
        cpu,
        IDLE_SECS,
        MAX_IDLE_CPU_MS,
    );
}

#[test]
#[ignore]
fn async_op_multiple_requests_then_idle() {
    let _ = &*abixio_ui::async_op::RUNTIME;

    // fire several async ops in sequence
    for i in 0..5 {
        let mut op = abixio_ui::async_op::AsyncOp::<String>::new();
        op.request(async move {
            tokio::time::sleep(Duration::from_millis(20)).await;
            Ok(format!("done {}", i))
        });
        thread::sleep(Duration::from_millis(100));
        op.poll();
        assert!(!op.pending);
    }

    // all done -- idle period
    let cpu = measure_cpu_ms(|| {
        thread::sleep(Duration::from_secs(IDLE_SECS));
    });

    assert!(
        cpu < MAX_IDLE_CPU_MS,
        "idle after 5 sequential requests consumed {:.1}ms CPU over {}s (max {}ms)",
        cpu,
        IDLE_SECS,
        MAX_IDLE_CPU_MS,
    );
}

#[test]
#[ignore]
fn perf_stats_after_recording_then_idle() {
    let mut stats = abixio_ui::perf::PerfStats::new();

    // simulate 100 frames of activity
    for _ in 0..100 {
        stats.record_frame();
    }

    // now idle
    let cpu = measure_cpu_ms(|| {
        thread::sleep(Duration::from_secs(IDLE_SECS));
    });

    assert!(
        cpu < MAX_IDLE_CPU_MS,
        "idle after 100 frames of stats recording consumed {:.1}ms CPU over {}s (max {}ms)",
        cpu,
        IDLE_SECS,
        MAX_IDLE_CPU_MS,
    );
}

#[test]
#[ignore]
fn async_op_created_but_never_used_idle() {
    let _ = &*abixio_ui::async_op::RUNTIME;

    // create ops but never fire them -- should be zero overhead
    let _op1 = abixio_ui::async_op::AsyncOp::<String>::new();
    let _op2 = abixio_ui::async_op::AsyncOp::<Vec<u8>>::new();
    let _op3 = abixio_ui::async_op::AsyncOp::<()>::new();

    let cpu = measure_cpu_ms(|| {
        thread::sleep(Duration::from_secs(IDLE_SECS));
    });

    assert!(
        cpu < MAX_IDLE_CPU_MS,
        "unused AsyncOps consumed {:.1}ms CPU over {}s (max {}ms)",
        cpu,
        IDLE_SECS,
        MAX_IDLE_CPU_MS,
    );
}

#[test]
#[ignore]
fn polling_completed_ops_does_not_spin() {
    let _ = &*abixio_ui::async_op::RUNTIME;

    let mut op = abixio_ui::async_op::AsyncOp::<String>::new();
    op.request(async { Ok("instant".to_string()) });
    thread::sleep(Duration::from_millis(100));
    op.poll();
    assert!(!op.pending);

    // repeatedly poll a completed op -- should be cheap/no-op
    let cpu = measure_cpu_ms(|| {
        for _ in 0..1000 {
            op.poll();
        }
        thread::sleep(Duration::from_secs(IDLE_SECS));
    });

    assert!(
        cpu < MAX_IDLE_CPU_MS,
        "polling completed op 1000 times then idle consumed {:.1}ms CPU over {}s (max {}ms)",
        cpu,
        IDLE_SECS,
        MAX_IDLE_CPU_MS,
    );
}

#[test]
#[ignore]
fn busy_loop_detected_as_high_cpu() {
    // sanity check: verify our measurement actually catches CPU usage
    let cpu = measure_cpu_ms(|| {
        let start = std::time::Instant::now();
        while start.elapsed() < Duration::from_millis(200) {
            // busy spin
            std::hint::spin_loop();
        }
    });

    assert!(
        cpu > 100.0,
        "busy loop only measured {:.1}ms CPU -- measurement is broken",
        cpu,
    );
}
