# todo

## ~~1. split app.rs~~ (done)

app.rs is 3,355 lines (45% of codebase). extract into modules that mirror the
existing views/ structure.

- extract Message enum into `src/message.rs`
- extract App struct fields into domain-grouped sub-structs (connection state,
  transfer state, admin state, testing state, share/presign state, bucket
  detail state, bulk delete state)
- move update() match arms into per-domain handler functions in separate files
  (e.g. `src/handlers/transfer.rs`, `src/handlers/admin.rs`,
  `src/handlers/connection.rs`)
- keep App::view() dispatching to views/ as-is -- that structure already works
- target: app.rs under 500 lines, just wiring

## ~~2. fix stale docs~~ (done)

features.md API parity table lists 7 operations as "not yet wired" that are
shipping in the code:

- GetBucketLifecycle (app.rs:2403, s3/client.rs:647)
- DeleteBucketLifecycle (app.rs:1904, s3/client.rs:682)
- GetBucketPolicy (app.rs:2394, s3/client.rs:608)
- DeleteBucketPolicy (app.rs:1885, s3/client.rs:635)
- Presign GET (app.rs:1852, s3/client.rs:583)
- GetBucketVersioning (app.rs:2436, s3/client.rs:454)
- PutBucketVersioning (app.rs:2007)

also fix:

- README.md "not yet implemented" still lists presigned sharing links -- remove
  that line, it ships
- features.md parity scores for presign, policy, lifecycle, versioning should
  reflect actual implementation
- any other "not yet wired" / "no" entries that are now "yes"

## 3. wire or kill perf counters

perf.rs (189 lines) feeds fake numbers to the settings view. features.md line
157 confirms "request and byte metrics are not currently wired to real network
activity."

option a: instrument S3Client methods to increment atomic counters for
requests sent and bytes transferred, feed those into PerfStats.
option b: delete perf.rs and remove the metrics section from settings view.

do not ship fake dashboards.

## 4. unit tests for s3/client.rs and config.rs

s3/client.rs (789 lines) and config.rs (102 lines) have zero unit tests.
smoke tests cover happy paths but miss:

- s3/client.rs: error mapping from SDK errors to String, empty list responses,
  malformed server responses, presign config edge cases (0 expiry, huge expiry),
  copy_source formatting for cross-bucket copy, batch delete chunking at 1000
  boundary
- config.rs: missing settings file (first launch), corrupt JSON, missing fields
  (forward compat), connection with empty name/endpoint

use mockall or manual mocks for the aws-sdk-s3 client trait if needed.

## 5. multipart upload

aws-sdk-s3 supports CreateMultipartUpload / UploadPart / CompleteMultipartUpload
natively. without this, uploads over ~5GB fail silently on most S3 backends.

- add multipart methods to S3Client: create_multipart, upload_part,
  complete_multipart, abort_multipart
- threshold: files > 100MB use multipart (configurable)
- part size: 8MB default
- wire progress reporting back to UI via channel or periodic message
- add abort-on-cancel so partial uploads don't leak
- update import workflow to use multipart for large files

## 6. add CI

no github actions exist. add `.github/workflows/ci.yml`:

- trigger: push and pull_request
- steps: cargo build, cargo test, cargo clippy -- -D warnings
- matrix: stable rust on windows (primary target)
- optional: cargo fmt --check

## 7. move async_op.rs behind cfg(test)

async_op.rs (56 lines) is documented as "used by tests, not the app"
(architecture.md line 28). gate it with `#[cfg(test)]` or move into a test
helper module so it does not compile into release builds.
