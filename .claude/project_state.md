# abixio-ui project state

## last session: 2026-04-05

### what we worked on
- shipped sync phases 1-4 in a single session
- phase 1: preview planner with real diff engine
- phase 2: copy execution from reviewed plan
- phase 3: guarded sync execution with delete guardrails
- phase 4: concurrent transfer workers, rclone-compatible filters (size suffixes, time filters, ** globs), throughput telemetry, bandwidth/multipart tunable fields (parse only)
- 137 tests pass (77 unit + 24 phase 4 sync + 13 handler + 23 integration-style)
- inline policy and lifecycle editors for buckets
- multipart upload for large files (>8MB)

### decisions made
- **sync is preview-first**: destructive sync only runs from a reviewed plan, never auto-executes
- **delete guardrails**: typed confirmation above risk thresholds, skip deletes after transfer failures unless ignore-errors enabled
- **concurrent transfer pool**: configurable worker count (default 4), loop dispatch in both Copying and DeletingDuring phases
- **rclone filter syntax**: binary units (K=1024), relative durations (1d/2w/1M/1y), RFC3339 absolute dates
- **backward compat globs**: single `*` crosses `/` when no `**` in pattern, `**` triggers path-boundary-aware matching
- **parse-only tunable fields**: bandwidth limit, multipart cutoff, multipart chunk size exist in UI but have no enforcement logic yet

### current state
- abixio-ui: compiles clean, 137 tests pass
- sync phases 1-4: shipped
- sync phase 5: not started (watch mode, rename tracking, prometheus, bidirectional)
- features.md parity scores updated to reflect phase 4

### next steps
- presigned upload URLs (todo item 1, server presigned auth ready)
- inline content viewer beyond 4KB (todo item 2)
- phase 5 long-tail sync features if prioritized
- split handlers/sync.rs (1625 lines, biggest file in project)
- integration test harness against real S3 endpoint
