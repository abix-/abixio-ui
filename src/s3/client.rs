use std::collections::HashMap;
use std::path::Path;
use std::sync::Arc;
use std::sync::atomic::{AtomicU64, Ordering};
use std::time::Duration;

use aws_credential_types::Credentials;
use aws_credential_types::credential_fn::provide_credentials_fn;
use aws_credential_types::provider::error::CredentialsError;
use aws_sdk_s3::Client;
use aws_sdk_s3::config::{AppName, BehaviorVersion, Builder, Region, timeout::TimeoutConfig};
use aws_sdk_s3::error::ProvideErrorMetadata;
use aws_sdk_s3::primitives::ByteStream;
use aws_sdk_s3::types::{
    BucketVersioningStatus, CompletedMultipartUpload, CompletedPart, Delete, ObjectIdentifier, Tag,
    Tagging, VersioningConfiguration,
};

use crate::s3::lifecycle_xml;

const MULTIPART_THRESHOLD: u64 = 8 * 1024 * 1024; // 8MB
const MULTIPART_PART_SIZE: usize = 8 * 1024 * 1024; // 8MB

/// Shared atomic counters for S3 network activity.
#[derive(Default)]
pub struct S3Stats {
    pub requests: AtomicU64,
    pub bytes_in: AtomicU64,
    pub bytes_out: AtomicU64,
}

#[derive(Clone)]
pub struct S3Client {
    client: Client,
    stats: Arc<S3Stats>,
}

impl S3Client {
    pub fn new(
        endpoint: &str,
        creds: Option<(&str, &str)>,
        region_name: &str,
    ) -> Result<Self, String> {
        let endpoint = endpoint.trim_end_matches('/').to_string();

        let timeout = TimeoutConfig::builder()
            .connect_timeout(Duration::from_secs(10))
            .operation_timeout(Duration::from_secs(60))
            .build();

        let mut builder = Builder::new()
            .behavior_version(BehaviorVersion::latest())
            .endpoint_url(&endpoint)
            .region(Region::new(region_name.to_string()))
            .force_path_style(true)
            .timeout_config(timeout)
            .app_name(AppName::new("abixio-ui").expect("valid app name"));

        builder = match creds {
            Some((access_key, secret_key)) => builder.credentials_provider(Credentials::new(
                access_key, secret_key, None, None, "static",
            )),
            None => builder
                .credentials_provider(provide_credentials_fn(|| async {
                    Err(CredentialsError::not_loaded("anonymous"))
                }))
                .allow_no_auth(),
        };

        Ok(Self {
            client: Client::from_conf(builder.build()),
            stats: Arc::new(S3Stats::default()),
        })
    }

    /// Create an anonymous client (no auth) for the given endpoint.
    pub fn anonymous(endpoint: &str) -> Result<Self, String> {
        Self::new(endpoint, None, "us-east-1")
    }

    /// Shared stats counters. Readable from any thread while S3 calls run.
    pub fn stats(&self) -> &Arc<S3Stats> {
        &self.stats
    }

    fn record_request(&self) {
        self.stats.requests.fetch_add(1, Ordering::Relaxed);
    }

    fn record_bytes_in(&self, n: u64) {
        self.stats.bytes_in.fetch_add(n, Ordering::Relaxed);
    }

    fn record_bytes_out(&self, n: u64) {
        self.stats.bytes_out.fetch_add(n, Ordering::Relaxed);
    }

    pub async fn list_buckets(&self) -> Result<Vec<BucketInfo>, String> {
        let resp = self
            .client
            .list_buckets()
            .send()
            .await
            .map_err(|e| e.to_string())?;
        self.record_request();

        Ok(resp
            .buckets()
            .iter()
            .map(|b| BucketInfo {
                name: b.name().unwrap_or_default().to_string(),
                creation_date: String::new(),
            })
            .collect())
    }

