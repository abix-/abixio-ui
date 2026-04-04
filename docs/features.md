# Features And MinIO Client Parity

This file is the source of truth for what `abixio-ui` ships today and how it
compares to MinIO Client `mc`.

The comparison is intentional and factual. `mc` is the operational baseline
many S3 and MinIO users already know, so it is the most useful parity target.
This document does not imply endorsement, certification, or affiliation with
MinIO.

## Available Now In abixio-ui

| Capability area | abixio-ui | MinIO Client `mc` | Parity | Notes |
|---|---|---|---|---|
| Connection setup and saved profiles | Saved connections, keychain-backed credentials, anonymous mode, test/connect flows | Aliases, config file, global flags, cert/TLS options, automation-oriented config | Partial | `abixio-ui` covers normal interactive connection management, not the full CLI/config surface. |
| Bucket listing and object browsing | Bucket list, object list, prefix navigation, object metadata panel | `mc ls`, `mc tree`, `mc stat`, related browse/list flows | Partial | Strong interactive browse support, but no tree/du/stat command breadth. |
| Bucket create/delete | Create bucket only | `mc mb`, `mc rb` | Partial | Bucket delete is still missing in `abixio-ui`. |
| Object upload/download/view metadata | Upload, download, HEAD metadata, delete, AbixIO object detail | `mc cp`, `mc cat`, `mc head`, `mc stat`, `mc rm` | Partial | Covers basic CRUD and metadata, but not the full object-operation surface. |
| Server-side copy/move/rename | Not implemented | `mc cp`, `mc mv` | None | No object copy, move, or rename workflow in the UI. |
| Recursive sync/mirror/diff | Not implemented | `mc mirror`, `mc diff` | None | No folder sync, replica, or drift comparison workflow. |
| Search/find/filtering | Prefix navigation only | `mc find` | Partial | Prefix browsing exists, but there is no search/filter query UI. |
| Bulk delete and batch object workflows | Single-object delete only | Recursive `mc rm`, batch-oriented workflows | None | No multi-select or recursive delete workflow. |
| Presigned sharing | Not implemented | `mc share` | None | No temporary share/download/upload URL generation. |
| Tags | Not implemented | `mc tag` | None | No bucket-tag or object-tag UI. |
| Versioning / recovery | Not implemented | `mc version`, `mc undo` | None | No version browse, restore, or rollback workflow. |
| Retention / legal hold | Not implemented | `mc retention`, `mc legalhold` | None | Governance/compliance controls are absent. |
| Bucket policy / anonymous access | Not implemented | `mc anonymous` | None | No bucket policy or public-access management UI. |
| Lifecycle / ILM / tiering | Not implemented | `mc ilm` | None | No lifecycle-rule or tiering controls. |
| Encryption config | Not implemented | `mc encrypt` | None | No bucket/object encryption configuration UI. |
| Replication / quota / events / watch | Not implemented | `mc replicate`, `mc quota`, `mc event`, `mc watch` | None | No replication, quota, event, or watch tooling. |
| SQL/object query and inline content inspection | Metadata only | `mc sql`, `mc cat`, `mc head` | Partial | Metadata inspection exists, but there is no object-content query or inline viewer workflow. |
| CLI/scriptability / automation | GUI only, plus in-app smoke tests | Full CLI, JSON/quiet modes, shell automation | None | `abixio-ui` is not a scripting surface. |
| MinIO-specific admin/support/IDP/license commands | Not implemented | `mc admin`, `mc support`, `mc idp`, `mc license` | Out of scope | These are MinIO platform-management features, not current `abixio-ui` goals. |
| AbixIO-specific admin features | Disks, healing, shard inspection, manual heal | No direct AbixIO equivalent | Out of scope | This is a deliberate `abixio-ui` specialization, not a parity gap. |

## Current abixio-ui Feature Table

