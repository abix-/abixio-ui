# Performance

Goal: zero waste. No CPU, disk, network, or GPU usage unless the user is
actively doing something. The app should be indistinguishable from idle
when nothing is happening.

## Rendering

egui defaults to continuous 60fps rendering. We do not want this.

**Repaint only on events:**
- User input (click, key, scroll, resize)
- Network response received (background task signals completion)
- Nothing else

### How it works

Each async network request uses a `tokio::sync::oneshot` channel. The background
task holds a clone of `egui::Context`. When the task completes:

```rust
// async_op.rs -- background task
RUNTIME.spawn(async move {
    let result = fut.await;
    let _ = tx.send(result);
    ctx.request_repaint();  // <-- THE ONLY repaint trigger
});
```

This is the **only** place `request_repaint()` is called. The UI thread's
`logic()` method polls the channel with `try_recv()` (non-blocking) but
**never calls request_repaint() itself**:

```rust
// app.rs -- logic() runs each frame
fn logic(&mut self, ctx: &egui::Context, _frame: &mut eframe::Frame) {
    self.perf.record_frame();
    self.buckets_op.poll();  // try_recv(), non-blocking
    self.objects_op.poll();
    // ...

    // NO request_repaint() here. Background tasks wake us when done.
}
```

### Timeline

```
t=0  user clicks bucket "test"
     -> request() spawns async task
     -> UI renders loading state (1 frame)
     -> UI goes idle (0 fps, 0 CPU)

t=50ms  background task completes
     -> sends result via oneshot channel
     -> calls ctx.request_repaint()
     -> UI wakes up, logic() polls result
     -> UI renders object list (1 frame)
     -> UI goes idle again

t=50ms..infinity  user staring at screen
     -> 0 fps, 0 CPU, 0 network
```

### Common mistake we avoid

A naive approach polls for pending requests every frame:

```rust
// BAD: burns CPU while waiting for network
if self.loading {
    ctx.request_repaint();  // 60fps render loop during every request!
}
```

We don't do this. The background task wakes us exactly once when done.

When idle: 0 fps, 0% CPU. egui sleeps until the next OS event.

## Network

**No polling. No background fetches. No keep-alive pings.**

Network calls happen only when the user explicitly navigates:
- Click a bucket: one GET request
- Click a prefix: one GET request
- Click refresh: one GET request
- Upload/delete: one PUT/DELETE, then one GET to refresh listing

When the user is staring at a listing, zero network traffic.

**Request batching:** one request at a time per view. If the user clicks
rapidly through buckets, cancel the in-flight request and send the new one.
Don't queue up stale requests.

```
user clicks bucket A -> send GET /A
user clicks bucket B before A responds -> cancel A, send GET /B
```

**AbixIO management endpoints** (disk health, heal status) are fetched only
when the user navigates to those tabs. Not polled. Not prefetched.

## Memory

**No object data in memory.** The UI holds:
- Current bucket listing (Vec of object names + sizes) -- tiny
- Connection list -- tiny
- UI state (selected bucket, prefix, scroll position) -- tiny

Object data is streamed directly: upload reads from disk to HTTP, download
writes from HTTP to disk. Never buffered entirely in memory (except for
small objects where it doesn't matter).

**Listing pagination:** for buckets with thousands of objects, use S3
continuation tokens. Load one page at a time. Don't fetch all 100k objects
into memory.

## Disk

**The app writes to disk only when:**
- User saves a new connection (one small JSON write)
- User changes preferences (one small JSON write)
- User downloads a file (writes to user-chosen path)
- OS keychain operations (handled by OS, not us)

No temp files. No caches. No logs to disk. No write-ahead anything.

## Startup

**Fast launch:**
- Read `connections.json` (one small file)
- Read `preferences.json` (one small file)
- Open window
- Done. No server contact until user selects a connection.

No splash screen. No "checking for updates." No preloading data.

## Thread model

Two threads total:
1. **UI thread** -- egui render loop. Sleeps when idle.
2. **Network thread** -- tokio runtime. Sleeps when no requests pending.

No thread pool. No worker pool. No timers. No scheduled tasks.

## Verified idle: real CPU measurement tests

Every idle code path is tested with Windows `GetProcessTimes`. Each test measures
actual process CPU time over a 2-second idle window. Threshold: <50ms CPU (<2.5%).
A 60fps render loop would consume ~2000ms (100% of one core) -- these tests catch it.

Run: `cargo test --test cpu_idle -- --ignored --test-threads=1`

| Test | Scenario | Proves |
|---|---|---|
| `perf_stats_idle` | Create PerfStats, don't call record_frame, sleep 2s | Stats module has zero background cost |
| `tokio_runtime_idle` | Init tokio runtime, no work, sleep 2s | Runtime thread sleeps when idle |
| `async_op_idle_after_completion` | Fire request, wait for done, sleep 2s | No lingering work after completion |
| `async_op_multiple_requests_then_idle` | Fire 5 sequential requests, all complete, sleep 2s | Idle after burst of activity |
| `perf_stats_after_recording_then_idle` | Record 100 frames + 100 requests, sleep 2s | Stats recording has no background effect |
| `async_op_created_but_never_used` | Create 3 AsyncOps, never fire, sleep 2s | Unused ops have zero overhead |
| `polling_completed_ops_does_not_spin` | Poll completed op 1000x, sleep 2s | Repeated polling is a no-op |
| `busy_loop_detected_as_high_cpu` | Spin for 200ms | Sanity: measurement actually works |

Source-level guards (run with `cargo test --test idle_guard`):

| Test | What it checks |
|---|---|
| `no_repaint_in_app_logic` | Zero `request_repaint()` calls in `src/app.rs` |
| `no_repaint_in_views` | Zero `request_repaint()` calls in any `src/views/*.rs` |
| `no_spinners_anywhere` | Zero `spinner()` calls in app.rs or views/ (spinners force 60fps animation) |
| `no_animation_widgets` | Zero `spinner()`, `progress_bar()`, `animate_bool`, `animate_value` in views/ |
| `async_op_has_exactly_one_repaint` | Exactly 1 `request_repaint()` in `src/async_op.rs` (the completion handler) |

### Why no spinners?

egui's `ui.spinner()` widget internally calls `request_repaint()` every frame
to animate the rotation. This creates a continuous 60fps render loop for the
entire duration of any loading state. We use static "Loading..." text instead.
The source guard test catches any spinner() introduced in future code.

## What we explicitly avoid

| Anti-pattern | Why | Our approach |
|---|---|---|
| Continuous rendering | Burns CPU/GPU for no reason | Repaint on events only |
| Polling repaint loop | `if pending { request_repaint() }` burns CPU while waiting | Background task calls repaint on completion |
| Background polling | Network + CPU waste when idle | Fetch on user action only |
| Object data caching | Memory waste, stale data | Always fetch live |
| Prefetching | Network waste, speculative | Load on navigate only |
| Connection keep-alive pings | Network waste | Connect on demand |
| Logging to disk | Disk I/O waste | stderr only, if at all |
| Analytics/telemetry | Network + privacy waste | None |
| Auto-refresh timers | CPU + network waste | Manual refresh button |
| Animation loops | GPU waste | No animations |