    pub async fn list_objects(
        &self,
        bucket: &str,
        prefix: &str,
        delimiter: &str,
    ) -> Result<ListObjectsResult, String> {
        let mut objects = Vec::new();
        let mut common_prefixes = Vec::new();
        let mut is_truncated = false;
        let mut continuation_token: Option<String> = None;

        loop {
            let mut req = self.client.list_objects_v2().bucket(bucket).prefix(prefix);

            if !delimiter.is_empty() {
                req = req.delimiter(delimiter);
            }
            if let Some(token) = &continuation_token {
                req = req.continuation_token(token);
            }

            let resp = req.send().await.map_err(|e| e.to_string())?;
            self.record_request();

            for obj in resp.contents() {
                objects.push(ObjectInfo {
                    key: obj.key().unwrap_or_default().to_string(),
                    size: obj.size().unwrap_or(0) as u64,
                    last_modified: obj
                        .last_modified()
                        .map(|t| t.to_string())
                        .unwrap_or_default(),
                    etag: obj.e_tag().unwrap_or_default().to_string(),
                });
            }

            for cp in resp.common_prefixes() {
                if let Some(p) = cp.prefix() {
                    common_prefixes.push(p.to_string());
                }
            }

            let truncated = resp.is_truncated().unwrap_or(false);
            if truncated {
                is_truncated = true;
                continuation_token = resp.next_continuation_token().map(|s| s.to_string());
                if continuation_token.is_none() {
                    break;
                }
            } else {
                break;
            }
        }

        Ok(ListObjectsResult {
            objects,
            common_prefixes,
            is_truncated,
        })
    }

    pub async fn create_bucket(&self, bucket: &str) -> Result<(), String> {
        self.client
            .create_bucket()
            .bucket(bucket)
            .send()
            .await
            .map_err(|e| e.to_string())?;
        self.record_request();
        Ok(())
    }

    pub async fn delete_bucket(&self, bucket: &str) -> Result<(), String> {
        self.client
            .delete_bucket()
            .bucket(bucket)
            .send()
            .await
            .map_err(|e| e.to_string())?;
        self.record_request();
        Ok(())
    }

    pub async fn put_object(
        &self,
        bucket: &str,
        key: &str,
        data: Vec<u8>,
        content_type: &str,
    ) -> Result<String, String> {
        let len = data.len() as u64;
        self.client
            .put_object()
            .bucket(bucket)
            .key(key)
            .content_type(content_type)
            .body(ByteStream::from(data))
            .send()
            .await
            .map_err(|e| e.to_string())?;
        self.record_request();
        self.record_bytes_out(len);
        Ok(String::new())
    }

    /// Upload a local file. Uses multipart upload for files > 8MB,
    /// single PutObject for smaller files. On multipart failure,
    /// aborts the upload to prevent orphaned parts.
    pub async fn upload_file(
        &self,
        bucket: &str,
        key: &str,
        path: &Path,
        content_type: &str,
    ) -> Result<String, String> {
        let meta = tokio::fs::metadata(path).await.map_err(|e| e.to_string())?;
        let size = meta.len();

        if size <= MULTIPART_THRESHOLD {
            let data = tokio::fs::read(path).await.map_err(|e| e.to_string())?;
            return self.put_object(bucket, key, data, content_type).await;
        }

        // multipart upload
        let upload_id = self
            .create_multipart_upload(bucket, key, content_type)
            .await?;
        match self.upload_parts(bucket, key, &upload_id, path, size).await {
            Ok(parts) => {
                self.complete_multipart_upload(bucket, key, &upload_id, parts)
                    .await?;
                Ok(String::new())
            }
            Err(e) => {
                let _ = self.abort_multipart_upload(bucket, key, &upload_id).await;
                Err(e)
            }
        }
    }

    async fn create_multipart_upload(
        &self,
        bucket: &str,
        key: &str,
        content_type: &str,
    ) -> Result<String, String> {
        let resp = self
            .client
            .create_multipart_upload()
            .bucket(bucket)
            .key(key)
            .content_type(content_type)
            .send()
            .await
            .map_err(|e| e.to_string())?;
        self.record_request();

        resp.upload_id()
            .map(|s| s.to_string())
            .ok_or_else(|| "server did not return upload_id".to_string())
    }

