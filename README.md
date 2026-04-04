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

abixio-ui is a standalone native desktop app built with [iced](https://iced.rs) 0.14. It connects to any S3-compatible endpoint over HTTP. No AWS SDK required -- it speaks raw S3 protocol.

When connected to an [AbixIO](https://github.com/abix-/abixio) server, additional management features (disk health, shard inspection, config) are automatically enabled.

iced 0.14 uses reactive rendering -- widgets only redraw when their state actually changes. Mouse movement over non-interactive areas causes zero redraws.

### Data authority

- **S3 endpoint** is the single source of truth for all bucket/object data
- **OS keychain** stores secret keys (planned, not yet implemented)
- **Local config** (`~/.abixio-ui/connections.json`) stores connection info only (planned)
- No local caching -- every navigation action fetches live from the server

## Usage

```bash
# connect to local AbixIO
abixio-ui --endpoint http://localhost:9000

# connect to any S3 endpoint
abixio-ui --endpoint http://minio.home:9000
```

## Build

```bash
cargo build --release
```

## What works / what doesn't

**Done:**
- Three-panel layout: icon sidebar + center content + right detail panel
- Dark / Light theme switching in Settings
- S3 client (raw HTTP via reqwest, XML parsing)
- Bucket list sidebar with create bucket
- Object browser with breadcrumb navigation and prefix drilling
- Object detail panel: full metadata from HEAD request (size, type, etag, all HTTP headers)
- Upload via native file dialog
- Download via native save dialog
- Delete with error display
- ESC keyboard shortcut to close detail panel
- Error bar with dismiss for failed operations
- Settings view: theme, connection info, performance stats, about
- Performance stats: message count, frame time tracking (5m sliding window)

**Not yet implemented:**
- Connection manager (multi-server, saved connections)
- OS keychain credential storage
- AbixIO-specific features (disk health, object inspector, config)
- Auth (AWS Sig V4 signing)
- Custom theme colors (using stock iced Dark/Light for now)

See [docs/](docs/) for architecture, data authority, performance, and iced standards.

## License

[GNU General Public License v3.0](LICENSE)
