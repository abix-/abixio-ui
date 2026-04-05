# todo

Current prioritized backlog for `abixio-ui`, ordered by user-facing gaps.
Detailed parity notes still live in `docs/features.md`.

## 1. inline policy editor

Bucket policy can be viewed and deleted, but not created or edited.

- add create/edit flow for bucket policy JSON
- validate and display policy save errors clearly
- keep delete flow as-is

## 2. inline lifecycle editor

Lifecycle rules can be viewed and deleted, but not created or edited.

- add create/edit flow for lifecycle rule JSON or structured fields
- preserve current view/delete behavior
- surface validation errors before submit when possible

## 3. presigned upload URLs

Presigned GET exists. Presigned PUT does not.

- add presigned upload URL generation from the object detail/share flow
- let the user choose expiry, matching the existing share URL pattern
- keep download URL generation unchanged

## 4. richer inline content viewer

The detail panel only previews the first 4KB of text objects.

- extend preview beyond the current text snippet
- improve handling for larger text objects
- define a clear fallback for binary or unsupported content

## 5. sync, mirror, and diff

Recursive import/export exists, but there is no sync-style workflow.

- add recursive sync or mirror between local and S3
- add drift/diff visibility before destructive changes
- make overwrite and delete behavior explicit

## 6. richer search and filters

Current search supports local filtering and recursive find by name/path only.

- add time, size, metadata, or tag-based filters
- preserve current simple substring and wildcard search

## 7. version rewind and recovery

Bucket versioning, version browsing, restore, and delete already exist.

- add undo or rewind-by-time style recovery workflow
- make version recovery easier for non-expert users

## later gaps

- recursive bulk object operations with more filters
- SQL-style object query
- CLI or automation surface
- retention and legal hold controls
- encryption setup
- replication, quota, events, and watch
