use reqwest::Client;

use super::xml::{ListAllMyBucketsResult, ListBucketResult};

#[derive(Clone)]
pub struct S3Client {
    endpoint: String,
    http: Client,
}

impl S3Client {
    pub fn new(endpoint: &str) -> Self {
        Self {
            endpoint: endpoint.trim_end_matches('/').to_string(),
            http: Client::new(),
        }
    }

    pub async fn list_buckets(&self) -> Result<Vec<BucketInfo>, String> {
        let resp = self
            .http
            .get(&format!("{}/", self.endpoint))
            .send()
            .await
            .map_err(|e| e.to_string())?;

        let status = resp.status();
        let body = resp.text().await.map_err(|e| e.to_string())?;
        if !status.is_success() {
            return Err(format!("list buckets: {} {}", status, body));
        }

        let parsed: ListAllMyBucketsResult =
            quick_xml::de::from_str(&body).map_err(|e| format!("xml parse: {}", e))?;

        Ok(parsed
            .buckets
            .bucket
            .into_iter()
            .map(|b| BucketInfo {
                name: b.name,
                creation_date: b.creation_date,
            })
            .collect())
    }

    pub async fn list_objects(
        &self,
        bucket: &str,
        prefix: &str,
        delimiter: &str,
    ) -> Result<ListObjectsResult, String> {
        let mut url = format!("{}/{}?list-type=2", self.endpoint, bucket);
        if !prefix.is_empty() {
            url.push_str(&format!("&prefix={}", prefix));
        }
        if !delimiter.is_empty() {
            url.push_str(&format!("&delimiter={}", delimiter));
        }

        let resp = self
            .http
            .get(&url)
            .send()
            .await
            .map_err(|e| e.to_string())?;

        let status = resp.status();
        let body = resp.text().await.map_err(|e| e.to_string())?;
        if !status.is_success() {
            return Err(format!("list objects: {} {}", status, body));
        }

        let parsed: ListBucketResult =
            quick_xml::de::from_str(&body).map_err(|e| format!("xml parse: {}", e))?;

        Ok(ListObjectsResult {
            objects: parsed
                .contents
                .into_iter()
                .map(|o| ObjectInfo {
                    key: o.key,
                    size: o.size,
                    last_modified: o.last_modified,
                    etag: o.etag,
                })
                .collect(),
            common_prefixes: parsed
                .common_prefixes
                .into_iter()
                .map(|p| p.prefix)
                .collect(),
            is_truncated: parsed.is_truncated,
        })
    }

    pub async fn create_bucket(&self, bucket: &str) -> Result<(), String> {
        let resp = self
            .http
            .put(&format!("{}/{}", self.endpoint, bucket))
            .send()
            .await
            .map_err(|e| e.to_string())?;

        if resp.status().is_success() {
            Ok(())
        } else {
            let body = resp.text().await.unwrap_or_default();
            Err(format!("create bucket: {}", body))
        }
    }

    pub async fn put_object(
        &self,
        bucket: &str,
        key: &str,
        data: Vec<u8>,
        content_type: &str,
    ) -> Result<String, String> {
        let resp = self
            .http
            .put(&format!("{}/{}/{}", self.endpoint, bucket, key))
            .header("Content-Type", content_type)
            .body(data)
            .send()
            .await
            .map_err(|e| e.to_string())?;

        if resp.status().is_success() {
            let etag = resp
                .headers()
                .get("etag")
                .and_then(|v| v.to_str().ok())
                .unwrap_or("")
                .to_string();
            Ok(etag)
        } else {
            let body = resp.text().await.unwrap_or_default();
            Err(format!("put object: {}", body))
        }
    }

    pub async fn get_object(&self, bucket: &str, key: &str) -> Result<Vec<u8>, String> {
        let resp = self
            .http
            .get(&format!("{}/{}/{}", self.endpoint, bucket, key))
            .send()
            .await
            .map_err(|e| e.to_string())?;

        if resp.status().is_success() {
            resp.bytes()
                .await
                .map(|b| b.to_vec())
                .map_err(|e| e.to_string())
        } else {
            let body = resp.text().await.unwrap_or_default();
            Err(format!("get object: {}", body))
        }
    }

    pub async fn head_object(&self, bucket: &str, key: &str) -> Result<ObjectDetail, String> {
        let resp = self
            .http
            .head(&format!("{}/{}/{}", self.endpoint, bucket, key))
            .send()
            .await
            .map_err(|e| e.to_string())?;

        if !resp.status().is_success() {
            return Err(format!("head object: {}", resp.status()));
        }

        let h = resp.headers();
        let get_header = |name: &str| -> String {
            h.get(name)
                .and_then(|v| v.to_str().ok())
                .unwrap_or("")
                .to_string()
        };

        let headers: Vec<(String, String)> = h
            .iter()
            .map(|(k, v)| (k.as_str().to_string(), v.to_str().unwrap_or("").to_string()))
            .collect();

        Ok(ObjectDetail {
            key: key.to_string(),
            size: get_header("content-length").parse().unwrap_or(0),
            content_type: get_header("content-type"),
            last_modified: get_header("last-modified"),
            etag: get_header("etag"),
            headers,
        })
    }

    pub async fn delete_object(&self, bucket: &str, key: &str) -> Result<(), String> {
        let resp = self
            .http
            .delete(&format!("{}/{}/{}", self.endpoint, bucket, key))
            .send()
            .await
            .map_err(|e| e.to_string())?;

        if resp.status().is_success() || resp.status().as_u16() == 204 {
            Ok(())
        } else {
            let body = resp.text().await.unwrap_or_default();
            Err(format!("delete object: {}", body))
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
