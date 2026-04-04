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
the render loop.

```
UI thread (egui, 60fps):
  - renders every frame
  - on user action: sends Request to background via channel
  - each frame: polls channel for Response, updates state

Background thread (tokio):
  - receives Request messages from channel
  - makes HTTP calls via reqwest
  - sends Response messages back via channel
```

Channel: `std::sync::mpsc`. The UI thread never blocks -- it calls `try_recv()`
each frame.

```rust
// send request (non-blocking)
if ui.button("Refresh").clicked() {
    self.tx.send(Request::ListObjects { bucket, prefix });
    self.loading = true;
}

// poll for response (non-blocking)
if let Ok(Response::Objects(list)) = self.rx.try_recv() {
    self.objects = list;
    self.loading = false;
}
```

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
