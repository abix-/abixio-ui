# Credential Storage

How abixio-ui stores connection profiles and credentials.

## Design principles

1. **No secrets on disk.** Access keys and secret keys are stored only in the
   OS keychain. The on-disk settings file contains names, endpoints, and regions
   -- never any keys.

2. **One profile, one entry.** Each connection is a single profile with
   everything needed to connect: name, endpoint, region, and optionally
   credentials. No separate credential objects to manage.

3. **Anonymous by default.** If a connection has no keys in the keychain, it
   connects without authentication. This is how MinIO and AbixIO work out of
   the box.

## What gets stored where

```
~/.abixio-ui/settings.json            (on disk, not secret)
  |
  +-- connections[]
        name        "aws-prod"
        endpoint    "https://s3.us-west-2.amazonaws.com"
        region      "us-west-2"

OS keychain                            (encrypted by OS)
  |
  +-- service: "abixio-ui"
        "aws-prod.access-key"  ->  "AKIA..."
        "aws-prod.secret-key"  ->  "wJalrXUtn..."
```

### settings.json

One file, flat structure:

```json
{
  "connections": [
    {
      "name": "local",
      "endpoint": "http://localhost:10000",
      "region": "us-east-1"
    },
    {
      "name": "aws-prod",
      "endpoint": "https://s3.us-west-2.amazonaws.com",
      "region": "us-west-2"
    }
  ]
}
```

This file contains NO secrets. You can safely back it up, commit it, or share
it. An attacker with access to this file learns your endpoint URLs but cannot
authenticate.

### OS keychain

Both the access key and secret key are stored in the operating system's native
credential manager:

| OS | Backend | Where to view |
|----|---------|---------------|
| Windows | Credential Manager | Control Panel > Credential Manager > Windows Credentials |
| macOS | Keychain | Keychain Access.app |
| Linux | secret-service | GNOME Keyring / KWallet |

Each connection stores two keychain entries:

- `{name}.access-key` -- the access key ID (e.g. `AKIA...`)
- `{name}.secret-key` -- the secret access key

The service name for all entries is `abixio-ui`.

If the saved key pair is missing, the connection is treated as anonymous.

## Connect flow

When you click "connect" on a connection profile:

1. Read `settings.json` to get the endpoint and region
2. Look up `{name}.access-key` and `{name}.secret-key` in the OS keychain
3. If both keys found: create an authenticated S3 client (AWS Sig V4 via rust-s3)
4. If no keys found: create an anonymous S3 client
5. List buckets to verify the connection works

## Add flow

When you add a new connection:

1. Validate the name:
   - first character must be a letter
   - remaining characters may be letters, digits, `-`, or `_`
2. Validate the endpoint (must be http:// or https://)
3. If access key and secret key are provided:
   - Store both in the OS keychain under the connection name
4. Save the connection (name, endpoint, region) to `settings.json`

If you leave the access key and secret key fields empty, the connection is
saved as anonymous -- no keychain entries are created.

## Edit flow

When you edit an existing connection:

1. The saved endpoint and region are loaded into the form
2. The access key and secret key fields start blank
3. If you enter both new keys, they replace the stored keychain entries
4. If you leave both key fields blank, the existing keychain entries are kept

There is no separate in-app action yet to clear stored keys and convert an
existing saved connection back to anonymous.

## Delete flow

When you delete a connection:

1. Remove the keychain entries `{name}.access-key` and `{name}.secret-key`
   (best-effort -- if they don't exist, that's fine)
2. Remove the connection from `settings.json`

## CLI override

You can bypass the connection manager entirely with CLI args:

```bash
# anonymous
abixio-ui --endpoint http://localhost:10000

# with credentials (not stored, used for this session only)
abixio-ui --endpoint http://localhost:10000 --access-key AKIA... --secret-key wJalrXUtn...
```

CLI credentials are never saved to disk or keychain. They exist only in memory
for the duration of the session.

## Comparison with MinIO mc

| | abixio-ui | MinIO mc |
|---|---|---|
| Secret storage | OS keychain (encrypted) | plaintext JSON in ~/.mc/config.json |
| Access key storage | OS keychain (encrypted) | plaintext JSON in ~/.mc/config.json |
| On-disk file | name + endpoint + region only | everything including secrets |
| If config file is stolen | attacker gets endpoint URLs | attacker gets full credentials |
| Credential model | one profile = one connection | one alias = one connection |

mc stores everything in one JSON file with secrets in plaintext:

```json
{
  "aliases": {
    "s3": {
      "url": "https://s3.amazonaws.com",
      "accessKey": "AKIA...",
      "secretKey": "wJalrXUtn...",    <-- plaintext on disk
      "api": "S3v4"
    }
  }
}
```

We store the same information but split it: non-secret data in settings.json,
secret data in the OS keychain. This means:

- `~/.abixio-ui/settings.json` can be safely backed up or version-controlled
- An attacker reading the config file cannot authenticate to your S3 endpoints
- Secrets are protected by OS-level access controls and encryption

## Relevant source files

- `src/config.rs` -- Settings struct, load/save, add/remove connections
- `src/keychain.rs` -- OS keychain wrapper (store_keys, get_keys, delete_keys)
- `src/views/connections.rs` -- connection manager UI
- `src/app.rs` -- ConnectTo message handler (resolve keys + create client)
