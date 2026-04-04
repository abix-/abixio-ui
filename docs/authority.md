# Data Authority

Who owns what data, where it lives, and how it flows.

## Authority map

| Data | Authoritative source | Local storage |
|------|---------------------|---------------|
| Buckets, objects, metadata | S3 endpoint (server) | None -- always fetched live |
| Disk health, shard info | AbixIO server (/_abixio/ API) | None -- always fetched live |
| Connection list | `~/.abixio-ui/settings.json` | Name, endpoint URL, region |
| Access keys | OS keychain | Never written to disk |
| Secret keys | OS keychain | Never written to disk |
| UI preferences | `~/.abixio-ui/settings.json` (planned) | Window size, theme, last connection |

## Rules

1. **The S3 endpoint is the single source of truth for all object data.**
   The UI never caches bucket listings, object data, or metadata locally.
   Every navigation action triggers a live fetch from the server.

2. **Keys never touch the filesystem.**
   Both access keys and secret keys are stored in the OS keychain only
   (Windows Credential Manager, macOS Keychain, Linux secret-service).
   `settings.json` stores connection names, endpoints, and regions -- never
   any keys.

3. **No optimistic updates.**
   All mutations (upload, delete, create bucket) go directly to the server.
   The UI waits for the server response, then re-fetches the listing.
   If the server says it failed, the UI shows an error. No local state divergence.

## Read path

```
user clicks bucket "test"
  -> UI sends Request::ListObjects to background thread
  -> background: GET /test?list-type=2 (live HTTP call)
  -> background sends Response::Objects back to UI
  -> UI renders object table

user clicks prefix "logs/"
  -> GET /test?list-type=2&prefix=logs/&delimiter=/
  -> render filtered table

user clicks refresh
  -> re-fetch current listing from server
```

No caching. No background polling. Simple and always consistent.

Tradeoff: one network round-trip per navigation click. For home/personal use
with local servers this is <10ms. For remote endpoints (AWS), it's the same
latency as any S3 client.

## Write path

```
upload:
  user picks file via native dialog
  -> PUT /bucket/key (await server response)
  -> 200: re-fetch listing, show success toast
  -> error: show error toast, listing unchanged

delete:
  user clicks delete, confirms in dialog
  -> DELETE /bucket/key (await server response)
  -> 204: re-fetch listing
  -> error: show error toast

create bucket:
  user types name, clicks create
  -> PUT /bucket (await server response)
  -> 200: re-fetch bucket list
  -> error: show error toast
```

## Credential storage

```
~/.abixio-ui/settings.json (on disk, not secret):
  {
    "connections": [
      {"name": "home", "endpoint": "http://nas:9000", "region": "us-east-1"},
      {"name": "aws", "endpoint": "https://s3.amazonaws.com", "region": "us-west-2"}
    ]
  }

OS keychain (encrypted by OS, per-connection):
  service "abixio-ui":
    "home.access-key" -> "mykey"
    "home.secret-key" -> "secret-key-here"
    "aws.access-key"  -> "AKIA..."
    "aws.secret-key"  -> "aws-secret-key"
```

On connect:
1. Read `settings.json` for endpoint URL + region
2. Read access key and secret key from OS keychain by connection name
3. If both keys present: sign requests with AWS Signature V4
4. If no keys: connect without auth (anonymous)

See [docs/credentials.md](credentials.md) for full details.

Keychain backends:
- Windows: Credential Manager
- macOS: Keychain
- Linux: secret-service (GNOME Keyring / KWallet)

## What is NOT stored locally

- Object data (never cached, always streamed from server)
- Bucket listings (fetched live every time)
- Server configuration (read from /_abixio/status on demand)
- Disk health data (fetched from /_abixio/disks on demand)
- Shard/erasure details (fetched from /_abixio/object-info on demand)
