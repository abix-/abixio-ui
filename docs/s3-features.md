# S3 Features in abixio-ui

How each S3 feature is exposed in the desktop UI. All operations use
`aws-sdk-s3` (official AWS SDK for Rust).

## Object CRUD

- **Upload**: native file picker, single PUT via `put_object()`
- **Download**: native save dialog, full GET via `get_object()`
- **Delete**: single object or multi-select bulk delete via `delete_objects()`
- **Copy**: server-side via `copy_object()`, same-bucket and cross-bucket
- **Move**: copy + delete
- **Rename**: copy to new key + delete old
- **Import folder**: recursive local-to-S3 upload
- **Export prefix**: recursive S3-to-local download

## Object Detail Panel

When an object is selected, the detail panel shows:

| Section | Data source | Description |
|---|---|---|
| Overview | `head_object()` | Size, content type, last modified, ETag |
| Storage | -- | Bucket name, full key path |
| HTTP Headers | `head_object()` | All response headers including custom `x-amz-meta-*` |
| Tags | `get_object_tagging()` | Key-value tag list with add/remove. Max 10 per S3 spec. |
| Versions | `list_object_versions()` | Version list with ID, date, size. Restore and delete buttons. |
| Preview | `get_object()` | First 4KB of object content as text. Shows for all objects. |
| Actions | -- | Download, Share, Copy, Move, Rename, Delete buttons. |
| AbixIO | `/_admin/object` | Shard inspection, manual heal (only when connected to AbixIO). |

## Presigned Sharing

"Share" button in object detail opens a modal:
1. Select expiry: 1 hour, 6 hours, 24 hours, 7 days
2. Click "Generate URL"
3. Presigned GET URL appears in a text field (select and copy)

Uses `aws-sdk-s3` presigning: `get_object().presigned(PresigningConfig)`.
URL is generated client-side. Server validates the signature on access.

## Object Versioning

### Bucket Detail
- Shows current versioning status (Enabled/Suspended/Disabled)
- "Enable Versioning" / "Suspend Versioning" buttons
- Uses `put_bucket_versioning()` / `get_bucket_versioning()`

### Object Detail
- "Versions" section lists all versions with:
  - Truncated version ID
  - Size
  - "(latest)" badge on current version
  - "(delete marker)" for delete markers
  - "Restore" button on non-latest versions (copies version data to new PUT)
  - "x" delete button for permanent version removal

## Object Tagging

"Tags" section in object detail panel:
- Lists existing tags as key-value rows with "x" remove button
- Add form: key + value text inputs + "Add" button
- Max 10 tags enforced in UI
- Uses `get_object_tagging()` / `put_object_tagging()` / `delete_object_tagging()`

## Bucket Detail Panel

When a bucket is selected, the detail panel shows:

| Section | Data source | Description |
|---|---|---|
| Overview | -- | Bucket name, current prefix, folder/object counts |
| Versioning | `get_bucket_versioning()` | Status + enable/suspend buttons |
| Bucket Tags | `get_bucket_tagging()` | Tag list with add/remove |
| Policy | `get_bucket_policy()` | Policy JSON display + delete button |
| Lifecycle | `get_bucket_lifecycle_configuration()` | Lifecycle rules display + delete button |
| Actions | -- | Refresh, Delete Bucket |

## Bucket Tags

Same UI as object tags but in the bucket detail panel:
- List, add, remove tags
- Uses `get_bucket_tagging()` / `put_bucket_tagging()` / `delete_bucket_tagging()`

## Bucket Policy

"Policy" section in bucket detail:
- Shows stored policy JSON (read-only)
- "Delete Policy" button to remove
- No inline editor yet (use `mc anonymous set-json` or API directly)

## Bucket Lifecycle

"Lifecycle" section in bucket detail:
- Shows lifecycle rules (ID, status, expiration, prefix)
- "Delete Lifecycle" button to remove all rules
- No inline rule editor yet (use `mc ilm` or API directly)

## Smoke Tests

The Testing tab runs end-to-end checks including:
- Object tagging round-trip (put, get, delete, verify)
- Versioning (enable, put twice, list versions, suspend)
- All tests report pass/fail with detail messages
