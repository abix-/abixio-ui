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
  app/
    mod.rs            # App state, Message enum, update(), view()
    types.rs          # App-owned state structs and workflow state
    transfer_ops.rs   # Shared transfer/import/export helpers
    handlers/         # Per-domain update handlers

  perf.rs             # performance stats (5m sliding window)
  config.rs           # settings.json persistence (connections + regions, no secrets)
  keychain.rs         # OS keychain wrapper (Windows/macOS/Linux)
  s3/
    mod.rs
    client.rs         # thin wrapper around aws-sdk-s3
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
    objects.rs        # object table with prefix navigation, filter, recursive find, multi-select
    detail.rs         # right context panel (metadata, actions, AbixIO admin, bulk delete modal)
    transfer.rs       # copy/move/rename modal, import/export workflows
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
The `tests/support/` helper exists only for the CPU idle tests. The app uses `Task::perform`.

## Reactive rendering

iced 0.14 uses reactive rendering by default:
- Widgets only redraw when their state changes
- Mouse movement over non-interactive areas = zero redraws
- No `request_repaint` calls anywhere in our code
- Framework handles all repaint scheduling

This is fundamentally different from immediate mode (egui), where every OS
event triggers a full UI rebuild.

## S3 client

Uses [aws-sdk-s3](https://docs.rs/aws-sdk-s3) (the official AWS SDK for Rust)
for all S3 operations. Thin wrapper in `s3/client.rs` maps SDK types to our
app types.

Previously used `rust-s3` 0.37, which was missing critical APIs (`DeleteObjects`
batch delete, `ListObjectVersions`, `GetBucketPolicy`, cross-bucket `CopyObject`).
Migrated to `aws-sdk-s3` to eliminate all API blockers.

Features provided by aws-sdk-s3:
- AWS Signature V4 request signing
- Anonymous access (empty credentials)
- Custom endpoints via `endpoint_url` (for MinIO, AbixIO, etc.)
- Path-style bucket addressing via `force_path_style`
- Server-side copy via `CopyObject` API (same-bucket and cross-bucket)
- Batch delete via `DeleteObjects` API (up to 1000 keys per request)
- Full S3 API surface (versioning, tagging, policies, lifecycle, presigning)

Operations (see `s3/client.rs` for full API):
- Object CRUD: `list_buckets`, `list_objects`, `list_objects_recursive`, `create_bucket`, `delete_bucket`, `put_object`, `get_object`, `head_object`, `delete_object`, `delete_objects` (batch), `copy_object`
- Tagging: `get_object_tags`, `put_object_tags`, `delete_object_tags`, `get_bucket_tags`, `put_bucket_tags`, `delete_bucket_tags`
- Versioning: `get_bucket_versioning`, `put_bucket_versioning`, `list_object_versions`, `get_object_version`, `delete_object_version`
- Presigning: `presign_get_object`
- Policy: `get_bucket_policy`, `put_bucket_policy`, `delete_bucket_policy`
- Lifecycle: `get_bucket_lifecycle`, `put_bucket_lifecycle`, `delete_bucket_lifecycle`

### Copy and move strategy

S3 has no native move or rename operation. Move is always copy-then-delete.

The S3 `CopyObject` API tells the server to copy data internally. The data
never leaves the server, and the server guarantees integrity. This is what
MinIO Client (`mc cp` and `mc mv`) uses for same-server operations. With
aws-sdk-s3, this works for both same-bucket and cross-bucket copies using
the `copy_source` parameter (`bucket/key` format).

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

When an object is selected, the right panel fires HEAD + tag + version +
preview requests in parallel, then displays:

1. **Filename** (large) + full path (small)
2. **Overview**: size, content type, last modified, ETag
3. **Storage**: bucket, key
4. **HTTP Headers**: response headers plus `x-amz-meta-*` entries
5. **Tags**: key-value tag list with add/remove (max 10)
6. **Versions**: version list with ID, date, size, restore, delete
7. **Preview**: first 4KB of object content as text
8. **Actions**: Download, Share, Copy, Move, Rename, Delete
9. **AbixIO** (when applicable): shard inspection, manual heal

When a bucket is selected:
1. **Overview**: bucket name, prefix
2. **Contents**: folder and object counts
3. **Versioning**: status + enable/suspend buttons
3. **Bucket Tags**: tag list with add/remove
4. **Policy**: inline JSON view, create, edit, delete
5. **Lifecycle**: inline XML view, create, edit, delete
6. **Actions**: Refresh, Delete Bucket

See [s3.md](s3.md) for full details on each operation and UI feature.

## Testing tab

The Testing tab runs end-to-end smoke checks against the currently connected
server. For AbixIO endpoints it also exercises the status, disks, healing, and
object-inspection admin calls.

## Dependencies

- `iced` 0.14. GUI framework with reactive rendering and Elm architecture.
- `aws-sdk-s3` 1.x. Official AWS S3 SDK for Rust. Full S3 API surface with SigV4 signing.
- `tokio`. Async runtime managed by iced internally.
- `keyring` 3. OS keychain access for Windows Credential Manager, macOS Keychain, and Linux secret-service.
- `dirs` 6. Cross-platform home directory resolution.
- `serde` / `serde_json`. Serialization.
- `rfd`. Native file dialogs for upload and download.
- `clap`. CLI argument parsing.
- `tracing`. Logging.
