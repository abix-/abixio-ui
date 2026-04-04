# Testing

How to test abixio-ui with a local AbixIO server.

## Prerequisites

Build both binaries:

```powershell
# in abixio repo
cd C:\code\abixio && cargo build --release

# in abixio-ui repo
cd C:\code\abixio-ui && cargo build --release
```

The exact output path depends on your Cargo target directory. In this
environment Cargo uses a shared target dir, so do not assume the binaries land
under this repo's local `target/` folder.

## Start the server

```powershell
# create disk directories
New-Item -ItemType Directory -Force -Path `
  C:\tmp\abixio\d1, `
  C:\tmp\abixio\d2, `
  C:\tmp\abixio\d3, `
  C:\tmp\abixio\d4 | Out-Null

# start with 4 disks, 2 data + 2 parity, no auth
abixio --listen 0.0.0.0:10000 `
  --disks C:\tmp\abixio\d1,C:\tmp\abixio\d2,C:\tmp\abixio\d3,C:\tmp\abixio\d4 `
  --data 2 --parity 2 --no-auth
```

Server is ready when you see `abixio listening on 0.0.0.0:10000`.

## Launch the UI

```powershell
# option 1: connect directly
abixio-ui --endpoint http://localhost:10000

# option 2: launch and use connection manager
abixio-ui
```

When connecting to AbixIO, the UI auto-detects it and shows admin tabs
(`D`=Disks, `H`=Healing) in the sidebar. Selecting an object also adds an
AbixIO section in the detail panel with shard inspection, `Refresh Inspect`,
and `Heal Object`.

## Test S3 operations via curl.exe

```powershell
# create bucket
curl.exe -X PUT http://localhost:10000/testbucket

# upload objects
curl.exe -X PUT -d "hello world" http://localhost:10000/testbucket/hello.txt
curl.exe -X PUT -d "second file" http://localhost:10000/testbucket/docs/readme.txt
curl.exe -X PUT -d "nested object" http://localhost:10000/testbucket/docs/deep/file.txt

# list buckets (XML)
curl.exe http://localhost:10000/

# list objects
curl.exe "http://localhost:10000/testbucket?list-type=2"

# list with prefix + delimiter
curl.exe "http://localhost:10000/testbucket?list-type=2&prefix=docs/&delimiter=/"

# get object
curl.exe http://localhost:10000/testbucket/hello.txt

# head object (metadata only)
curl.exe -I http://localhost:10000/testbucket/hello.txt

# delete object
curl.exe -X DELETE http://localhost:10000/testbucket/hello.txt
```

## Test admin API

```powershell
# server status (AbixIO detection endpoint)
curl.exe http://localhost:10000/_admin/status
# expected: {"server":"abixio","version":"0.1.0","uptime_secs":...}

# disk health
curl.exe http://localhost:10000/_admin/disks
# expected: per-disk path, online status, space usage, bucket/object counts

# healing status
curl.exe http://localhost:10000/_admin/heal
# expected: mrf_pending, scanner stats

# inspect object shards
curl.exe "http://localhost:10000/_admin/object?bucket=testbucket&key=hello.txt"
# expected: per-shard status (ok/missing/corrupt), checksums, distribution map
```

## Test erasure resilience

This proves data survives disk failures. With 2 data + 2 parity, you can lose any 2 disks.

```powershell
# upload a test object
curl.exe -X PUT -d "important data" http://localhost:10000/testbucket/resilience-test.txt

# verify all 4 shards are ok
curl.exe "http://localhost:10000/_admin/object?bucket=testbucket&key=resilience-test.txt"

# delete shards on 2 of 4 disks (simulating disk failure)
Remove-Item -Recurse -Force C:\tmp\abixio\d3\testbucket\resilience-test.txt
Remove-Item -Recurse -Force C:\tmp\abixio\d4\testbucket\resilience-test.txt

# data is still readable (Reed-Solomon reconstruction)
curl.exe http://localhost:10000/testbucket/resilience-test.txt
# expected: "important data"

# inspect shows missing shards
curl.exe "http://localhost:10000/_admin/object?bucket=testbucket&key=resilience-test.txt"
# expected: 2 shards "ok", 2 shards "missing"

# trigger manual heal to rebuild missing shards
curl.exe -X POST "http://localhost:10000/_admin/heal?bucket=testbucket&key=resilience-test.txt"
# expected: {"result":"repaired","shards_fixed":2}

# verify all shards restored
curl.exe "http://localhost:10000/_admin/object?bucket=testbucket&key=resilience-test.txt"
# expected: all 4 shards "ok"
```

## Test connection manager

1. Launch `abixio-ui` with no args
2. Click "+" (Connections) in the sidebar
3. Add a connection: name=`local`, endpoint=`http://localhost:10000`, region=`us-east-1`, leave keys empty
4. Click "add"
5. Click "test". Should show "connection ok" in the bottom status bar
6. Click "connect". Switches to Browse view, admin tabs appear
7. Click "D" (Disks). Shows disk table
8. Click "H" (Healing). Shows MRF queue + scanner stats
9. Browse to an object and select it. The right detail panel should show
   object metadata plus an AbixIO section with shard status
10. Click `Refresh Inspect`. Shard inspection reloads
11. Click `Heal Object`. Confirmation modal appears before the heal request is sent

## In-app smoke tests

1. Connect to a server first
2. Click `T` in the sidebar
3. Click `run tests`
4. Review the PASS/FAIL table

For AbixIO endpoints the Testing tab also checks the admin status, disks,
healing, and object-inspection APIs. It also deletes its test buckets during
cleanup now that bucket delete exists in the app.

## In-app AbixIO object admin

1. Connect to AbixIO and select an object in Browse
2. Confirm the right detail panel shows:
   erasure summary, shard distribution, per-shard status, and checksums
3. Click `Refresh Inspect`
4. Confirm the AbixIO section reloads without clearing the normal S3 metadata
5. Click `Heal Object`
6. Confirm the modal opens and the request is not sent until confirmation
7. Confirm a successful heal updates the inline result text and refreshes both
   shard inspection and the Healing view data

## Test with non-AbixIO S3

Connect to any S3-compatible endpoint (AWS, MinIO, Backblaze). Admin tabs will NOT appear since `/_admin/status` returns 404. S3 browsing works normally.

```powershell
# example: connect to AWS
abixio-ui --endpoint https://s3.us-west-2.amazonaws.com --access-key AKIA... --secret-key wJalr...
```

## Clean up

```powershell
Remove-Item -Recurse -Force C:\tmp\abixio
```
