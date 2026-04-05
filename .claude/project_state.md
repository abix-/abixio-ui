# abixio-ui project state

## last session: 2026-04-04

### what we worked on
- abixio server: structured error responses (RequestId + Resource in XML, x-amz-request-id header)
- abixio server: presigned URL auth (SigV4 query param verification with expiration)
- abixio server: conditional requests (If-Match, If-None-Match, If-Modified-Since, If-Unmodified-Since)
- abixio server: object + bucket tagging endpoints (6 S3 endpoints)
- abixio-ui: object tagging UI in detail panel (view, add, remove)
- abixio-ui: tagging smoke tests

### decisions made
- **request_id as hex nanos**: matches MinIO pattern, generated in dispatch(), set on all responses via post-processing
- **error XML includes RequestId + Resource**: matches S3 spec, AWS SDKs parse these
- **presigned auth reuses existing crypto**: same derive_signing_key, canonical_uri, sha256_hex. new verify_presigned_v4 reads from query params instead of Authorization header
- **canonical query for presigned excludes X-Amz-Signature**: matches S3/MinIO spec
- **conditional check order**: If-None-Match (304), If-Modified-Since (304), If-Match (412), If-Unmodified-Since (412) -- matches MinIO/S3 spec

### current state
- abixio server: compiles clean, 121 tests pass, 17 S3 endpoints
- abixio-ui: compiles clean, all tests pass
- server compliance: 8/10 (up from 7/10)
- presigned URL auth: working (was "unknown")
- conditional requests: working (was missing)
- error responses: now include RequestId, Resource, x-amz-request-id header

### next steps
- wire presigned sharing UI in abixio-ui (server presigned auth is ready)
- bucket tagging UI (server endpoints exist)
- version browser (requires server versioning -- large scope)
- multipart upload (required for files >5GB -- large scope)
- search filters (time, size) to improve find from 6/10
