use std::path::PathBuf;

use crate::s3::client::BucketInfo;

pub const CURRENT_CONNECTION_ID: &str = "__current__";

#[derive(Debug, Clone)]
pub struct StartupOptions {
    pub endpoint: Option<String>,
    pub creds: Option<(String, String)>,
    pub auto_run_tests: bool,
    pub test_report_path: Option<PathBuf>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum TransferMode {
    CopyObject,
    MoveObject,
    ImportFolder,
    ExportPrefix,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum OverwritePolicy {
    Ask,
    OverwriteAll,
    SkipAll,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum TransferEndpoint {
    S3 {
        connection_id: String,
        bucket: String,
        key: String,
    },
    Local {
        path: PathBuf,
    },
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct TransferItem {
    pub source: TransferEndpoint,
    pub destination: TransferEndpoint,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum TransferStepResult {
    Conflict(TransferItem),
    Copied(String),
    Skipped(String),
}

#[derive(Debug, Clone)]
pub struct TransferState {
    pub mode: TransferMode,
    pub destination_connection_id: String,
    pub destination_bucket: String,
    pub destination_key: String,
    pub destination_buckets: Option<Result<Vec<BucketInfo>, String>>,
    pub loading_destination_buckets: bool,
    pub local_path: Option<PathBuf>,
    pub source_bucket: Option<String>,
    pub source_key: Option<String>,
    pub source_prefix: Option<String>,
    pub items: Vec<TransferItem>,
    pub next_index: usize,
    pub completed: usize,
    pub skipped: usize,
    pub failed: usize,
    pub current_item: Option<String>,
    pub pending_conflict: Option<TransferItem>,
    pub overwrite_policy: OverwritePolicy,
    pub preparing: bool,
    pub running: bool,
    pub summary: Option<String>,
}

#[derive(Debug, Clone)]
pub struct BucketDeleteState {
    pub bucket: String,
    pub confirm_name: String,
    pub preview_loading: bool,
    pub object_keys: Vec<String>,
    pub total_objects: usize,
    pub deleted_objects: usize,
    pub next_index: usize,
    pub deleting: bool,
    pub summary: Option<String>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum BucketDeleteStepResult {
    ObjectDeleted(String),
    BucketDeleted(String),
}

#[derive(Debug, Clone)]
pub struct BulkDeleteState {
    pub bucket: String,
    pub keys: Vec<String>,
    pub total: usize,
    pub deleted: usize,
    pub next_index: usize,
    pub deleting: bool,
    pub summary: Option<String>,
}

pub struct PrefixDeleteState {
    pub bucket: String,
    pub prefix: String,
    pub keys: Vec<String>,
    pub loading: bool,
    pub total: usize,
    pub deleted: usize,
    pub next_index: usize,
    pub deleting: bool,
    pub summary: Option<String>,
}
