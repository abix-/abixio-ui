# Diff, Copy, And Sync

This document is the working design for the upcoming high-performance sync
engine in `abixio-ui`.

It exists because sync is not "just another transfer flow." It is a
reconciliation engine with destructive behavior, performance tradeoffs, and
operator expectations shaped by tools like MinIO Client `mc mirror` and
`rclone`.

## Goals

- Match the mental model of `mc mirror` and `rclone copy/sync`
- Prioritize throughput and scale, not just correctness on small directories
- Expose meaningful tunables instead of hiding performance behavior
- Show a full diff before destructive actions
- Keep the implementation one-way in v1

## Current Status

The repo now contains the Phase 1 scaffold:

- sync state and message plumbing in `src/app/mod.rs` and `src/app/types.rs`
- sync handler skeleton in `src/app/handlers/sync.rs`
- sync planner module in `src/app/sync_ops.rs`
- sync UI section in `src/views/sync.rs`
- sync-oriented recursive S3 listing helper in `src/s3/client.rs`

What is present today:

- source and destination selection for S3 or local endpoints
- sync mode selection (`Diff`, `Copy`, `Sync`)
- compare mode and list mode selectors
- advanced tuning and filter form fields
- sync telemetry/state storage
- a real preview planner wired to local and S3 enumeration

What is not present yet:

- real source enumeration in the handler flow
- real destination enumeration in the handler flow
- full diff preview from actual snapshots
- copy or sync execution
- delete guardrails
- bandwidth and multipart tunables

This means sync is **scaffolded, not shipped**.

## Product Model

The sync workflow is intentionally split into three product flows:

1. `Diff`
   - Read source and destination
   - Build a plan of `create`, `update`, `delete`, `skip`, and `conflict`
   - No writes

2. `Copy`
   - Apply creates and updates
   - Never delete destination extras

3. `Sync`
   - Flexible reconcile workflow with presets and advanced policy controls
   - Default preset is `Converge`: overwrite changed destination objects and delete destination extras
   - Destination should match source when the selected policy says it should

`Diff` comes first. `Copy` and `Sync` build on the same plan model and shared engine.

## Endpoint Support

Phase 1 and beyond are designed around three one-way endpoint combinations:

- local -> S3
- S3 -> local
- S3 -> S3

Bidirectional sync is explicitly out of scope for now.

## Performance Model

High performance comes from reducing work and pipelining aggressively:

- minimize API round trips
- separate listing, compare, transfer, and delete concurrency
- prefer server-side copy for S3 -> S3
- use multipart upload/copy for large objects
- avoid unnecessary HEAD requests or checksum work unless requested
- expose the tradeoffs clearly in the UI

The design is influenced by the documented behavior of:

- MinIO Client `mc mirror`
- `rclone copy`
- `rclone sync`

The user-facing semantics are intentionally hybrid:

- `Copy` matches `rclone copy`
- `Sync` is the flexible reconcile workflow
- the default `Sync` preset (`Converge`) is equivalent to `rclone sync` and `mc mirror --overwrite --remove`
- advanced sync controls still let the user express weaker MinIO-style combinations such as overwrite-without-remove or remove-without-overwrite

## Tunables

These are the critical tunables the sync engine is expected to expose over time.

Listing:

- `Streaming`
- `FastList`
- `TopUp`
- list worker count

Compare:

- `SizeOnly`
- `SizeAndModTime`
- `UpdateIfSourceNewer`
- `ChecksumIfAvailable`
- `AlwaysOverwrite`
- compare worker count

Planning / memory:

- planner item limit
- fast-list toggle
- prefer server modtime toggle

Execution phases after Diff:

- transfer worker count
- delete worker count
- multipart cutoff
- multipart chunk size
- multipart concurrency
- verification policy
- upload/download rate limits

## Telemetry

The sync workflow should be observable while planning and executing.

The plan already has a telemetry shape for:

- stage
- source scanned
- destination scanned
- compared
- filtered
- started time
- last update time

Later phases should add:

- bytes planned and transferred
- requests/sec
- throughput
- active workers by stage
- retry count
- delete progress
- API latency summaries
- exportable plan/execution reports

## Phased Delivery

### Phase 1: Preview Planner

Deliver:

- dedicated Sync section
- source and destination config for local or S3
- compare and list strategy selection
- sync presets plus advanced reconcile policy controls
- advanced tuning fields
- sync plan data types
- real source/destination enumeration
- real diff planner
- plan preview table

No writes.

Acceptance:

- users can build a real diff plan for local <-> S3 and S3 -> S3
- plan contains create/update/delete/skip/conflict actions
- summary counts and bytes are visible
- invalid configurations are blocked early

### Phase 2: Copy Execution

Deliver:

- execute `create` and `update` plan items
- server-side copy for S3 -> S3 where possible
- multipart upload for large local -> S3 operations
- execution telemetry
- retry and overwrite policy integration

No deletes yet.

### Phase 3: Sync Execution

Deliver:

- execute delete actions
- delete guardrails
- destructive confirmation flow
- delete worker pool
- delete-phase tuning and reporting

### Phase 4: Performance And Advanced Filters

Deliver:

- richer include/exclude behavior
- newer-than / older-than semantics
- size filters
- fast-list and top-up optimizations
- more detailed performance instrumentation
- tunable memory / verification tradeoffs

### Phase 5: Long-Tail Features

Possible additions:

- watch mode
- rename tracking ideas
- Prometheus metrics
- metadata preservation improvements
- future bidirectional design investigation

## Design Constraints

- `Sync` is preview-first by default, with an expert direct-run bypass kept behind advanced controls
- destructive deletes must never be hidden behind a small checkbox
- the compare engine must be deterministic and explainable
- performance settings must describe their tradeoffs
- operator visibility is part of the feature, not a later nicety

## Summary

The sync subsystem is being built as a first-class feature with a real planning
engine, not a larger copy modal. The current repo now has a working preview
planner for `Diff`, `Copy`, and policy-backed `Sync`. The next implementation
milestone is execution: first copy-from-plan, then destructive sync with
guardrails.
