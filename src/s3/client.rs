use std::collections::HashMap;
use std::time::Duration;

use aws_credential_types::Credentials;
use aws_credential_types::credential_fn::provide_credentials_fn;
use aws_credential_types::provider::error::CredentialsError;
use aws_sdk_s3::config::{AppName, BehaviorVersion, Builder, Region, timeout::TimeoutConfig};
use aws_sdk_s3::primitives::ByteStream;
use aws_sdk_s3::types::{
    BucketVersioningStatus, Delete, ObjectIdentifier, Tag, Tagging, VersioningConfiguration,
};
use aws_sdk_s3::Client;

#[derive(Clone)]
pub struct S3Client {
    client: Client,
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
            Some((access_key, secret_key)) => builder.credentials_provider(
                Credentials::new(access_key, secret_key, None, None, "static"),
            ),
            None => builder
                .credentials_provider(provide_credentials_fn(|| async {
                    Err(CredentialsError::not_loaded("anonymous"))
                }))
                .allow_no_auth(),
        };

        Ok(Self {
            client: Client::from_conf(builder.build()),
        })
    }

    /// Create an anonymous client (no auth) for the given endpoint.
    pub fn anonymous(endpoint: &str) -> Result<Self, String> {
        Self::new(endpoint, None, "us-east-1")
    }

    pub async fn list_buckets(&self) -> Result<Vec<BucketInfo>, String> {
        let resp = self
            .client
            .list_buckets()
            .send()
            .await
            .map_err(|e| e.to_string())?;

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
            let mut req = self
                .client
                .list_objects_v2()
                .bucket(bucket)
                .prefix(prefix);

            if !delimiter.is_empty() {
                req = req.delimiter(delimiter);
            }
            if let Some(token) = &continuation_token {
                req = req.continuation_token(token);
            }

            let resp = req.send().await.map_err(|e| e.to_string())?;

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
        Ok(())
    }

    pub async fn delete_bucket(&self, bucket: &str) -> Result<(), String> {
        self.client
            .delete_bucket()
            .bucket(bucket)
            .send()
            .await
            .map_err(|e| e.to_string())?;
        Ok(())
    }

    pub async fn put_object(
        &self,
        bucket: &str,
        key: &str,
        data: Vec<u8>,
        content_type: &str,
    ) -> Result<String, String> {
        self.client
            .put_object()
            .bucket(bucket)
            .key(key)
            .content_type(content_type)
            .body(ByteStream::from(data))
            .send()
            .await
            .map_err(|e| e.to_string())?;
        Ok(String::new())
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

        let bytes = resp
            .body
            .collect()
            .await
            .map_err(|e| e.to_string())?
            .into_bytes();

        Ok(bytes.to_vec())
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
            let mut req = self
                .client
                .list_objects_v2()
                .bucket(bucket)
                .prefix(prefix);

            if let Some(token) = &continuation_token {
                req = req.continuation_token(token);
            }

            let resp = req.send().await.map_err(|e| e.to_string())?;

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

    pub async fn delete_object(&self, bucket: &str, key: &str) -> Result<(), String> {
        self.client
            .delete_object()
            .bucket(bucket)
            .key(key)
            .send()
            .await
            .map_err(|e| e.to_string())?;
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

        let mut tags = HashMap::new();
        for tag in resp.tag_set() {
            tags.insert(
                tag.key().to_string(),
                tag.value().to_string(),
            );
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
            .map(|(k, v)| Tag::builder().key(k).value(v).build().expect("tag fields set"))
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
        Ok(())
    }

    pub async fn delete_object_tags(
        &self,
        bucket: &str,
        key: &str,
    ) -> Result<(), String> {
        self.client
            .delete_object_tagging()
            .bucket(bucket)
            .key(key)
            .send()
            .await
            .map_err(|e| e.to_string())?;
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

        Ok(resp
            .status()
            .map(|s| s.as_str().to_string())
            .unwrap_or_default())
    }

    pub async fn put_bucket_versioning(
        &self,
        bucket: &str,
        status: &str,
    ) -> Result<(), String> {
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

        let mut versions = Vec::new();
        for v in resp.versions() {
            versions.push(VersionInfo {
                key: v.key().unwrap_or_default().to_string(),
                version_id: v.version_id().unwrap_or("null").to_string(),
                is_latest: v.is_latest().unwrap_or(false),
                is_delete_marker: false,
                size: v.size().unwrap_or(0) as u64,
                last_modified: v
                    .last_modified()
                    .map(|t| t.to_string())
                    .unwrap_or_default(),
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

        let bytes = resp
            .body
            .collect()
            .await
            .map_err(|e| e.to_string())?
            .into_bytes();

        Ok(bytes.to_vec())
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

        Ok(presigned.uri().to_string())
    }

    // -- bucket policy --

    pub async fn get_bucket_policy(&self, bucket: &str) -> Result<String, String> {
        let resp = self
            .client
            .get_bucket_policy()
            .bucket(bucket)
            .send()
            .await
            .map_err(|e| e.to_string())?;

        Ok(resp.policy().unwrap_or("").to_string())
    }

    pub async fn put_bucket_policy(
        &self,
        bucket: &str,
        policy_json: &str,
    ) -> Result<(), String> {
        self.client
            .put_bucket_policy()
            .bucket(bucket)
            .policy(policy_json)
            .send()
            .await
            .map_err(|e| e.to_string())?;
        Ok(())
    }

    pub async fn delete_bucket_policy(&self, bucket: &str) -> Result<(), String> {
        self.client
            .delete_bucket_policy()
            .bucket(bucket)
            .send()
            .await
            .map_err(|e| e.to_string())?;
        Ok(())
    }

    // -- bucket lifecycle --

    pub async fn get_bucket_lifecycle(&self, bucket: &str) -> Result<String, String> {
        let resp = self
            .client
            .get_bucket_lifecycle_configuration()
            .bucket(bucket)
            .send()
            .await
            .map_err(|e| e.to_string())?;

        // serialize rules back to a displayable string
        let rules = resp.rules();
        let mut result = String::new();
        for rule in rules {
            result.push_str(&format!(
                "ID: {}, Status: {:?}\n",
                rule.id().unwrap_or(""),
                rule.status()
            ));
            if let Some(exp) = rule.expiration() {
                if let Some(days) = exp.days() {
                    result.push_str(&format!("  Expiration: {} days\n", days));
                }
            }
            if let Some(filter) = rule.filter() {
                if let Some(prefix) = filter.prefix() {
                    result.push_str(&format!("  Prefix: {}\n", prefix));
                }
            }
        }
        if result.is_empty() {
            result = "No lifecycle rules".to_string();
        }
        Ok(result)
    }

    pub async fn delete_bucket_lifecycle(&self, bucket: &str) -> Result<(), String> {
        self.client
            .delete_bucket_lifecycle()
            .bucket(bucket)
            .send()
            .await
            .map_err(|e| e.to_string())?;
        Ok(())
    }

    // -- bucket tags --

    pub async fn get_bucket_tags(
        &self,
        bucket: &str,
    ) -> Result<HashMap<String, String>, String> {
        let resp = self
            .client
            .get_bucket_tagging()
            .bucket(bucket)
            .send()
            .await
            .map_err(|e| e.to_string())?;

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
        Ok(())
    }

    pub async fn delete_bucket_tags(&self, bucket: &str) -> Result<(), String> {
        self.client
            .delete_bucket_tagging()
            .bucket(bucket)
            .send()
            .await
            .map_err(|e| e.to_string())?;
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
