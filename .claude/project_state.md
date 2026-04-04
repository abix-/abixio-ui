# abixio-ui project state

## last session: 2026-04-04

### what we worked on
- added AWS Sig V4 authentication support
- built connection manager with flexible credential model
- replaced hand-rolled S3 client (reqwest+quick-xml) with rust-s3 v0.37
- added OS keychain integration for secret key storage

### decisions made
- **rust-s3 over hand-rolled**: reviewed MinIO mc (Go) and rust-s3 crate. mc delegates all signing to minio-go; we follow same pattern with rust-s3. zero custom crypto
- **credentials decoupled from connections**: credentials.json stores access_key_id+region, connections.json references credentials by name. one credential can be shared across many connections, or connections can be anonymous
- **keychain over plaintext**: mc stores secrets as plaintext JSON on disk. we use OS keychain (Windows Credential Manager, macOS Keychain, Linux secret-service) via keyring crate. secret keys never touch the filesystem
- **tokio-rustls-tls feature**: chose rustls over native-tls to align with existing reqwest+rustls setup and avoid OpenSSL dependency

### current state
- v0.2.0 on master, commit 597a6cf
- compiles clean, clippy clean (only pre-existing lifetime elision warnings in view functions)
- connection manager UI works: add/remove connections and credentials, connect/switch
- S3 browsing works with both authenticated and anonymous endpoints
- CLI args: --endpoint (optional), --access-key, --secret-key

### file layout changes
- deleted: src/s3/xml.rs (rust-s3 handles XML internally)
- rewrote: src/s3/client.rs (thin wrapper around rust-s3 Bucket)
- added: src/connection.rs, src/credential.rs, src/keychain.rs, src/views/connections.rs

### next steps
- AbixIO-specific features (disk health, object inspector, config management)
- custom theme colors (teal accent, high contrast)
- edit existing connections/credentials (currently only add/remove)
- "test connection" button in connections UI
- multipart upload progress indicator (rust-s3 supports it)
