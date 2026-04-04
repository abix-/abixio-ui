# Architecture

## Overview

abixio-ui is a native desktop app built with egui (eframe). It connects to any
S3-compatible endpoint over HTTP. When connected to an AbixIO server, additional
management features are enabled automatically.

```
+-------------------+     HTTP/S3      +-------------------+
|   abixio-ui       | <=============> |  S3 endpoint      |
|   (desktop app)   |                  |  (any S3 server)  |
+-------------------+                  +-------------------+
```

## Components

```
src/
  main.rs             # eframe app entry, tokio runtime init
  app.rs              # top-level App struct, main layout, frame loop
  s3_client.rs        # raw S3 HTTP calls via reqwest + XML parsing
  abixio_client.rs    # AbixIO management API calls (JSON)
  state.rs            # app state: selected connection, bucket, prefix, object
  theme.rs            # colors, fonts, spacing
  views/
    buckets.rs        # left sidebar: bucket list
    objects.rs        # right panel: object table with prefix navigation
    upload.rs         # file picker dialog + upload progress
    inspector.rs      # object detail: shards, checksums, erasure info
    disks.rs          # disk health dashboard
    config.rs         # server config viewer
    connections.rs    # multi-server connection manager
```

## Async model

egui runs on a single thread in immediate mode. Network calls must not block
the render loop. Pattern learned from egui-async crate internals, simplified
for our needs.

### Runtime setup

Single global tokio runtime, lazy-initialized:

```rust
static RUNTIME: LazyLock<tokio::runtime::Runtime> =
    LazyLock::new(|| tokio::runtime::Runtime::new().unwrap());
```

### Per-request oneshot channels (not mpsc)

Each async request gets its own `tokio::sync::oneshot` channel. No message
routing, no request IDs, no multiplexing. The UI holds the receiver, the
background task holds the sender.

```rust
// state per async operation
struct AsyncOp<T> {
    rx: Option<oneshot::Receiver<Result<T, String>>>,
    data: Option<Result<T, String>>,
    pending: bool,
}

impl<T: Send + 'static> AsyncOp<T> {
    fn request<F>(&mut self, ctx: &egui::Context, fut: F)
    where F: Future<Output = Result<T, String>> + Send + 'static {
        let (tx, rx) = oneshot::channel();
        let ctx = ctx.clone();
        RUNTIME.spawn(async move {
            let result = fut.await;
            let _ = tx.send(result);
            ctx.request_repaint(); // wake UI thread
        });
        self.rx = Some(rx);
        self.pending = true;
    }

    fn poll(&mut self) {
        if let Some(rx) = &mut self.rx {
            match rx.try_recv() {
                Ok(result) => {
                    self.data = Some(result);
                    self.pending = false;
                    self.rx = None;
                }
                Err(oneshot::error::TryRecvError::Empty) => {} // still running
                Err(oneshot::error::TryRecvError::Closed) => {
                    self.pending = false;
                    self.rx = None;
                }
            }
        }
    }
}
```

### Repaint only when needed

- `ctx.request_repaint()` is called ONLY from background tasks after completion
- The UI thread never calls `request_repaint()` in the render loop
- While pending: one extra repaint per frame to check the channel (via poll)
- While idle: zero repaints, zero CPU

```rust
fn ui(&mut self, ui: &mut egui::Ui, _frame: &mut eframe::Frame) {
    // poll all active async ops
    self.buckets_op.poll();
    self.objects_op.poll();

    // only request another frame if something is pending
    if self.buckets_op.pending || self.objects_op.pending {
        ui.ctx().request_repaint();
    }

    // render based on current state
    // ...
}
```

### eframe 0.34 API

Using the new split trait (App::update replaced with App::logic + App::ui):
- `logic()` -- called every frame, handles non-UI state (poll async ops here)
- `ui()` -- called when repainting, receives `&mut egui::Ui`
- `Ui` derefs to `Context`, so `ui.input()` works directly

## S3 client

