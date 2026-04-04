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

## What we avoid

| Anti-pattern | Why | Iced way |
|---|---|---|
| Blocking in update() | Freezes UI | Use Task::perform for async |
| Mutation in view() | View must be pure | Only read state in view() |
| Manual request_repaint | iced handles reactivity | Return Task from update() |
| Boolean loading flags | Less idiomatic | Use enum states (Loading/Loaded/Error) |
| Ignoring Err results | User doesn't see failures | Display error in UI |
