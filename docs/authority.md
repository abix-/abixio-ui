# Data Authority

Who owns what data, where it lives, and how it flows.

## Authority map

| Data | Authoritative source | Local storage |
|------|---------------------|---------------|
| Buckets, objects, metadata | S3 endpoint | None |
| AbixIO status, disks, heal data | AbixIO `/_admin/*` API | None |
| Saved connection list | `~/.abixio-ui/settings.json` | Name, endpoint, region |
| Saved access keys | OS keychain | Never written to disk |
| Saved secret keys | OS keychain | Never written to disk |
| CLI credentials | Process memory only | Not persisted |
| Theme and transient UI state | In-memory only | Not persisted |

## Rules

1. **The server is authoritative.**
   Bucket listings, object listings, object metadata, and AbixIO admin data are
   fetched live from the connected endpoint. The app does not keep a local data
   cache.

2. **Saved secrets stay in the keychain.**
   `settings.json` stores only non-secret connection metadata. Access keys and
   secret keys for saved profiles live in the OS keychain.

3. **CLI secrets are session-only.**
   Credentials passed with `--access-key` and `--secret-key` are used to build
   the initial client for that process and are not written to disk or keychain.

4. **Writes are server-first.**
   Upload, delete, and create-bucket operations wait for the server result and
   then refresh the relevant listing. On failure, the app shows the error in
   the bottom status bar.

## Read path

```text
user selects a bucket
  -> App::update sets loading state
  -> Task::batch fires in parallel:
     - S3 list_objects (object listing)
     - S3 get_bucket_versioning (versioning status)
     - S3 get_bucket_policy (policy JSON)
     - S3 get_bucket_lifecycle (lifecycle rules)
     - S3 get_bucket_tagging (bucket tags)
  -> Each result updates state independently
  -> UI renders bucket detail + object listing

user selects an object
  -> App::update sets loading state
  -> Task::batch fires in parallel:
     - S3 head_object (metadata)
     - S3 get_object_tagging (tags)
     - S3 list_object_versions (versions)
     - S3 get_object (first 4KB preview)
     - if AbixIO: /_admin/object (shard inspection)
  -> Each result updates state independently
  -> UI renders the object detail panel

user connects to an AbixIO server
  -> Task::perform probes GET /_admin/status
  -> if server == "abixio", app fetches /_admin/disks and /_admin/heal
```

There is no background polling and no local cache invalidation logic because
the app re-reads the authoritative source when the user asks for data.

## Write path

```text
upload:
  user picks a file in a native dialog
  -> PUT object
  -> on success: refresh current object listing
  -> on error: show bottom status/error bar

delete:
  user clicks Delete in the object detail panel
  -> DELETE object
  -> on success: clear selection and refresh current listing
  -> on error: show bottom status/error bar

create bucket:
  user enters a bucket name
  -> PUT bucket
  -> on success: refresh bucket list
  -> on error: show bottom status/error bar

manual heal:
  user clicks Heal Object in the AbixIO object detail panel
  -> modal confirmation opens
  -> POST /_admin/heal?bucket=...&key=...
  -> on success: refresh object inspection and heal-status data
  -> on error: show inline heal error and bottom status/error bar
```

The current UI does not show success toasts. It does ask for confirmation
before object heal, but it still does not ask for confirmation before
dispatching object delete.

## Credential storage

```text
~/.abixio-ui/settings.json
  {
    "connections": [
      {"name": "home", "endpoint": "http://nas:10000", "region": "us-east-1"},
      {"name": "aws", "endpoint": "https://s3.amazonaws.com", "region": "us-west-2"}
    ]
  }

OS keychain service "abixio-ui"
  "home.access-key" -> "mykey"
  "home.secret-key" -> "secret-key-here"
  "aws.access-key"  -> "AKIA..."
  "aws.secret-key"  -> "aws-secret-key"
```

See [docs/credentials.md](credentials.md) for the connection and edit flows.

## What is not stored locally

- Object data
- Bucket listings
- AbixIO status, disk, heal, and object-inspection responses
- Persisted UI preferences such as theme, window size, or last active connection
