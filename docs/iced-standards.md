# Iced Standards

How we use iced 0.14 in abixio-ui, based on the official examples.

## Application setup

We use the functional API (not the Application trait):

```rust
iced::application(boot, update, view)
    .theme(App::theme)
    .title(App::title)
    .subscription(App::subscription)
    .window_size((1024.0, 768.0))
    .run()
```

Reference: `todos` example, `pane_grid` example.

## Elm architecture (MVU)

| Concept | Our implementation | Iced pattern |
|---|---|---|
| Model | `App` struct | State struct with all data |
| Update | `App::update(&mut self, Message) -> Task<Message>` | Match on Message, mutate state, return Task |
| View | `App::view(&self) -> Element<Message>` | Pure function of state, no mutation |
| Boot | `App::new(endpoint) -> (Self, Task<Message>)` | Initial state + initial async tasks |

## Messages

All user actions and async results are Message variants:

```rust
enum Message {
    // user actions
    SelectBucket(String),
    Upload,
    Delete(String, String),

    // async results
    BucketsLoaded(Result<Vec<BucketInfo>, String>),
    DeleteDone(Result<(), String>),
}
```

Reference: `todos` example uses `Message::Loaded(Result<...>)` pattern.

## Async operations

Use `Task::perform(future, message_mapper)`:

```rust
Task::perform(
    async move { client.list_buckets().await },
    Message::BucketsLoaded,
)
```

- Never block `update()` with synchronous I/O
- Map async results to Message variants
- Handle both Ok and Err in the result Message handler
- Reference: `download_progress` example

## Keyboard events

Use `subscription()` with `keyboard::listen()`:

```rust
fn subscription(&self) -> Subscription<Message> {
    keyboard::listen().filter_map(|event| match event {
        keyboard::Event::KeyPressed { key, .. } => match key {
            keyboard::Key::Named(key::Named::Escape) => Some(Message::ClearSelection),
            _ => None,
        },
        _ => None,
    })
}
```

Reference: `todos` example uses subscriptions for Tab key.

## Error handling

Every async result is `Result<T, String>`. Both Ok and Err must be handled
in the update() match arm. Errors are stored in state and displayed in view:

```rust
Message::DeleteDone(Err(e)) => {
    self.error = Some(e);
    Task::none()
}
```

Silently dropping errors is a bug.

## View composition

Views are pure functions that return `Element<Message>`. No mutation in views.
Decompose complex UIs into methods on App:

```rust
fn view(&self) -> Element<Message> {
    column![
        self.top_bar(),
        self.main_content(),
    ].into()
}
```

## Theme

Use `Theme::custom()` for custom palettes. Stock `Theme::Dark` / `Theme::Light`
for now, custom colors are a future improvement.

## File I/O in Tasks

Always use `tokio::fs` (async) inside `Task::perform`, never `std::fs` (sync).
Blocking I/O on the tokio thread pool stalls other tasks.

```rust
// GOOD: non-blocking
Task::perform(async move {
    let data = tokio::fs::read(&file).await.map_err(|e| e.to_string())?;
    client.put_object(&bucket, &key, data, "application/octet-stream").await
}, Message::UploadDone)

// BAD: blocks tokio thread
Task::perform(async move {
    let data = std::fs::read(&file).map_err(|e| e.to_string())?; // BLOCKING
    ...
}, ...)
```

## Performance metrics

`perf.record_frame()` is called in `update()`, not `view()`. This counts
state updates, not actual renders. In iced, each `update()` triggers at most
one `view()` call, so these closely approximate actual frames. The metrics
are labeled "Updates" not "FPS" to be accurate.

`view()` takes `&self` (immutable) so mutable perf counters can't live there
without interior mutability. Counting in `update()` is the pragmatic choice.

## Known limitations

### File dialogs block the UI thread

`rfd::FileDialog::pick_file()` and `save_file()` are synchronous calls in
`update()`. This freezes the window until the user picks a file or cancels.
There is no async file dialog API available on Windows via rfd.

This is documented and accepted. The freeze is brief (user is interacting
with the dialog) and doesn't cause data loss or corruption.

## What we avoid

| Anti-pattern | Why | Iced way |
|---|---|---|
| Blocking I/O in Task | Stalls tokio thread pool | Use `tokio::fs` not `std::fs` |
| Blocking in update() | Freezes UI | Use Task::perform for async |
| Mutation in view() | View must be pure | Only read state in view() |
| Manual request_repaint | iced handles reactivity | Return Task from update() |
| Boolean loading flags | Less idiomatic | Use enum states (Loading/Loaded/Error) |
| Ignoring Err results | User doesn't see failures | Display error in UI |
| Calling metrics "FPS" | update() != render | Label as "Updates/sec" |
