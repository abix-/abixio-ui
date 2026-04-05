# Features And MinIO Client Parity

This file is the source of truth for what `abixio-ui` ships today and how it
compares to MinIO Client `mc`.

The comparison is intentional and factual. `mc` is the operational baseline
many S3 and MinIO users already know, so it is the most useful parity target.
This document does not imply endorsement, certification, or affiliation with
MinIO.

## Available Now In abixio-ui

| Capability area | abixio-ui | MinIO Client `mc` | Parity Score | Parity | Notes |
|---|---|---|---|---|---|
| Connection setup and saved profiles | Saved connections, keychain-backed credentials, anonymous mode, test and connect flows | Aliases, config file, global flags, cert and TLS options, automation-oriented config | 6/10 | Partial | `abixio-ui` covers normal interactive connection management, not the full CLI and config surface. |
| Bucket listing and object browsing | Bucket list, object list, prefix navigation, breadcrumbs, object metadata panel | `mc ls`, `mc tree`, `mc stat`, related browse and list flows | 7/10 | Partial | Strong interactive browse support, but no tree, `du`, or full stat-style command breadth. |
| Object upload, download, metadata, and delete | Upload, download, HEAD metadata, single-object delete, batch delete, AbixIO object detail | `mc cp`, `mc cat`, `mc head`, `mc stat`, `mc rm` | 8/10 | Partial | Covers the main single-object CRUD and metadata workflows. Batch delete uses S3 DeleteObjects API (1000 keys/call). Inline content viewing and advanced copy flags are still missing. |
| AbixIO-specific admin features | Disks (with pluggable backend info), healing, shard inspection, manual heal | No direct AbixIO equivalent | n/a | Out of scope | This is a deliberate `abixio-ui` specialization, not a parity gap. AbixIO server now supports pluggable storage backends via the `Backend` trait. |
| Copy, move, rename, import, and export | Server-side copy (same-bucket and cross-bucket), move (copy+delete), rename, recursive folder import, recursive prefix export, overwrite prompts | Core `mc cp` and `mc mv` workflows | 8/10 | Partial | Copy uses S3 server-side CopyObject API for both same-bucket and cross-bucket operations. Move and rename use copy-then-delete. No multi-source copy or advanced `mc cp` option surface. |
| Search, find, and filtering | Filter box with substring and wildcard matching on current listing, plus recursive Find across all prefixes | `mc find` | 6/10 | Partial | Local instant filter on loaded objects plus recursive find with wildcard or substring pattern. Missing: time, size, metadata, and tag filters. |
| Bucket create and delete | Create bucket modal and recursive bucket delete with typed-name confirmation | `mc mb`, `mc rb` | 7/10 | Partial | Core bucket lifecycle now exists. Advanced options and CLI flags are still missing. |
| Presigned sharing | Share button in detail panel with expiry picker and generated URL | `mc share` | 7/10 | Partial | Presigned GET URL generation with configurable expiry. No upload URLs yet. |
| Recursive sync, mirror, and diff | Not implemented | `mc mirror`, `mc diff` | 0/10 | None | No folder sync, replica, or drift comparison workflow. |
| Versioning and recovery | Enable/suspend versioning per bucket, version list in detail panel, restore old versions, delete specific versions | `mc version`, `mc undo` | 7/10 | Partial | Full versioning support: enable/suspend per bucket, version list with restore and delete. No undo or rewind-by-time yet. |
| Bulk delete and batch object workflows | Multi-select bulk delete, recursive prefix delete, S3 DeleteObjects batch API (1000 keys/call) | Recursive `mc rm`, batch-oriented workflows | 7/10 | Partial | Multi-select bulk delete and recursive prefix delete with confirmation modals. Uses S3 DeleteObjects API. No time/size filters, no dry-run. |
| SQL, object query, and inline content inspection | Preview of first 4KB of text objects in detail panel | `mc sql`, `mc cat`, `mc head` | 5/10 | Partial | Inline text preview. No SQL query or binary viewer. |
| Tags | Object and bucket tags in detail panels (view, add, remove) | `mc tag` | 8/10 | Partial | Object and bucket tags via S3 tagging API. No recursive tag set. |
| Bucket policy and anonymous access | View and delete bucket policy in detail panel | `mc anonymous` | 5/10 | Partial | Policy JSON view and delete. No inline editor yet. |
| CLI, scripting, and automation | GUI only, plus in-app smoke tests and auto-run test mode | Full CLI, JSON and quiet modes, shell automation | 1/10 | None | `abixio-ui` is still a desktop app, not an operations CLI. |
| Retention and legal hold | Not implemented | `mc retention`, `mc legalhold` | 0/10 | None | Governance and compliance controls are absent. |
| Lifecycle, ILM, and tiering | View and delete lifecycle config in bucket detail | `mc ilm` | 4/10 | Partial | Lifecycle view and delete. No inline rule editor. |
| Encryption config | Not implemented | `mc encrypt` | 0/10 | None | No bucket or object encryption configuration UI. |
| Replication, quota, events, and watch | Not implemented | `mc replicate`, `mc quota`, `mc event`, `mc watch` | 0/10 | None | No replication, quota, event, or watch tooling. |
| MinIO-specific admin, support, IDP, and license commands | Not implemented | `mc admin`, `mc support`, `mc idp`, `mc license` | n/a | Out of scope | These are MinIO platform-management features, not current `abixio-ui` goals. |

