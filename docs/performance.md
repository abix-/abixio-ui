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
- **No animations.** We don't use animated widgets. Static text for loading states.
- **No background work.** No health checks, no keep-alive pings, no prefetching,
  no scheduled tasks.
- **Reactive rendering.** iced 0.14 redraws when application state changes.
  Mouse movement over non-interactive areas does not schedule work in our code.

## Rendering and update model

- User actions enter `App::update(...)`
- Async completions from `Task::perform(...)` re-enter `App::update(...)`
- The app does not call `request_repaint()`
- The performance counters shown in Settings count `update()` activity, not
  exact GPU-presented frames

This means the app stays quiet while idle and only wakes up for user input,
window events, or async completions.

## Rendering architecture

iced 0.14 uses reactive rendering by default (PR #2662). The framework diffs
widget output between frames and only redraws widgets whose state changed.

There is no `request_repaint()` in our code. iced handles all repaint
scheduling internally based on state changes from `update()`.

Previous versions of this app used egui (immediate mode) which rebuilt the
entire UI on every OS event. We migrated to iced specifically to eliminate
unnecessary redraws.

## Network

**No polling. No background fetches. No keep-alive pings.**

Normal network calls happen when the app needs live server data:
- startup, if `--endpoint` is provided
- connect or test a saved connection
- list buckets
- browse a bucket or prefix
- load object metadata
- upload, download, delete, or create a bucket
- probe AbixIO and refresh the Disks or Healing views
- run the built-in Testing tab against the current server

When the user is staring at a listing, zero network traffic.

## Memory

The UI holds:
- Current bucket listing (Vec of names + sizes), tiny
- Current object listing, tiny
- Connection info, tiny
- UI state (selected bucket, prefix, section), tiny

No object data cached in memory. Uploads read from disk to HTTP.
Downloads write from HTTP to disk.

## Disk

The app writes to disk only when:
- User downloads a file (writes to user-chosen path)
- User adds, edits, or removes saved connections (`settings.json`)

No temp files. No caches. No logs to disk.

## Startup

- Parse CLI args
- Open window
- If `--endpoint` is provided: create the client and fetch bucket list
- Otherwise: show the Connections view with no startup network request
- Done. No splash screen. No update checks.

## Current metric limitations

The Settings view shows live request and byte counters sourced from shared
atomic counters in the S3 client. Update counters track `App::update()` calls.

## Verified idle: real CPU measurement tests

Every idle code path is tested with Windows `GetProcessTimes`. Each test
measures actual process CPU time over a 2-second idle window. Threshold:
<50ms CPU (<2.5%). A 60fps render loop would consume ~2000ms.

Run: `cargo test --test cpu_idle -- --ignored --test-threads=1`

| Test | Scenario | Proves |
|---|---|---|
| `perf_stats_idle_near_zero_cpu` | Create PerfStats, sleep 2s | Stats module has zero cost |
| `tokio_runtime_idle_near_zero_cpu` | Init runtime, no work, sleep 2s | Runtime sleeps when idle |
| `async_op_idle_after_completion_near_zero_cpu` | Fire request, wait, sleep 2s | No lingering work |
| `async_op_multiple_requests_then_idle` | 5 sequential requests, sleep 2s | Idle after burst |
| `perf_stats_after_recording_then_idle` | Record 100 frames, sleep 2s | No background effect |
| `async_op_created_but_never_used_idle` | Create 3 ops, sleep 2s | Unused ops = zero cost |
| `polling_completed_ops_does_not_spin` | Poll 1000x, sleep 2s | Polling is no-op |
| `busy_loop_detected_as_high_cpu` | Spin 200ms | Sanity: measurement works |

Source-level guards (run with `cargo test --test idle_guard`):

| Test | What it checks |
|---|---|
| `no_repaint_in_app_logic` | Zero `request_repaint()` in `src/app/**/*.rs` |
| `no_repaint_in_views` | Zero `request_repaint()` in `src/views/*.rs` |
| `no_spinners_anywhere` | Zero `spinner()` in app or views |
| `no_animation_widgets` | No banned animated widget patterns in any src/ file |
| `no_repaint_anywhere` | Zero `request_repaint()` calls anywhere under `src/` |

## What we explicitly avoid

| Anti-pattern | Why | Our approach |
|---|---|---|
| Immediate mode UI | Rebuilds entire UI every event | iced reactive: only changed widgets |
| Continuous rendering | Burns CPU/GPU for nothing | iced reactive: idle = 0 fps |
| Background polling | Network + CPU waste | Fetch on user action only |
| Object data caching | Memory waste, stale data | Always fetch live |
| Prefetching | Network waste, speculative | Load on navigate only |
| Connection keep-alive pings | Network waste | Connect on demand |
| Logging to disk | Disk I/O waste | stderr only, if at all |
| Analytics/telemetry | Network + privacy waste | None |
| Auto-refresh timers | CPU + network waste | Manual refresh button |
| Animated widgets | Force continuous rendering | Static loading text |
