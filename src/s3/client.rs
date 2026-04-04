use s3::BucketConfiguration;
use s3::bucket::Bucket;
use s3::creds::Credentials;
use s3::region::Region;

#[derive(Clone)]
pub struct S3Client {
    region: Region,
    credentials: Credentials,
    path_style: bool,
}

impl S3Client {
    pub fn new(
        endpoint: &str,
        creds: Option<(&str, &str)>,
        region_name: &str,
    ) -> Result<Self, String> {
        let region = Region::Custom {
            region: region_name.to_string(),
            endpoint: endpoint.trim_end_matches('/').to_string(),
        };
        let credentials = match creds {
            Some((access_key, secret_key)) => {
                Credentials::new(Some(access_key), Some(secret_key), None, None, None)
                    .map_err(|e| e.to_string())?
            }
            None => Credentials::anonymous().map_err(|e| e.to_string())?,
        };
        Ok(Self {
            region,
            credentials,
            path_style: true,
        })
    }

    /// Create an anonymous client (no auth) for the given endpoint.
    pub fn anonymous(endpoint: &str) -> Result<Self, String> {
        Self::new(endpoint, None, "us-east-1")
    }

    fn bucket(&self, name: &str) -> Result<Box<Bucket>, String> {
        let b = Bucket::new(name, self.region.clone(), self.credentials.clone())
            .map_err(|e| e.to_string())?;
        Ok(if self.path_style {
            b.with_path_style()
        } else {
            b
        })
    }

