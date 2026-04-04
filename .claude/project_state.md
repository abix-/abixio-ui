# abixio-ui project state

## last session: 2026-04-04

### what we worked on
- object search and filter feature matching mc find patterns
- local instant filter: text_input in toolbar filters loaded objects/folders by case-insensitive substring
- recursive find: "Find" button lists all objects under current prefix recursively, filters by wildcard or substring
- wildcard matching: supports `*` and `?` patterns, falls back to substring when no wildcards present
- docs updated: features.md parity score 3/10 -> 6/10, README, architecture

### decisions made
- **two-level approach**: local filter (instant, no network) + recursive find (network call) -- mirrors mc find's recursive list + client-side filter
- **wildcard syntax**: `*` matches any sequence, `?` matches one char, no wildcards = substring match. case-insensitive always
- **no external crate**: wildcard_match is ~30 lines, same approach as minio/pkg/wildcard
- **filter clears on navigation**: SelectBucket, NavigatePrefix, ConnectTo, Refresh, CreateBucketDone, BucketDeleted all clear filter and find results
- **find results as separate state**: `find_results` is independent of `objects`, shown as flat list with full key paths

### current state
- ui compiles clean, all tests pass (5 new wildcard tests)
- new in src/app.rs: wildcard_match(), ObjectFilterChanged/Find/FindComplete/ClearFind messages, object_filter/find_results/finding fields
- new in src/s3/client.rs: list_objects_recursive() method
- rewritten src/views/objects.rs: filter input + find button in toolbar, local filter on objects/folders, find results view with clear button, match count display
- not yet committed or tested against a live server

### next steps
- run against live abixio server to verify filter and find work
- run against live server to verify all existing e2e tests still pass
- object shard inspector (per-object view showing shard status on each disk)
- manual heal button in object detail panel
- bulk delete (multi-select)
- auto-refresh for disks/healing views (iced subscription timer)
- custom theme colors
