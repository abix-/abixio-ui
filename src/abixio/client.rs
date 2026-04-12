use hmac::{Hmac, Mac};
use reqwest::Client;
use sha2::{Digest, Sha256};
use time::OffsetDateTime;

use super::types::*;

type HmacSha256 = Hmac<Sha256>;

/// HTTP client for AbixIO admin API (/_admin/* endpoints).
/// Supports optional AWS Sig V4 signing for authenticated servers.
#[derive(Clone)]
pub struct AdminClient {
    endpoint: String,
    http: Client,
    credentials: Option<(String, String)>,
    region: String,
}

impl AdminClient {
    pub fn new(endpoint: &str, credentials: Option<(&str, &str)>, region: &str) -> Self {
        Self::new_with_ca_pem(endpoint, credentials, region, None)
    }

    pub fn new_with_ca_pem(
        endpoint: &str,
        credentials: Option<(&str, &str)>,
        region: &str,
        ca_pem: Option<&[u8]>,
    ) -> Self {
        let mut http = Client::builder();
        if let Some(ca_pem) = ca_pem
            && let Ok(cert) = reqwest::Certificate::from_pem(ca_pem)
        {
            http = http.add_root_certificate(cert);
        }
        Self {
            endpoint: endpoint.trim_end_matches('/').to_string(),
            http: http.build().expect("build admin http client"),
            credentials: credentials.map(|(a, s)| (a.to_string(), s.to_string())),
            region: region.to_string(),
        }
    }

    fn url(&self, path: &str) -> String {
        format!("{}/_admin/{}", self.endpoint, path)
    }

    fn url_with_query(&self, path: &str, params: &[(&str, &str)]) -> String {
        let mut url = reqwest::Url::parse(&self.url(path)).expect("valid admin url");
        url.query_pairs_mut().extend_pairs(params);
        url.into()
    }

    async fn signed_get(&self, url: &str) -> Result<reqwest::Response, String> {
        let mut builder = self.http.get(url);

        if let Some((ref ak, ref sk)) = self.credentials {
            let headers = sig_v4_headers("GET", url, ak, sk, &self.region);
            for (k, v) in &headers {
                builder = builder.header(k.as_str(), v.as_str());
            }
        }

        builder.send().await.map_err(|e| e.to_string())
    }

    async fn signed_post(&self, url: &str) -> Result<reqwest::Response, String> {
        let mut builder = self.http.post(url);

        if let Some((ref ak, ref sk)) = self.credentials {
            let headers = sig_v4_headers("POST", url, ak, sk, &self.region);
            for (k, v) in &headers {
                builder = builder.header(k.as_str(), v.as_str());
            }
        }

        builder.send().await.map_err(|e| e.to_string())
    }

    /// Probe if this endpoint is an AbixIO server.
    pub async fn probe(&self) -> Option<StatusResponse> {
        let resp = self.signed_get(&self.url("status")).await.ok()?;
        if !resp.status().is_success() {
            return None;
        }
        let status: StatusResponse = resp.json().await.ok()?;
        if status.server == "abixio" {
            Some(status)
        } else {
            None
        }
    }

    pub async fn status(&self) -> Result<StatusResponse, String> {
        let resp = self.signed_get(&self.url("status")).await?;
        resp.json().await.map_err(|e| e.to_string())
    }

    pub async fn disks(&self) -> Result<DisksResponse, String> {
        let resp = self.signed_get(&self.url("disks")).await?;
        resp.json().await.map_err(|e| e.to_string())
    }

    pub async fn heal_status(&self) -> Result<HealStatusResponse, String> {
        let resp = self.signed_get(&self.url("heal")).await?;
        resp.json().await.map_err(|e| e.to_string())
    }

    pub async fn inspect_object(
        &self,
        bucket: &str,
        key: &str,
    ) -> Result<ObjectInspectResponse, String> {
        let url = self.url_with_query("object", &[("bucket", bucket), ("key", key)]);
        let resp = self.signed_get(&url).await?;
        resp.json().await.map_err(|e| e.to_string())
    }

    pub async fn heal_object(&self, bucket: &str, key: &str) -> Result<HealResponse, String> {
        let url = self.url_with_query("heal", &[("bucket", bucket), ("key", key)]);
        let resp = self.signed_post(&url).await?;
        resp.json().await.map_err(|e| e.to_string())
    }

    pub async fn cluster_status(&self) -> Result<ClusterStatusResponse, String> {
        let resp = self.signed_get(&self.url("cluster/status")).await?;
        resp.json().await.map_err(|e| e.to_string())
    }

    pub async fn cluster_nodes(&self) -> Result<ClusterNodesResponse, String> {
        let resp = self.signed_get(&self.url("cluster/nodes")).await?;
        resp.json().await.map_err(|e| e.to_string())
    }

    pub async fn cluster_epochs(&self) -> Result<ClusterEpochsResponse, String> {
        let resp = self.signed_get(&self.url("cluster/epochs")).await?;
        resp.json().await.map_err(|e| e.to_string())
    }

    pub async fn cluster_topology(&self) -> Result<ClusterTopologyResponse, String> {
        let resp = self.signed_get(&self.url("cluster/topology")).await?;
        resp.json().await.map_err(|e| e.to_string())
    }

