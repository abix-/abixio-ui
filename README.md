# abixio-ui

Native desktop S3 manager and AbixIO server admin. Browse, upload, download, and manage objects on any S3-compatible endpoint. Full server management when connected to AbixIO.

## Features

- **S3 object browser** -- browse buckets, navigate prefixes, upload/download/delete
- **Multi-server connections** -- save, edit, test, and switch between endpoints
- **AWS Sig V4 auth** -- connect to AWS, MinIO, AbixIO, Backblaze, or any authenticated endpoint
- **OS keychain** -- access keys and secret keys stored in Windows Credential Manager / macOS Keychain / Linux secret-service. Zero secrets on disk
- **AbixIO admin** -- when connected to an AbixIO server, the UI auto-detects it and enables:
  - **Disk health dashboard** -- per-disk status, space usage, bucket/object counts
  - **Healing monitor** -- MRF queue depth, integrity scanner stats, manual heal trigger
  - **Object shard inspector** -- per-shard status, checksums, erasure distribution

## How it works

Built with [iced](https://iced.rs) 0.14 (reactive rendering, Elm architecture). Connects via [rust-s3](https://github.com/durch/rust-s3) for S3 operations with full Sig V4 signing.

When connected to an [AbixIO](https://github.com/abix-/abixio) server, the UI probes `/_admin/status`. If it responds with `"server": "abixio"`, admin tabs (Disks, Healing) appear in the sidebar. Non-AbixIO S3 endpoints work fine -- admin tabs are simply hidden.

### Data authority

- **S3 endpoint** is the single source of truth for all bucket/object data
- **OS keychain** stores access keys and secret keys (encrypted by OS)
- **Local config** (`~/.abixio-ui/settings.json`) stores connection profiles (name, endpoint, region -- no secrets)
- No local caching -- every navigation action fetches live from the server

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

```bash
# start AbixIO server (4 disks, 2+2 erasure, no auth)
mkdir -p /tmp/abixio/{d1,d2,d3,d4}
abixio --listen 0.0.0.0:10000 \
  --disks /tmp/abixio/d1,/tmp/abixio/d2,/tmp/abixio/d3,/tmp/abixio/d4 \
  --data 2 --parity 2 --no-auth

# launch UI
abixio-ui --endpoint http://localhost:10000

# create a bucket and upload via curl
curl -X PUT http://localhost:10000/testbucket
curl -X PUT -d "hello world" http://localhost:10000/testbucket/hello.txt

# verify admin API
curl http://localhost:10000/_admin/status
curl http://localhost:10000/_admin/disks
curl "http://localhost:10000/_admin/object?bucket=testbucket&key=hello.txt"
```

## Build

```bash
cargo build --release    # produces target/release/abixio-ui (~18 MB)
```

## What works

- Three-panel layout: icon sidebar + center content + right detail panel
- Dark / Light theme switching
- S3 client via rust-s3 with AWS Sig V4 signing
- Connection manager: add, edit, remove, test, switch
- OS keychain credential storage (both access key + secret key)
- Bucket list with create bucket
- Object browser with breadcrumb navigation and prefix drilling
- Object detail panel: full metadata from HEAD request
- Upload/download via native file dialogs
- Delete with error display
- AbixIO auto-detection on connect
- Disk health dashboard (AbixIO only)
- Healing status monitor (AbixIO only)
- Admin API client with Sig V4 signing for `/_admin/*` endpoints
- ESC to close detail panel, error bar with dismiss
- Performance stats (5m sliding window)
- CLI args: `--endpoint`, `--access-key`, `--secret-key`

## Not yet implemented

- Object shard inspector in detail panel
- Auto-refresh timer for admin views
- Custom theme colors (using stock iced Dark/Light for now)
- Multipart upload progress

See [docs/](docs/) for architecture, credential storage, data authority, and more.

## License

[GNU General Public License v3.0](LICENSE)
