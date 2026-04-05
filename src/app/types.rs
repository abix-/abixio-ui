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
