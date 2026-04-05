# abixio-ui

Native desktop S3 manager and AbixIO admin UI. Built with
[iced](https://iced.rs) and [aws-sdk-s3](https://docs.rs/aws-sdk-s3).

## Status

**Early development. Not production-ready.**

This project is 2 days old (first commit 2026-04-04). It is ~11k lines of Rust
across 85 commits, written almost entirely by AI with human direction. The core
browse, upload, download, copy, sync, and admin workflows function against real
S3 endpoints, but the project has no releases, no packaging, no user base, and
no field hours.

What works today:
- S3 object CRUD, versioning, tagging, policy, lifecycle, presigned URLs
- Recursive diff/copy/sync with concurrent transfers and rclone-style filters
- AbixIO admin (disks, healing, shard inspection, manual heal)
- 145 automated tests including integration tests against real abixio servers

What is honest:
- No CI beyond `cargo check/clippy/test` on Windows (no cross-platform testing)
- No installer, no release binaries, no update mechanism
- Sync bandwidth enforcement fields exist in the UI but do nothing
- Object content preview is limited to 4KB text
- No retention, encryption, replication, or quota controls
- CLI/scripting surface is zero -- this is a GUI-only tool
- The integration test suite finds real bugs in the abixio server (tagging,
  chunked encoding in relay, bucket cleanup) that are not yet fixed

If you need a production S3 tool today, use
[mc](https://github.com/minio/mc) or [rclone](https://rclone.org).

## Features

- **S3 object browser.** Buckets, prefix navigation, breadcrumbs, filter, recursive find, upload (multipart for large files), download, copy, move, rename, delete (single, multi-select, batch, recursive prefix).
- **Object versioning.** Enable/suspend versioning per bucket. Version list in detail panel with restore and delete.
- **Bucket policy and lifecycle editors.** Inline JSON policy editor and inline XML lifecycle editor in the bucket detail panel.
- **Object tagging.** View, add, remove tags in the detail panel.
- **Multi-server connections.** Save, edit, test, switch. OS keychain for credentials.
- **AWS Sig V4 auth.** Works with AWS, MinIO, AbixIO, Backblaze, or any S3-compatible endpoint.
- **AbixIO admin.** Auto-detected when connected to AbixIO. Disk health, healing monitor, shard inspection, manual heal.
- **Built-in smoke tests.** End-to-end S3 API checks including tagging and versioning.

## Usage

```bash
abixio-ui                                          # connection manager
abixio-ui --endpoint http://localhost:10000         # direct connect
abixio-ui --endpoint https://s3.amazonaws.com \
  --access-key AKIA... --secret-key wJalr...        # with credentials
```

## Build

```bash
cargo build --release
```

## Not yet implemented

- Presigned upload URLs (download URLs implemented)
- Bandwidth enforcement and watch mode for sync (see `docs/sync.md`)
- Full inline content viewer (first 4KB text preview exists)

## Documentation

| Doc | Subject |
|---|---|
| [s3.md](docs/s3.md) | S3 SDK config, every operation, UI features, response handling |
| [features.md](docs/features.md) | MinIO client parity tracking |
| [architecture.md](docs/architecture.md) | App architecture, layout, async model |
| [sync.md](docs/sync.md) | Sync design, performance model, and phased rollout |
| [credentials.md](docs/credentials.md) | OS keychain credential storage |
| [testing.md](docs/testing.md) | In-app smoke tests and automated integration test harness |

## Related

- **[abixio](https://github.com/abix-/abixio)** -- the S3-compatible object store server. Erasure coding, versioning, multipart upload, 196 tests. This UI connects to it (and any other S3-compatible endpoint).

## License

[GNU General Public License v3.0](LICENSE)
