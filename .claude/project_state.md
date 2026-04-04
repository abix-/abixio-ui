# abixio-ui project state

## last session: 2026-04-04

### what we worked on
- simplified credential model: one settings.json + OS keychain (no separate credentials.json)
- added edit connection, test connection buttons
- changed default port from 9000 to 10000 (avoid looking like minio)
- fixed stale docs (README, authority, credentials)

### decisions made
- **single settings.json**: connections store name+endpoint+region on disk, both access key and secret key in OS keychain. zero secrets on disk
- **port 10000**: abixio default port, easy to type, not commonly used
- **rust-s3 for s3 client**: replaced hand-rolled reqwest+quick-xml with rust-s3 v0.37
- **keychain over plaintext**: mc stores secrets as plaintext JSON; we use OS keychain via keyring crate

### current state
- v0.2.0 on master
- compiles clean, clippy clean (only pre-existing lifetime elision warnings)
- connection manager: add/edit/remove/test/connect
- S3 browsing with AWS Sig V4 auth or anonymous
- CLI: --endpoint (optional), --access-key, --secret-key

### file layout
- src/config.rs -- Settings struct, connection persistence
- src/keychain.rs -- OS keychain wrapper (store_keys, get_keys, delete_keys)
- src/s3/client.rs -- thin wrapper around rust-s3 Bucket
- src/views/connections.rs -- connection manager UI
- docs/credentials.md -- how credential storage works

### next steps
- AbixIO-specific features (disk health, object inspector, config)
- custom theme colors (teal accent, high contrast)
- multipart upload progress indicator
