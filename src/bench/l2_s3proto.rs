//! L2: S3 protocol (isolated)
//!
//! Measures ONLY the cost of s3s header parsing, SigV4 verification,
//! AbixioS3 dispatch, and VolumePool routing. No TCP, no real storage.
//!
//! How it works:
//! - Creates an s3s service with NullBackend (all writes discarded,
//!   all reads return empty)
//! - Each request creates a tokio::io::duplex in-memory pipe
//! - hyper client and server connect over the pipe (no TCP socket)
//! - Times each request through s3s dispatch
//!
//! Why in-memory pipe instead of TCP: TCP adds ~0.2ms on Windows
//! loopback. That would contaminate L2 with L1 overhead. The duplex
//! pipe removes TCP entirely so we measure only s3s.
//!
//! What this number means: the per-request cost of S3 protocol
//! handling. This is overhead that every S3 request pays on top of
//! whatever storage work the request does.

use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::Arc;
use std::time::Instant;

use abixio::cluster::{ClusterConfig, ClusterManager};
use abixio::s3_route::AbixioDispatch;
use abixio::storage::volume_pool::VolumePool;
use abixio::storage::{Backend, Store};

use super::stats::{human_size, iters_for_size, BenchResult};

pub async fn run(sizes: &[usize], iters_override: Option<usize>) -> Vec<BenchResult> {
    let mut results = Vec::new();

    eprintln!("--- L2: S3 protocol (isolated, in-memory pipe, NullBackend) ---");

    let backends: Vec<Box<dyn Backend>> = vec![Box::new(NullBackend::new())];
    let pool = Arc::new(VolumePool::new(backends).unwrap());
    pool.make_bucket("bench").await.unwrap();

    let cluster = Arc::new(
        ClusterManager::new(ClusterConfig {
            node_id: "bench".into(),
            advertise_s3: "http://127.0.0.1:0".into(),
            advertise_cluster: "http://127.0.0.1:0".into(),
            nodes: Vec::new(),
            access_key: String::new(),
            secret_key: String::new(),
            no_auth: true,
            disk_paths: Vec::new(),
        })
        .unwrap(),
    );

    let s3 = abixio::s3_service::AbixioS3::new(Arc::clone(&pool), Arc::clone(&cluster));
    let mut builder = s3s::service::S3ServiceBuilder::new(s3);
    builder.set_validation(abixio::s3_service::RelaxedNameValidation);
    let s3_service = builder.build();
    let dispatch = Arc::new(AbixioDispatch::new(s3_service, None, None));

    for &size in sizes {
        let data = vec![0x42u8; size];
        let iters = iters_override.unwrap_or_else(|| iters_for_size(size));
        let label = human_size(size);

        // warmup
        for i in 0..3 {
            let uri = format!("/bench/warmup_{}", i);
            duplex_put(&dispatch, &uri, &data).await;
        }

        // PUT
        let mut timings = Vec::with_capacity(iters);
        for i in 0..iters {
            let uri = format!("/bench/s2_{}_{}", label, i);
            let t = Instant::now();
            duplex_put(&dispatch, &uri, &data).await;
            timings.push(t.elapsed());
        }
        results.push(BenchResult {
            layer: "L2".into(),
            op: "s3s_put".into(),
            size,
            iters,
            write_path: None,
            write_cache: None,
            read_cache: None,
            server: None,
            client: None,
            timings,
        });

        // GET
        let mut timings = Vec::with_capacity(iters);
        for i in 0..iters {
            let uri = format!("/bench/s2_{}_{}", label, i);
            let t = Instant::now();
            duplex_get(&dispatch, &uri).await;
            timings.push(t.elapsed());
        }
        results.push(BenchResult {
            layer: "L2".into(),
            op: "s3s_get".into(),
            size,
            iters,
            write_path: None,
            write_cache: None,
            read_cache: None,
            server: None,
            client: None,
            timings,
        });

        eprintln!("  {} done ({} iters)", label, iters);
    }

    results
}