Raw HTTP via reqwest. No AWS SDK. Parses S3 XML responses with quick-xml.

Operations:
- `list_buckets(endpoint)` -- GET /
- `list_objects(endpoint, bucket, prefix, delimiter)` -- GET /bucket?list-type=2&...
- `put_object(endpoint, bucket, key, data, content_type)` -- PUT /bucket/key
- `get_object(endpoint, bucket, key)` -- GET /bucket/key
- `head_object(endpoint, bucket, key)` -- HEAD /bucket/key
- `delete_object(endpoint, bucket, key)` -- DELETE /bucket/key

Auth: AWS Signature V4 when credentials are configured. Skipped for no-auth endpoints.

Works with any S3-compatible server: AbixIO, AWS, MinIO, Backblaze B2, etc.

## AbixIO detection

On connect, the UI probes `GET /_abixio/status`:

- 200 + JSON response: this is an AbixIO server. Enable management tabs
  (disk health, object inspector, config viewer).
- 404 or non-JSON: generic S3 endpoint. Show S3 features only.

## AbixIO management API

When connected to AbixIO, the UI calls these endpoints (served by AbixIO server):

```
GET  /_abixio/status        -> {version, uptime, data_shards, parity_shards, disk_count}
GET  /_abixio/disks         -> [{path, online, used_bytes, free_bytes, shard_count}, ...]
GET  /_abixio/object-info   -> ?bucket=X&key=Y -> {size, etag, erasure, shards: [{disk, index, checksum, present}, ...]}
GET  /_abixio/heal/status   -> {mrf_queue_depth, scanner_progress, last_scan_time}
POST /_abixio/heal/trigger  -> force immediate integrity scan
```

All return JSON. These endpoints are AbixIO-specific and do not exist on other
S3 servers.

## Layout

Three-panel layout:

```
+---+-- center content --+----- right detail ----+
|nav|                     |                       |
| B | bucket + object     | context-dependent     |
| D | browser, or admin   | metadata panel        |
| C | dashboard           |                       |
| H |                     | appears on selection  |
| + |                     | hides on ESC/close    |
+---+---------------------+-----------------------+
40px     flexible              280px
```

- **Left**: icon rail (40px, fixed). Section navigation.
- **Center**: main content. Changes based on selected section.
- **Right**: detail panel. Shows full metadata for selected object/bucket.
  Hidden when nothing is selected.

## Design rules

### Color

- **Red is reserved for errors and destructive actions only.** A red element
  that is not an error confuses users. Never use red for accent, selection,
  or branding.
- **Accent color: teal (#2dd4bf).** Used for selection highlights and active
  states. High contrast against dark backgrounds.
- **Text contrast:** primary text (#eeeeee) on dark panels (#1a1c2e).
  Labels use muted (#8899aa). Minimum 4.5:1 contrast ratio.
- **Links: bright blue (#5cb8ff).** Distinct from body text, not red.

### Theme

- Dark mode is the default.
- Themes will be configurable in settings (future).
- All color constants are defined in one place (`src/app.rs` theme section)
  for easy swapping.

### Detail panel

When an object is selected, the right panel fires a HEAD request to get full
HTTP headers, then displays:

1. **Filename** (large) + full path (small, muted)
2. **Overview**: size, content type, last modified, ETag
3. **Storage**: bucket, key, prefix
4. **HTTP Headers**: all raw response headers
5. **Erasure Shards**: per-disk shard info (AbixIO only)
6. **Actions**: Download, Delete (Delete styled as destructive/red)

## Dependencies

- `eframe` -- egui desktop wrapper (windowing, rendering)
- `egui_extras` -- table widget, image support
- `reqwest` -- async HTTP client
- `tokio` -- async runtime (for background network thread)
- `quick-xml` -- S3 XML response parsing
- `serde` / `serde_json` -- serialization
- `rfd` -- native file dialogs (upload/download)
- `keyring` -- OS keychain access for secret keys
- `clap` -- CLI argument parsing
