# S3 Client Audit

How well `abixio-ui` uses `aws-sdk-s3`. This doc covers SDK configuration
and response type handling from the **client's** perspective.

For S3 API compliance (which endpoints a server should implement), see
the abixio server repo's `docs/s3-compliance.md`.

Ratings are 1-10 where 10 means "fully handled, matches SDK best practices"
and 1 means "completely ignored, likely to cause problems."

## SDK Configuration

### Configuration set in `src/s3/client.rs`

| Config | What we set | Rating | Assessment |
|---|---|---|---|
| `behavior_version` | `BehaviorVersion::latest()` | 10/10 | Required. Set correctly. |
| `region` | User-provided or `us-east-1` | 9/10 | Correct. CLI defaults to `us-east-1`, saved connections store region. Minor gap: no region auto-detection. |
| `credentials_provider` | Static credentials for auth, failing provider + `allow_no_auth()` for anonymous | 9/10 | Authenticated uses static SigV4. Anonymous uses a failing credentials provider combined with `allow_no_auth()` so requests are sent unsigned (no Authorization header). |
| `endpoint_url` | User-provided endpoint | 10/10 | Correct. Required for non-AWS servers. |
| `force_path_style` | `true` | 10/10 | Correct. Required for MinIO, AbixIO, and most self-hosted S3-compatible servers. |
| `timeout_config` | connect: 10s, operation: 60s | 9/10 | Prevents UI hangs on unreachable servers. Operation timeout covers large uploads/downloads. |
| `app_name` | `abixio-ui` | 9/10 | Appears in User-Agent header. Helps server operators identify traffic. |

### Configuration NOT set (using SDK defaults)

#### Retry and timeout -- high impact

| Config | SDK default | Rating | Assessment |
|---|---|---|---|
| `retry_config` | 3 attempts, exponential backoff | 5/10 | Acceptable for normal operations. Problem: connection tests to bad endpoints take 3x as long. Could set max_attempts(1) for connection tests. |
| `stalled_stream_protection` | Enabled, default grace period | 7/10 | Good default. Protects against stalled downloads. |

#### Authentication -- medium impact

| Config | SDK default | Rating | Assessment |
|---|---|---|---|
| `allow_no_auth` | Enabled for anonymous connections | 9/10 | Anonymous connections use a failing credentials provider + `allow_no_auth()`. Requests are sent unsigned. |
| `identity_cache` | Lazy caching | 8/10 | Fine. Credentials are static. |
| `auth_scheme_resolver` | Default (SigV4) | 10/10 | Correct for all S3-compatible servers. |

#### Checksums -- low impact

| Config | SDK default | Rating | Assessment |
|---|---|---|---|
| `response_checksum_validation` | Standard validation | 8/10 | Good. Validates when server provides checksums. |
| `request_checksum_calculation` | Standard calculation | 8/10 | Good. Adds checksums per S3 spec. |
| `aws_chunked_encoding_chunk_size` | 64 KiB | 7/10 | Fine for current object sizes. |

#### S3 feature flags -- no impact for custom endpoints

| Config | SDK default | Rating | Assessment |
|---|---|---|---|
| `accelerate` | Disabled | 10/10 | Not relevant for custom endpoints. |
| `use_dual_stack` | Disabled | 10/10 | Not relevant for custom endpoints. |
| `use_fips` | Disabled | 10/10 | Not relevant for custom endpoints. |
| `use_arn_region` | Disabled | 10/10 | Not relevant for custom endpoints. |
| `disable_multi_region_access_points` | Enabled | 9/10 | Not relevant for custom endpoints. |
| `disable_s3_express_session_auth` | Enabled | 9/10 | Not relevant for custom endpoints. |

#### HTTP client and observability

| Config | SDK default | Rating | Assessment |
|---|---|---|---|
| `http_client` | Smithy default (hyper + rustls) | 8/10 | Matches our TLS stack. |
| `sleep_impl` | Standard async sleep (tokio) | 10/10 | Correct. |
| `time_source` | System clock | 10/10 | Correct. |
| `app_name` | `abixio-ui` | 9/10 | Set. Appears in User-Agent header. |
| `invocation_id_generator` | Random UUID | 10/10 | Correct. |

