# Features

This file describes the current feature set in this repo. It is intentionally
factual: it lists what the app does today, what exists only partially, and what
is not implemented in the normal UI.

## Available Now

### Core S3 Browser

- Connect to S3-compatible endpoints over HTTP or HTTPS.
- Start from saved connections or from CLI flags: `--endpoint`,
  `--access-key`, and `--secret-key`.
- List buckets.
- Create buckets.
- Browse objects inside a bucket.
- Navigate prefixes using common prefixes and breadcrumb buttons.
- Upload a file from a native file picker.
- Download a selected object to a chosen path.
- Delete a selected object.
- View object metadata from a HEAD request, including size, content type, last
  modified time, ETag, and selected HTTP headers.
- Close the object detail panel with `Esc`.

### Connections And Credentials

- Save named connections with endpoint and region in
  `~/.abixio-ui/settings.json`.
- Store access keys and secret keys in the OS keychain via `keyring`.
- Support anonymous connections when no keychain entries exist.
- Test a saved connection by listing buckets.
- Edit, remove, and switch between saved connections inside the app.

### AbixIO-Specific UI

- Probe `/_admin/status` after connecting.
- Show `D` and `H` sidebar tabs only when the endpoint identifies itself as
  AbixIO.
- Disks view:
  per-disk path, online/offline status, total/used/free bytes, bucket count,
  object count, and summary totals.
- Healing view:
  MRF queue depth, worker count, scanner status, scanner counters, scan/heal
  intervals, and basic server info from the status probe.
- Manual refresh buttons for Disks and Healing.
- Object detail panel section for AbixIO-selected objects:
  erasure summary, shard distribution, per-shard status, checksum display,
  manual inspect refresh, and modal-confirmed manual heal.

### App UI And Diagnostics

- Dark and Light theme switching for the current session.
- Top bar showing the active connection or endpoint.
- Bottom error bar with dismiss action.
- Built-in Testing tab that runs end-to-end smoke checks against the currently
  connected server.

## Present In Code, But Not Fully Exposed

- The Settings view shows network counters, but request and byte metrics are
  not currently wired to real network activity.
- Leaving credential fields blank while editing a saved connection keeps the
  existing keychain entries. There is no clean in-place "make this saved
  connection anonymous" flow yet.

## Not Implemented

- Auto-refresh timers for admin views.
- Multipart upload progress.
- Delete bucket action.
- Persisted UI preferences such as theme, window size, or last active
  connection.
- Success toasts or delete confirmation dialogs for object deletion.
- Bucket detail panel in the normal browsing flow.
- Cleanup of the temporary buckets created by the Testing tab.

## Current Behavior Notes

- Direct CLI connections always use region `us-east-1`. If you need another
  region, use a saved connection profile.
- The Testing tab creates a timestamped bucket and removes test objects, but it
  does not delete the bucket itself because the app does not yet implement
  bucket deletion.
- If a feature is not listed under `Available Now`, it should not be described
  as shipping.