    async fn upload_parts(
        &self,
        bucket: &str,
        key: &str,
        upload_id: &str,
        path: &Path,
        size: u64,
    ) -> Result<Vec<CompletedPart>, String> {
        use tokio::io::AsyncReadExt;

        let mut file = tokio::fs::File::open(path)
            .await
            .map_err(|e| e.to_string())?;
        let mut parts = Vec::new();
        let mut part_number: i32 = 1;
        let mut remaining = size;

        while remaining > 0 {
            let chunk_size = (remaining as usize).min(MULTIPART_PART_SIZE);
            let mut buf = vec![0u8; chunk_size];
            let mut read = 0;
            while read < chunk_size {
                let n = file
                    .read(&mut buf[read..])
                    .await
                    .map_err(|e| e.to_string())?;
                if n == 0 {
                    break;
                }
                read += n;
            }
            buf.truncate(read);
            remaining -= read as u64;

            let resp = self
                .client
                .upload_part()
                .bucket(bucket)
                .key(key)
                .upload_id(upload_id)
                .part_number(part_number)
                .body(ByteStream::from(buf.clone()))
                .send()
                .await
                .map_err(|e| e.to_string())?;
            self.record_request();
            self.record_bytes_out(buf.len() as u64);

            let etag = resp.e_tag().unwrap_or_default().to_string();
            parts.push(
                CompletedPart::builder()
                    .e_tag(etag)
                    .part_number(part_number)
                    .build(),
            );
            part_number += 1;
        }

        Ok(parts)
    }

    async fn complete_multipart_upload(
        &self,
        bucket: &str,
        key: &str,
        upload_id: &str,
        parts: Vec<CompletedPart>,
    ) -> Result<(), String> {
        let completed = CompletedMultipartUpload::builder()
            .set_parts(Some(parts))
            .build();

        self.client
            .complete_multipart_upload()
            .bucket(bucket)
            .key(key)
            .upload_id(upload_id)
            .multipart_upload(completed)
            .send()
            .await
            .map_err(|e| e.to_string())?;
        self.record_request();
        Ok(())
    }

    async fn abort_multipart_upload(
        &self,
        bucket: &str,
        key: &str,
        upload_id: &str,
    ) -> Result<(), String> {
        self.client
            .abort_multipart_upload()
            .bucket(bucket)
            .key(key)
            .upload_id(upload_id)
            .send()
            .await
            .map_err(|e| e.to_string())?;
        self.record_request();
        Ok(())
    }

    pub async fn get_object(&self, bucket: &str, key: &str) -> Result<Vec<u8>, String> {
        let resp = self
            .client
            .get_object()
            .bucket(bucket)
            .key(key)
            .send()
            .await
            .map_err(|e| e.to_string())?;
        self.record_request();

        let bytes = resp
            .body
            .collect()
            .await
            .map_err(|e| e.to_string())?
            .into_bytes();

        let data = bytes.to_vec();
        self.record_bytes_in(data.len() as u64);
        Ok(data)
    }

    pub async fn download_object_to_file(
        &self,
        bucket: &str,
        key: &str,
        path: &Path,
    ) -> Result<u64, String> {
        use tokio::io::AsyncWriteExt;

        let resp = self
            .client
            .get_object()
            .bucket(bucket)
            .key(key)
            .send()
            .await
            .map_err(|e| e.to_string())?;
        self.record_request();

        if let Some(parent) = path.parent() {
            tokio::fs::create_dir_all(parent)
                .await
                .map_err(|e| e.to_string())?;
        }

        let mut file = tokio::fs::File::create(path)
            .await
            .map_err(|e| e.to_string())?;
        let mut reader = resp.body.into_async_read();
        let copied = tokio::io::copy(&mut reader, &mut file)
            .await
            .map_err(|e| e.to_string())?;
        file.flush().await.map_err(|e| e.to_string())?;
        self.record_bytes_in(copied);
        Ok(copied)
    }

    pub async fn put_object_stream(
        &self,
        bucket: &str,
        key: &str,
        body: ByteStream,
        content_type: &str,
        content_length: Option<u64>,
    ) -> Result<(), String> {
        let mut request = self
            .client
            .put_object()
            .bucket(bucket)
            .key(key)
            .content_type(content_type)
            .body(body);
        if let Some(length) = content_length {
            request = request.content_length(length as i64);
        }
        request.send().await.map_err(|e| e.to_string())?;
        self.record_request();
        if let Some(length) = content_length {
            self.record_bytes_out(length);
        }
        Ok(())
    }

    pub async fn relay_object_to_s3(
        &self,
        bucket: &str,
        key: &str,
        destination_client: &S3Client,
        destination_bucket: &str,
        destination_key: &str,
    ) -> Result<u64, String> {
        let resp = self
            .client
            .get_object()
            .bucket(bucket)
            .key(key)
            .send()
            .await
            .map_err(|e| e.to_string())?;
        self.record_request();

        let content_type = resp
            .content_type()
            .unwrap_or("application/octet-stream")
            .to_string();
        let content_length = resp.content_length().map(|length| length as u64);
        destination_client
            .put_object_stream(
                destination_bucket,
                destination_key,
                resp.body,
                &content_type,
                content_length,
            )
            .await?;
        if let Some(length) = content_length {
            self.record_bytes_in(length);
            Ok(length)
        } else {
            Ok(0)
        }
    }

