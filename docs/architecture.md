# Architecture

## Overview

abixio-ui is a native desktop app built with [iced](https://iced.rs) 0.14.
It connects to any S3-compatible endpoint over HTTP. When connected to an
AbixIO server, additional management features are enabled automatically.

```
+-------------------+     HTTP/S3      +-------------------+
|   abixio-ui       | <=============> |  S3 endpoint      |
|   (desktop app)   |                  |  (any S3 server)  |
+-------------------+                  +-------------------+
```

## Components

```
src/
  main.rs             # iced::application() entry point
  app.rs              # App state, Message enum, update(), view()
  async_op.rs         # AsyncOp helper (used by tests, not the app)
  perf.rs             # performance stats (5m sliding window)
  s3/
    mod.rs
    client.rs         # raw S3 HTTP calls via reqwest
    xml.rs            # S3 XML response deserialization
  views/
    mod.rs
    sidebar.rs        # left icon rail navigation
    buckets.rs        # bucket list + browse_view (bucket panel + object panel)
    objects.rs        # object table with prefix navigation
    detail.rs         # right context panel (object/bucket metadata)
    settings.rs       # settings view (theme, perf stats, about)
```

## Elm architecture (iced pattern)

iced uses the Elm architecture: Model-View-Update (MVU).

**Boot:** `App::new(endpoint) -> (App, Task<Message>)`
- Creates initial state
- Returns initial Task to fetch bucket list

**Update:** `App::update(&mut self, Message) -> Task<Message>`
- Receives a Message (user action or async result)
- Mutates state
- Returns Task for any async work needed
- Never blocks -- file dialogs are the one exception (known limitation)

**View:** `App::view(&self) -> Element<Message>`
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
`async_op.rs` exists only for the CPU idle tests -- the app uses Task::perform.

## Reactive rendering

iced 0.14 uses reactive rendering by default:
- Widgets only redraw when their state changes
- Mouse movement over non-interactive areas = zero redraws
- No `request_repaint` calls anywhere in our code
- Framework handles all repaint scheduling

This is fundamentally different from immediate mode (egui) where every OS
event triggers a full UI rebuild.

## S3 client

Raw HTTP via reqwest. No AWS SDK. Parses S3 XML responses with quick-xml.

Operations:
- `list_buckets(endpoint)` -- GET /
- `list_objects(endpoint, bucket, prefix, delimiter)` -- GET /bucket?list-type=2&...
- `put_object(endpoint, bucket, key, data, content_type)` -- PUT /bucket/key
- `get_object(endpoint, bucket, key)` -- GET /bucket/key
- `head_object(endpoint, bucket, key)` -- HEAD /bucket/key (returns all headers)
- `delete_object(endpoint, bucket, key)` -- DELETE /bucket/key

Works with any S3-compatible server: AbixIO, AWS, MinIO, Backblaze B2, etc.

## AbixIO detection

On connect, the UI will probe `GET /_abixio/status` (not yet implemented):

- 200 + JSON: AbixIO server, enable management tabs
- 404 or error: generic S3, show S3 features only

## Layout

Three-panel layout:

```
+---+-- center content --+----- right detail ----+
|nav|                     |                       |
| B | bucket + object     | context-dependent     |
|   | browser, or admin   | metadata panel        |
|   | dashboard           |                       |
| + |                     | appears on selection  |
| S |                     | hides on ESC/close    |
+---+---------------------+-----------------------+
40px     flexible              280px
```

- **Left**: icon rail (40px). B=Browse, +=Connections, S=Settings
- **Center**: main content, changes based on selected section
- **Right**: detail panel, shows full metadata for selected object/bucket.
  Hidden when nothing selected. Fires HEAD request on selection.

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

When an object is selected, the right panel fires a HEAD request to get full
HTTP headers, then displays:

1. **Filename** (large) + full path (small)
2. **Overview**: size, content type, last modified, ETag
3. **Storage**: bucket, key
4. **HTTP Headers**: all raw response headers
5. **Actions**: Download, Delete

## Dependencies

- `iced` 0.14 -- GUI framework (reactive rendering, Elm architecture)
- `reqwest` -- async HTTP client
- `tokio` -- async runtime (managed by iced internally)
- `quick-xml` -- S3 XML response parsing
- `serde` / `serde_json` -- serialization
- `rfd` -- native file dialogs (upload/download)
- `clap` -- CLI argument parsing
- `tracing` -- logging
