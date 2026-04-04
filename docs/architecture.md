# Architecture

## Overview

abixio-ui is a native desktop app built with [iced](https://iced.rs) 0.14.
It connects to any S3-compatible endpoint over HTTP. When connected to an
AbixIO server, additional management views are enabled automatically.

```
+-------------------+     HTTP/S3      +-------------------+
|   abixio-ui       | <=============> |  S3 endpoint      |
|   (desktop app)   |   /_admin/*     |  (any S3 server)  |
+-------------------+                  +-------------------+
```

When connected to an AbixIO server, the UI also communicates via `/_admin/*`
JSON endpoints for disk health, healing status, and per-object inspection/heal
actions from the selected-object detail panel. AbixIO is auto-detected on
connect via `/_admin/status`.

## Components

```
src/
  main.rs             # iced::application() entry point
  app.rs              # App state, Message enum, update(), view()
  async_op.rs         # AsyncOp helper (used by tests, not the app)
  perf.rs             # performance stats (5m sliding window)
  config.rs           # settings.json persistence (connections + regions, no secrets)
  keychain.rs         # OS keychain wrapper (Windows/macOS/Linux)
  s3/
    mod.rs
    client.rs         # thin wrapper around rust-s3 Bucket API
  abixio/
    mod.rs
    client.rs         # admin API client (reqwest + Sig V4 signing)
    types.rs          # JSON response types for /_admin/* endpoints
  views/
    mod.rs
    sidebar.rs        # left icon rail (shows D/H tabs for AbixIO)
    buckets.rs        # bucket list + browse_view (bucket panel + object panel)
    connections.rs    # connection manager UI
    disks.rs          # disk health dashboard (AbixIO only)
    healing.rs        # healing status + scanner stats (AbixIO only)
    objects.rs        # object table with prefix navigation, filter, and recursive find
    detail.rs         # right context panel (selected object metadata + AbixIO object admin)
    settings.rs       # settings view (theme, perf stats, about)
    testing.rs        # in-app end-to-end smoke tests
```

## Elm architecture (iced pattern)

iced uses the Elm architecture: Model-View-Update (MVU).

**Boot:** `App::new(endpoint, creds) -> (App, Task<Message>)`
- Creates initial state
- Returns an initial bucket-list Task only when an endpoint is provided on the CLI
- Without a CLI endpoint, starts on the Connections view with no startup network request

**Update:** `App::update(&mut self, Message) -> Task<Message>`
- Receives a Message (user action or async result)
- Mutates state
- Returns Task for any async work needed
- Never blocks. File dialogs are the one exception.

**View:** `App::view(&self) -> Element<'_, Message>`
- Pure function of state, no mutation
- Returns widget tree that iced diffs against previous frame
- Only redraws widgets whose output actually changed (reactive rendering)

**Subscription:** `App::subscription(&self) -> Subscription<Message>`
- Keyboard listener (ESC -> ClearSelection)
- No polling, no timers

## Async model

iced handles async natively via `Task::perform(future, message_mapper)`.
No manual channels, no runtime management, no request_repaint.

```rust
// fire async request
Task::perform(
    async move { client.list_buckets().await },
    Message::BucketsLoaded,
)

// handle result in update()
Message::BucketsLoaded(Ok(buckets)) => {
    self.buckets = Some(Ok(buckets));
    Task::none()
}
Message::BucketsLoaded(Err(e)) => {
    self.buckets = Some(Err(e));
    Task::none()
}
```

iced manages the tokio runtime internally. We don't create or manage one.
`async_op.rs` exists only for the CPU idle tests. The app uses `Task::perform`.

## Reactive rendering

iced 0.14 uses reactive rendering by default:
- Widgets only redraw when their state changes
- Mouse movement over non-interactive areas = zero redraws
- No `request_repaint` calls anywhere in our code
- Framework handles all repaint scheduling

This is fundamentally different from immediate mode (egui) where every OS
event triggers a full UI rebuild.

## S3 client

Uses [rust-s3](https://github.com/durch/rust-s3) v0.37 for all S3 operations.
Thin wrapper in `s3/client.rs` maps rust-s3 types to our app types.

Features provided by rust-s3:
- AWS Signature V4 request signing (hmac-sha256)
- Anonymous access (no credentials)
- Custom endpoints via `Region::Custom`
- Path-style bucket addressing (for MinIO, AbixIO, etc.)
- Server-side copy via `CopyObject` API
- Multipart upload support

Operations:
- `list_buckets()`. Lists all buckets.
- `list_objects(bucket, prefix, delimiter)`. Lists objects with prefix and delimiter support.
- `list_objects_recursive(bucket, prefix)`. Flat listing for find/search.
- `create_bucket(bucket)`. Creates a new bucket.
- `delete_bucket(bucket)`. Deletes a bucket.
- `put_object(bucket, key, data, content_type)`. Uploads an object.
- `get_object(bucket, key)`. Downloads an object.
- `head_object(bucket, key)`. Gets object metadata (ETag, size, content type, headers).
- `delete_object(bucket, key)`. Deletes an object.
- `copy_object(src_bucket, src_key, dst_bucket, dst_key)`. Server-side copy. For same-bucket operations, uses the S3 `CopyObject` API where data never leaves the server. For cross-bucket copies on the same endpoint, falls back to GET + PUT.

### Copy and move strategy

S3 has no native move or rename operation. Move is always copy-then-delete.

For copies within the same bucket, the S3 `CopyObject` API tells the server
to copy data internally. The data never leaves the server, and the server
guarantees integrity. This is what MinIO Client (`mc cp` and `mc mv`) uses
for same-server operations. The server returns an ETag in the response to
confirm the copy succeeded.

For cross-bucket copies on the same endpoint, rust-s3 only exposes
same-bucket server-side copy (`copy_object_internal`). Cross-bucket falls
back to downloading the object and re-uploading it. This is still correct
but uses more bandwidth.

For move operations, the source is only deleted after the copy returns
success. If the copy fails for any reason, the source is untouched.

Works with any S3-compatible server: AbixIO, AWS, MinIO, Backblaze B2, etc.

## Connection manager

Each connection is a single profile stored in `~/.abixio-ui/settings.json`:

- **On disk**: name, endpoint, region (no secrets)
- **OS keychain**: access key + secret key (encrypted by OS)

If a connection has no keychain entries, it connects anonymously.
Editing an existing connection with blank key fields keeps the existing
keychain entries; there is not yet a separate "clear stored keys" UI.
See [docs/credentials.md](credentials.md) for full details.

## AbixIO detection

On connect, the UI probes `GET /_admin/status`:

- 200 + JSON with `"server": "abixio"`: set `is_abixio = true`, show D (Disks) and H (Healing) tabs in sidebar, auto-fetch disk + heal data
- 404 or error: generic S3 server, admin tabs hidden

The admin client (`src/abixio/client.rs`) signs requests with Sig V4 using the same credentials as S3.
The app exposes the Disks and Healing views in normal UI flow. For selected
objects on AbixIO connections, the detail panel also fetches `/_admin/object`
and exposes a confirmed manual heal action via `POST /_admin/heal`.

### Admin API endpoints (served by AbixIO server)

| Endpoint | Method | Description |
|----------|--------|-------------|
| `/_admin/status` | GET | Server identity, version, uptime, erasure config |
| `/_admin/disks` | GET | Per-disk status, space, bucket/object counts |
| `/_admin/heal` | GET | MRF queue depth, scanner stats |
| `/_admin/heal?bucket=X&key=Y` | POST | Trigger manual heal for one object from the detail panel |
| `/_admin/object?bucket=X&key=Y` | GET | Per-shard status for one selected object |

## Layout

Three-panel layout:

```
+---+-- center content --+----- right detail ----+
|nav|                     |                       |
| B | bucket + object     | context-dependent     |
|   | browser, or admin   | metadata panel        |
|   | dashboard           |                       |
| + |                     | appears on selection  |
| T |                     |                       |
| S |                     | hides on ESC/close    |
+---+---------------------+-----------------------+
40px     flexible              280px
```

- **Left**: icon rail (40px). B=Browse, +=Connections, T=Testing, S=Settings.
  D=Disks and H=Healing appear only for AbixIO connections.
- **Center**: main content, changes based on selected section
- **Right**: detail panel, shows metadata for the selected object.
  Hidden when nothing is selected. Fires a HEAD request on object selection,
  and on AbixIO connections also fetches object inspection in parallel.

## Design rules

### Color

- **Red is for errors and destructive actions only**
- Currently using stock iced `Theme::Dark` / `Theme::Light`
- Custom theme colors planned (teal accent, high contrast)

### Theme

- Dark mode is the default
- Two options: Dark, Light. Switchable in Settings > Appearance
- Uses iced's built-in `Theme::Dark` / `Theme::Light`

### Error handling

- All async `Err` results are displayed in a dismissable error bar at bottom
- Errors are never silently dropped

### Detail panel

When an object is selected, the right panel fires a HEAD request, then
displays:

1. **Filename** (large) + full path (small)
2. **Overview**: size, content type, last modified, ETag
3. **Storage**: bucket, key
4. **HTTP Headers**: selected response headers plus object metadata entries
5. **Actions**: Download, Delete
6. **AbixIO** (when applicable): erasure summary, shard distribution,
   per-shard status/checksum, Refresh Inspect, Heal Object

## Testing tab

The Testing tab runs end-to-end smoke checks against the currently connected
server. For AbixIO endpoints it also exercises the status, disks, healing, and
object-inspection admin calls.

## Dependencies

- `iced` 0.14. GUI framework with reactive rendering and Elm architecture.
- `rust-s3` 0.37. S3 client with Sig V4 signing. It brings in `reqwest` and `quick-xml`.
- `tokio`. Async runtime managed by iced internally.
- `keyring` 3. OS keychain access for Windows Credential Manager, macOS Keychain, and Linux secret-service.
- `dirs` 6. Cross-platform home directory resolution.
- `serde` / `serde_json`. Serialization.
- `rfd`. Native file dialogs for upload and download.
- `clap`. CLI argument parsing.
- `tracing`. Logging.
