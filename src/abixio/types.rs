use serde::Deserialize;

#[derive(Debug, Clone, Deserialize)]
pub struct StatusResponse {
    pub server: String,
    pub version: String,
    pub uptime_secs: u64,
    pub data_shards: usize,
    pub parity_shards: usize,
    pub total_disks: usize,
    pub listen: String,
    pub auth_enabled: bool,
    pub scan_interval: String,
    pub heal_interval: String,
    pub mrf_workers: usize,
}

#[derive(Debug, Clone, Deserialize)]
pub struct DisksResponse {
    pub disks: Vec<DiskInfo>,
}

#[derive(Debug, Clone, Deserialize)]
pub struct DiskInfo {
    pub index: usize,
    pub path: String,
    pub online: bool,
    pub total_bytes: u64,
    pub used_bytes: u64,
    pub free_bytes: u64,
    pub bucket_count: usize,
    pub object_count: usize,
}

#[derive(Debug, Clone, Deserialize)]
pub struct HealStatusResponse {
    pub mrf_pending: usize,
    pub mrf_workers: usize,
    pub scanner: ScannerStatus,
}

#[derive(Debug, Clone, Deserialize)]
pub struct ScannerStatus {
    pub running: bool,
    pub scan_interval: String,
    pub heal_interval: String,
    pub objects_scanned: u64,
    pub objects_healed: u64,
    pub last_scan_started: u64,
    pub last_scan_duration_secs: u64,
}

#[derive(Debug, Clone, Deserialize)]
pub struct ObjectInspectResponse {
    pub bucket: String,
    pub key: String,
    pub size: u64,
    pub etag: String,
    pub content_type: String,
    pub created_at: u64,
    pub erasure: ErasureInfo,
    pub shards: Vec<ShardInfo>,
}

#[derive(Debug, Clone, Deserialize)]
pub struct ErasureInfo {
    pub data: usize,
    pub parity: usize,
    pub distribution: Vec<usize>,
}

#[derive(Debug, Clone, Deserialize)]
pub struct ShardInfo {
    pub index: usize,
    pub disk: usize,
    pub status: String,
    pub checksum: Option<String>,
}

#[derive(Debug, Clone, Deserialize)]
pub struct HealResponse {
    pub result: String,
    pub shards_fixed: Option<usize>,
    pub error: Option<String>,
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn status_response_deserialize() {
        let json = r#"{
            "server": "abixio",
            "version": "0.1.0",
            "uptime_secs": 3600,
            "data_shards": 2,
            "parity_shards": 2,
            "total_disks": 4,
            "listen": "0.0.0.0:10000",
            "auth_enabled": false,
            "scan_interval": "5m",
            "heal_interval": "1m",
            "mrf_workers": 4
        }"#;
        let status: StatusResponse = serde_json::from_str(json).unwrap();
        assert_eq!(status.server, "abixio");
        assert_eq!(status.version, "0.1.0");
        assert_eq!(status.uptime_secs, 3600);
        assert_eq!(status.data_shards, 2);
        assert_eq!(status.parity_shards, 2);
        assert_eq!(status.total_disks, 4);
        assert!(!status.auth_enabled);
    }

    #[test]
    fn disks_response_deserialize() {
        let json = r#"{
            "disks": [{
                "index": 0,
                "path": "/mnt/d1",
                "online": true,
                "total_bytes": 1000000,
                "used_bytes": 500000,
                "free_bytes": 500000,
                "bucket_count": 3,
                "object_count": 42
            }]
        }"#;
        let disks: DisksResponse = serde_json::from_str(json).unwrap();
        assert_eq!(disks.disks.len(), 1);
        assert!(disks.disks[0].online);
        assert_eq!(disks.disks[0].path, "/mnt/d1");
        assert_eq!(disks.disks[0].object_count, 42);
    }

    #[test]
    fn heal_status_response_deserialize() {
        let json = r#"{
            "mrf_pending": 5,
            "mrf_workers": 4,
            "scanner": {
                "running": true,
                "scan_interval": "5m",
                "heal_interval": "1m",
                "objects_scanned": 100,
                "objects_healed": 2,
                "last_scan_started": 1700000000,
                "last_scan_duration_secs": 30
            }
        }"#;
        let heal: HealStatusResponse = serde_json::from_str(json).unwrap();
        assert_eq!(heal.mrf_pending, 5);
        assert!(heal.scanner.running);
        assert_eq!(heal.scanner.objects_scanned, 100);
        assert_eq!(heal.scanner.objects_healed, 2);
    }

    #[test]
    fn object_inspect_response_deserialize() {
        let json = r#"{
            "bucket": "testbucket",
            "key": "hello.txt",
            "size": 128,
            "etag": "abc123",
            "content_type": "text/plain",
            "created_at": 1700000000,
            "erasure": {
                "data": 2,
                "parity": 2,
                "distribution": [0, 1, 2, 3]
            },
            "shards": [{
                "index": 0,
                "disk": 0,
                "status": "ok",
                "checksum": "sha256abc"
            }, {
                "index": 1,
                "disk": 1,
                "status": "missing",
                "checksum": null
            }]
        }"#;
        let inspect: ObjectInspectResponse = serde_json::from_str(json).unwrap();
        assert_eq!(inspect.bucket, "testbucket");
        assert_eq!(inspect.key, "hello.txt");
        assert_eq!(inspect.size, 128);
        assert_eq!(inspect.erasure.data, 2);
        assert_eq!(inspect.erasure.parity, 2);
        assert_eq!(inspect.erasure.distribution, vec![0, 1, 2, 3]);
        assert_eq!(inspect.shards.len(), 2);
        assert_eq!(inspect.shards[0].status, "ok");
        assert_eq!(inspect.shards[0].checksum, Some("sha256abc".to_string()));
        assert_eq!(inspect.shards[1].status, "missing");
        assert!(inspect.shards[1].checksum.is_none());
    }

    #[test]
    fn heal_response_deserialize_with_shards() {
        let json = r#"{"result": "repaired", "shards_fixed": 2, "error": null}"#;
        let heal: HealResponse = serde_json::from_str(json).unwrap();
        assert_eq!(heal.result, "repaired");
        assert_eq!(heal.shards_fixed, Some(2));
        assert!(heal.error.is_none());
    }

    #[test]
    fn heal_response_deserialize_with_error() {
        let json = r#"{"result": "failed", "shards_fixed": null, "error": "disk offline"}"#;
        let heal: HealResponse = serde_json::from_str(json).unwrap();
        assert_eq!(heal.result, "failed");
        assert!(heal.shards_fixed.is_none());
        assert_eq!(heal.error, Some("disk offline".to_string()));
    }

    #[test]
    fn heal_response_no_optional_fields() {
        let json = r#"{"result": "ok"}"#;
        let heal: HealResponse = serde_json::from_str(json).unwrap();
        assert_eq!(heal.result, "ok");
        assert!(heal.shards_fixed.is_none());
        assert!(heal.error.is_none());
    }

    #[test]
    fn disk_info_offline() {
        let json = r#"{
            "index": 2,
            "path": "/mnt/d3",
            "online": false,
            "total_bytes": 0,
            "used_bytes": 0,
            "free_bytes": 0,
            "bucket_count": 0,
            "object_count": 0
        }"#;
        let disk: DiskInfo = serde_json::from_str(json).unwrap();
        assert!(!disk.online);
        assert_eq!(disk.index, 2);
    }
}