/// PUT via in-memory duplex pipe: hyper client -> duplex -> hyper server -> s3s
async fn duplex_put(dispatch: &Arc<AbixioDispatch>, uri: &str, data: &[u8]) {
    use http_body_util::BodyExt;

    let (client_io, server_io) = tokio::io::duplex(1024 * 1024);

    let d = dispatch.clone();
    let server = tokio::spawn(async move {
        let io = hyper_util::rt::TokioIo::new(server_io);
        let svc = hyper::service::service_fn(move |req| {
            let d = d.clone();
            async move { Ok::<_, hyper::Error>(d.dispatch(req).await) }
        });
        let _ = hyper::server::conn::http1::Builder::new()
            .serve_connection(io, svc)
            .await;
    });

    let io = hyper_util::rt::TokioIo::new(client_io);
    let (mut sender, conn) = hyper::client::conn::http1::handshake(io).await.unwrap();
    tokio::spawn(conn);

    let req = hyper::Request::builder()
        .method(hyper::Method::PUT)
        .uri(uri)
        .body(http_body_util::Full::new(bytes::Bytes::from(data.to_vec())))
        .unwrap();

    let resp = sender.send_request(req).await.unwrap();
    assert!(resp.status().is_success(), "PUT failed: {}", resp.status());
    let _ = resp.into_body().collect().await;

    server.abort();
}

/// GET via in-memory duplex pipe: hyper client -> duplex -> hyper server -> s3s
async fn duplex_get(dispatch: &Arc<AbixioDispatch>, uri: &str) {
    use http_body_util::BodyExt;

    let (client_io, server_io) = tokio::io::duplex(1024 * 1024);

    let d = dispatch.clone();
    let server = tokio::spawn(async move {
        let io = hyper_util::rt::TokioIo::new(server_io);
        let svc = hyper::service::service_fn(move |req| {
            let d = d.clone();
            async move { Ok::<_, hyper::Error>(d.dispatch(req).await) }
        });
        let _ = hyper::server::conn::http1::Builder::new()
            .serve_connection(io, svc)
            .await;
    });

    let io = hyper_util::rt::TokioIo::new(client_io);
    let (mut sender, conn) = hyper::client::conn::http1::handshake(io).await.unwrap();
    tokio::spawn(conn);

    let req = hyper::Request::builder()
        .method(hyper::Method::GET)
        .uri(uri)
        .body(http_body_util::Full::new(bytes::Bytes::new()))
        .unwrap();

    let resp = sender.send_request(req).await.unwrap();
    let _ = resp.into_body().collect().await;

    server.abort();
}

// NullBackend: zero-cost Backend that returns Ok() for everything.
// Isolates s3s protocol overhead from storage work.

pub struct NullBackend {
    volume_id: std::sync::Mutex<String>,
    bucket_created: AtomicBool,
}

impl NullBackend {
    pub fn new() -> Self {
        Self {
            volume_id: std::sync::Mutex::new(String::new()),
            bucket_created: AtomicBool::new(false),
        }
    }
}

struct NullShardWriter;

#[async_trait::async_trait]
impl abixio::storage::ShardWriter for NullShardWriter {
    async fn write_chunk(&mut self, _chunk: &[u8]) -> Result<(), abixio::storage::StorageError> {
        Ok(())
    }
    async fn finalize(
        self: Box<Self>,
        _meta: &abixio::storage::metadata::ObjectMeta,
    ) -> Result<(), abixio::storage::StorageError> {
        Ok(())
    }
}