    pub async fn head_object(&self, bucket: &str, key: &str) -> Result<ObjectDetail, String> {
        let resp = self
            .client
            .head_object()
            .bucket(bucket)
            .key(key)
            .send()
            .await
            .map_err(|e| e.to_string())?;
        self.record_request();

        let mut headers = Vec::new();
        if let Some(v) = resp.content_type() {
            headers.push(("content-type".to_string(), v.to_string()));
        }
        if let Some(v) = resp.content_length() {
            headers.push(("content-length".to_string(), v.to_string()));
        }
        if let Some(v) = resp.last_modified() {
            headers.push(("last-modified".to_string(), v.to_string()));
        }
        if let Some(v) = resp.e_tag() {
            headers.push(("etag".to_string(), v.to_string()));
        }
        if let Some(v) = resp.cache_control() {
            headers.push(("cache-control".to_string(), v.to_string()));
        }
        if let Some(v) = resp.content_disposition() {
            headers.push(("content-disposition".to_string(), v.to_string()));
        }
        if let Some(v) = resp.content_encoding() {
            headers.push(("content-encoding".to_string(), v.to_string()));
        }
        if let Some(v) = resp.accept_ranges() {
            headers.push(("accept-ranges".to_string(), v.to_string()));
        }
        if let Some(v) = resp.expiration() {
            headers.push(("x-amz-expiration".to_string(), v.to_string()));
        }
        if let Some(meta) = resp.metadata() {
            for (k, v) in meta {
                headers.push((format!("x-amz-meta-{}", k), v.to_string()));
            }
        }

        Ok(ObjectDetail {
            key: key.to_string(),
            size: resp.content_length().unwrap_or(0) as u64,
            content_type: resp.content_type().unwrap_or_default().to_string(),
            last_modified: resp
                .last_modified()
                .map(|t| t.to_string())
                .unwrap_or_default(),
            etag: resp.e_tag().unwrap_or_default().to_string(),
            headers,
        })
    }

    /// Server-side copy. Uses the S3 CopyObject API for both same-bucket and
    /// cross-bucket copies. The copy source format is "bucket/key".
    pub async fn copy_object(
        &self,
        src_bucket: &str,
        src_key: &str,
        dst_bucket: &str,
        dst_key: &str,
    ) -> Result<(), String> {
        let copy_source = format!("{}/{}", src_bucket, src_key);
        self.client
            .copy_object()
            .copy_source(&copy_source)
            .bucket(dst_bucket)
            .key(dst_key)
            .send()
            .await
            .map_err(|e| e.to_string())?;
        self.record_request();
        Ok(())
    }

    pub async fn list_objects_recursive(
        &self,
        bucket: &str,
        prefix: &str,
    ) -> Result<ListObjectsResult, String> {
        let mut objects = Vec::new();
        let mut is_truncated = false;
        let mut continuation_token: Option<String> = None;

        loop {
            let mut req = self.client.list_objects_v2().bucket(bucket).prefix(prefix);

            if let Some(token) = &continuation_token {
                req = req.continuation_token(token);
            }

            let resp = req.send().await.map_err(|e| e.to_string())?;
            self.record_request();

            for obj in resp.contents() {
                objects.push(ObjectInfo {
                    key: obj.key().unwrap_or_default().to_string(),
                    size: obj.size().unwrap_or(0) as u64,
                    last_modified: obj
                        .last_modified()
                        .map(|t| t.to_string())
                        .unwrap_or_default(),
                    etag: obj.e_tag().unwrap_or_default().to_string(),
                });
            }

            let truncated = resp.is_truncated().unwrap_or(false);
            if truncated {
                is_truncated = true;
                continuation_token = resp.next_continuation_token().map(|s| s.to_string());
                if continuation_token.is_none() {
                    break;
                }
            } else {
                break;
            }
        }

        Ok(ListObjectsResult {
            objects,
            common_prefixes: Vec::new(),
            is_truncated,
        })
    }

