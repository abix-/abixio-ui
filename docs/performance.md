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
- **Reactive rendering.** iced 0.14 only redraws widgets whose state changed.
  Mouse movement over non-interactive areas causes zero redraws. This is handled
  by the framework, not by us.

### What triggers a redraw

| Trigger | Source | Frequency |
|---|---|---|
| User click/key/scroll | OS event -> iced | Only when user acts |
| Async task completes | iced Task system | Once per completed request |
| Widget state change | iced reactive diff | Only the changed widget |
| Mouse over interactive widget | iced hover state | Only if hover changes appearance |

### What does NOT trigger a redraw

| Scenario | Why not |
|---|---|
| User idle, mouse stopped | No events = no frames |
| Mouse over non-interactive area | iced reactive: no state change = no redraw |
| Network request in-flight | iced Task handles completion internally |
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
| Click theme switch | None | 1 frame (theme update) | Idle |
| Press ESC | None | 1 frame (close detail panel) | Idle |
| Type in text field | None | 1 frame per keystroke | Idle on blur |
| Move mouse over interactive widget | None | 1 frame (hover state change) | Idle when stops |
| Move mouse over non-interactive area | None | 0 frames (iced reactive) | Already idle |
| Window resize/move | None | 1 frame per OS resize event | Idle when done |

### Never

| Scenario | Renders |
|---|---|
| User idle, mouse stopped, window focused | 0 |
| User idle, window unfocused | 0 |
| Mouse over plain text/labels | 0 (iced reactive) |
| Network request in-flight | 0 (until completion) |
| App minimized | 0 |
| After any operation completes | 0 (after the 1 completion frame) |

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

Network calls happen only when the user explicitly acts:
- Click a bucket: one GET request
- Click a prefix: one GET request
- Click refresh: one GET request
- Upload/delete: one PUT/DELETE, then one GET to refresh listing

When the user is staring at a listing, zero network traffic.

## Memory

The UI holds:
- Current bucket listing (Vec of names + sizes) -- tiny
- Current object listing -- tiny
- Connection info -- tiny
- UI state (selected bucket, prefix, section) -- tiny

No object data cached in memory. Uploads read from disk to HTTP.
Downloads write from HTTP to disk.

## Disk

The app writes to disk only when:
- User downloads a file (writes to user-chosen path)
- Future: save connections.json (not yet implemented)

No temp files. No caches. No logs to disk.

## Startup

- Parse CLI args
- Open window
- Fetch bucket list (one GET request)
- Done. No splash screen. No update checks.

## Verified idle: real CPU measurement tests

Every idle code path is tested with Windows `GetProcessTimes`. Each test
measures actual process CPU time over a 2-second idle window. Threshold:
<50ms CPU (<2.5%). A 60fps render loop would consume ~2000ms.

Run: `cargo test --test cpu_idle -- --ignored --test-threads=1`

| Test | Scenario | Proves |
|---|---|---|
| `perf_stats_idle` | Create PerfStats, sleep 2s | Stats module has zero cost |
| `tokio_runtime_idle` | Init runtime, no work, sleep 2s | Runtime sleeps when idle |
| `async_op_idle_after_completion` | Fire request, wait, sleep 2s | No lingering work |
| `async_op_multiple_requests_then_idle` | 5 sequential requests, sleep 2s | Idle after burst |
| `perf_stats_after_recording_then_idle` | Record 100 frames, sleep 2s | No background effect |
| `async_op_created_but_never_used` | Create 3 ops, sleep 2s | Unused ops = zero cost |
| `polling_completed_ops_does_not_spin` | Poll 1000x, sleep 2s | Polling is no-op |
| `busy_loop_detected_as_high_cpu` | Spin 200ms | Sanity: measurement works |

Source-level guards (run with `cargo test --test idle_guard`):

| Test | What it checks |
|---|---|
| `no_repaint_in_app_logic` | Zero `request_repaint()` in `src/app.rs` |
| `no_repaint_in_views` | Zero `request_repaint()` in `src/views/*.rs` |
| `no_spinners_anywhere` | Zero `spinner()` in app or views |
| `no_animation_widgets` | No banned animated widget patterns in any src/ file |
| `async_op_has_exactly_one_repaint` | AsyncOp test helper has 0 repaint calls (removed after iced migration) |

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