## Current abixio-ui Feature Table

| Area | Feature | Status | Parity Score | Notes |
|---|---|---|---|---|
| Core S3 | Connect to S3-compatible endpoints over HTTP or HTTPS | Yes | 8/10 | Works with saved connections or direct CLI launch. |
| Core S3 | Start from CLI endpoint and credential flags | Yes | 6/10 | Supports `--endpoint`, `--access-key`, and `--secret-key`. |
| Core S3 | List buckets | Yes | 8/10 | Live server read. |
| Core S3 | Create buckets | Yes | 7/10 | Create uses a dedicated modal on the current connection. |
| Core S3 | Delete buckets | Yes | 7/10 | Recursive delete is implemented with typed-name confirmation. |
| Core S3 | Browse objects | Yes | 7/10 | Prefix navigation and breadcrumbs are implemented. |
| Core S3 | Upload files | Yes | 8/10 | Uses a native file picker. |
| Core S3 | Download files | Yes | 8/10 | Uses a native save dialog. |
| Core S3 | Delete objects | Yes | 6/10 | Single and bulk delete with multi-select. Batch delete uses S3 DeleteObjects API. |
| Core S3 | View object metadata | Yes | 8/10 | Uses HEAD metadata in the detail panel. |
| Core S3 | Copy object | Yes | 8/10 | Server-side copy for same-bucket and cross-bucket via S3 CopyObject API. |
| Core S3 | Import local folder recursively | Yes | 6/10 | Recursive local-to-S3 copy is implemented. |
| Core S3 | Export prefix recursively | Yes | 6/10 | Recursive S3-to-local export is implemented. |
| Core S3 | Close detail panel with `Esc` | Yes | n/a | Keyboard shortcut is wired. |
| Connections | Save named connections | Yes | 7/10 | Stored in `~/.abixio-ui/settings.json`. |
| Connections | Store credentials in OS keychain | Yes | 8/10 | Uses `keyring`. |
| Connections | Anonymous connections | Yes | 6/10 | Works when no keychain entries exist. |
| Connections | Test, edit, remove, and switch connections | Yes | 7/10 | Available in the Connections view. |
| AbixIO | Detect AbixIO automatically | Yes | n/a | Probes `/_admin/status`. |
| AbixIO | Disks view | Yes | n/a | Shows backend status, space, bucket counts, and object counts. Server reports backend type and label (e.g. `local:/mnt/d1`). |
| AbixIO | Healing view | Yes | n/a | Shows MRF and scanner state. |
| AbixIO | Manual refresh for Disks and Healing | Yes | n/a | On-demand only. |
| AbixIO | Object shard inspection | Yes | n/a | Lives in the object detail panel. |
| AbixIO | Manual object heal | Yes | n/a | Confirmation modal required. |
| UI | Dark and Light theme switch | Yes | n/a | Session-only setting. |
| UI | Top bar with active connection | Yes | n/a | Shows connection name or endpoint. |
| UI | Bottom error bar | Yes | n/a | Dismissable. |
| UI | Built-in smoke tests | Yes | 2/10 | Testing tab runs end-to-end checks. Useful, but this is not general CLI automation parity. |
| UI | Auto-run smoke tests with JSON report | Yes | 2/10 | `--run-tests` writes a report and keeps the app open. This helps verification, not daily object operations. |
| Gaps | Move and rename | Yes | 7/10 | Server-side copy + delete. Move and Rename buttons in detail panel. |
| Gaps | Search and find | Partial | 6/10 | Filter box for local listing plus recursive Find. No time/size/metadata filters yet. |
| Gaps | Bucket delete | Yes | 7/10 | Implemented as recursive delete with typed-name confirmation. |
| Gaps | Presigned sharing | Yes | 7/10 | Share button with expiry picker. No upload URLs. |
| Gaps | Mirror, diff, sync | No | 0/10 | No recursive sync workflow. |
| Gaps | Versioning and recovery | Partial | 7/10 | Enable/suspend per bucket, version list, restore, delete version. No undo or rewind. |
| Gaps | Bulk object operations | Partial | 7/10 | Multi-select bulk delete and recursive prefix delete with S3 DeleteObjects batch API. No time/size filtering yet. |
| Gaps | Object query and inline content inspection | Partial | 5/10 | First 4KB text preview in detail panel. No SQL query or binary viewer. |
| Gaps | Tags | Partial | 8/10 | Object and bucket tags in detail panels (view, add, remove). No recursive tag set. |
| Gaps | Policy and anonymous access | Partial | 5/10 | View + delete policy. No inline editor. |
| Gaps | CLI or automation surface | No | 1/10 | This is still a desktop app, not a scriptable CLI. |
| Gaps | Retention and legal hold | No | 0/10 | No governance UI yet. |
| Gaps | Lifecycle and ILM | Partial | 4/10 | View + delete lifecycle. No rule editor. |
| Gaps | Encryption setup | No | 0/10 | No encryption configuration UI. |
| Gaps | Replication, quota, events, and watch | No | 0/10 | Outside current desktop workflow coverage. |