    pub async fn list_objects_recursive_for_sync(
        &self,
        bucket: &str,
        prefix: &str,
    ) -> Result<Vec<crate::app::SyncObject>, String> {
        let listing = self.list_objects_recursive(bucket, prefix).await?;
        Ok(listing
            .objects
            .into_iter()
            .map(|object| crate::app::SyncObject {
                relative_path: object
                    .key
                    .strip_prefix(prefix)
                    .unwrap_or(&object.key)
                    .trim_start_matches('/')
                    .to_string(),
                size: object.size,
                modified: if object.last_modified.is_empty() {
                    None
                } else {
                    Some(object.last_modified)
                },
                etag: if object.etag.is_empty() {
                    None
                } else {
                    Some(object.etag)
                },
                is_dir_marker: false,
            })
            .collect())
    }

    pub async fn delete_object(&self, bucket: &str, key: &str) -> Result<(), String> {
        self.client
            .delete_object()
            .bucket(bucket)
            .key(key)
            .send()
            .await
            .map_err(|e| e.to_string())?;
        self.record_request();
        Ok(())
    }

    /// Batch delete up to 1000 keys per call using the S3 DeleteObjects API.
    /// Returns the list of keys that failed to delete (empty on full success).
    pub async fn delete_objects(
        &self,
        bucket: &str,
        keys: &[String],
    ) -> Result<Vec<String>, String> {
        let identifiers: Vec<ObjectIdentifier> = keys
            .iter()
            .map(|k| {
                ObjectIdentifier::builder()
                    .key(k)
                    .build()
                    .expect("key is required and provided")
            })
            .collect();

        let delete = Delete::builder()
            .set_objects(Some(identifiers))
            .quiet(true)
            .build()
            .map_err(|e| e.to_string())?;

        let resp = self
            .client
            .delete_objects()
            .bucket(bucket)
            .delete(delete)
            .send()
            .await
            .map_err(|e| e.to_string())?;
        self.record_request();

        let mut failed = Vec::new();
        for err in resp.errors() {
            if let Some(key) = err.key() {
                failed.push(key.to_string());
            }
        }

        Ok(failed)
    }

    pub async fn get_object_tags(
        &self,
        bucket: &str,
        key: &str,
    ) -> Result<HashMap<String, String>, String> {
        let resp = self
            .client
            .get_object_tagging()
            .bucket(bucket)
            .key(key)
            .send()
            .await
            .map_err(|e| e.to_string())?;
        self.record_request();

        let mut tags = HashMap::new();
        for tag in resp.tag_set() {
            tags.insert(tag.key().to_string(), tag.value().to_string());
        }
        Ok(tags)
    }

    pub async fn put_object_tags(
        &self,
        bucket: &str,
        key: &str,
        tags: HashMap<String, String>,
    ) -> Result<(), String> {
        let tag_set: Vec<Tag> = tags
            .into_iter()
            .map(|(k, v)| {
                Tag::builder()
                    .key(k)
                    .value(v)
                    .build()
                    .expect("tag fields set")
            })
            .collect();

        let tagging = Tagging::builder()
            .set_tag_set(Some(tag_set))
            .build()
            .map_err(|e| e.to_string())?;

        self.client
            .put_object_tagging()
            .bucket(bucket)
            .key(key)
            .tagging(tagging)
            .send()
            .await
            .map_err(|e| e.to_string())?;
        self.record_request();
        Ok(())
    }

    pub async fn delete_object_tags(&self, bucket: &str, key: &str) -> Result<(), String> {
        self.client
            .delete_object_tagging()
            .bucket(bucket)
            .key(key)
            .send()
            .await
            .map_err(|e| e.to_string())?;
        self.record_request();
        Ok(())
    }

    // -- versioning --

    pub async fn get_bucket_versioning(&self, bucket: &str) -> Result<String, String> {
        let resp = self
            .client
            .get_bucket_versioning()
            .bucket(bucket)
            .send()
            .await
            .map_err(|e| e.to_string())?;
        self.record_request();

        Ok(resp
            .status()
            .map(|s| s.as_str().to_string())
            .unwrap_or_default())
    }

    pub async fn put_bucket_versioning(&self, bucket: &str, status: &str) -> Result<(), String> {
        let vs = match status {
            "Enabled" => BucketVersioningStatus::Enabled,
            "Suspended" => BucketVersioningStatus::Suspended,
            _ => return Err(format!("invalid versioning status: {}", status)),
        };
        let config = VersioningConfiguration::builder().status(vs).build();
        self.client
            .put_bucket_versioning()
            .bucket(bucket)
            .versioning_configuration(config)
            .send()
            .await
            .map_err(|e| e.to_string())?;
        self.record_request();
        Ok(())
    }

