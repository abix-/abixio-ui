# abixio-ui

**Status: early development -- compiles, not usable yet**

Native desktop S3 manager. Browse, upload, download, and manage objects on any S3-compatible endpoint.

## Planned features

- **S3 object browser** -- browse buckets, navigate prefixes, upload/download/delete
- **Multi-server** -- connect to multiple endpoints (AWS, MinIO, AbixIO, Backblaze, etc.)
- **Disk health** -- view disk status, space usage, healing progress (AbixIO servers only)
- **Object inspector** -- see shard distribution, checksums, erasure config (AbixIO servers only)
- **Config management** -- view/edit server configuration (AbixIO servers only)

## How it works

abixio-ui is a standalone native desktop app built with egui. It connects to any S3-compatible endpoint over HTTP. No AWS SDK required -- it speaks raw S3 protocol.

When connected to an [AbixIO](https://github.com/abix-/abixio) server, additional management features (disk health, shard inspection, config) are automatically enabled.

### Data authority

- **S3 endpoint** is the single source of truth for all bucket/object data
- **OS keychain** stores secret keys (Windows Credential Manager / macOS Keychain / Linux secret-service)
- **Local config** (`~/.abixio-ui/connections.json`) stores connection info only (no secrets)
- No local caching -- every navigation action fetches live from the server

### Architecture

```
+-------------------+     HTTP/S3      +-------------------+
|   abixio-ui       | <=============> |  S3 endpoint      |
|   (desktop app)   |                  |  (any S3 server)  |
+-------------------+                  +-------------------+
        |
        v
  ~/.abixio-ui/
    connections.json   # endpoint name, URL, access_key (NO secrets)
    preferences.json   # window size, theme, last connection
    OS keychain        # secret keys only
```

## Usage

```bash
# connect to local AbixIO
abixio-ui --endpoint http://localhost:9000

# connect to AWS
abixio-ui --endpoint https://s3.amazonaws.com

# connect to MinIO
abixio-ui --endpoint http://minio.home:9000

# or just launch and use the connection manager
abixio-ui
```

## Build

```bash
cargo build --release
# produces target/release/abixio-ui
```

## What works / what doesn't

**Done:**
- Three-panel layout: icon sidebar + center content + right detail panel
- Custom dark theme (teal accent, high contrast) + light theme
- Theme switching in Settings (Dark / Light / System)
- Async operation helper (tokio oneshot channels, non-blocking UI)
- S3 client (raw HTTP via reqwest, XML parsing)
- Bucket list sidebar with create bucket
- Object browser with breadcrumb navigation and prefix drilling
- Object detail panel: full metadata from HEAD request (size, type, etag, all HTTP headers)
- Upload via native file dialog
- Download via native save dialog
- Delete with immediate refresh
- Performance monitoring: FPS, frame time, network requests, bytes in/out (5m sliding window)
- Settings view: theme, connection info, performance stats, about

**Not yet implemented:**
- Connection manager (multi-server, saved connections)
- OS keychain credential storage
- AbixIO-specific features (disk health, object inspector, config)
- Auth (AWS Sig V4 signing)

See [docs/](docs/) for architecture, data authority, and performance docs.

## License

[GNU General Public License v3.0](LICENSE)