### Remaining configuration items

| Priority | Issue | Fix |
|---|---|---|
| **Should fix** | Retry config causes slow connection tests | `max_attempts(1)` for test path |

### Configuration rating: 9/10

---

## Response Type Handling

How well we handle the types and fields returned by S3 API responses.

### ListObjectsV2 response

| Field | Type | We handle it | Rating | Assessment |
|---|---|---|---|---|
| `contents` | `Vec<Object>` | yes | 10/10 | Map to `ObjectInfo`. |
| `contents[].key` | `Option<String>` | yes | 10/10 | Unwrap with default. |
| `contents[].size` | `Option<i64>` | yes | 9/10 | Cast to u64. Negative sizes theoretically possible but never happen. |
| `contents[].last_modified` | `Option<DateTime>` | yes | 7/10 | Convert via `.to_string()`. Format may not match what rust-s3 produced. Should verify display format. |
| `contents[].e_tag` | `Option<String>` | yes | 10/10 | Unwrap with default. |
| `contents[].storage_class` | `Option<StorageClass>` | no | 3/10 | Not shown in UI. Would be useful in detail panel. |
| `contents[].owner` | `Option<Owner>` | no | 3/10 | Not shown. Low priority. |
| `common_prefixes` | `Vec<CommonPrefix>` | yes | 10/10 | Used for folder navigation. |
| `is_truncated` | `Option<bool>` | yes | 10/10 | Pagination handled correctly. |
| `next_continuation_token` | `Option<String>` | yes | 10/10 | Pagination loop works. |
| `key_count` | `Option<i32>` | no | 6/10 | Not used. Could show total count in UI. |

### HeadObject response

| Field | Type | We handle it | Rating | Assessment |
|---|---|---|---|---|
| `content_type` | `Option<String>` | yes | 10/10 | Shown in detail panel. |
| `content_length` | `Option<i64>` | yes | 10/10 | Size display. |
| `last_modified` | `Option<DateTime>` | yes | 7/10 | Same format concern as listing. |
| `e_tag` | `Option<String>` | yes | 10/10 | Shown in detail panel. |
| `cache_control` | `Option<String>` | yes | 10/10 | In headers section. |
| `content_disposition` | `Option<String>` | yes | 10/10 | In headers section. |
| `content_encoding` | `Option<String>` | yes | 10/10 | In headers section. |
| `accept_ranges` | `Option<String>` | yes | 10/10 | In headers section. |
| `expiration` | `Option<String>` | yes | 10/10 | In headers section. |
| `metadata` | `Option<HashMap<String, String>>` | yes | 10/10 | Shown as x-amz-meta-* headers. |
| `storage_class` | `Option<StorageClass>` | no | 3/10 | Not shown. Would be useful. |
| `version_id` | `Option<String>` | no | 2/10 | Not shown. Needed for versioning feature. |
| `server_side_encryption` | `Option<ServerSideEncryption>` | no | 3/10 | Not shown. Would indicate if object is encrypted. |
| `parts_count` | `Option<i32>` | no | 2/10 | Multipart info. Low priority. |
| `object_lock_mode` | `Option<ObjectLockMode>` | no | 1/10 | Retention. Out of 1.0 scope. |
| `object_lock_retain_until_date` | `Option<DateTime>` | no | 1/10 | Same. |
| `object_lock_legal_hold_status` | `Option<ObjectLockLegalHoldStatus>` | no | 1/10 | Same. |

### DeleteObjects response

| Field | Type | We handle it | Rating | Assessment |
|---|---|---|---|---|
| `deleted` | `Vec<DeletedObject>` | no | 5/10 | Not checked. We assume success if no errors. Could count for progress reporting. |
| `errors` | `Vec<Error>` | yes | 8/10 | Return failed keys. Could also surface error codes and messages. |
| `errors[].key` | `Option<String>` | yes | 10/10 | Collected into failed list. |
| `errors[].code` | `Option<String>` | no | 4/10 | Not surfaced. Would help diagnose permission vs not-found vs other. |
| `errors[].message` | `Option<String>` | no | 4/10 | Same. |