| Area | Feature | Status | Notes |
|---|---|---|---|
| Core S3 | Connect to S3-compatible endpoints over HTTP or HTTPS | Yes | Works with saved connections or direct CLI launch. |
| Core S3 | Start from CLI endpoint and credential flags | Yes | Supports `--endpoint`, `--access-key`, and `--secret-key`. |
| Core S3 | List buckets | Yes | Live server read. |
| Core S3 | Create buckets | Yes | Bucket delete is still missing. |
| Core S3 | Browse objects | Yes | Prefix navigation and breadcrumbs are implemented. |
| Core S3 | Upload files | Yes | Uses a native file picker. |
| Core S3 | Download files | Yes | Uses a native save dialog. |
| Core S3 | Delete objects | Yes | Single-object delete only. |
| Core S3 | View object metadata | Yes | Uses HEAD metadata in the detail panel. |
| Core S3 | Close detail panel with `Esc` | Yes | Keyboard shortcut is wired. |
| Connections | Save named connections | Yes | Stored in `~/.abixio-ui/settings.json`. |
| Connections | Store credentials in OS keychain | Yes | Uses `keyring`. |
| Connections | Anonymous connections | Yes | Works when no keychain entries exist. |
| Connections | Test, edit, remove, and switch connections | Yes | Available in the Connections view. |
| AbixIO | Detect AbixIO automatically | Yes | Probes `/_admin/status`. |
| AbixIO | Disks view | Yes | Shows disk status, space, bucket counts, and object counts. |
| AbixIO | Healing view | Yes | Shows MRF and scanner state. |
| AbixIO | Manual refresh for Disks and Healing | Yes | On-demand only. |
| AbixIO | Object shard inspection | Yes | Lives in the object detail panel. |
| AbixIO | Manual object heal | Yes | Confirmation modal required. |
| UI | Dark and Light theme switch | Yes | Session-only setting. |
| UI | Top bar with active connection | Yes | Shows connection name or endpoint. |
| UI | Bottom error bar | Yes | Dismissable. |
| UI | Built-in smoke tests | Yes | Testing tab runs end-to-end checks. |
| Gaps | Bucket delete | No | Not implemented yet. |
| Gaps | Copy, move, rename | No | No server-side copy workflow. |
| Gaps | Mirror, diff, sync | No | No recursive sync workflow. |
| Gaps | Presigned sharing | No | No share-link generation. |
| Gaps | Tags, versioning, retention, legal hold | No | No governance UI yet. |
| Gaps | Policies, lifecycle, encryption, replication, quota, events, watch | No | Outside current S3-client coverage. |
| Gaps | CLI or automation surface | No | This is a desktop app, not a scriptable CLI. |

## MinIO Client Features Not Yet In abixio-ui

- Bucket deletion.
- Object copy, move, and rename.
- Recursive sync, mirror, and diff workflows.
- Search and find queries beyond prefix navigation.
- Bulk object operations.
- Presigned sharing links.
- Bucket and object tags.
- Versioning, undo, and recovery tooling.
- Retention and legal-hold controls.
- Bucket policy and anonymous-access management.
- Lifecycle, ILM, and tiering controls.
- Encryption setup controls.
- Replication, quota, event, and watch workflows.
- SQL/object query tools and inline object-content inspection.
- CLI/scriptable automation surface.

## Intentional Scope Differences

- `abixio-ui` is a desktop UI first. `mc` is a general-purpose CLI optimized
  for scripting and operator workflows.
- `abixio-ui` includes AbixIO-specific admin features that `mc` does not target:
  disks, healing, shard inspection, and manual heal.
- MinIO-specific platform-admin features such as `mc admin`, `mc support`,
  `mc idp`, and `mc license` are not current parity targets for `abixio-ui`.

## Current Behavior Notes

- The Settings view shows network counters, but request and byte metrics are
  not currently wired to real network activity.
- Leaving credential fields blank while editing a saved connection keeps the
  existing keychain entries. There is no clean in-place "make this saved
  connection anonymous" flow yet.
- Direct CLI connections always use region `us-east-1`. If you need another
  region, use a saved connection profile.
- The Testing tab creates a timestamped bucket and removes test objects, but it
  does not delete the bucket itself because the app does not yet implement
  bucket deletion.
- If a feature is not listed under `Available Now In abixio-ui`, it should not
  be described as shipping.
