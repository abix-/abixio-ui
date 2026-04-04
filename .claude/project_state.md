# abixio-ui project state

## last session: 2026-04-04

### what we worked on
- added AbixIO admin management API and UI
- server: /_admin/status, /_admin/disks, /_admin/heal, /_admin/object endpoints
- ui: admin client with sig v4 signing, disks dashboard, healing status view
- auto-detects AbixIO servers on connect, shows admin tabs (D=disks, H=healing) in sidebar
- changed default port to 10000 across both repos

### decisions made
- **/_admin/ prefix**: underscore prefix is collision-proof (S3 bucket names must start with letter/number). checked first in router before S3 dispatch
- **same auth for admin**: reuses S3 Sig V4 auth for admin endpoints, same as MinIO's approach (verified in madmin-go source)
- **manual sig v4 in ui**: implemented ~30 line signing with hmac+sha2 rather than fighting with aws-sigv4 crate's API. same approach as rust-s3's signing.rs
- **port 10000**: default port for abixio, easy to type, not commonly used
- **one settings.json + keychain**: simplified credential model, both access key and secret key in OS keychain

### current state
- both repos compile clean, all 101 server tests pass
- server: admin module at src/admin/ with handlers, types, heal stats
- ui: admin module at src/abixio/ with client, types; views at src/views/disks.rs, healing.rs
- sidebar dynamically shows/hides admin tabs based on AbixIO detection

### next steps
- object shard inspector (per-object view showing shard status on each disk)
- manual heal button in object detail panel
- custom theme colors
- auto-refresh for disks/healing views (iced subscription timer)
