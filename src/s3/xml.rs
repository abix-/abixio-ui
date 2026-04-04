use serde::Deserialize;

#[derive(Debug, Deserialize)]
#[serde(rename = "ListAllMyBucketsResult")]
pub struct ListAllMyBucketsResult {
    #[serde(rename = "Buckets")]
    pub buckets: BucketsContainer,
}

#[derive(Debug, Deserialize)]
pub struct BucketsContainer {
    #[serde(rename = "Bucket", default)]
    pub bucket: Vec<BucketXml>,
}

#[derive(Debug, Clone, Deserialize)]
pub struct BucketXml {
    #[serde(rename = "Name")]
    pub name: String,
    #[serde(rename = "CreationDate", default)]
    pub creation_date: String,
}

#[derive(Debug, Deserialize)]
#[serde(rename = "ListBucketResult")]
pub struct ListBucketResult {
    #[serde(rename = "Name", default)]
    pub name: String,
    #[serde(rename = "Prefix", default)]
    pub prefix: String,
    #[serde(rename = "IsTruncated", default)]
    pub is_truncated: bool,
    #[serde(rename = "Contents", default)]
    pub contents: Vec<ObjectXml>,
    #[serde(rename = "CommonPrefixes", default)]
    pub common_prefixes: Vec<PrefixXml>,
    #[serde(rename = "NextContinuationToken", default)]
    pub next_continuation_token: String,
}

#[derive(Debug, Clone, Deserialize)]
pub struct ObjectXml {
    #[serde(rename = "Key")]
    pub key: String,
    #[serde(rename = "Size", default)]
    pub size: u64,
    #[serde(rename = "LastModified", default)]
    pub last_modified: String,
    #[serde(rename = "ETag", default)]
    pub etag: String,
}

#[derive(Debug, Clone, Deserialize)]
pub struct PrefixXml {
    #[serde(rename = "Prefix")]
    pub prefix: String,
}
