use serde::Deserialize;

#[derive(Debug, Clone, Copy, Default, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum ServiceState {
    Joining,
    SyncingEpoch,
    Fenced,
    #[default]
    Ready,
}

impl std::fmt::Display for ServiceState {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::Joining => write!(f, "joining"),
            Self::SyncingEpoch => write!(f, "syncing epoch"),
            Self::Fenced => write!(f, "fenced"),
            Self::Ready => write!(f, "ready"),
        }
    }
}

#[derive(Debug, Clone, Deserialize, Default)]
pub struct ClusterSummary {
    #[serde(default)]
    pub enabled: bool,
    #[serde(default)]
    pub cluster_id: String,
    #[serde(default)]
    pub node_id: String,
    pub topology_hash: Option<String>,
    #[serde(default)]
    pub state: ServiceState,
    #[serde(default)]
    pub epoch_id: u64,
    #[serde(default)]
    pub leader_id: String,
    #[serde(default)]
    pub node_count: usize,
    #[serde(default)]
    pub voter_count: usize,
    #[serde(default)]
    pub reachable_voters: usize,
    #[serde(default)]
    pub quorum: usize,
    pub fenced_reason: Option<String>,
}

#[derive(Debug, Clone, Deserialize)]
pub struct ClusterNodeStatus {
    pub node_id: String,
    pub advertise_s3: String,
    pub advertise_cluster: String,
    pub state: ServiceState,
    pub voter: bool,
    pub reachable: bool,
    pub total_disks: usize,
    pub last_heartbeat_unix_secs: u64,
}

#[derive(Debug, Clone, Deserialize)]
pub struct ClusterVolumeStatus {
    pub volume_id: String,
    pub node_id: String,
    pub path: String,
}

#[derive(Debug, Clone, Deserialize)]
pub struct ClusterEpoch {
    pub epoch_id: u64,
    pub leader_id: String,
    pub committed_at_unix_secs: u64,
    pub voter_count: usize,
    pub reachable_voters: usize,
}

#[derive(Debug, Clone, Deserialize)]
pub struct ClusterTopology {
    pub cluster_id: String,
    pub epoch: ClusterEpoch,
    pub nodes: Vec<ClusterNodeStatus>,
    pub volumes: Vec<ClusterVolumeStatus>,
}

#[derive(Debug, Clone, Deserialize)]
pub struct ClusterStatusResponse {
    pub summary: ClusterSummary,
    pub topology: ClusterTopology,
}

#[derive(Debug, Clone, Deserialize)]
pub struct ClusterNodesResponse {
    pub nodes: Vec<ClusterNodeStatus>,
}

#[derive(Debug, Clone, Deserialize)]
pub struct ClusterEpochsResponse {
    pub epochs: Vec<ClusterEpoch>,
}

#[derive(Debug, Clone, Deserialize)]
pub struct ClusterTopologyResponse {
    pub topology: ClusterTopology,
}

#[derive(Debug, Clone, Deserialize)]
pub struct EcConfig {
    pub ftt: usize,
}

#[derive(Debug, Clone, Deserialize)]
pub struct StatusResponse {
    pub server: String,
    pub version: String,
    pub uptime_secs: u64,
    pub default_ftt: usize,
    pub total_disks: usize,
    pub listen: String,
    pub auth_enabled: bool,
    pub scan_interval: String,
    pub heal_interval: String,
    pub mrf_workers: usize,
    #[serde(default)]
    pub cluster: ClusterSummary,
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
    #[serde(default)]
    pub epoch_id: u64,
    #[serde(default)]
    pub set_id: String,
    pub distribution: Vec<usize>,
    #[serde(default)]
    pub node_ids: Vec<String>,
    #[serde(default)]
    pub volume_ids: Vec<String>,
}

