# abixio-ui

Native desktop S3 manager and AbixIO admin UI. Browse, upload, download, and
manage objects on any S3-compatible endpoint. When connected to AbixIO, the
app exposes the current Disks and Healing views.

## Features

- **S3 object browser.** Browse buckets, navigate prefixes, create and delete buckets, upload, download, copy, move, rename, or delete objects. Filter and find by name with wildcard support. Multi-select bulk delete.
- **Multi-server connections.** Save, edit, test, and switch between endpoints.
- **AWS Sig V4 auth.** Connect to AWS, MinIO, AbixIO, Backblaze, or any authenticated endpoint.
- **OS keychain.** Access keys and secret keys live in Windows Credential Manager, macOS Keychain, or Linux secret-service. Secrets do not go on disk.
- **AbixIO admin.** When connected to an AbixIO server, the UI auto-detects it and enables:
  - **Disk health dashboard.** Shows per-disk status, space usage, and bucket or object counts.
  - **Healing monitor.** Shows MRF queue depth, integrity scanner stats, and refresh-on-demand.
  - **Object admin detail panel.** Shows shard inspection, manual inspect refresh, and confirmed manual heal.
- **Built-in smoke tests.** A Testing tab can run end-to-end checks against the current server.

## How it works

Built with [iced](https://iced.rs) 0.14 (reactive rendering, Elm architecture). Connects via [aws-sdk-s3](https://docs.rs/aws-sdk-s3) (official AWS SDK for Rust) for all S3 operations.

When connected to an [AbixIO](https://github.com/abix-/abixio) server, the UI
probes `/_admin/status`. If it responds with `"server": "abixio"`, admin tabs
(Disks, Healing) appear in the sidebar. Non-AbixIO S3 endpoints work fine. The
admin tabs are simply hidden.

### Data authority

- **S3 endpoint** is the single source of truth for all bucket/object data
- **OS keychain** stores access keys and secret keys (encrypted by OS)
- **Local config** (`~/.abixio-ui/settings.json`) stores connection profiles such as name, endpoint, and region. It does not store secrets.
- No local caching. Every navigation action fetches live from the server.

## Usage

```bash
# launch with connection manager (recommended)
abixio-ui

# connect directly to a local AbixIO server
abixio-ui --endpoint http://localhost:10000

# connect with credentials
abixio-ui --endpoint https://s3.us-west-2.amazonaws.com --access-key AKIA... --secret-key wJalr...
```

## Quick test

```powershell
# start AbixIO server (4 disks, 2+2 erasure, no auth)
New-Item -ItemType Directory -Force -Path `
  C:\tmp\abixio\d1, `
  C:\tmp\abixio\d2, `
  C:\tmp\abixio\d3, `
  C:\tmp\abixio\d4 | Out-Null

abixio --listen 0.0.0.0:10000 `
  --disks C:\tmp\abixio\d1,C:\tmp\abixio\d2,C:\tmp\abixio\d3,C:\tmp\abixio\d4 `
  --data 2 --parity 2 --no-auth

# launch UI
abixio-ui --endpoint http://localhost:10000

# create a bucket and upload via curl.exe
curl.exe -X PUT http://localhost:10000/testbucket
curl.exe -X PUT -d "hello world" http://localhost:10000/testbucket/hello.txt

# verify admin API
curl.exe http://localhost:10000/_admin/status
curl.exe http://localhost:10000/_admin/disks
curl.exe "http://localhost:10000/_admin/object?bucket=testbucket&key=hello.txt"
```

## Build

```bash
cargo build --release    # release binary goes to Cargo's target dir
```

## What works

- S3 client via aws-sdk-s3 (official AWS SDK) with full API surface available
- Core object CRUD: upload, download, copy, move, rename, delete (single and batch)
- Batch delete via S3 DeleteObjects API (1000 keys/call)
- Recursive prefix delete: delete a folder and all objects under it
- Server-side copy via CopyObject API (same-bucket and cross-bucket)
- Bucket lifecycle: create, delete (recursive with confirmation), list
- Object browser with breadcrumb navigation, prefix drilling, filter, recursive find
- Multi-select with bulk delete, select all (respects filter)
- Recursive folder import and recursive prefix export
- Connection manager: add, edit, remove, test, switch between endpoints
- OS keychain credential storage (Windows Credential Manager, macOS Keychain, Linux secret-service)
- Anonymous connections (unsigned requests, no credentials required)
- Connect timeout (10s) and operation timeout (60s) to prevent UI hangs
- Object detail panel: HEAD metadata, HTTP headers, custom metadata, actions
- AbixIO admin: auto-detection, disk health, healing monitor, shard inspection, manual heal
- Three-panel layout, dark/light theme, ESC to close, error bar
- Built-in smoke test tab
- CLI args: `--endpoint`, `--access-key`, `--secret-key`

## Not yet implemented

- Object tagging (read/write/delete)
- Version browser
- Bucket policies
- Multipart upload for large files
- Auto-refresh timer for admin views
- Custom theme colors

See [docs/features.md](docs/features.md) for mc parity tracking,
[docs/s3-client-audit.md](docs/s3-client-audit.md) for SDK configuration audit,
and [docs/](docs/) for architecture, credentials, and data authority.

## License

[GNU General Public License v3.0](LICENSE)
