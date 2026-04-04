# Testing

How to test abixio-ui with a local AbixIO server.

## Prerequisites

Build both binaries:

```bash
# in abixio repo
cd C:\code\abixio && cargo build --release

# in abixio-ui repo
cd C:\code\abixio-ui && cargo build --release
```

Binaries:
- `target/release/abixio.exe` (2.3 MB) -- server
- `target/release/abixio-ui.exe` (18 MB) -- desktop UI

## Start the server

```bash
# create disk directories
mkdir -p C:/tmp/abixio/{d1,d2,d3,d4}

# start with 4 disks, 2 data + 2 parity, no auth
abixio --listen 0.0.0.0:10000 \
  --disks C:/tmp/abixio/d1,C:/tmp/abixio/d2,C:/tmp/abixio/d3,C:/tmp/abixio/d4 \
  --data 2 --parity 2 --no-auth
```

Server is ready when you see `abixio listening on 0.0.0.0:10000`.

## Launch the UI

```bash
# option 1: connect directly
abixio-ui --endpoint http://localhost:10000

# option 2: launch and use connection manager
abixio-ui
```

When connecting to AbixIO, the UI auto-detects it and shows admin tabs (D=Disks, H=Healing) in the sidebar.

## Test S3 operations via curl

```bash
# create bucket
curl -X PUT http://localhost:10000/testbucket

# upload objects
curl -X PUT -d "hello world" http://localhost:10000/testbucket/hello.txt
curl -X PUT -d "second file" http://localhost:10000/testbucket/docs/readme.txt
curl -X PUT -d "nested object" http://localhost:10000/testbucket/docs/deep/file.txt

# list buckets (XML)
curl http://localhost:10000/

# list objects
curl "http://localhost:10000/testbucket?list-type=2"

# list with prefix + delimiter
curl "http://localhost:10000/testbucket?list-type=2&prefix=docs/&delimiter=/"

# get object
curl http://localhost:10000/testbucket/hello.txt

# head object (metadata only)
curl -I http://localhost:10000/testbucket/hello.txt

# delete object
curl -X DELETE http://localhost:10000/testbucket/hello.txt
```

## Test admin API

```bash
# server status (AbixIO detection endpoint)
curl http://localhost:10000/_admin/status
# expected: {"server":"abixio","version":"0.1.0","uptime_secs":...}

# disk health
curl http://localhost:10000/_admin/disks
# expected: per-disk path, online status, space usage, bucket/object counts

# healing status
curl http://localhost:10000/_admin/heal
# expected: mrf_pending, scanner stats

# inspect object shards
curl "http://localhost:10000/_admin/object?bucket=testbucket&key=hello.txt"
# expected: per-shard status (ok/missing/corrupt), checksums, distribution map
```

## Test erasure resilience

This proves data survives disk failures. With 2 data + 2 parity, you can lose any 2 disks.

```bash
# upload a test object
curl -X PUT -d "important data" http://localhost:10000/testbucket/resilience-test.txt

# verify all 4 shards are ok
curl "http://localhost:10000/_admin/object?bucket=testbucket&key=resilience-test.txt"

# delete shards on 2 of 4 disks (simulating disk failure)
rm -rf C:/tmp/abixio/d3/testbucket/resilience-test.txt
rm -rf C:/tmp/abixio/d4/testbucket/resilience-test.txt

# data is still readable (Reed-Solomon reconstruction)
curl http://localhost:10000/testbucket/resilience-test.txt
# expected: "important data"

# inspect shows missing shards
curl "http://localhost:10000/_admin/object?bucket=testbucket&key=resilience-test.txt"
# expected: 2 shards "ok", 2 shards "missing"

# trigger manual heal to rebuild missing shards
curl -X POST "http://localhost:10000/_admin/heal?bucket=testbucket&key=resilience-test.txt"
# expected: {"result":"repaired","shards_fixed":2}

# verify all shards restored
curl "http://localhost:10000/_admin/object?bucket=testbucket&key=resilience-test.txt"
# expected: all 4 shards "ok"
```

## Test connection manager

1. Launch `abixio-ui` with no args
2. Click "+" (Connections) in the sidebar
3. Add a connection: name=`local`, endpoint=`http://localhost:10000`, region=`us-east-1`, leave keys empty
4. Click "add"
5. Click "test" -- should show "connection ok"
6. Click "connect" -- switches to Browse view, admin tabs appear
7. Click "D" (Disks) -- shows disk table
8. Click "H" (Healing) -- shows MRF queue + scanner stats

## Test with non-AbixIO S3

Connect to any S3-compatible endpoint (AWS, MinIO, Backblaze). Admin tabs will NOT appear since `/_admin/status` returns 404. S3 browsing works normally.

```bash
# example: connect to AWS
abixio-ui --endpoint https://s3.us-west-2.amazonaws.com --access-key AKIA... --secret-key wJalr...
```

## Clean up

```bash
rm -rf C:/tmp/abixio
```