    pub async fn list_object_versions(
        &self,
        bucket: &str,
        prefix: &str,
    ) -> Result<Vec<VersionInfo>, String> {
        let resp = self
            .client
            .list_object_versions()
            .bucket(bucket)
            .prefix(prefix)
            .send()
            .await
            .map_err(|e| e.to_string())?;
        self.record_request();

        let mut versions = Vec::new();
        for v in resp.versions() {
            versions.push(VersionInfo {
                key: v.key().unwrap_or_default().to_string(),
                version_id: v.version_id().unwrap_or("null").to_string(),
                is_latest: v.is_latest().unwrap_or(false),
                is_delete_marker: false,
                size: v.size().unwrap_or(0) as u64,
                last_modified: v.last_modified().map(|t| t.to_string()).unwrap_or_default(),
                etag: v.e_tag().unwrap_or_default().to_string(),
            });
        }
        for dm in resp.delete_markers() {
            versions.push(VersionInfo {
                key: dm.key().unwrap_or_default().to_string(),
                version_id: dm.version_id().unwrap_or("null").to_string(),
                is_latest: dm.is_latest().unwrap_or(false),
                is_delete_marker: true,
                size: 0,
                last_modified: dm
                    .last_modified()
                    .map(|t| t.to_string())
                    .unwrap_or_default(),
                etag: String::new(),
            });
        }
        // sort: latest first
        versions.sort_by(|a, b| b.last_modified.cmp(&a.last_modified));
        Ok(versions)
    }

    pub async fn get_object_version(
        &self,
        bucket: &str,
        key: &str,
        version_id: &str,
    ) -> Result<Vec<u8>, String> {
        let resp = self
            .client
            .get_object()
            .bucket(bucket)
            .key(key)
            .version_id(version_id)
            .send()
            .await
            .map_err(|e| e.to_string())?;
        self.record_request();

        let bytes = resp
            .body
            .collect()
            .await
            .map_err(|e| e.to_string())?
            .into_bytes();

        let data = bytes.to_vec();
        self.record_bytes_in(data.len() as u64);
        Ok(data)
    }

    pub async fn delete_object_version(
        &self,
        bucket: &str,
        key: &str,
        version_id: &str,
    ) -> Result<(), String> {
        self.client
            .delete_object()
            .bucket(bucket)
            .key(key)
            .version_id(version_id)
            .send()
            .await
            .map_err(|e| e.to_string())?;
        self.record_request();
        Ok(())
    }

    // -- presigned URLs --

    pub async fn presign_get_object(
        &self,
        bucket: &str,
        key: &str,
        expires_secs: u64,
    ) -> Result<String, String> {
        let presigning_config = aws_sdk_s3::presigning::PresigningConfig::builder()
            .expires_in(Duration::from_secs(expires_secs))
            .build()
            .map_err(|e| e.to_string())?;

        let presigned = self
            .client
            .get_object()
            .bucket(bucket)
            .key(key)
            .presigned(presigning_config)
            .await
            .map_err(|e| e.to_string())?;
        self.record_request();

        Ok(presigned.uri().to_string())
    }

    // -- bucket policy --

    pub async fn get_bucket_policy(&self, bucket: &str) -> Result<Option<String>, String> {
        let resp = self.client.get_bucket_policy().bucket(bucket).send().await;
        let resp = match resp {
            Ok(resp) => resp,
            Err(error) if error.code() == Some("NoSuchBucketPolicy") => return Ok(None),
            Err(error) => return Err(error.to_string()),
        };
        self.record_request();

        Ok(Some(resp.policy().unwrap_or("").to_string()))
    }

    pub async fn put_bucket_policy(&self, bucket: &str, policy_json: &str) -> Result<(), String> {
        self.client
            .put_bucket_policy()
            .bucket(bucket)
            .policy(policy_json)
            .send()
            .await
            .map_err(|e| e.to_string())?;
        self.record_request();
        Ok(())
    }

    pub async fn delete_bucket_policy(&self, bucket: &str) -> Result<(), String> {
        self.client
            .delete_bucket_policy()
            .bucket(bucket)
            .send()
            .await
            .map_err(|e| e.to_string())?;
        self.record_request();
        Ok(())
    }