## MinIO Client Features Not Yet In abixio-ui

- Move and rename: multi-source move, recursive prefix move (single-object move/rename exists).
- Search and find: time, size, metadata, and tag filters (basic name/path filtering exists).
- Presigned upload URLs (download URLs implemented).
- Recursive sync, mirror, and diff workflows.
- Versioning: undo/rewind-by-time (version browse, restore, and delete exist).
- Bulk object operations: time/size filters.
- SQL queries on object content.
- Inline policy editor (view/delete exists, no create/edit).
- Inline lifecycle rule editor (view/delete exists, no create/edit).
- CLI and scriptable automation surface.
- Retention and legal-hold controls.
- Encryption setup controls.
- Replication, quota, event, and watch workflows.

## S3 API Parity: mc (minio-go) vs aws-sdk-s3 vs abixio-ui

`mc` talks to S3 through `minio-go`. `abixio-ui` talks to S3 through
`aws-sdk-s3` (the official AWS SDK for Rust). This table tracks which S3
API operations each library exposes and whether `abixio-ui` uses them.

Previously used `rust-s3` 0.37, which was missing `DeleteObjects`,
`ListObjectVersions`, `GetBucketPolicy`, and cross-bucket `CopyObject`.
Migrated to `aws-sdk-s3` to eliminate all API blockers.

