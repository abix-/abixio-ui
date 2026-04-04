# Performance

Goal: zero waste. No CPU, disk, network, or GPU usage unless the user is
actively doing something. The app should be indistinguishable from idle
when nothing is happening.

## Rendering

egui defaults to continuous 60fps rendering. We do not want this.

**Repaint only on events:**
- User input (click, key, scroll, resize)
- Network response received (channel has data)
- Nothing else

```rust
// eframe NativeOptions
eframe::NativeOptions {
    // only repaint when something changes
    vsync: true,
    ..Default::default()
}

// in App::update(), request repaint only when needed
fn update(&mut self, ctx: &egui::Context, _frame: &mut eframe::Frame) {
    // check for network responses
    if self.rx.try_recv().is_ok() {
        // got data, will repaint naturally this frame
    }

    // if a request is in-flight, poll next frame (not continuous -- just one more)
    if self.loading {
        ctx.request_repaint();
    }

    // otherwise: no request_repaint() = no rendering until next user input
}
```

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

## What we explicitly avoid

| Anti-pattern | Why | Our approach |
|---|---|---|
| Continuous rendering | Burns CPU/GPU for no reason | Repaint on events only |
| Background polling | Network + CPU waste when idle | Fetch on user action only |
| Object data caching | Memory waste, stale data | Always fetch live |
| Prefetching | Network waste, speculative | Load on navigate only |
| Connection keep-alive pings | Network waste | Connect on demand |
| Logging to disk | Disk I/O waste | stderr only, if at all |
| Analytics/telemetry | Network + privacy waste | None |
| Auto-refresh timers | CPU + network waste | Manual refresh button |
| Animation loops | GPU waste | No animations |