    // -- bucket lifecycle --

    pub async fn get_bucket_lifecycle(&self, bucket: &str) -> Result<Option<String>, String> {
        let resp = self
            .client
            .get_bucket_lifecycle_configuration()
            .bucket(bucket)
            .send()
            .await;
        let resp = match resp {
            Ok(resp) => resp,
            Err(error) if error.code() == Some("NoSuchLifecycleConfiguration") => return Ok(None),
            Err(error) => return Err(error.to_string()),
        };
        self.record_request();

        let config = aws_sdk_s3::types::BucketLifecycleConfiguration::builder()
            .set_rules(Some(resp.rules().to_vec()))
            .build()
            .map_err(|error| error.to_string())?;
        let xml = lifecycle_xml::lifecycle_configuration_to_xml(
            &config,
            resp.transition_default_minimum_object_size(),
        )?;
        Ok(Some(xml))
    }

    pub async fn put_bucket_lifecycle(
        &self,
        bucket: &str,
        lifecycle_xml_text: &str,
    ) -> Result<(), String> {
        let (config, transition_default_minimum_object_size) =
            lifecycle_xml::lifecycle_configuration_from_xml(lifecycle_xml_text)?;

        self.client
            .put_bucket_lifecycle_configuration()
            .bucket(bucket)
            .lifecycle_configuration(config)
            .set_transition_default_minimum_object_size(transition_default_minimum_object_size)
            .send()
            .await
            .map_err(|error| error.to_string())?;
        self.record_request();
        Ok(())
    }

    pub async fn delete_bucket_lifecycle(&self, bucket: &str) -> Result<(), String> {
        self.client
            .delete_bucket_lifecycle()
            .bucket(bucket)
            .send()
            .await
            .map_err(|e| e.to_string())?;
        self.record_request();
        Ok(())
    }

    // -- bucket tags --

    pub async fn get_bucket_tags(&self, bucket: &str) -> Result<HashMap<String, String>, String> {
        let resp = self
            .client
            .get_bucket_tagging()
            .bucket(bucket)
            .send()
            .await
            .map_err(|e| e.to_string())?;
        self.record_request();

        let mut tags = HashMap::new();
        for tag in resp.tag_set() {
            tags.insert(tag.key().to_string(), tag.value().to_string());
        }
        Ok(tags)
    }

    pub async fn put_bucket_tags(
        &self,
        bucket: &str,
        tags: HashMap<String, String>,
    ) -> Result<(), String> {
        let tag_set: Vec<Tag> = tags
            .into_iter()
            .map(|(k, v)| Tag::builder().key(k).value(v).build().expect("tag fields"))
            .collect();

        let tagging = Tagging::builder()
            .set_tag_set(Some(tag_set))
            .build()
            .map_err(|e| e.to_string())?;

        self.client
            .put_bucket_tagging()
            .bucket(bucket)
            .tagging(tagging)
            .send()
            .await
            .map_err(|e| e.to_string())?;
        self.record_request();
        Ok(())
    }

    pub async fn delete_bucket_tags(&self, bucket: &str) -> Result<(), String> {
        self.client
            .delete_bucket_tagging()
            .bucket(bucket)
            .send()
            .await
            .map_err(|e| e.to_string())?;
        self.record_request();
        Ok(())
    }
}

#[derive(Debug, Clone, PartialEq)]
pub struct VersionInfo {
    pub key: String,
    pub version_id: String,
    pub is_latest: bool,
    pub is_delete_marker: bool,
    pub size: u64,
    pub last_modified: String,
    pub etag: String,
}

#[derive(Debug, Clone)]
pub struct BucketInfo {
    pub name: String,
    pub creation_date: String,
}

#[derive(Debug, Clone, PartialEq)]
pub struct ObjectInfo {
    pub key: String,
    pub size: u64,
    pub last_modified: String,
    pub etag: String,
}

#[derive(Debug, Clone, PartialEq)]
pub struct ObjectDetail {
    pub key: String,
    pub size: u64,
    pub content_type: String,
    pub last_modified: String,
    pub etag: String,
    pub headers: Vec<(String, String)>,
}

#[derive(Debug, Clone)]
pub struct ListObjectsResult {
    pub objects: Vec<ObjectInfo>,
    pub common_prefixes: Vec<String>,
    pub is_truncated: bool,
}

