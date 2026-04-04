# abixio-ui

**Status: early development -- compiles, basic S3 browsing works**

Native desktop S3 manager. Browse, upload, download, and manage objects on any S3-compatible endpoint.

## Planned features

- **S3 object browser** -- browse buckets, navigate prefixes, upload/download/delete
- **Multi-server** -- connect to multiple endpoints (AWS, MinIO, AbixIO, Backblaze, etc.)
- **Disk health** -- view disk status, space usage, healing progress (AbixIO servers only)
- **Object inspector** -- see shard distribution, checksums, erasure config (AbixIO servers only)
- **Config management** -- view/edit server configuration (AbixIO servers only)

## How it works

abixio-ui is a standalone native desktop app built with [iced](https://iced.rs) 0.14. It connects to any S3-compatible endpoint using the [rust-s3](https://github.com/durch/rust-s3) library with full AWS Signature V4 authentication support.

When connected to an [AbixIO](https://github.com/abix-/abixio) server, additional management features (disk health, shard inspection, config) are automatically enabled.

iced 0.14 uses reactive rendering -- widgets only redraw when their state actually changes. Mouse movement over non-interactive areas causes zero redraws.

### Data authority

- **S3 endpoint** is the single source of truth for all bucket/object data
- **OS keychain** stores access keys and secret keys (Windows Credential Manager / macOS Keychain / Linux secret-service)
- **Local config** (`~/.abixio-ui/settings.json`) stores connection profiles (name, endpoint, region -- no secrets)
- No local caching -- every navigation action fetches live from the server

## Usage

```bash
# connect to local AbixIO (no auth)
abixio-ui --endpoint http://localhost:10000

# connect to any S3 endpoint with credentials
abixio-ui --endpoint https://s3.us-west-2.amazonaws.com --access-key AKIA... --secret-key wJalr...

# launch without args to manage connections in the UI
abixio-ui
```

## Build

```bash
cargo build --release
```

## What works / what doesn't

**Done:**
- Three-panel layout: icon sidebar + center content + right detail panel
- Dark / Light theme switching in Settings
- S3 client via [rust-s3](https://github.com/durch/rust-s3) with AWS Sig V4 signing
- **Connection manager** -- add, edit, remove, test, switch connections from the UI
- **OS keychain storage** -- both access keys and secret keys stored in OS keychain, never on disk
- Bucket list sidebar with create bucket
- Object browser with breadcrumb navigation and prefix drilling
- Object detail panel: full metadata from HEAD request (size, type, etag, headers)
- Upload via native file dialog
- Download via native save dialog
- Delete with error display
- ESC keyboard shortcut to close detail panel
- Error bar with dismiss for failed operations
- Settings view: theme, connection info, performance stats, about
- Performance stats: message count, frame time tracking (5m sliding window)
- CLI args: `--endpoint`, `--access-key`, `--secret-key` for scripted use

**Not yet implemented:**
- AbixIO-specific features (disk health, object inspector, config)
- Custom theme colors (using stock iced Dark/Light for now)

See [docs/](docs/) for architecture, data authority, performance, and iced standards.

## License

[GNU General Public License v3.0](LICENSE)