    pub async fn list_buckets(&self) -> Result<Vec<BucketInfo>, String> {
        let resp = Bucket::list_buckets(self.region.clone(), self.credentials.clone())
            .await
            .map_err(|e| e.to_string())?;

        Ok(resp
            .bucket_names()
            .map(|name| BucketInfo {
                name,
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
        let b = self.bucket(bucket)?;
        let delim = if delimiter.is_empty() {
            None
        } else {
            Some(delimiter.to_string())
        };

        let results = b
            .list(prefix.to_string(), delim)
            .await
            .map_err(|e| e.to_string())?;

        let mut objects = Vec::new();
        let mut common_prefixes = Vec::new();
        let mut is_truncated = false;

        for page in &results {
            for obj in &page.contents {
                objects.push(ObjectInfo {
                    key: obj.key.clone(),
                    size: obj.size,
                    last_modified: obj.last_modified.clone(),
                    etag: obj.e_tag.clone().unwrap_or_default(),
                });
            }
            if let Some(prefixes) = &page.common_prefixes {
                for p in prefixes {
                    common_prefixes.push(p.prefix.clone());
                }
            }
            if page.is_truncated {
                is_truncated = true;
            }
        }

        Ok(ListObjectsResult {
            objects,
            common_prefixes,
            is_truncated,
        })
    }

    pub async fn create_bucket(&self, bucket: &str) -> Result<(), String> {
        Bucket::create_with_path_style(
            bucket,
            self.region.clone(),
            self.credentials.clone(),
            BucketConfiguration::default(),
        )
        .await
        .map_err(|e| e.to_string())?;
        Ok(())
    }

    pub async fn delete_bucket(&self, bucket: &str) -> Result<(), String> {
        let b = self.bucket(bucket)?;
        let code = b.delete().await.map_err(|e| e.to_string())?;

        if (200..300).contains(&code) {
            Ok(())
        } else {
            Err(format!("delete bucket: {}", code))
        }
    }

    pub async fn put_object(
        &self,
        bucket: &str,
        key: &str,
        data: Vec<u8>,
        content_type: &str,
    ) -> Result<String, String> {
        let b = self.bucket(bucket)?;
        let resp = b
            .put_object_with_content_type(key, &data, content_type)
            .await
            .map_err(|e| e.to_string())?;

        if (200..300).contains(&resp.status_code()) {
            Ok(String::new())
        } else {
            Err(format!(
                "put object: {} {}",
                resp.status_code(),
                String::from_utf8_lossy(resp.as_slice())
            ))
        }
    }

    pub async fn get_object(&self, bucket: &str, key: &str) -> Result<Vec<u8>, String> {
        let b = self.bucket(bucket)?;
        let resp = b.get_object(key).await.map_err(|e| e.to_string())?;

        if (200..300).contains(&resp.status_code()) {
            Ok(resp.to_vec())
        } else {
            Err(format!(
                "get object: {} {}",
                resp.status_code(),
                String::from_utf8_lossy(resp.as_slice())
            ))
        }
    }

    pub async fn head_object(&self, bucket: &str, key: &str) -> Result<ObjectDetail, String> {
        let b = self.bucket(bucket)?;
        let (head, code) = b.head_object(key).await.map_err(|e| e.to_string())?;

        if !(200..300).contains(&code) {
            return Err(format!("head object: {}", code));
        }

        let mut headers = Vec::new();
        if let Some(v) = &head.content_type {
            headers.push(("content-type".to_string(), v.clone()));
        }
        if let Some(v) = &head.content_length {
            headers.push(("content-length".to_string(), v.to_string()));
        }
        if let Some(v) = &head.last_modified {
            headers.push(("last-modified".to_string(), v.clone()));
        }
        if let Some(v) = &head.e_tag {
            headers.push(("etag".to_string(), v.clone()));
        }
        if let Some(v) = &head.cache_control {
            headers.push(("cache-control".to_string(), v.clone()));
        }
        if let Some(v) = &head.content_disposition {
            headers.push(("content-disposition".to_string(), v.clone()));
        }
        if let Some(v) = &head.content_encoding {
            headers.push(("content-encoding".to_string(), v.clone()));
        }
        if let Some(v) = &head.accept_ranges {
            headers.push(("accept-ranges".to_string(), v.clone()));
        }
        if let Some(v) = &head.expiration {
            headers.push(("x-amz-expiration".to_string(), v.clone()));
        }
        if let Some(meta) = &head.metadata {
            for (k, v) in meta {
                headers.push((format!("x-amz-meta-{}", k), v.clone()));
            }
        }

        Ok(ObjectDetail {
            key: key.to_string(),
            size: head.content_length.unwrap_or(0) as u64,
            content_type: head.content_type.unwrap_or_default(),
            last_modified: head.last_modified.unwrap_or_default(),
            etag: head.e_tag.unwrap_or_default(),
            headers,
        })
    }

    /// Server-side copy. For same-bucket copies, uses the S3 CopyObject API
    /// (data never leaves the server). For cross-bucket copies on the same
    /// endpoint, falls back to GET + PUT since rust-s3 only exposes
    /// same-bucket server-side copy.
    pub async fn copy_object(
        &self,
        src_bucket: &str,
        src_key: &str,
        dst_bucket: &str,
        dst_key: &str,
    ) -> Result<(), String> {
        if src_bucket == dst_bucket {
            let b = self.bucket(dst_bucket)?;
            let code = b
                .copy_object_internal(src_key, dst_key)
                .await
                .map_err(|e| e.to_string())?;
            if (200..300).contains(&code) {
                Ok(())
            } else {
                Err(format!("copy object: {}", code))
            }
        } else {
            // cross-bucket: GET from source, PUT to destination
            let data = self.get_object(src_bucket, src_key).await?;
            let detail = self.head_object(src_bucket, src_key).await?;
            let content_type = if detail.content_type.is_empty() {
                "application/octet-stream"
            } else {
                &detail.content_type
            };
            self.put_object(dst_bucket, dst_key, data, content_type)
                .await?;
            Ok(())
        }
    }

    pub async fn list_objects_recursive(
        &self,
        bucket: &str,
        prefix: &str,
    ) -> Result<ListObjectsResult, String> {
        let b = self.bucket(bucket)?;
        let results = b
            .list(prefix.to_string(), None)
            .await
            .map_err(|e| e.to_string())?;

        let mut objects = Vec::new();
        let mut is_truncated = false;

        for page in &results {
            for obj in &page.contents {
                objects.push(ObjectInfo {
                    key: obj.key.clone(),
                    size: obj.size,
                    last_modified: obj.last_modified.clone(),
                    etag: obj.e_tag.clone().unwrap_or_default(),
                });
            }
            if page.is_truncated {
                is_truncated = true;
            }
        }

        Ok(ListObjectsResult {
            objects,
            common_prefixes: Vec::new(),
            is_truncated,
        })
    }

    pub async fn delete_object(&self, bucket: &str, key: &str) -> Result<(), String> {
        let b = self.bucket(bucket)?;
        let resp = b.delete_object(key).await.map_err(|e| e.to_string())?;

        let code = resp.status_code();
        if (200..300).contains(&code) {
            Ok(())
        } else {
            Err(format!(
                "delete object: {} {}",
                code,
                String::from_utf8_lossy(resp.as_slice())
            ))
        }
    }
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