#[cfg(test)]
mod tests {
    use super::*;

    // -- S3Stats --

    #[test]
    fn s3_stats_default_is_zero() {
        let stats = S3Stats::default();
        assert_eq!(stats.requests.load(Ordering::Relaxed), 0);
        assert_eq!(stats.bytes_in.load(Ordering::Relaxed), 0);
        assert_eq!(stats.bytes_out.load(Ordering::Relaxed), 0);
    }

    #[test]
    fn s3_stats_increments() {
        let stats = S3Stats::default();
        stats.requests.fetch_add(5, Ordering::Relaxed);
        stats.bytes_in.fetch_add(1000, Ordering::Relaxed);
        stats.bytes_out.fetch_add(200, Ordering::Relaxed);
        assert_eq!(stats.requests.load(Ordering::Relaxed), 5);
        assert_eq!(stats.bytes_in.load(Ordering::Relaxed), 1000);
        assert_eq!(stats.bytes_out.load(Ordering::Relaxed), 200);
    }

    #[test]
    fn s3_stats_shared_via_arc() {
        let stats = Arc::new(S3Stats::default());
        let clone = stats.clone();
        clone.requests.fetch_add(1, Ordering::Relaxed);
        assert_eq!(stats.requests.load(Ordering::Relaxed), 1);
    }

    // -- S3Client constructors --

    #[test]
    fn new_client_with_creds() {
        let client = S3Client::new(
            "http://localhost:10000",
            Some(("AKID", "secret12")),
            "us-east-1",
        );
        assert!(client.is_ok());
    }

    #[test]
    fn new_client_anonymous() {
        let client = S3Client::anonymous("http://localhost:10000");
        assert!(client.is_ok());
    }

    #[test]
    fn new_client_strips_trailing_slash() {
        let client = S3Client::new("http://localhost:10000/", None, "us-east-1").unwrap();
        // can't inspect endpoint directly, but construction should succeed
        assert_eq!(client.stats().requests.load(Ordering::Relaxed), 0);
    }

    #[test]
    fn new_client_https() {
        let client = S3Client::new(
            "https://s3.us-west-2.amazonaws.com",
            Some(("AKID", "secret12")),
            "us-west-2",
        );
        assert!(client.is_ok());
    }

    #[test]
    fn stats_shared_between_clones() {
        let client = S3Client::anonymous("http://localhost:10000").unwrap();
        let clone = client.clone();
        client.record_request();
        assert_eq!(clone.stats().requests.load(Ordering::Relaxed), 1);
    }

    #[test]
    fn stats_track_bytes() {
        let client = S3Client::anonymous("http://localhost:10000").unwrap();
        client.record_bytes_out(500);
        client.record_bytes_in(3000);
        assert_eq!(client.stats().bytes_out.load(Ordering::Relaxed), 500);
        assert_eq!(client.stats().bytes_in.load(Ordering::Relaxed), 3000);
    }

    // -- data types --

    #[test]
    fn object_info_clone() {
        let info = ObjectInfo {
            key: "test.txt".to_string(),
            size: 100,
            last_modified: "2025-01-01".to_string(),
            etag: "abc".to_string(),
        };
        let clone = info.clone();
        assert_eq!(info, clone);
    }

    #[test]
    fn version_info_delete_marker() {
        let v = VersionInfo {
            key: "test.txt".to_string(),
            version_id: "v1".to_string(),
            is_latest: false,
            is_delete_marker: true,
            size: 0,
            last_modified: "2025-01-01".to_string(),
            etag: String::new(),
        };
        assert!(v.is_delete_marker);
        assert!(!v.is_latest);
    }

    #[test]
    fn object_detail_headers() {
        let detail = ObjectDetail {
            key: "test.txt".to_string(),
            size: 42,
            content_type: "text/plain".to_string(),
            last_modified: String::new(),
            etag: "abc".to_string(),
            headers: vec![("content-type".to_string(), "text/plain".to_string())],
        };
        assert_eq!(detail.headers.len(), 1);
        assert_eq!(detail.headers[0].0, "content-type");
    }

    #[test]
    fn list_objects_result_empty() {
        let result = ListObjectsResult {
            objects: Vec::new(),
            common_prefixes: Vec::new(),
            is_truncated: false,
        };
        assert!(result.objects.is_empty());
        assert!(!result.is_truncated);
    }
}
