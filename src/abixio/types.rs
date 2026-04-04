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
