# Performance

## Design principle

**Minimum necessary work.** The app does the absolute minimum rendering,
network, disk, and CPU work required to fulfill the user's action. Nothing
more. Ever.

If the user is not interacting, the process is indistinguishable from idle:
0 fps, 0% CPU, 0 network traffic, 0 disk I/O. When the user clicks something,
we do exactly what's needed (one fetch, one render) and go back to idle.

This is not an optimization. It is the architecture. Every design decision
flows from this:

- **No polling.** We don't check if data changed. The server doesn't push to us.
  Data is fetched when the user asks for it.
- **No caching.** We don't store data locally. Every navigation is a live fetch.
  This means no stale data, no cache invalidation, no disk writes.
- **No animations.** Spinners, progress bars, and animated transitions are banned.
  They force 60fps continuous rendering for their entire duration. We use static
  text ("Loading...") instead.
- **No background work.** No health checks, no keep-alive pings, no prefetching,
  no scheduled tasks. The tokio runtime sleeps when there's no work.
- **One repaint trigger.** The only code that calls `request_repaint()` is the
  async task completion handler in `async_op.rs`. Nothing else. This is enforced
  by automated tests.

### What triggers a repaint

| Trigger | Source | Frequency |
|---|---|---|
| User input (click, key, scroll) | OS event | Only when user acts |
| Async task completes | `async_op.rs` calls `request_repaint()` | Once per completed request |
| ScrollArea deceleration | egui internal | Decays to zero, stops on its own |
| Tooltip delay | egui internal | One-shot timer |
| Grid layout stabilization | egui internal | 1 extra frame |

### What does NOT trigger a repaint

| Scenario | Why not |
|---|---|
| User staring at screen | No events = no frames |
| Network request in-flight | We don't poll; background task wakes us on completion |
| After any operation completes | Renders once, then idle |
| App in background/minimized | No events = no frames |
| Between user clicks | No events = no frames |

## Complete repaint inventory

Every repaint in the app, exhaustively listed. If it's not in this table,
it should not cause a repaint. If it does, that's a bug.

### Startup (1 repaint)

| Action | Network | Renders | Then |
|---|---|---|---|
| App launches | GET / (list buckets) | 1 frame on completion | Idle |

### User actions (each = 1 network call + 1 repaint on completion)

| User action | Network call | Renders | Then |
|---|---|---|---|
| Click bucket in sidebar | GET /bucket?list-type=2 | 1 frame | Idle |
| Click prefix (folder) | GET /bucket?list-type=2&prefix=x/ | 1 frame | Idle |
| Click breadcrumb segment | GET /bucket?list-type=2&prefix=x/ | 1 frame | Idle |
| Click "Refresh" button | GET /bucket?list-type=2 | 1 frame | Idle |
| Click "Refresh All" button | GET / (list buckets) | 1 frame | Idle |
| Click object in table | HEAD /bucket/key | 1 frame (detail panel) | Idle |
| Click "Upload" button | PUT /bucket/key, then GET listing | 2 frames | Idle |
| Click "Delete" button | DELETE /bucket/key, then GET listing | 2 frames | Idle |
| Click "Download" button | GET /bucket/key, write to disk | 1 frame | Idle |
| Click "+" (create bucket) | PUT /bucket, then GET / | 2 frames | Idle |
| Click sidebar nav icon | None | 1 frame (section switch) | Idle |
| Click theme switch | None | 1 frame (visuals update) | Idle |
| Press ESC | None | 1 frame (close detail panel) | Idle |
| Type in text field | None | 1 frame per keystroke | Idle on blur |

### egui internal (interaction-driven, not continuous)

| Trigger | When | Duration |
|---|---|---|
| ScrollArea deceleration | After scroll gesture | Decays to zero in ~500ms |
| Tooltip delay | Hovering a widget | One-shot timer, then idle |
| Grid layout stabilization | First render of a grid | 1 extra frame |
| Menu open/close | Click menu item | Transition frames only |

### Never

| Scenario | Renders |
|---|---|
| User idle, window focused | 0 |
| User idle, window unfocused | 0 |
| Network request in-flight | 0 (until completion signal) |
| App minimized | 0 |
| After any operation completes | 0 (after the 1 completion frame) |

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
| `no_spinners_anywhere` | Zero `spinner()` calls in app.rs or views/ |
| `no_animation_widgets` | Scans ALL `src/**/*.rs` for banned patterns (see below) |
| `async_op_has_exactly_one_repaint` | Exactly 1 `request_repaint()` in `src/async_op.rs` |

### Banned patterns (audited from egui 0.34.1 source)

We audited every `request_repaint` call in egui's source to identify which
widgets/calls cause CONTINUOUS repainting vs one-shot interaction repaints.

**Banned (cause continuous frame rendering):**

| Pattern | egui source | Why it burns CPU |
|---|---|---|
| `spinner()` | `widgets/spinner.rs:40` | Animates every frame |
| `progress_bar(` | `widgets/progress_bar.rs:138` | Animates fill every frame |
| `animate_bool` | `context.rs:3236` | Triggers repaint until animation completes |
| `animate_value` | `context.rs:3262` | Same |
| `request_repaint_after_secs` | Timed repaint | Creates hidden polling timer |
| `request_repaint_after(` | Timed repaint | Same |

**Allowed (only repaint during active user interaction):**

| Widget | egui source | When it repaints |
|---|---|---|
| `CollapsingHeader` | `collapsing_header.rs:72` | Only during open/close transition |
| `ScrollArea` | `scroll_area.rs:850,894,1133,1483` | Only during scroll deceleration |
| `Tooltip` | `tooltip.rs:258,364,376` | Only for show delay timer |
| `Grid` | `grid.rs:280` | One extra frame for layout stabilization |
| `Menu` | `menu.rs:569,704` | Only during open/close |
| `Area` (drag) | `area.rs:552,644,688` | Only while user is dragging |
| `Resize` | `resize.rs:216` | One extra frame for counter delay |

The `no_animation_widgets` test scans every `.rs` file under `src/` for banned
patterns. `async_op.rs` is excluded (it has the one allowed `request_repaint`).

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