| S3 API Operation | mc / minio-go | aws-sdk-s3 | abixio-ui | Notes |
|---|---|---|---|---|
| ListBuckets | yes | `list_buckets` | yes | |
| CreateBucket | yes | `create_bucket` | yes | |
| DeleteBucket | yes | `delete_bucket` | yes | |
| ListObjectsV2 | yes | `list_objects_v2` | yes | |
| GetObject | yes | `get_object` | yes | |
| PutObject | yes | `put_object` | yes | |
| HeadObject | yes | `head_object` | yes | |
| DeleteObject | yes | `delete_object` | yes | |
| DeleteObjects (batch) | yes (1000/req) | `delete_objects` | yes | 1000 keys/call, returns failed keys |
| CopyObject | yes | `copy_object` | yes | same-bucket and cross-bucket server-side copy |
| GetObjectTagging | yes | `get_object_tagging` | yes | wired to detail panel |
| PutObjectTagging | yes | `put_object_tagging` | yes | add tag from detail panel |
| DeleteObjectTagging | yes | `delete_object_tagging` | yes | remove tag from detail panel |
| GetBucketLifecycle | yes | `get_bucket_lifecycle` | yes | wired to bucket detail panel |
| PutBucketLifecycle | yes | `put_bucket_lifecycle` | no | client method exists, no UI editor yet |
| DeleteBucketLifecycle | yes | `delete_bucket_lifecycle` | yes | wired to bucket detail panel |
| Presign GET | yes | presigning config | yes | share button with expiry picker in detail panel |
| Presign PUT | yes | presigning config | no | not yet implemented |
| ListObjectVersions | yes | `list_object_versions` | yes | wired to detail panel versions list |
| GetBucketPolicy | yes | `get_bucket_policy` | yes | wired to bucket detail panel |
| PutBucketPolicy | yes | `put_bucket_policy` | no | client method exists, no UI editor yet |
| DeleteBucketPolicy | yes | `delete_bucket_policy` | yes | wired to bucket detail panel |
| GetBucketVersioning | yes | `get_bucket_versioning` | yes | wired to bucket detail panel |
| PutBucketVersioning | yes | `put_bucket_versioning` | yes | enable/suspend buttons in bucket detail |
| PutObjectRetention | yes | `put_object_retention` | no | not yet implemented |
| PutBucketEncryption | yes | `put_bucket_encryption` | no | not yet implemented |

No API blockers remain. Every S3 operation mc uses is available in aws-sdk-s3.
Most operations are wired to the UI. Remaining unwired operations
(`PutBucketLifecycle`, `PutBucketPolicy`, `Presign PUT`, `PutObjectRetention`,
`PutBucketEncryption`) can be added without library changes.

## Intentional Scope Differences

- `abixio-ui` is a desktop UI first. `mc` is a general-purpose CLI optimized
  for scripting and operator workflows.
- `abixio-ui` includes AbixIO-specific admin features that `mc` does not target:
  disks, healing, shard inspection, and manual heal.
- MinIO-specific platform-admin features such as `mc admin`, `mc support`,
  `mc idp`, and `mc license` are not current parity targets for `abixio-ui`.

## Current Behavior Notes

- Rough parity summary: `abixio-ui` is strongest in browse, object CRUD (copy,
  move, rename, bulk delete), recursive import and export, saved connections,
  versioning, tags, and AbixIO-specific admin. It is still weak in scripting
  parity and advanced S3 management features (inline policy/lifecycle editors,
  retention, encryption).
- The Settings view shows network counters, but request and byte metrics are
  not currently wired to real network activity.
- Leaving credential fields blank while editing a saved connection keeps the
  existing keychain entries. There is no clean in-place "make this saved
  connection anonymous" flow yet.
- Direct CLI connections always use region `us-east-1`. If you need another
  region, use a saved connection profile.
- The Testing tab now deletes both its empty and non-empty test buckets during
  cleanup.
- If a feature is not listed under `Available Now In abixio-ui`, it should not
  be described as shipping.