    pub async fn get_bucket_ftt(&self, bucket: &str) -> Result<EcConfig, String> {
        let resp = self
            .signed_get(&self.url(&format!("bucket/{}/ftt", bucket)))
            .await?;
        resp.json().await.map_err(|e| e.to_string())
    }

    pub async fn set_bucket_ftt(&self, bucket: &str, ftt: usize) -> Result<EcConfig, String> {
        let url = self.url_with_query(
            &format!("bucket/{}/ftt", bucket),
            &[("ftt", &ftt.to_string())],
        );
        let mut builder = self.http.put(&url);
        if let Some((ref ak, ref sk)) = self.credentials {
            let headers = sig_v4_headers("PUT", &url, ak, sk, &self.region);
            for (k, v) in &headers {
                builder = builder.header(k.as_str(), v.as_str());
            }
        }
        let resp = builder.send().await.map_err(|e| e.to_string())?;
        resp.json().await.map_err(|e| e.to_string())
    }
}

// -- Sig V4 signing (same approach as rust-s3 signing.rs) --

const EMPTY_SHA256: &str = "e3b0c44298fc1c149afbf4c8996fb92427ae41e4649b934ca495991b7852b855";

fn sig_v4_headers(
    method: &str,
    url: &str,
    access_key: &str,
    secret_key: &str,
    region: &str,
) -> Vec<(String, String)> {
    let now = OffsetDateTime::now_utc();
    let date_stamp = now
        .format(&time::macros::format_description!("[year][month][day]"))
        .unwrap_or_default();
    let amz_date = now
        .format(&time::macros::format_description!(
            "[year][month][day]T[hour][minute][second]Z"
        ))
        .unwrap_or_default();

    // parse url
    let parsed = reqwest::Url::parse(url).expect("valid url");
    let host = match parsed.port() {
        Some(p) => format!("{}:{}", parsed.host_str().unwrap_or("localhost"), p),
        None => parsed.host_str().unwrap_or("localhost").to_string(),
    };
    let path = parsed.path();
    let raw_query = parsed.query().unwrap_or("");

    let canonical_query = canonical_query_string(raw_query);

    // canonical headers (sorted)
    let signed_headers = "host;x-amz-content-sha256;x-amz-date";
    let canonical_headers = format!(
        "host:{}\nx-amz-content-sha256:{}\nx-amz-date:{}\n",
        host, EMPTY_SHA256, amz_date
    );

    // canonical request
    let canonical_request = format!(
        "{}\n{}\n{}\n{}\n{}\n{}",
        method, path, canonical_query, canonical_headers, signed_headers, EMPTY_SHA256
    );

    let credential_scope = format!("{}/{}/s3/aws4_request", date_stamp, region);
    let string_to_sign = format!(
        "AWS4-HMAC-SHA256\n{}\n{}\n{}",
        amz_date,
        credential_scope,
        hex::encode(Sha256::digest(canonical_request.as_bytes()))
    );

    // signing key
    let k_date = hmac_sha256(
        format!("AWS4{}", secret_key).as_bytes(),
        date_stamp.as_bytes(),
    );
    let k_region = hmac_sha256(&k_date, region.as_bytes());
    let k_service = hmac_sha256(&k_region, b"s3");
    let k_signing = hmac_sha256(&k_service, b"aws4_request");

    let signature = hex::encode(hmac_sha256(&k_signing, string_to_sign.as_bytes()));

    let authorization = format!(
        "AWS4-HMAC-SHA256 Credential={}/{}, SignedHeaders={}, Signature={}",
        access_key, credential_scope, signed_headers, signature
    );

    vec![
        ("authorization".to_string(), authorization),
        ("x-amz-content-sha256".to_string(), EMPTY_SHA256.to_string()),
        ("x-amz-date".to_string(), amz_date),
        ("host".to_string(), host),
    ]
}

fn hmac_sha256(key: &[u8], data: &[u8]) -> Vec<u8> {
    let mut mac = HmacSha256::new_from_slice(key).expect("valid key length");
    mac.update(data);
    mac.finalize().into_bytes().to_vec()
}

fn canonical_query_string(query: &str) -> String {
    if query.is_empty() {
        return String::new();
    }

    let mut pairs: Vec<&str> = query.split('&').collect();
    pairs.sort_unstable();
    pairs.join("&")
}

#[cfg(test)]
mod tests {
    use super::{AdminClient, canonical_query_string};

    #[test]
    fn admin_url_builder_encodes_bucket_and_key() {
        let client = AdminClient::new("http://127.0.0.1:10000", None, "us-east-1");
        let url = client.url_with_query(
            "object",
            &[("bucket", "my bucket"), ("key", "dir one/inspect me.txt")],
        );

        assert_eq!(
            url,
            "http://127.0.0.1:10000/_admin/object?bucket=my+bucket&key=dir+one%2Finspect+me.txt"
        );
    }

    #[test]
    fn canonical_query_keeps_encoded_values_sorted() {
        let canonical = canonical_query_string("key=dir+one%2Finspect+me.txt&bucket=my+bucket");

        assert_eq!(canonical, "bucket=my+bucket&key=dir+one%2Finspect+me.txt");
    }
}
