# AbixIO UI

## Status

**Early development. Not production-ready.**

This repository is the desktop UI for the AbixIO server. It is a native Rust GUI for browsing S3-compatible storage and for managing AbixIO-specific admin features such as disk health, healing, and shard inspection.

Current reality as of 2026-04-05:
- First commit: 2026-04-04
- No installer, release binaries, or update mechanism
- 145 automated tests, including integration coverage against real AbixIO servers
- Core browse, upload, download, copy, sync, and admin flows work against live S3 endpoints
- This is still a young project with no production history or field time

What works today:
- Bucket and object browsing with recursive navigation and search
- Upload, download, copy, move, rename, and delete workflows
- Multipart upload support for large objects
- Object versioning, tagging, bucket policy, and lifecycle editing
- Recursive diff, copy, and sync with concurrent transfers and rclone-style filters
- Multi-server connection management with OS keychain credential storage
- AbixIO admin screens for disks, healing, shard inspection, and manual heal
- Built-in smoke tests plus automated integration tests against the AbixIO server

What is still missing or rough:
- No CI beyond `cargo check`, `cargo clippy`, and `cargo test` on Windows
- No cross-platform validation or packaging
- Sync bandwidth controls exist in the UI but do nothing yet
- Object content preview is limited to the first 4 KB of text
- No retention, encryption, replication, or quota management controls
- No CLI or scripting surface; this is GUI-only
- Integration tests still expose real server-side bugs that are not fixed yet

If you need a production S3 client today, use [mc](https://github.com/minio/mc) or [rclone](https://rclone.org).

## What AbixIO UI Is

AbixIO UI is the native desktop client and admin console for the AbixIO server.

It can also connect to other S3-compatible endpoints, but that is secondary. The primary purpose of this repo is to give AbixIO server users a desktop interface for:
- Browsing buckets and objects
- Running file transfer workflows
- Managing bucket-level S3 features
- Inspecting AbixIO server health and healing state
- Investigating shard placement and repair operations

Built with [iced](https://iced.rs) and [aws-sdk-s3](https://docs.rs/aws-sdk-s3).

## Features

- **Desktop S3 browser.** Buckets, prefix navigation, breadcrumbs, filters, recursive find, upload, download, copy, move, rename, and delete.
- **AbixIO server admin.** Auto-detects AbixIO servers and exposes disk health, healing monitor, shard inspection, and manual heal actions.
- **Object versioning.** Enable or suspend bucket versioning and manage object versions from the detail panel.
- **Tagging, policy, and lifecycle editors.** Inline editing for object tags, bucket policy JSON, and lifecycle XML.
- **Multi-server connections.** Save, edit, test, and switch between endpoints with OS keychain credential storage.
- **AWS Signature V4 authentication.** Works with AbixIO and other S3-compatible endpoints.
- **Built-in verification tools.** Includes in-app smoke tests and automated integration coverage.

## Usage

```bash
abixio-ui                                            # connection manager
abixio-ui --endpoint http://localhost:10000         # connect to local AbixIO server
abixio-ui --endpoint https://s3.amazonaws.com \
  --access-key AKIA... --secret-key wJalr...        # connect to another S3 endpoint
```

## Build

```bash
cargo build --release
```

## Not Yet Implemented

- Presigned upload URLs
- Working bandwidth enforcement and watch mode for sync
- Full inline content viewer beyond the current 4 KB text preview

## Documentation

| Doc | Subject |
|---|---|
| [s3.md](docs/s3.md) | S3 SDK config, operations, UI features, response handling |
| [features.md](docs/features.md) | MinIO client parity tracking |
| [architecture.md](docs/architecture.md) | App architecture, layout, async model |
| [sync.md](docs/sync.md) | Sync design, performance model, phased rollout |
| [credentials.md](docs/credentials.md) | OS keychain credential storage |
| [testing.md](docs/testing.md) | In-app smoke tests and automated integration test harness |

## Related Project

- **[abixio](https://github.com/abix-/abixio)**: the AbixIO server that this UI is built to manage

## License

[GNU General Public License v3.0](LICENSE)