#[async_trait::async_trait]
impl Backend for NullBackend {
    async fn open_shard_writer(
        &self, _b: &str, _k: &str, _v: Option<&str>,
    ) -> Result<Box<dyn abixio::storage::ShardWriter>, abixio::storage::StorageError> {
        Ok(Box::new(NullShardWriter))
    }
    async fn write_shard(
        &self, _b: &str, _k: &str, _data: &[u8], _meta: &abixio::storage::metadata::ObjectMeta,
    ) -> Result<(), abixio::storage::StorageError> {
        Ok(())
    }
    async fn read_shard(
        &self, _b: &str, _k: &str,
    ) -> Result<(Vec<u8>, abixio::storage::metadata::ObjectMeta), abixio::storage::StorageError> {
        Ok((Vec::new(), abixio::storage::metadata::ObjectMeta::default()))
    }
    async fn delete_object(&self, _b: &str, _k: &str) -> Result<(), abixio::storage::StorageError> {
        Ok(())
    }
    async fn list_objects(&self, _b: &str, _p: &str) -> Result<Vec<String>, abixio::storage::StorageError> {
        Ok(Vec::new())
    }
    async fn list_buckets(&self) -> Result<Vec<String>, abixio::storage::StorageError> {
        Ok(vec!["bench".into()])
    }
    async fn make_bucket(&self, _b: &str) -> Result<(), abixio::storage::StorageError> {
        self.bucket_created.store(true, Ordering::SeqCst);
        Ok(())
    }
    async fn delete_bucket(&self, _b: &str) -> Result<(), abixio::storage::StorageError> {
        Ok(())
    }
    async fn bucket_exists(&self, _b: &str) -> bool {
        self.bucket_created.load(Ordering::SeqCst)
    }
    async fn bucket_created_at(&self, _b: &str) -> u64 {
        1700000000
    }
    async fn stat_object(
        &self, _b: &str, _k: &str,
    ) -> Result<abixio::storage::metadata::ObjectMeta, abixio::storage::StorageError> {
        Ok(abixio::storage::metadata::ObjectMeta::default())
    }
    async fn update_meta(
        &self, _b: &str, _k: &str, _meta: &abixio::storage::metadata::ObjectMeta,
    ) -> Result<(), abixio::storage::StorageError> {
        Ok(())
    }
    async fn read_meta_versions(
        &self, _b: &str, _k: &str,
    ) -> Result<Vec<abixio::storage::metadata::ObjectMeta>, abixio::storage::StorageError> {
        Ok(Vec::new())
    }
    async fn write_meta_versions(
        &self, _b: &str, _k: &str, _v: &[abixio::storage::metadata::ObjectMeta],
    ) -> Result<(), abixio::storage::StorageError> {
        Ok(())
    }
    async fn write_versioned_shard(
        &self, _b: &str, _k: &str, _vid: &str, _data: &[u8], _meta: &abixio::storage::metadata::ObjectMeta,
    ) -> Result<(), abixio::storage::StorageError> {
        Ok(())
    }
    async fn read_versioned_shard(
        &self, _b: &str, _k: &str, _vid: &str,
    ) -> Result<(Vec<u8>, abixio::storage::metadata::ObjectMeta), abixio::storage::StorageError> {
        Ok((Vec::new(), abixio::storage::metadata::ObjectMeta::default()))
    }
    async fn delete_version_data(
        &self, _b: &str, _k: &str, _vid: &str,
    ) -> Result<(), abixio::storage::StorageError> {
        Ok(())
    }
    async fn read_bucket_settings(
        &self, _b: &str,
    ) -> abixio::storage::metadata::BucketSettings {
        abixio::storage::metadata::BucketSettings::default()
    }
    async fn write_bucket_settings(
        &self, _b: &str, _s: &abixio::storage::metadata::BucketSettings,
    ) -> Result<(), abixio::storage::StorageError> {
        Ok(())
    }
    fn info(&self) -> abixio::storage::BackendInfo {
        abixio::storage::BackendInfo {
            label: "null".into(),
            volume_id: self.volume_id.lock().unwrap().clone(),
            backend_type: "null".into(),
            total_bytes: None,
            used_bytes: None,
            free_bytes: None,
        }
    }
    fn set_volume_id(&mut self, id: String) {
        *self.volume_id.lock().unwrap() = id;
    }
}
