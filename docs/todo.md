# todo

Current prioritized backlog for `abixio-ui`, ordered by user-facing gaps.
Detailed parity notes still live in `docs/features.md`.

## 1. presigned upload URLs

Presigned GET exists. Presigned PUT does not.

- add presigned upload URL generation from the object detail/share flow
- let the user choose expiry, matching the existing share URL pattern
- keep download URL generation unchanged

## 2. richer inline content viewer

The detail panel only previews the first 4KB of text objects.

- extend preview beyond the current text snippet
- improve handling for larger text objects
- define a clear fallback for binary or unsupported content

## 3. diff, copy, and sync

Recursive import/export exists, but there is no sync-style workflow.

Current status:
- The app now has a real preview planner for `Diff`, `Copy`, and policy-backed `Sync`
- `Copy` execution from plan is now shipped
- `Sync` execution with deletes and guardrails is still pending

Implementation phases:
- Phase 1: harden diff planning and preview
- Phase 2: copy execution from plan
- Phase 3: sync execution with guarded deletes
- Phase 4: performance tuning, fast-list/top-up, richer filters, telemetry

See `docs/sync.md` for the full design and rollout plan.

## 4. richer search and filters

Current search supports local filtering and recursive find by name/path only.

- add time, size, metadata, or tag-based filters
- preserve current simple substring and wildcard search

## 5. version rewind and recovery

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