### GetObject response

| Field | Type | We handle it | Rating | Assessment |
|---|---|---|---|---|
| `body` | `ByteStream` | yes | 8/10 | Collected into Vec<u8>. Works but loads entire object into memory. For large files, should stream to disk. |
| `content_type` | `Option<String>` | no | 5/10 | Not used during download. Could use for file extension suggestion. |
| `content_length` | `Option<i64>` | no | 4/10 | Not used. Could show download progress. |

### CopyObject response

| Field | Type | We handle it | Rating | Assessment |
|---|---|---|---|---|
| `copy_object_result` | `Option<CopyObjectResult>` | no | 5/10 | Not checked. Contains ETag and LastModified of the copy. Could verify copy integrity. |

### Error responses (all operations)

| Aspect | Rating | Assessment |
|---|---|---|
| SDK errors mapped to String | 6/10 | All errors go through `.map_err(\|e\| e.to_string())`. Works but loses structured error info. Cannot distinguish AccessDenied vs NoSuchBucket vs network error. Should parse `SdkError` variants for better UI messages. |
| HTTP status codes | 5/10 | Not checked. aws-sdk-s3 returns typed errors, but we flatten them to strings. |
| Retry-able vs permanent errors | 4/10 | SDK handles retry internally, but we don't surface "retrying..." to the user. |

---

## S3 API Operations Used by abixio-ui

Which S3 operations the client calls and what's missing for planned features.

### Implemented

| S3 API | Client method | Rating | Notes |
|---|---|---|---|
| ListBuckets | `list_buckets()` | 10/10 | |
| ListObjectsV2 | `list_objects()`, `list_objects_recursive()` | 10/10 | Full pagination |
| CreateBucket | `create_bucket()` | 8/10 | No region/ACL options |
| DeleteBucket | `delete_bucket()` | 10/10 | |
| GetObject | `get_object()` | 8/10 | Loads full object into memory |
| PutObject | `put_object()` | 8/10 | No multipart for large files |
| HeadObject | `head_object()` | 9/10 | |
| DeleteObject | `delete_object()` | 10/10 | |
| DeleteObjects | `delete_objects()` | 9/10 | Not yet wired to recursive prefix delete UI |
| CopyObject | `copy_object()` | 10/10 | Same-bucket and cross-bucket |

### Not yet implemented (needed for planned features)

| S3 API | Needed for | Priority |
|---|---|---|
| ~~GetObjectTagging, PutObjectTagging, DeleteObjectTagging~~ | ~~Object tags feature~~ | Done |
| ~~ListObjectVersions~~ | ~~Version browser~~ | Done |
| ~~GetBucketVersioning, PutBucketVersioning~~ | ~~Versioning toggle~~ | Done |
| GetBucketPolicy, PutBucketPolicy, DeleteBucketPolicy | Policy management | 1.x |
| GetBucketEncryption, PutBucketEncryption | Encryption config | 1.x |
| GetBucketTagging, PutBucketTagging, DeleteBucketTagging | Bucket tags | 1.x |
| CreateMultipartUpload, UploadPart, CompleteMultipartUpload, AbortMultipartUpload | Large file upload | 1.x |
| ListMultipartUploads | Stale upload cleanup | 1.x |
| HeadBucket | Fast bucket existence check | 1.x |
| GetBucketLocation | Show region in bucket detail | 1.x |

### Out of scope

Object lock, retention, legal hold, replication, notifications, logging,
website hosting, analytics, metrics, inventory, intelligent-tiering, S3
Express, transfer acceleration. These are AWS-specific admin features not
relevant to a desktop S3 client targeting self-hosted servers.

## Overall rating: 8/10

SDK config is solid (9/10). Anonymous auth sends unsigned requests, timeouts
prevent UI hangs, app name is set. Response handling is solid for
implemented APIs but ignores some useful fields (storage class, version ID,
encryption status). Error handling still flattens structured errors to
strings.
