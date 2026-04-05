use std::path::PathBuf;

use iced::widget::text_editor;

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

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum SyncMode {
    Diff,
    Copy,
    Sync,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum SyncPreset {
    Converge,
    UpdateOnly,
    Exact,
    Custom,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum SyncDestinationNewerPolicy {
    SourceWins,
    Skip,
    Conflict,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum SyncDeletePhase {
    Before,
    During,
    After,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct SyncPolicy {
    pub overwrite_changed: bool,
    pub delete_extras: bool,
    pub destination_newer_policy: SyncDestinationNewerPolicy,
    pub delete_phase: SyncDeletePhase,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum SyncCompareMode {
    SizeOnly,
    SizeAndModTime,
    UpdateIfSourceNewer,
    ChecksumIfAvailable,
    AlwaysOverwrite,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum SyncListMode {
    Streaming,
    FastList,
    TopUp,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum SyncEndpointKind {
    S3,
    Local,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum SyncEndpoint {
    S3 {
        connection_id: String,
        bucket: String,
        prefix: String,
    },
    Local {
        path: PathBuf,
    },
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct SyncFilterSet {
    pub include_patterns_text: String,
    pub exclude_patterns_text: String,
    pub newer_than_text: String,
    pub older_than_text: String,
    pub min_size_text: String,
    pub max_size_text: String,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct SyncTuning {
    pub list_mode: SyncListMode,
    pub compare_mode: SyncCompareMode,
    pub list_workers_text: String,
    pub compare_workers_text: String,
    pub fast_list_enabled: bool,
    pub prefer_server_modtime: bool,
    pub max_planner_items_text: String,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct SyncObject {
    pub relative_path: String,
    pub size: u64,
    pub modified: Option<String>,
    pub etag: Option<String>,
    pub is_dir_marker: bool,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum SyncPlanAction {
    Create,
    Update,
    Delete,
    Skip,
    Conflict,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum SyncPlanReason {
    MissingOnDestination,
    MissingOnSource,
    SizeMismatch,
    SourceNewer,
    DestinationNewer,
    ChecksumMismatch,
    FilteredOut,
    Identical,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct SyncPlanItem {
    pub action: SyncPlanAction,
    pub reason: SyncPlanReason,
    pub relative_path: String,
    pub source: Option<SyncObject>,
    pub destination: Option<SyncObject>,
}

#[derive(Debug, Clone, PartialEq, Eq, Default)]
pub struct SyncPlanSummary {
    pub creates: usize,
    pub updates: usize,
    pub deletes: usize,
    pub skips: usize,
    pub conflicts: usize,
    pub bytes_to_create: u64,
    pub bytes_to_update: u64,
    pub bytes_to_delete: u64,
    pub source_scanned: usize,
    pub destination_scanned: usize,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct SyncPlan {
    pub items: Vec<SyncPlanItem>,
    pub summary: SyncPlanSummary,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct SyncTelemetry {
    pub stage: String,
    pub source_scanned: usize,
    pub destination_scanned: usize,
    pub compared: usize,
    pub filtered: usize,
    pub started_at: Option<String>,
    pub last_update_at: Option<String>,
}

#[derive(Debug, Clone)]
pub struct SyncState {
    pub mode: SyncMode,
    pub preset: SyncPreset,
    pub policy: SyncPolicy,
    pub source_kind: SyncEndpointKind,
    pub destination_kind: SyncEndpointKind,
    pub source_connection_id: String,
    pub destination_connection_id: String,
    pub source_bucket: String,
    pub destination_bucket: String,
    pub source_prefix: String,
    pub destination_prefix: String,
    pub source_local_path: Option<PathBuf>,
    pub destination_local_path: Option<PathBuf>,
    pub source_buckets: Option<Result<Vec<BucketInfo>, String>>,
    pub destination_buckets: Option<Result<Vec<BucketInfo>, String>>,
    pub loading_source_buckets: bool,
    pub loading_destination_buckets: bool,
    pub tuning: SyncTuning,
    pub filters: SyncFilterSet,
    pub running: bool,
    pub plan: Option<SyncPlan>,
    pub source_snapshot: Option<Vec<SyncObject>>,
    pub destination_snapshot: Option<Vec<SyncObject>>,
    pub telemetry: SyncTelemetry,
    pub error: Option<String>,
    pub show_advanced: bool,
    pub preview_before_run: bool,
    pub allow_direct_run: bool,
}

impl SyncState {
    pub fn new(current_connection_id: String) -> Self {
        Self {
            mode: SyncMode::Diff,
            preset: SyncPreset::Converge,
            policy: SyncPreset::Converge.policy(),
            source_kind: SyncEndpointKind::S3,
            destination_kind: SyncEndpointKind::S3,
            source_connection_id: current_connection_id.clone(),
            destination_connection_id: current_connection_id,
            source_bucket: String::new(),
            destination_bucket: String::new(),
            source_prefix: String::new(),
            destination_prefix: String::new(),
            source_local_path: None,
            destination_local_path: None,
            source_buckets: None,
            destination_buckets: None,
            loading_source_buckets: false,
            loading_destination_buckets: false,
            tuning: SyncTuning {
                list_mode: SyncListMode::Streaming,
                compare_mode: SyncCompareMode::SizeAndModTime,
                list_workers_text: "8".to_string(),
                compare_workers_text: "8".to_string(),
                fast_list_enabled: false,
                prefer_server_modtime: true,
                max_planner_items_text: "250000".to_string(),
            },
            filters: SyncFilterSet {
                include_patterns_text: String::new(),
                exclude_patterns_text: String::new(),
                newer_than_text: String::new(),
                older_than_text: String::new(),
                min_size_text: String::new(),
                max_size_text: String::new(),
            },
            running: false,
            plan: None,
            source_snapshot: None,
            destination_snapshot: None,
            telemetry: SyncTelemetry {
                stage: "Idle".to_string(),
                source_scanned: 0,
                destination_scanned: 0,
                compared: 0,
                filtered: 0,
                started_at: None,
                last_update_at: None,
            },
            error: None,
            show_advanced: false,
            preview_before_run: true,
            allow_direct_run: false,
        }
    }
}

impl SyncPreset {
    pub fn title(self) -> &'static str {
        match self {
            Self::Converge => "Converge",
            Self::UpdateOnly => "Update Only",
            Self::Exact => "Exact",
            Self::Custom => "Custom",
        }
    }

    pub fn description(self) -> &'static str {
        match self {
            Self::Converge => "Source wins. Update changed objects and delete destination extras.",
            Self::UpdateOnly => "Update changed objects, but keep destination extras.",
            Self::Exact => {
                "Require review for destination-newer conflicts before exact convergence."
            }
            Self::Custom => "Advanced policy overrides are active.",
        }
    }

    pub fn policy(self) -> SyncPolicy {
        match self {
            Self::Converge => SyncPolicy {
                overwrite_changed: true,
                delete_extras: true,
                destination_newer_policy: SyncDestinationNewerPolicy::SourceWins,
                delete_phase: SyncDeletePhase::After,
            },
            Self::UpdateOnly => SyncPolicy {
                overwrite_changed: true,
                delete_extras: false,
                destination_newer_policy: SyncDestinationNewerPolicy::SourceWins,
                delete_phase: SyncDeletePhase::After,
            },
            Self::Exact => SyncPolicy {
                overwrite_changed: true,
                delete_extras: true,
                destination_newer_policy: SyncDestinationNewerPolicy::Conflict,
                delete_phase: SyncDeletePhase::After,
            },
            Self::Custom => SyncPolicy {
                overwrite_changed: true,
                delete_extras: true,
                destination_newer_policy: SyncDestinationNewerPolicy::SourceWins,
                delete_phase: SyncDeletePhase::After,
            },
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum BucketDocumentKind {
    Policy,
    Lifecycle,
}

impl BucketDocumentKind {
    pub fn title(self) -> &'static str {
        match self {
            Self::Policy => "Policy",
            Self::Lifecycle => "Lifecycle",
        }
    }

    pub fn empty_label(self) -> &'static str {
        match self {
            Self::Policy => "No policy configured.",
            Self::Lifecycle => "No lifecycle configuration.",
        }
    }

    pub fn create_label(self) -> &'static str {
        match self {
            Self::Policy => "Create Policy",
            Self::Lifecycle => "Create Lifecycle",
        }
    }

    pub fn edit_label(self) -> &'static str {
        match self {
            Self::Policy => "Edit Policy",
            Self::Lifecycle => "Edit Lifecycle",
        }
    }

    pub fn delete_label(self) -> &'static str {
        match self {
            Self::Policy => "Delete Policy",
            Self::Lifecycle => "Delete Lifecycle",
        }
    }

    pub fn save_error_prefix(self) -> &'static str {
        match self {
            Self::Policy => "save policy failed",
            Self::Lifecycle => "save lifecycle failed",
        }
    }

    pub fn delete_error_prefix(self) -> &'static str {
        match self {
            Self::Policy => "delete policy failed",
            Self::Lifecycle => "delete lifecycle failed",
        }
    }

    pub fn validation_empty_error(self) -> &'static str {
        match self {
            Self::Policy => "Policy JSON cannot be empty.",
            Self::Lifecycle => "Lifecycle XML cannot be empty.",
        }
    }

    pub fn example(self) -> &'static str {
        match self {
            Self::Policy => "{\n  \"Version\": \"2012-10-17\",\n  \"Statement\": []\n}",
            Self::Lifecycle => {
                "<LifecycleConfiguration>\n  <Rule>\n    <ID>expire-logs</ID>\n    <Filter>\n      <Prefix>logs/</Prefix>\n    </Filter>\n    <Status>Enabled</Status>\n    <Expiration>\n      <Days>30</Days>\n    </Expiration>\n  </Rule>\n</LifecycleConfiguration>"
            }
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum BucketDocumentLoadState {
    Absent,
    Loaded(String),
    Error(String),
}

pub struct BucketDocumentState {
    pub loaded: Option<BucketDocumentLoadState>,
    pub editor: text_editor::Content,
    pub editing: bool,
    pub saving: bool,
    pub error: Option<String>,
}

impl Default for BucketDocumentState {
    fn default() -> Self {
        Self::new()
    }
}

impl BucketDocumentState {
    pub fn new() -> Self {
        Self {
            loaded: None,
            editor: text_editor::Content::new(),
            editing: false,
            saving: false,
            error: None,
        }
    }

    pub fn reset(&mut self) {
        self.loaded = None;
        self.reset_editor("");
    }

    pub fn start_editing(&mut self) {
        let text = match &self.loaded {
            Some(BucketDocumentLoadState::Loaded(text)) => text.as_str(),
            _ => "",
        };
        self.editing = true;
        self.saving = false;
        self.error = None;
        self.editor = text_editor::Content::with_text(text);
    }

    pub fn cancel_editing(&mut self) {
        let text = match &self.loaded {
            Some(BucketDocumentLoadState::Loaded(text)) => text.clone(),
            _ => String::new(),
        };
        self.reset_editor(&text);
    }

    pub fn set_loaded(&mut self, loaded: BucketDocumentLoadState) {
        let text = match &loaded {
            BucketDocumentLoadState::Loaded(text) => text.clone(),
            _ => String::new(),
        };
        self.loaded = Some(loaded);
        if !self.editing {
            self.editor = text_editor::Content::with_text(&text);
        }
        self.saving = false;
        self.error = None;
    }

    pub fn reset_editor(&mut self, text: &str) {
        self.editor = text_editor::Content::with_text(text);
        self.editing = false;
        self.saving = false;
        self.error = None;
    }
}
