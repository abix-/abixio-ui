# abixio-ui project state

## last session: 2026-04-04

### what we worked on
- full object versioning across both repos (server storage model + S3 endpoints + UI)
- server: versioned disk layout (versions.json + uuid data dirs per version)
- server: proper erasure encode/decode for versioned shards
- server: PutBucketVersioning, GetBucketVersioning, ListObjectVersions endpoints
- server: version-id on GET/HEAD/DELETE/PUT, delete markers
- client: version methods (get/put versioning, list versions, get/delete version)
- ui: versioning toggle on bucket, version list in object detail, restore, delete version
- 33 new S3 integration tests (47 total, was 14)
- tests cover: tagging, conditionals, request-id, versioning, copy, batch delete, range, metadata

### decisions made
- **versioned encode/decode**: added encode_and_write_versioned + read_and_decode_versioned that write/read directly to/from version uuid dirs. no legacy path for versioned objects
- **version index**: versions.json per object dir, newest-first array of VersionEntry
- **delete markers**: entry in versions.json with is_delete_marker=true, no data dir
- **suspended versioning**: PUTs use version_id="null", overwrites existing null version
- **find_latest_version**: handlers check versions.json first, fall back to legacy shard.dat for unversioned

### current state
- abixio server: 154 tests pass (94 unit + 13 admin + 47 s3 integration)
- abixio-ui: compiles clean, all tests pass
- server: 20 S3 endpoints implemented
- versioning parity: 7/10 (was 0/10)
- tags parity: 7/10
- s3 compliance: 4/10 (honest -- 20 of ~100 endpoints)

### next steps
- presigned sharing UI (server presigned auth is ready, need UI button + modal)
- search filters (time, size) -- 6/10 to 8/10
- inline content viewer (mc cat equivalent) -- 2/10 to 5/10
- multipart upload (required for files >5GB -- large scope)