#[derive(Debug, Clone, Deserialize)]
pub struct ShardInfo {
    pub index: usize,
    pub disk: usize,
    #[serde(default)]
    pub node_id: String,
    #[serde(default)]
    pub volume_id: String,
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
            "default_ftt": 1,
            "total_disks": 4,
            "listen": "0.0.0.0:10000",
            "auth_enabled": false,
            "scan_interval": "5m",
            "heal_interval": "1m",
            "mrf_workers": 4,
            "cluster": {
                "enabled": true,
                "cluster_id": "abc",
                "node_id": "node-1",
                "state": "ready",
                "epoch_id": 3,
                "leader_id": "node-1",
                "node_count": 2,
                "voter_count": 2,
                "reachable_voters": 2,
                "quorum": 2
            }
        }"#;
        let status: StatusResponse = serde_json::from_str(json).unwrap();
        assert_eq!(status.server, "abixio");
        assert_eq!(status.version, "0.1.0");
        assert_eq!(status.uptime_secs, 3600);
        assert_eq!(status.default_ftt, 1);
        assert_eq!(status.total_disks, 4);
        assert!(!status.auth_enabled);
        assert!(status.cluster.enabled);
        assert_eq!(status.cluster.node_count, 2);
        assert_eq!(status.cluster.state, ServiceState::Ready);
    }

    #[test]
    fn status_response_without_cluster() {
        let json = r#"{
            "server": "abixio",
            "version": "0.1.0",
            "uptime_secs": 100,
            "default_ftt": 1,
            "total_disks": 4,
            "listen": "0.0.0.0:10000",
            "auth_enabled": false,
            "scan_interval": "5m",
            "heal_interval": "1m",
            "mrf_workers": 4
        }"#;
        let status: StatusResponse = serde_json::from_str(json).unwrap();
        assert!(!status.cluster.enabled);
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
                "epoch_id": 5,
                "set_id": "set-abc",
                "distribution": [0, 1, 2, 3],
                "node_ids": ["node-1", "node-1", "node-2", "node-2"],
                "volume_ids": ["vol-0", "vol-1", "vol-2", "vol-3"]
            },
            "shards": [{
                "index": 0,
                "disk": 0,
                "node_id": "node-1",
                "volume_id": "vol-0",
                "status": "ok",
                "checksum": "sha256abc"
            }, {
                "index": 1,
                "disk": 1,
                "node_id": "node-1",
                "volume_id": "vol-1",
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
        assert_eq!(inspect.erasure.epoch_id, 5);
        assert_eq!(inspect.erasure.set_id, "set-abc");
        assert_eq!(inspect.erasure.distribution, vec![0, 1, 2, 3]);
        assert_eq!(inspect.erasure.node_ids.len(), 4);
        assert_eq!(inspect.erasure.volume_ids.len(), 4);
        assert_eq!(inspect.shards.len(), 2);
        assert_eq!(inspect.shards[0].status, "ok");
        assert_eq!(inspect.shards[0].node_id, "node-1");
        assert_eq!(inspect.shards[0].volume_id, "vol-0");
        assert_eq!(inspect.shards[0].checksum, Some("sha256abc".to_string()));
        assert_eq!(inspect.shards[1].status, "missing");
        assert!(inspect.shards[1].checksum.is_none());
    }

    #[test]
    fn object_inspect_backward_compat() {
        let json = r#"{
            "bucket": "b",
            "key": "k",
            "size": 10,
            "etag": "e",
            "content_type": "text/plain",
            "created_at": 0,
            "erasure": {
                "data": 2,
                "parity": 2,
                "distribution": [0, 1, 2, 3]
            },
            "shards": [{
                "index": 0,
                "disk": 0,
                "status": "ok",
                "checksum": null
            }]
        }"#;
        let inspect: ObjectInspectResponse = serde_json::from_str(json).unwrap();
        assert_eq!(inspect.erasure.epoch_id, 0);
        assert!(inspect.erasure.set_id.is_empty());
        assert!(inspect.erasure.node_ids.is_empty());
        assert!(inspect.shards[0].node_id.is_empty());
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

    #[test]
    fn cluster_node_status_deserialize() {
        let json = r#"{
            "node_id": "node-1",
            "advertise_s3": "http://10.0.0.1:10000",
            "advertise_cluster": "http://10.0.0.1:10000",
            "state": "ready",
            "voter": true,
            "reachable": true,
            "total_disks": 4,
            "last_heartbeat_unix_secs": 1700000000
        }"#;
        let node: ClusterNodeStatus = serde_json::from_str(json).unwrap();
        assert_eq!(node.node_id, "node-1");
        assert_eq!(node.state, ServiceState::Ready);
        assert!(node.voter);
        assert!(node.reachable);
    }

    #[test]
    fn cluster_epoch_deserialize() {
        let json = r#"{
            "epoch_id": 5,
            "leader_id": "node-1",
            "committed_at_unix_secs": 1700000000,
            "voter_count": 3,
            "reachable_voters": 3
        }"#;
        let epoch: ClusterEpoch = serde_json::from_str(json).unwrap();
        assert_eq!(epoch.epoch_id, 5);
        assert_eq!(epoch.leader_id, "node-1");
    }

    #[test]
    fn ec_config_deserialize() {
        let json = r#"{"ftt": 1}"#;
        let config: EcConfig = serde_json::from_str(json).unwrap();
        assert_eq!(config.ftt, 1);
    }

    #[test]
    fn service_state_variants() {
        let json = r#""joining""#;
        let s: ServiceState = serde_json::from_str(json).unwrap();
        assert_eq!(s, ServiceState::Joining);

        let json = r#""syncing_epoch""#;
        let s: ServiceState = serde_json::from_str(json).unwrap();
        assert_eq!(s, ServiceState::SyncingEpoch);

        let json = r#""fenced""#;
        let s: ServiceState = serde_json::from_str(json).unwrap();
        assert_eq!(s, ServiceState::Fenced);
    }
}
