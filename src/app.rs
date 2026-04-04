use std::collections::HashSet;
use std::path::{Path, PathBuf};
use std::sync::Arc;

use iced::keyboard;
use iced::widget::{button, column, container, row, stack, text};
use iced::{Element, Length, Subscription, Task, Theme};
use time::OffsetDateTime;
use time::format_description::well_known::Rfc3339;

use crate::abixio::client::AdminClient;
use crate::abixio::types::{
    DisksResponse, HealResponse, HealStatusResponse, ObjectInspectResponse, StatusResponse,
};
use crate::config::{self, Connection, Settings};
use crate::s3::client::{BucketInfo, ListObjectsResult, ObjectDetail, ObjectInfo, S3Client};
use crate::views::testing::TestResult;

pub(crate) const CURRENT_CONNECTION_ID: &str = "__current__";

#[derive(Debug, Clone)]
pub struct StartupOptions {
    pub endpoint: Option<String>,
    pub creds: Option<(String, String)>,
    pub auto_run_tests: bool,
    pub test_report_path: Option<PathBuf>,
}

// -- messages --

#[derive(Debug, Clone)]
pub enum Message {
    SelectSection(Section),
    SelectBucket(String),
    NavigatePrefix(String),
    SelectObject(String),
    ClearSelection,

    BucketsLoaded(Result<Vec<BucketInfo>, String>),
    ObjectsLoaded(Result<ListObjectsResult, String>),
    DetailLoaded(Result<ObjectDetail, String>),
    UploadDone(Result<String, String>),
    DeleteDone(Result<(), String>),
    CreateBucketDone {
        bucket: String,
        result: Result<(), String>,
    },
    DownloadDone(Result<String, String>),

    Refresh,
    RefreshAll,
    Upload,
    Delete(String, String),
    Download(String, String),
    OpenCopyObject,
    OpenMoveObject,
    OpenRenameObject,
    OpenImportFolder,
    OpenExportPrefix,
    CloseTransferModal,
    TransferDestinationConnectionChanged(String),
    TransferDestinationBucketChanged(String),
    TransferDestinationKeyChanged(String),
    TransferDestinationBucketsLoaded(Result<Vec<BucketInfo>, String>),
    StartTransfer,
    TransferPrepared(Result<Vec<TransferItem>, String>),
    TransferStepFinished(Result<TransferStepResult, String>),
    TransferConflictOverwrite,
    TransferConflictSkip,
    TransferConflictOverwriteAll,
    TransferConflictSkipAll,
    NewBucketNameChanged(String),
    OpenCreateBucketModal,
    CloseCreateBucketModal,
    CreateBucket,
    OpenDeleteBucketModal,
    CloseDeleteBucketModal,
    BucketDeletePreviewLoaded {
        bucket: String,
        result: Result<Vec<ObjectInfo>, String>,
    },
    BucketDeleteConfirmNameChanged(String),
    ConfirmDeleteBucket,
    BucketDeleteStepFinished(Result<BucketDeleteStepResult, String>),
    SetTheme(AppTheme),
    DismissError,

    // connection manager
    ConnectTo(String),
    AddConnection,
    EditConnection(String),
    RemoveConnection(String),
    TestConnection(String),
    TestConnectionResult(String, Result<(), String>),
    NewConnNameChanged(String),
    NewConnEndpointChanged(String),
    NewConnRegionChanged(String),
    NewConnAccessKeyChanged(String),
    NewConnSecretKeyChanged(String),

    // admin (abixio-specific)
    AbixioDetected(Option<StatusResponse>),
    DisksLoaded(Result<DisksResponse, String>),
    HealStatusLoaded(Result<HealStatusResponse, String>),
    ObjectInspectLoaded {
        bucket: String,
        key: String,
        result: Result<ObjectInspectResponse, String>,
    },
    RefreshDisks,
    RefreshHealStatus,
    RefreshObjectInspect,
    OpenHealConfirm,
    CancelHealConfirm,
    ConfirmHealObject,
    HealObjectFinished {
        bucket: String,
        key: String,
        result: Result<HealResponse, String>,
    },

    // object filter / find
    ObjectFilterChanged(String),
    Find,
    FindComplete(Result<ListObjectsResult, String>),
    ClearFind,

    // multi-select / bulk delete
    ToggleObjectSelected(String),
    SelectAllObjects,
    ClearObjectSelection,
    OpenBulkDeleteModal,
    CloseBulkDeleteModal,
    ConfirmBulkDelete,
    BulkDeleteBatchFinished(Result<usize, String>),

    // prefix delete
    OpenPrefixDeleteModal(String),
    ClosePrefixDeleteModal,
    PrefixDeleteListLoaded(Result<Vec<String>, String>),
    ConfirmPrefixDelete,
    PrefixDeleteBatchFinished(Result<usize, String>),

    // testing
    RunTests,
    TestsComplete(Vec<TestResult>),
    AutoStartTests,
    TestReportWritten(Result<PathBuf, String>),
}

#[derive(Debug, Clone, Copy, PartialEq)]
pub enum Section {
    Browse,
    Disks,
    Config,
    Healing,
    Connections,
    Settings,
    Testing,
}

#[derive(Debug, Clone, Copy, PartialEq)]
pub enum AppTheme {
    Dark,
    Light,
}

#[derive(Debug, Clone, PartialEq)]
pub enum Selection {
    None,
    Bucket(String),
    Object { bucket: String, key: String },
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

// -- state --

pub struct App {
    pub client: Arc<S3Client>,
    pub endpoint: String,
    pub section: Section,
    pub selection: Selection,
    pub theme: AppTheme,

    pub buckets: Option<Result<Vec<BucketInfo>, String>>,
    pub objects: Option<Result<ListObjectsResult, String>>,
    pub detail: Option<Result<ObjectDetail, String>>,

    pub selected_bucket: Option<String>,
    pub current_prefix: String,
    pub new_bucket_name: String,
    pub create_bucket_modal_open: bool,
    pub create_bucket_modal_error: Option<String>,

    pub loading_buckets: bool,
    pub loading_objects: bool,
    pub loading_detail: bool,

    pub error: Option<String>,
    pub perf: crate::perf::PerfStats,

    // connection manager
    pub settings: Settings,
    pub active_connection: Option<String>,
    pub editing_connection: Option<String>,

    // admin (abixio-specific)
    pub admin_client: Option<Arc<AdminClient>>,
    pub is_abixio: bool,
    pub server_status: Option<StatusResponse>,
    pub disks_data: Option<Result<DisksResponse, String>>,
    pub heal_data: Option<Result<HealStatusResponse, String>>,
    pub object_inspect: Option<Result<ObjectInspectResponse, String>>,
    pub loading_object_inspect: bool,
    pub object_inspect_target: Option<(String, String)>,
    pub heal_confirm_target: Option<(String, String)>,
    pub healing_object: bool,
    pub healing_target: Option<(String, String)>,
    pub heal_result: Option<String>,
    pub transfer: Option<TransferState>,
    pub bucket_delete: Option<BucketDeleteState>,

    // connection form
    pub new_conn_name: String,
    pub new_conn_endpoint: String,
    pub new_conn_region: String,
    pub new_conn_access_key: String,
    pub new_conn_secret_key: String,

    // object filter / find
    pub object_filter: String,
    pub find_results: Option<Result<ListObjectsResult, String>>,
    pub finding: bool,

    // multi-select / bulk delete
    pub selected_keys: HashSet<String>,
    pub bulk_delete: Option<BulkDeleteState>,
    pub prefix_delete: Option<PrefixDeleteState>,

    // testing
    pub test_results: Vec<TestResult>,
    pub test_running: bool,
    pub test_progress: String,
    pub auto_run_tests: bool,
    pub auto_test_started: bool,
    pub test_report_path: Option<PathBuf>,
    pub test_started_at: Option<String>,
}

impl App {
    pub fn new(startup: StartupOptions) -> (Self, Task<Message>) {
        let settings = config::load().unwrap_or_default();
        let endpoint = startup.endpoint;
        let creds = startup.creds;

        let (client, start_endpoint, section, loading_buckets) = if let Some(ref ep) = endpoint {
            let c = match &creds {
                Some((ak, sk)) => S3Client::new(ep, Some((ak, sk)), "us-east-1"),
                None => S3Client::anonymous(ep),
            };
            match c {
                Ok(client) => (Arc::new(client), ep.clone(), Section::Browse, true),
                Err(e) => {
                    tracing::error!("failed to create s3 client: {}", e);
                    let fallback =
                        S3Client::anonymous("http://localhost:10000").expect("fallback client");
                    (
                        Arc::new(fallback),
                        String::new(),
                        Section::Connections,
                        false,
                    )
                }
            }
        } else {
            let fallback = S3Client::anonymous("http://localhost:10000").expect("fallback client");
            (
                Arc::new(fallback),
                String::new(),
                Section::Connections,
                false,
            )
        };

        let start_section = if startup.auto_run_tests && loading_buckets {
            Section::Testing
        } else {
            section
        };

        let app = Self {
            client: client.clone(),
            endpoint: start_endpoint.clone(),
            section: start_section,
            selection: Selection::None,
            theme: AppTheme::Dark,
            buckets: None,
            objects: None,
            detail: None,
            selected_bucket: None,
            current_prefix: String::new(),
            new_bucket_name: String::new(),
            create_bucket_modal_open: false,
            create_bucket_modal_error: None,
            loading_buckets,
            loading_objects: false,
            loading_detail: false,
            error: None,
            perf: crate::perf::PerfStats::new(),
            settings,
            active_connection: None,
            editing_connection: None,
            admin_client: None,
            is_abixio: false,
            server_status: None,
            disks_data: None,
            heal_data: None,
            object_inspect: None,
            loading_object_inspect: false,
            object_inspect_target: None,
            heal_confirm_target: None,
            healing_object: false,
            healing_target: None,
            heal_result: None,
            transfer: None,
            bucket_delete: None,
            new_conn_name: String::new(),
            new_conn_endpoint: String::new(),
            new_conn_region: "us-east-1".to_string(),
            new_conn_access_key: String::new(),
            new_conn_secret_key: String::new(),
            object_filter: String::new(),
            find_results: None,
            finding: false,
            selected_keys: HashSet::new(),
            bulk_delete: None,
            prefix_delete: None,
            test_results: Vec::new(),
            test_running: false,
            test_progress: String::new(),
            auto_run_tests: startup.auto_run_tests,
            auto_test_started: false,
            test_report_path: startup.test_report_path,
            test_started_at: None,
        };

        let task = if loading_buckets {
            let mut tasks = vec![];
            let c = client.clone();
            tasks.push(Task::perform(
                async move { c.list_buckets().await },
                Message::BucketsLoaded,
            ));
            let admin = Arc::new(AdminClient::new(
                &start_endpoint,
                creds.as_ref().map(|(a, s)| (a.as_str(), s.as_str())),
                "us-east-1",
            ));
            let mut app = app;
            app.admin_client = Some(admin.clone());
            let probe_task =
                Task::perform(async move { admin.probe().await }, Message::AbixioDetected);
            tasks.push(probe_task);
            return (app, Task::batch(tasks));
        } else {
            Task::none()
        };
        (app, task)
    }

    pub fn title(&self) -> String {
        "abixio-ui".to_string()
    }

    pub fn theme(&self) -> Theme {
        match self.theme {
            AppTheme::Dark => Theme::Dark,
            AppTheme::Light => Theme::Light,
        }
    }

    pub fn subscription(&self) -> Subscription<Message> {
        keyboard::listen().filter_map(|event| match event {
            keyboard::Event::KeyPressed {
                key: keyboard::Key::Named(keyboard::key::Named::Escape),
                ..
            } => Some(Message::ClearSelection),
            _ => None,
        })
    }

    pub fn update(&mut self, message: Message) -> Task<Message> {
        // record_frame counts update() calls, not view() renders.
        // in iced, each update triggers at most one render, so this
        // closely approximates actual frame count.
        self.perf.record_frame();

        match message {
            Message::SelectSection(s) => {
                self.section = s;
                Task::none()
            }
            Message::SelectBucket(name) => {
                self.selected_bucket = Some(name.clone());
                self.current_prefix.clear();
                self.object_filter.clear();
                self.selected_keys.clear();
                self.find_results = None;
                self.selection = Selection::Bucket(name);
                self.clear_object_admin_state();
                self.loading_objects = true;
                self.cmd_fetch_objects()
            }
            Message::NavigatePrefix(prefix) => {
                self.current_prefix = prefix;
                self.object_filter.clear();
                self.selected_keys.clear();
                self.find_results = None;
                self.selection = Selection::None;
                self.clear_object_admin_state();
                self.loading_objects = true;
                self.cmd_fetch_objects()
            }
            Message::SelectObject(key) => {
                let bucket = self.selected_bucket.clone().unwrap_or_default();
                self.clear_object_admin_state();
                self.selection = Selection::Object {
                    bucket: bucket.clone(),
                    key: key.clone(),
                };
                self.loading_detail = true;
                if self.is_abixio && self.admin_client.is_some() {
                    self.loading_object_inspect = true;
                    self.object_inspect_target = Some((bucket.clone(), key.clone()));
                    Task::batch(vec![
                        self.cmd_fetch_detail(&bucket, &key),
                        self.cmd_fetch_object_inspect(&bucket, &key),
                    ])
                } else {
                    self.cmd_fetch_detail(&bucket, &key)
                }
            }
            Message::ClearSelection => {
                self.selection = Selection::None;
                self.clear_object_admin_state();
                Task::none()
            }

            Message::BucketsLoaded(r) => {
                self.loading_buckets = false;
                self.buckets = Some(r);
                Task::none()
            }
            Message::ObjectsLoaded(r) => {
                self.loading_objects = false;
                self.objects = Some(r);
                Task::none()
            }
            Message::DetailLoaded(r) => {
                self.loading_detail = false;
                self.detail = Some(r);
                Task::none()
            }
            Message::UploadDone(Ok(_)) => {
                self.loading_objects = true;
                self.cmd_fetch_objects()
            }
            Message::UploadDone(Err(e)) => {
                self.error = Some(format!("Upload failed: {}", e));
                Task::none()
            }
            Message::DeleteDone(Ok(())) => {
                self.selection = Selection::None;
                self.clear_object_admin_state();
                self.loading_objects = true;
                self.cmd_fetch_objects()
            }
            Message::DeleteDone(Err(e)) => {
                self.error = Some(format!("Delete failed: {}", e));
                Task::none()
            }
            Message::CreateBucketDone {
                bucket,
                result: Ok(()),
            } => {
                self.create_bucket_modal_open = false;
                self.create_bucket_modal_error = None;
                self.selected_bucket = Some(bucket.clone());
                self.selection = Selection::Bucket(bucket);
                self.current_prefix.clear();
                self.object_filter.clear();
                self.selected_keys.clear();
                self.find_results = None;
                self.objects = None;
                self.detail = None;
                self.loading_buckets = true;
                self.loading_objects = true;
                Task::batch(vec![self.cmd_fetch_buckets(), self.cmd_fetch_objects()])
            }
            Message::CreateBucketDone { result: Err(e), .. } => {
                self.create_bucket_modal_error = Some(e.clone());
                self.error = Some(format!("Create bucket failed: {}", e));
                Task::none()
            }
            Message::DownloadDone(Ok(_)) => Task::none(),
            Message::DownloadDone(Err(e)) => {
                self.error = Some(format!("Download failed: {}", e));
                Task::none()
            }

            Message::ObjectFilterChanged(value) => {
                self.object_filter = value;
                Task::none()
            }
            Message::Find => {
                let bucket = match &self.selected_bucket {
                    Some(b) => b.clone(),
                    None => return Task::none(),
                };
                if self.object_filter.is_empty() {
                    return Task::none();
                }
                self.finding = true;
                self.find_results = None;
                let client = self.client.clone();
                let prefix = self.current_prefix.clone();
                let pattern = self.object_filter.clone();
                Task::perform(
                    async move {
                        let result = client.list_objects_recursive(&bucket, &prefix).await?;
                        let filtered: Vec<_> = result
                            .objects
                            .into_iter()
                            .filter(|obj| wildcard_match(&pattern, &obj.key))
                            .collect();
                        Ok(ListObjectsResult {
                            objects: filtered,
                            common_prefixes: Vec::new(),
                            is_truncated: result.is_truncated,
                        })
                    },
                    Message::FindComplete,
                )
            }
            Message::FindComplete(r) => {
                self.finding = false;
                self.find_results = Some(r);
                Task::none()
            }
            Message::ClearFind => {
                self.find_results = None;
                self.selected_keys.clear();
                Task::none()
            }

            Message::ToggleObjectSelected(key) => {
                if !self.selected_keys.remove(&key) {
                    self.selected_keys.insert(key);
                }
                Task::none()
            }
            Message::SelectAllObjects => {
                if let Some(Ok(result)) = &self.objects {
                    let filter = self.object_filter.to_ascii_lowercase();
                    for obj in &result.objects {
                        let display = obj
                            .key
                            .strip_prefix(&self.current_prefix)
                            .unwrap_or(&obj.key);
                        if filter.is_empty()
                            || display.to_ascii_lowercase().contains(&filter)
                        {
                            self.selected_keys.insert(obj.key.clone());
                        }
                    }
                }
                if let Some(Ok(result)) = &self.find_results {
                    for obj in &result.objects {
                        self.selected_keys.insert(obj.key.clone());
                    }
                }
                Task::none()
            }
            Message::ClearObjectSelection => {
                self.selected_keys.clear();
                Task::none()
            }
            Message::OpenBulkDeleteModal => {
                let bucket = match &self.selected_bucket {
                    Some(b) => b.clone(),
                    None => return Task::none(),
                };
                if self.selected_keys.is_empty() {
                    return Task::none();
                }
                let keys: Vec<String> = self.selected_keys.iter().cloned().collect();
                let total = keys.len();
                self.bulk_delete = Some(BulkDeleteState {
                    bucket,
                    keys,
                    total,
                    deleted: 0,
                    next_index: 0,
                    deleting: false,
                    summary: None,
                });
                Task::none()
            }
            Message::CloseBulkDeleteModal => {
                self.bulk_delete = None;
                Task::none()
            }
            Message::ConfirmBulkDelete => {
                self.cmd_process_next_bulk_delete_step()
            }
            Message::BulkDeleteBatchFinished(result) => {
                let Some(state) = self.bulk_delete.as_mut() else {
                    return Task::none();
                };
                match result {
                    Ok(count) => {
                        state.deleted += count;
                        state.summary = Some(format!(
                            "Deleting: {}/{} done",
                            state.deleted, state.total
                        ));
                        self.cmd_process_next_bulk_delete_step()
                    }
                    Err(error) => {
                        state.deleting = false;
                        state.summary = Some(format!(
                            "Stopped after {}/{}: {}",
                            state.deleted, state.total, error
                        ));
                        self.error = Some(format!("Bulk delete failed: {}", error));
                        Task::none()
                    }
                }
            }

            // prefix delete
            Message::OpenPrefixDeleteModal(prefix) => {
                let bucket = match &self.selected_bucket {
                    Some(b) => b.clone(),
                    None => return Task::none(),
                };
                self.prefix_delete = Some(PrefixDeleteState {
                    bucket: bucket.clone(),
                    prefix: prefix.clone(),
                    keys: Vec::new(),
                    loading: true,
                    total: 0,
                    deleted: 0,
                    next_index: 0,
                    deleting: false,
                    summary: None,
                });
                let client = self.client.clone();
                Task::perform(
                    async move {
                        let result = client.list_objects_recursive(&bucket, &prefix).await?;
                        Ok(result.objects.into_iter().map(|o| o.key).collect())
                    },
                    Message::PrefixDeleteListLoaded,
                )
            }
            Message::ClosePrefixDeleteModal => {
                self.prefix_delete = None;
                Task::none()
            }
            Message::PrefixDeleteListLoaded(result) => {
                let Some(state) = self.prefix_delete.as_mut() else {
                    return Task::none();
                };
                state.loading = false;
                match result {
                    Ok(keys) => {
                        state.total = keys.len();
                        state.keys = keys;
                    }
                    Err(e) => {
                        state.summary = Some(format!("Failed to list: {}", e));
                    }
                }
                Task::none()
            }
            Message::ConfirmPrefixDelete => self.cmd_process_next_prefix_delete_batch(),
            Message::PrefixDeleteBatchFinished(result) => {
                let Some(state) = self.prefix_delete.as_mut() else {
                    return Task::none();
                };
                match result {
                    Ok(count) => {
                        state.deleted += count;
                        state.summary = Some(format!(
                            "Deleting: {}/{} done",
                            state.deleted, state.total
                        ));
                        self.cmd_process_next_prefix_delete_batch()
                    }
                    Err(error) => {
                        state.deleting = false;
                        state.summary = Some(format!(
                            "Stopped after {}/{}: {}",
                            state.deleted, state.total, error
                        ));
                        self.error = Some(format!("Prefix delete failed: {}", error));
                        Task::none()
                    }
                }
            }

            Message::Refresh => {
                self.find_results = None;
                self.selected_keys.clear();
                self.loading_objects = true;
                self.cmd_fetch_objects()
            }
            Message::RefreshAll => {
                self.loading_buckets = true;
                self.cmd_fetch_buckets()
            }
            Message::Upload => {
                let file = rfd::FileDialog::new().pick_file();
                let file = match file {
                    Some(f) => f,
                    None => return Task::none(),
                };
                let client = self.client.clone();
                let bucket = match &self.selected_bucket {
                    Some(b) => b.clone(),
                    None => return Task::none(),
                };
                let prefix = self.current_prefix.clone();
                let filename = file
                    .file_name()
                    .map(|n| n.to_string_lossy().to_string())
                    .unwrap_or_else(|| "upload".to_string());
                let key = format!("{}{}", prefix, filename);
                Task::perform(
                    async move {
                        let data = tokio::fs::read(&file).await.map_err(|e| e.to_string())?;
                        client
                            .put_object(&bucket, &key, data, "application/octet-stream")
                            .await
                    },
                    Message::UploadDone,
                )
            }
            Message::Delete(bucket, key) => {
                let client = self.client.clone();
                Task::perform(
                    async move { client.delete_object(&bucket, &key).await },
                    Message::DeleteDone,
                )
            }
            Message::Download(bucket, key) => {
                let filename = key.rsplit('/').next().unwrap_or(&key).to_string();
                let save_path = rfd::FileDialog::new().set_file_name(&filename).save_file();
                let save_path = match save_path {
                    Some(p) => p,
                    None => return Task::none(),
                };
                let client = self.client.clone();
                Task::perform(
                    async move {
                        let data = client.get_object(&bucket, &key).await?;
                        tokio::fs::write(&save_path, &data)
                            .await
                            .map_err(|e| e.to_string())?;
                        Ok(save_path.to_string_lossy().to_string())
                    },
                    Message::DownloadDone,
                )
            }
            Message::OpenCopyObject => {
                let Some((bucket, key)) = self.current_selected_object() else {
                    return Task::none();
                };
                let destination_connection_id = self.current_connection_id();
                let destination_buckets = if destination_connection_id == CURRENT_CONNECTION_ID {
                    self.buckets.clone()
                } else {
                    None
                };
                self.transfer = Some(TransferState {
                    mode: TransferMode::CopyObject,
                    destination_connection_id: destination_connection_id.clone(),
                    destination_bucket: bucket.clone(),
                    destination_key: key.clone(),
                    destination_buckets,
                    loading_destination_buckets: false,
                    local_path: None,
                    source_bucket: Some(bucket),
                    source_key: Some(key),
                    source_prefix: None,
                    items: Vec::new(),
                    next_index: 0,
                    completed: 0,
                    skipped: 0,
                    failed: 0,
                    current_item: None,
                    pending_conflict: None,
                    overwrite_policy: OverwritePolicy::Ask,
                    preparing: false,
                    running: false,
                    summary: None,
                });
                if destination_connection_id == CURRENT_CONNECTION_ID {
                    Task::none()
                } else {
                    if let Some(transfer) = self.transfer.as_mut() {
                        transfer.loading_destination_buckets = true;
                    }
                    self.cmd_fetch_transfer_buckets(&destination_connection_id)
                }
            }
            Message::OpenMoveObject => {
                let Some((bucket, key)) = self.current_selected_object() else {
                    return Task::none();
                };
                self.transfer = Some(TransferState {
                    mode: TransferMode::MoveObject,
                    destination_connection_id: CURRENT_CONNECTION_ID.to_string(),
                    destination_bucket: bucket.clone(),
                    destination_key: key.clone(),
                    destination_buckets: self.buckets.clone(),
                    loading_destination_buckets: false,
                    local_path: None,
                    source_bucket: Some(bucket),
                    source_key: Some(key),
                    source_prefix: None,
                    items: Vec::new(),
                    next_index: 0,
                    completed: 0,
                    skipped: 0,
                    failed: 0,
                    current_item: None,
                    pending_conflict: None,
                    overwrite_policy: OverwritePolicy::Ask,
                    preparing: false,
                    running: false,
                    summary: None,
                });
                Task::none()
            }
            Message::OpenRenameObject => {
                let Some((bucket, key)) = self.current_selected_object() else {
                    return Task::none();
                };
                self.transfer = Some(TransferState {
                    mode: TransferMode::MoveObject,
                    destination_connection_id: CURRENT_CONNECTION_ID.to_string(),
                    destination_bucket: bucket.clone(),
                    destination_key: key.clone(),
                    destination_buckets: self.buckets.clone(),
                    loading_destination_buckets: false,
                    local_path: None,
                    source_bucket: Some(bucket),
                    source_key: Some(key),
                    source_prefix: None,
                    items: Vec::new(),
                    next_index: 0,
                    completed: 0,
                    skipped: 0,
                    failed: 0,
                    current_item: None,
                    pending_conflict: None,
                    overwrite_policy: OverwritePolicy::Ask,
                    preparing: false,
                    running: false,
                    summary: None,
                });
                Task::none()
            }
            Message::OpenImportFolder => {
                let Some(bucket) = self.selected_bucket.clone() else {
                    return Task::none();
                };
                let Some(path) = rfd::FileDialog::new().pick_folder() else {
                    return Task::none();
                };
                self.transfer = Some(TransferState {
                    mode: TransferMode::ImportFolder,
                    destination_connection_id: self.current_connection_id(),
                    destination_bucket: bucket,
                    destination_key: self.current_prefix.clone(),
                    destination_buckets: self.buckets.clone(),
                    loading_destination_buckets: false,
                    local_path: Some(path),
                    source_bucket: None,
                    source_key: None,
                    source_prefix: None,
                    items: Vec::new(),
                    next_index: 0,
                    completed: 0,
                    skipped: 0,
                    failed: 0,
                    current_item: None,
                    pending_conflict: None,
                    overwrite_policy: OverwritePolicy::Ask,
                    preparing: false,
                    running: false,
                    summary: None,
                });
                Task::none()
            }
            Message::OpenExportPrefix => {
                let Some(bucket) = self.selected_bucket.clone() else {
                    return Task::none();
                };
                let Some(path) = rfd::FileDialog::new().pick_folder() else {
                    return Task::none();
                };
                self.transfer = Some(TransferState {
                    mode: TransferMode::ExportPrefix,
                    destination_connection_id: self.current_connection_id(),
                    destination_bucket: bucket.clone(),
                    destination_key: self.current_prefix.clone(),
                    destination_buckets: None,
                    loading_destination_buckets: false,
                    local_path: Some(path),
                    source_bucket: Some(bucket),
                    source_key: None,
                    source_prefix: Some(self.current_prefix.clone()),
                    items: Vec::new(),
                    next_index: 0,
                    completed: 0,
                    skipped: 0,
                    failed: 0,
                    current_item: None,
                    pending_conflict: None,
                    overwrite_policy: OverwritePolicy::Ask,
                    preparing: false,
                    running: false,
                    summary: None,
                });
                Task::none()
            }
            Message::CloseTransferModal => {
                if self.transfer.as_ref().is_some_and(|t| t.running) {
                    return Task::none();
                }
                self.transfer = None;
                Task::none()
            }
            Message::TransferDestinationConnectionChanged(connection_id) => {
                let Some(transfer) = self.transfer.as_mut() else {
                    return Task::none();
                };
                transfer.destination_connection_id = connection_id.clone();
                transfer.destination_buckets = None;
                transfer.loading_destination_buckets = true;
                transfer.destination_bucket.clear();
                transfer.summary = None;
                self.cmd_fetch_transfer_buckets(&connection_id)
            }
            Message::TransferDestinationBucketChanged(bucket) => {
                if let Some(transfer) = self.transfer.as_mut() {
                    transfer.destination_bucket = bucket;
                    transfer.summary = None;
                }
                Task::none()
            }
            Message::TransferDestinationKeyChanged(key) => {
                if let Some(transfer) = self.transfer.as_mut() {
                    transfer.destination_key = key;
                    transfer.summary = None;
                }
                Task::none()
            }
            Message::TransferDestinationBucketsLoaded(result) => {
                if let Some(transfer) = self.transfer.as_mut() {
                    transfer.loading_destination_buckets = false;
                    transfer.destination_buckets = Some(result);
                }
                Task::none()
            }
            Message::StartTransfer => {
                if !self.transfer_can_start() {
                    return Task::none();
                }
                let Some(transfer) = self.transfer.as_mut() else {
                    return Task::none();
                };
                transfer.preparing = true;
                transfer.summary = None;
                match transfer.mode {
                    TransferMode::CopyObject | TransferMode::MoveObject => {
                        let item = TransferItem {
                            source: TransferEndpoint::S3 {
                                connection_id: CURRENT_CONNECTION_ID.to_string(),
                                bucket: transfer.source_bucket.clone().unwrap_or_default(),
                                key: transfer.source_key.clone().unwrap_or_default(),
                            },
                            destination: TransferEndpoint::S3 {
                                connection_id: transfer.destination_connection_id.clone(),
                                bucket: transfer.destination_bucket.clone(),
                                key: transfer.destination_key.clone(),
                            },
                        };
                        Task::perform(async move { Ok(vec![item]) }, Message::TransferPrepared)
                    }
                    TransferMode::ImportFolder => {
                        let root = transfer.local_path.clone().unwrap_or_default();
                        let bucket = transfer.destination_bucket.clone();
                        let prefix = transfer.destination_key.clone();
                        Task::perform(
                            async move { prepare_import_items(root, &bucket, &prefix) },
                            Message::TransferPrepared,
                        )
                    }
                    TransferMode::ExportPrefix => {
                        let client = self.client.clone();
                        let bucket = transfer.source_bucket.clone().unwrap_or_default();
                        let prefix = transfer.source_prefix.clone().unwrap_or_default();
                        let root = transfer.local_path.clone().unwrap_or_default();
                        Task::perform(
                            async move { prepare_export_items(client, &bucket, &prefix, &root).await },
                            Message::TransferPrepared,
                        )
                    }
                }
            }
            Message::TransferPrepared(result) => {
                let Some(transfer) = self.transfer.as_mut() else {
                    return Task::none();
                };
                transfer.preparing = false;
                match result {
                    Ok(items) => {
                        transfer.items = items;
                        transfer.next_index = 0;
                        transfer.completed = 0;
                        transfer.skipped = 0;
                        transfer.failed = 0;
                        transfer.running = true;
                        transfer.pending_conflict = None;
                        transfer.current_item = None;
                        if transfer.items.is_empty() {
                            transfer.running = false;
                            transfer.summary = Some("Nothing to copy.".to_string());
                            Task::none()
                        } else {
                            self.cmd_process_next_transfer_step()
                        }
                    }
                    Err(error) => {
                        transfer.running = false;
                        transfer.summary = Some(format!("Transfer preparation failed: {}", error));
                        Task::none()
                    }
                }
            }
            Message::TransferStepFinished(result) => {
                let Some(transfer) = self.transfer.as_mut() else {
                    return Task::none();
                };
                match result {
                    Ok(TransferStepResult::Conflict(item)) => {
                        transfer.pending_conflict = Some(item);
                        transfer.current_item = None;
                        transfer.running = false;
                        Task::none()
                    }
                    Ok(TransferStepResult::Copied(label)) => {
                        transfer.completed += 1;
                        transfer.next_index += 1;
                        transfer.current_item = Some(label);
                        self.cmd_process_next_transfer_step()
                    }
                    Ok(TransferStepResult::Skipped(label)) => {
                        transfer.skipped += 1;
                        transfer.next_index += 1;
                        transfer.current_item = Some(label);
                        self.cmd_process_next_transfer_step()
                    }
                    Err(error) => {
                        transfer.failed += 1;
                        transfer.next_index += 1;
                        self.error = Some(format!("Transfer failed: {}", error));
                        self.cmd_process_next_transfer_step()
                    }
                }
            }
            Message::TransferConflictOverwrite => self.resolve_transfer_conflict(false, false),
            Message::TransferConflictSkip => self.resolve_transfer_conflict(true, false),
            Message::TransferConflictOverwriteAll => self.resolve_transfer_conflict(false, true),
            Message::TransferConflictSkipAll => self.resolve_transfer_conflict(true, true),
            Message::OpenCreateBucketModal => {
                self.new_bucket_name.clear();
                self.create_bucket_modal_error = None;
                self.create_bucket_modal_open = true;
                Task::none()
            }
            Message::CloseCreateBucketModal => {
                self.create_bucket_modal_open = false;
                self.create_bucket_modal_error = None;
                Task::none()
            }
            Message::CreateBucket => {
                let name = self.new_bucket_name.trim().to_string();
                if name.is_empty() {
                    self.create_bucket_modal_error = Some("Bucket name is required.".to_string());
                    return Task::none();
                }
                let client = self.client.clone();
                self.create_bucket_modal_error = None;
                Task::perform(
                    async move {
                        let result = client.create_bucket(&name).await;
                        (name, result)
                    },
                    |(bucket, result)| Message::CreateBucketDone { bucket, result },
                )
            }
            Message::OpenDeleteBucketModal => {
                let Some(bucket) = self.current_selected_bucket() else {
                    return Task::none();
                };
                self.bucket_delete = Some(BucketDeleteState {
                    bucket: bucket.clone(),
                    confirm_name: String::new(),
                    preview_loading: true,
                    object_keys: Vec::new(),
                    total_objects: 0,
                    deleted_objects: 0,
                    next_index: 0,
                    deleting: false,
                    summary: None,
                });
                let client = self.client.clone();
                Task::perform(
                    async move {
                        let result = client
                            .list_objects(&bucket, "", "")
                            .await
                            .map(|listing| listing.objects);
                        (bucket, result)
                    },
                    |(bucket, result)| Message::BucketDeletePreviewLoaded { bucket, result },
                )
            }
            Message::CloseDeleteBucketModal => {
                if self
                    .bucket_delete
                    .as_ref()
                    .is_some_and(|state| state.deleting)
                {
                    return Task::none();
                }
                self.bucket_delete = None;
                Task::none()
            }
            Message::BucketDeletePreviewLoaded { bucket, result } => {
                let Some(state) = self.bucket_delete.as_mut() else {
                    return Task::none();
                };
                if state.bucket != bucket {
                    return Task::none();
                }
                state.preview_loading = false;
                match result {
                    Ok(objects) => {
                        state.total_objects = objects.len();
                        state.object_keys = objects.into_iter().map(|object| object.key).collect();
                        state.summary = Some(if state.total_objects == 0 {
                            "Bucket is empty.".to_string()
                        } else {
                            format!(
                                "Bucket contains {} object(s). Delete will remove them recursively.",
                                state.total_objects
                            )
                        });
                    }
                    Err(error) => {
                        state.summary = Some(format!("Preview failed: {}", error));
                    }
                }
                Task::none()
            }
            Message::BucketDeleteConfirmNameChanged(value) => {
                if let Some(state) = self.bucket_delete.as_mut() {
                    state.confirm_name = value;
                }
                Task::none()
            }
            Message::ConfirmDeleteBucket => self.cmd_process_next_bucket_delete_step(),
            Message::BucketDeleteStepFinished(result) => {
                let Some(state) = self.bucket_delete.as_mut() else {
                    return Task::none();
                };
                match result {
                    Ok(BucketDeleteStepResult::ObjectDeleted(label)) => {
                        state.deleted_objects += 1;
                        state.next_index += 1;
                        state.summary = Some(format!(
                            "Deleting objects: {}/{} complete. Last: {}",
                            state.deleted_objects, state.total_objects, label
                        ));
                        self.cmd_process_next_bucket_delete_step()
                    }
                    Ok(BucketDeleteStepResult::BucketDeleted(bucket)) => {
                        self.bucket_delete = None;
                        if self.selected_bucket.as_deref() == Some(&bucket) {
                            self.selected_bucket = None;
                            self.selection = Selection::None;
                            self.current_prefix.clear();
                            self.object_filter.clear();
                            self.selected_keys.clear();
                            self.find_results = None;
                            self.objects = None;
                            self.detail = None;
                            self.clear_object_admin_state();
                        }
                        self.loading_buckets = true;
                        Task::batch(vec![self.cmd_fetch_buckets(), Task::none()])
                    }
                    Err(error) => {
                        state.deleting = false;
                        state.summary = Some(format!(
                            "Delete stopped after {} of {} objects: {}",
                            state.deleted_objects, state.total_objects, error
                        ));
                        self.error = Some(format!("Delete bucket failed: {}", error));
                        if self.selected_bucket.as_deref() == Some(&state.bucket) {
                            self.loading_objects = true;
                            Task::batch(vec![self.cmd_fetch_buckets(), self.cmd_fetch_objects()])
                        } else {
                            self.loading_buckets = true;
                            self.cmd_fetch_buckets()
                        }
                    }
                }
            }
            Message::SetTheme(t) => {
                self.theme = t;
                Task::none()
            }
            Message::NewBucketNameChanged(val) => {
                self.new_bucket_name = val;
                self.create_bucket_modal_error = None;
                Task::none()
            }
            Message::DismissError => {
                self.error = None;
                Task::none()
            }

            // -- connection manager --
            Message::ConnectTo(name) => {
                let conn = match self.settings.connections.iter().find(|c| c.name == name) {
                    Some(c) => c.clone(),
                    None => {
                        self.error = Some(format!("connection '{}' not found", name));
                        return Task::none();
                    }
                };

                let creds = match conn.resolve_keys() {
                    Ok(keys) => keys,
                    Err(e) => {
                        self.error = Some(format!("keychain error: {}", e));
                        return Task::none();
                    }
                };

                match S3Client::new(
                    &conn.endpoint,
                    creds.as_ref().map(|(a, s)| (a.as_str(), s.as_str())),
                    &conn.region,
                ) {
                    Ok(client) => {
                        self.client = Arc::new(client);
                        self.endpoint = conn.endpoint.clone();
                        self.active_connection = Some(name);
                        self.section = Section::Browse;
                        self.selection = Selection::None;
                        self.buckets = None;
                        self.objects = None;
                        self.detail = None;
                        self.selected_bucket = None;
                        self.current_prefix.clear();
                        self.object_filter.clear();
                        self.selected_keys.clear();
                        self.find_results = None;
                        self.clear_object_admin_state();
                        self.loading_buckets = true;
                        self.is_abixio = false;
                        self.server_status = None;
                        self.disks_data = None;
                        self.heal_data = None;

                        // create admin client and probe for AbixIO
                        let admin = Arc::new(AdminClient::new(
                            &conn.endpoint,
                            creds.as_ref().map(|(a, s)| (a.as_str(), s.as_str())),
                            &conn.region,
                        ));
                        self.admin_client = Some(admin.clone());

                        Task::batch(vec![
                            self.cmd_fetch_buckets(),
                            Task::perform(
                                async move { admin.probe().await },
                                Message::AbixioDetected,
                            ),
                        ])
                    }
                    Err(e) => {
                        self.error = Some(format!("connect failed: {}", e));
                        Task::none()
                    }
                }
            }
            Message::AddConnection => {
                let name = self.new_conn_name.trim().to_string();
                let endpoint = self.new_conn_endpoint.trim().to_string();
                let region = self.new_conn_region.trim().to_string();
                let access_key = self.new_conn_access_key.trim().to_string();
                let secret_key = self.new_conn_secret_key.clone();

                if name.is_empty() || endpoint.is_empty() {
                    self.error = Some("name and endpoint are required".to_string());
                    return Task::none();
                }
                if !config::is_valid_name(&name) {
                    self.error = Some(
                        "name must start with a letter, only alphanumeric/dash/underscore"
                            .to_string(),
                    );
                    return Task::none();
                }
                if !config::is_valid_endpoint(&endpoint) {
                    self.error = Some("endpoint must start with http:// or https://".to_string());
                    return Task::none();
                }
                // if one key is provided, both must be
                if access_key.is_empty() != secret_key.is_empty() {
                    self.error =
                        Some("provide both access key and secret key, or neither".to_string());
                    return Task::none();
                }
                if !config::is_valid_access_key(&access_key) {
                    self.error = Some("access key must be at least 3 characters".to_string());
                    return Task::none();
                }
                if !config::is_valid_secret_key(&secret_key) {
                    self.error = Some("secret key must be at least 8 characters".to_string());
                    return Task::none();
                }

                let conn = Connection {
                    name,
                    endpoint,
                    region: if region.is_empty() {
                        "us-east-1".to_string()
                    } else {
                        region
                    },
                };

                if let Err(e) =
                    config::add_connection(&mut self.settings, conn, &access_key, &secret_key)
                {
                    self.error = Some(format!("save failed: {}", e));
                } else {
                    self.new_conn_name.clear();
                    self.new_conn_endpoint.clear();
                    self.new_conn_region = "us-east-1".to_string();
                    self.new_conn_access_key.clear();
                    self.new_conn_secret_key.clear();
                    self.editing_connection = None;
                }
                Task::none()
            }
            Message::EditConnection(name) => {
                if let Some(conn) = self.settings.connections.iter().find(|c| c.name == name) {
                    self.new_conn_name = conn.name.clone();
                    self.new_conn_endpoint = conn.endpoint.clone();
                    self.new_conn_region = conn.region.clone();
                    self.new_conn_access_key.clear();
                    self.new_conn_secret_key.clear();
                    self.editing_connection = Some(name);
                }
                Task::none()
            }
            Message::TestConnection(name) => {
                let conn = match self.settings.connections.iter().find(|c| c.name == name) {
                    Some(c) => c.clone(),
                    None => return Task::none(),
                };
                let creds = match conn.resolve_keys() {
                    Ok(keys) => keys,
                    Err(e) => {
                        self.error = Some(format!("keychain error: {}", e));
                        return Task::none();
                    }
                };
                let client = match S3Client::new(
                    &conn.endpoint,
                    creds.as_ref().map(|(a, s)| (a.as_str(), s.as_str())),
                    &conn.region,
                ) {
                    Ok(c) => Arc::new(c),
                    Err(e) => {
                        self.error = Some(format!("test failed: {}", e));
                        return Task::none();
                    }
                };
                let conn_name = name.clone();
                Task::perform(
                    async move { client.list_buckets().await.map(|_| ()) },
                    move |result| Message::TestConnectionResult(conn_name.clone(), result),
                )
            }
            Message::TestConnectionResult(name, result) => {
                match result {
                    Ok(()) => self.error = Some(format!("'{}': connection ok", name)),
                    Err(e) => self.error = Some(format!("'{}': {}", name, e)),
                }
                Task::none()
            }
            Message::RemoveConnection(name) => {
                if let Err(e) = config::remove_connection(&mut self.settings, &name) {
                    self.error = Some(format!("remove failed: {}", e));
                }
                if self.active_connection.as_deref() == Some(&name) {
                    self.active_connection = None;
                }
                Task::none()
            }
            Message::NewConnNameChanged(v) => {
                self.new_conn_name = v;
                Task::none()
            }
            Message::NewConnEndpointChanged(v) => {
                self.new_conn_endpoint = v;
                Task::none()
            }
            Message::NewConnRegionChanged(v) => {
                self.new_conn_region = v;
                Task::none()
            }
            Message::NewConnAccessKeyChanged(v) => {
                self.new_conn_access_key = v;
                Task::none()
            }
            Message::NewConnSecretKeyChanged(v) => {
                self.new_conn_secret_key = v;
                Task::none()
            }

            // -- admin (abixio-specific) --
            Message::AbixioDetected(status) => {
                if let Some(s) = status {
                    self.is_abixio = true;
                    self.server_status = Some(s);
                    // auto-fetch disks + heal status
                    let admin = self.admin_client.clone();
                    let mut tasks = vec![
                        Task::perform(
                            async move {
                                if let Some(a) = admin.as_ref() {
                                    a.disks().await
                                } else {
                                    Err("no admin client".to_string())
                                }
                            },
                            Message::DisksLoaded,
                        ),
                        {
                            let admin = self.admin_client.clone();
                            Task::perform(
                                async move {
                                    if let Some(a) = admin.as_ref() {
                                        a.heal_status().await
                                    } else {
                                        Err("no admin client".to_string())
                                    }
                                },
                                Message::HealStatusLoaded,
                            )
                        },
                    ];
                    if self.auto_run_tests && !self.auto_test_started {
                        tasks.push(Task::perform(async {}, |_| Message::AutoStartTests));
                    }
                    return Task::batch(tasks);
                } else {
                    self.is_abixio = false;
                    self.server_status = None;
                    if self.auto_run_tests && !self.auto_test_started {
                        return Task::perform(async {}, |_| Message::AutoStartTests);
                    }
                }
                Task::none()
            }
            Message::DisksLoaded(result) => {
                self.disks_data = Some(result);
                Task::none()
            }
            Message::HealStatusLoaded(result) => {
                self.heal_data = Some(result);
                Task::none()
            }
            Message::ObjectInspectLoaded {
                bucket,
                key,
                result,
            } => {
                if !self.selected_object_matches(&bucket, &key) {
                    return Task::none();
                }
                self.loading_object_inspect = false;
                self.object_inspect_target = None;
                self.object_inspect = Some(result);
                Task::none()
            }
            Message::RefreshDisks => {
                let admin = self.admin_client.clone();
                Task::perform(
                    async move {
                        if let Some(a) = admin.as_ref() {
                            a.disks().await
                        } else {
                            Err("no admin client".to_string())
                        }
                    },
                    Message::DisksLoaded,
                )
            }
            Message::RefreshHealStatus => {
                let admin = self.admin_client.clone();
                Task::perform(
                    async move {
                        if let Some(a) = admin.as_ref() {
                            a.heal_status().await
                        } else {
                            Err("no admin client".to_string())
                        }
                    },
                    Message::HealStatusLoaded,
                )
            }
            Message::RefreshObjectInspect => {
                let Some((bucket, key)) = self.current_selected_object() else {
                    return Task::none();
                };
                if !self.is_abixio || self.admin_client.is_none() {
                    return Task::none();
                }
                self.loading_object_inspect = true;
                self.object_inspect_target = Some((bucket.clone(), key.clone()));
                self.cmd_fetch_object_inspect(&bucket, &key)
            }
            Message::OpenHealConfirm => {
                let Some((bucket, key)) = self.current_selected_object() else {
                    return Task::none();
                };
                if !self.is_abixio || self.admin_client.is_none() || self.healing_object {
                    return Task::none();
                }
                self.heal_confirm_target = Some((bucket, key));
                Task::none()
            }
            Message::CancelHealConfirm => {
                self.heal_confirm_target = None;
                Task::none()
            }
            Message::ConfirmHealObject => {
                let Some((bucket, key)) = self.heal_confirm_target.take() else {
                    return Task::none();
                };
                self.healing_object = true;
                self.healing_target = Some((bucket.clone(), key.clone()));
                self.heal_result = Some("Healing object...".to_string());
                self.cmd_heal_object(&bucket, &key)
            }
            Message::HealObjectFinished {
                bucket,
                key,
                result,
            } => {
                let healing_matches =
                    self.healing_target.as_ref() == Some(&(bucket.clone(), key.clone()));
                if healing_matches {
                    self.healing_object = false;
                    self.healing_target = None;
                }
                if !self.selected_object_matches(&bucket, &key) {
                    return Task::none();
                }
                match result {
                    Ok(heal) => {
                        let suffix = heal
                            .shards_fixed
                            .map(|count| format!(" ({} shards fixed)", count))
                            .unwrap_or_default();
                        self.heal_result = Some(format!("{}{}", heal.result, suffix));
                        self.loading_object_inspect = true;
                        self.object_inspect_target = Some((bucket.clone(), key.clone()));
                        Task::batch(vec![
                            self.cmd_fetch_object_inspect(&bucket, &key),
                            self.refresh_heal_status_task(),
                        ])
                    }
                    Err(error) => {
                        self.heal_result = Some(format!("Heal failed: {}", error));
                        self.error = Some(format!("Heal failed: {}", error));
                        Task::none()
                    }
                }
            }

            // -- testing --
            Message::RunTests => self.begin_tests(),
            Message::TestsComplete(results) => {
                self.test_running = false;
                let passed = results.iter().filter(|r| r.passed).count();
                let total = results.len();
                self.test_progress = format!("done: {}/{} passed", passed, total);
                self.test_results = results;
                if let Some(path) = self.test_report_path.clone() {
                    let report = crate::views::testing::TestReport {
                        app_version: env!("CARGO_PKG_VERSION").to_string(),
                        endpoint: self.endpoint.clone(),
                        started_at: self
                            .test_started_at
                            .clone()
                            .unwrap_or_else(|| now_rfc3339()),
                        finished_at: now_rfc3339(),
                        total,
                        passed,
                        failed: total - passed,
                        results: self.test_results.clone(),
                    };
                    Task::perform(
                        async move { crate::views::testing::write_test_report(path, report).await },
                        Message::TestReportWritten,
                    )
                } else {
                    Task::none()
                }
            }
            Message::AutoStartTests => {
                if self.auto_run_tests && !self.auto_test_started {
                    self.auto_test_started = true;
                    self.begin_tests()
                } else {
                    Task::none()
                }
            }
            Message::TestReportWritten(result) => match result {
                Ok(path) => {
                    println!("{}", path.display());
                    Task::none()
                }
                Err(error) => {
                    self.error = Some(format!("Failed to write test report: {}", error));
                    Task::none()
                }
            },
        }
    }

    pub fn view(&self) -> Element<'_, Message> {
        let sidebar = self.sidebar_view();
        let content: Element<'_, Message> = match self.section {
            Section::Browse => self.browse_view(),
            Section::Connections => self.connections_view(),
            Section::Disks if self.is_abixio => self.disks_view(),
            Section::Healing if self.is_abixio => self.healing_view(),
            Section::Testing => self.testing_view(),
            Section::Settings => self.settings_view(),
            _ => container(text("Coming soon").size(14)).padding(20).into(),
        };

        let mut main_row = row![container(sidebar).width(40), content,];

        if matches!(
            self.selection,
            Selection::Object { .. } | Selection::Bucket(_)
        ) {
            main_row = main_row.push(container(self.detail_view()).width(280));
        }

        let conn_label = match &self.active_connection {
            Some(name) => format!("{} ({})", name, self.endpoint),
            None if !self.endpoint.is_empty() => self.endpoint.clone(),
            None => "not connected".to_string(),
        };
        let top_bar = container(
            row![
                text("abixio-ui").size(14),
                text(" | ").size(14),
                text(conn_label).size(12),
            ]
            .spacing(4)
            .padding(6),
        )
        .width(Length::Fill);

        let mut layout = column![top_bar, iced::widget::rule::horizontal(1), main_row]
            .width(Length::Fill)
            .height(Length::Fill);

        // error bar at bottom
        if let Some(err) = &self.error {
            layout = layout.push(
                container(
                    row![
                        text(err.clone()).size(12),
                        button(text("dismiss").size(10))
                            .style(button::text)
                            .on_press(Message::DismissError),
                    ]
                    .spacing(8),
                )
                .padding(6)
                .width(Length::Fill),
            );
        }

        let base: Element<'_, Message> = layout.into();
        let with_heal = if self.heal_confirm_target.is_some() {
            stack![base, self.heal_confirm_modal()].into()
        } else {
            base
        };
        let with_delete = if self.bucket_delete.is_some() {
            stack![with_heal, self.bucket_delete_modal()].into()
        } else {
            with_heal
        };
        let with_create = if self.create_bucket_modal_open {
            stack![with_delete, self.create_bucket_modal()].into()
        } else {
            with_delete
        };
        let with_transfer = if self.transfer.is_some() {
            stack![with_create, self.transfer_modal()].into()
        } else {
            with_create
        };
        let with_bulk = if self.bulk_delete.is_some() {
            stack![with_transfer, self.bulk_delete_modal()].into()
        } else {
            with_transfer
        };
        if self.prefix_delete.is_some() {
            stack![with_bulk, self.prefix_delete_modal()].into()
        } else {
            with_bulk
        }
    }

    // -- commands --

    fn cmd_fetch_buckets(&self) -> Task<Message> {
        let client = self.client.clone();
        Task::perform(
            async move { client.list_buckets().await },
            Message::BucketsLoaded,
        )
    }

    fn cmd_fetch_objects(&self) -> Task<Message> {
        let client = self.client.clone();
        let bucket = match &self.selected_bucket {
            Some(b) => b.clone(),
            None => return Task::none(),
        };
        let prefix = self.current_prefix.clone();
        Task::perform(
            async move { client.list_objects(&bucket, &prefix, "/").await },
            Message::ObjectsLoaded,
        )
    }

    fn cmd_fetch_detail(&self, bucket: &str, key: &str) -> Task<Message> {
        let client = self.client.clone();
        let bucket = bucket.to_string();
        let key = key.to_string();
        Task::perform(
            async move { client.head_object(&bucket, &key).await },
            Message::DetailLoaded,
        )
    }

    fn cmd_fetch_object_inspect(&self, bucket: &str, key: &str) -> Task<Message> {
        let admin = self.admin_client.clone();
        let bucket = bucket.to_string();
        let key = key.to_string();
        Task::perform(
            async move {
                let result = if let Some(a) = admin.as_ref() {
                    a.inspect_object(&bucket, &key).await
                } else {
                    Err("no admin client".to_string())
                };
                (bucket, key, result)
            },
            |(bucket, key, result)| Message::ObjectInspectLoaded {
                bucket,
                key,
                result,
            },
        )
    }

    fn cmd_heal_object(&self, bucket: &str, key: &str) -> Task<Message> {
        let admin = self.admin_client.clone();
        let bucket = bucket.to_string();
        let key = key.to_string();
        Task::perform(
            async move {
                let result = if let Some(a) = admin.as_ref() {
                    a.heal_object(&bucket, &key).await
                } else {
                    Err("no admin client".to_string())
                };
                (bucket, key, result)
            },
            |(bucket, key, result)| Message::HealObjectFinished {
                bucket,
                key,
                result,
            },
        )
    }

    fn refresh_heal_status_task(&self) -> Task<Message> {
        let admin = self.admin_client.clone();
        Task::perform(
            async move {
                if let Some(a) = admin.as_ref() {
                    a.heal_status().await
                } else {
                    Err("no admin client".to_string())
                }
            },
            Message::HealStatusLoaded,
        )
    }

    fn current_selected_object(&self) -> Option<(String, String)> {
        match &self.selection {
            Selection::Object { bucket, key } => Some((bucket.clone(), key.clone())),
            _ => None,
        }
    }

    fn current_selected_bucket(&self) -> Option<String> {
        match &self.selection {
            Selection::Bucket(bucket) => Some(bucket.clone()),
            Selection::Object { bucket, .. } => Some(bucket.clone()),
            Selection::None => self.selected_bucket.clone(),
        }
    }

    fn selected_object_matches(&self, bucket: &str, key: &str) -> bool {
        matches!(
            &self.selection,
            Selection::Object {
                bucket: selected_bucket,
                key: selected_key,
            } if selected_bucket == bucket && selected_key == key
        )
    }

    fn clear_object_admin_state(&mut self) {
        self.object_inspect = None;
        self.loading_object_inspect = false;
        self.object_inspect_target = None;
        self.heal_confirm_target = None;
        self.healing_object = false;
        self.healing_target = None;
        self.heal_result = None;
    }

    fn begin_tests(&mut self) -> Task<Message> {
        if self.test_running || self.endpoint.is_empty() {
            return Task::none();
        }
        self.test_running = true;
        self.test_results.clear();
        self.test_progress = "running tests...".to_string();
        self.test_started_at = Some(now_rfc3339());
        let client = self.client.clone();
        let admin = if self.is_abixio {
            self.admin_client.clone()
        } else {
            None
        };
        Task::perform(
            async move { crate::views::testing::run_e2e_tests(client, admin).await },
            Message::TestsComplete,
        )
    }

    pub fn current_connection_id(&self) -> String {
        self.active_connection
            .clone()
            .unwrap_or_else(|| CURRENT_CONNECTION_ID.to_string())
    }

    pub fn current_connection_label(&self) -> String {
        self.active_connection
            .clone()
            .unwrap_or_else(|| "Current connection".to_string())
    }

    pub fn available_connection_options(&self) -> Vec<String> {
        let mut options = vec![self.current_connection_label()];
        for conn in &self.settings.connections {
            if self.active_connection.as_deref() != Some(&conn.name) {
                options.push(conn.name.clone());
            }
        }
        options
    }

    pub fn selected_transfer_connection_label(&self) -> Option<String> {
        let transfer = self.transfer.as_ref()?;
        if transfer.destination_connection_id == CURRENT_CONNECTION_ID {
            Some(self.current_connection_label())
        } else {
            Some(transfer.destination_connection_id.clone())
        }
    }

    pub fn transfer_can_start(&self) -> bool {
        let Some(transfer) = &self.transfer else {
            return false;
        };
        if transfer.preparing || transfer.running || transfer.loading_destination_buckets {
            return false;
        }
        match transfer.mode {
            TransferMode::CopyObject | TransferMode::MoveObject => {
                !transfer.destination_bucket.is_empty()
                    && !transfer.destination_key.is_empty()
                    && !self.transfer_points_to_same_object(transfer)
            }
            TransferMode::ImportFolder | TransferMode::ExportPrefix => {
                transfer.local_path.is_some()
            }
        }
    }

    fn transfer_points_to_same_object(&self, transfer: &TransferState) -> bool {
        matches!(
            (&transfer.source_bucket, &transfer.source_key),
            (Some(bucket), Some(key))
                if transfer.destination_connection_id == CURRENT_CONNECTION_ID
                    && transfer.destination_bucket == *bucket
                    && transfer.destination_key == *key
        )
    }

    fn cmd_fetch_transfer_buckets(&self, connection_id: &str) -> Task<Message> {
        let connection_id = connection_id.to_string();
        let result = self.make_client_for_connection(&connection_id);
        match result {
            Ok(client) => Task::perform(
                async move { client.list_buckets().await },
                Message::TransferDestinationBucketsLoaded,
            ),
            Err(error) => Task::perform(
                async move { Err(error) },
                Message::TransferDestinationBucketsLoaded,
            ),
        }
    }

    fn cmd_process_next_transfer_step(&mut self) -> Task<Message> {
        let Some(transfer) = self.transfer.as_mut() else {
            return Task::none();
        };
        if transfer.next_index >= transfer.items.len() {
            transfer.running = false;
            transfer.pending_conflict = None;
            transfer.current_item = None;
            transfer.summary = Some(format!(
                "Done. Copied: {}. Skipped: {}. Failed: {}.",
                transfer.completed, transfer.skipped, transfer.failed
            ));
            let should_refresh = matches!(
                transfer.mode,
                TransferMode::ImportFolder | TransferMode::CopyObject | TransferMode::MoveObject
            ) && transfer.destination_connection_id == CURRENT_CONNECTION_ID;
            return if should_refresh {
                self.loading_objects = true;
                self.cmd_fetch_objects()
            } else {
                Task::none()
            };
        }
        transfer.running = true;
        transfer.pending_conflict = None;
        let item = transfer.items[transfer.next_index].clone();
        transfer.current_item = Some(item.label());
        let overwrite_policy = transfer.overwrite_policy;
        let is_move = transfer.mode == TransferMode::MoveObject;
        let source_client = self.client.clone();
        let destination_client = match &item.destination {
            TransferEndpoint::S3 { connection_id, .. } => match self
                .make_client_for_connection(connection_id)
            {
                Ok(client) => Some(client),
                Err(error) => {
                    return Task::perform(async move { Err(error) }, Message::TransferStepFinished);
                }
            },
            TransferEndpoint::Local { .. } => None,
        };
        Task::perform(
            async move {
                run_transfer_step(source_client, destination_client, item, overwrite_policy, is_move)
                    .await
            },
            Message::TransferStepFinished,
        )
    }

    fn resolve_transfer_conflict(&mut self, skip: bool, remember: bool) -> Task<Message> {
        let Some(transfer) = self.transfer.as_mut() else {
            return Task::none();
        };
        let Some(item) = transfer.pending_conflict.take() else {
            return Task::none();
        };
        if remember {
            transfer.overwrite_policy = if skip {
                OverwritePolicy::SkipAll
            } else {
                OverwritePolicy::OverwriteAll
            };
        }
        if skip {
            transfer.skipped += 1;
            transfer.next_index += 1;
            transfer.current_item = Some(item.label());
            self.cmd_process_next_transfer_step()
        } else {
            transfer.running = true;
            let source_client = self.client.clone();
            let destination_client = match &item.destination {
                TransferEndpoint::S3 { connection_id, .. } => {
                    match self.make_client_for_connection(connection_id) {
                        Ok(client) => Some(client),
                        Err(error) => {
                            return Task::perform(
                                async move { Err(error) },
                                Message::TransferStepFinished,
                            );
                        }
                    }
                }
                TransferEndpoint::Local { .. } => None,
            };
            Task::perform(
                async move {
                    run_transfer_step(
                        source_client,
                        destination_client,
                        item,
                        OverwritePolicy::OverwriteAll,
                        false,
                    )
                    .await
                },
                Message::TransferStepFinished,
            )
        }
    }

    pub(crate) fn bucket_delete_can_start(&self) -> bool {
        let Some(state) = &self.bucket_delete else {
            return false;
        };
        !state.preview_loading
            && !state.deleting
            && state.confirm_name == state.bucket
            && !state.bucket.is_empty()
    }

    fn cmd_process_next_bucket_delete_step(&mut self) -> Task<Message> {
        let Some(state) = self.bucket_delete.as_mut() else {
            return Task::none();
        };
        if state.preview_loading || state.confirm_name != state.bucket {
            return Task::none();
        }
        state.deleting = true;

        if state.next_index < state.object_keys.len() {
            let client = self.client.clone();
            let bucket = state.bucket.clone();
            let key = state.object_keys[state.next_index].clone();
            return Task::perform(
                async move {
                    client.delete_object(&bucket, &key).await?;
                    Ok(BucketDeleteStepResult::ObjectDeleted(key))
                },
                Message::BucketDeleteStepFinished,
            );
        }

        let client = self.client.clone();
        let bucket = state.bucket.clone();
        Task::perform(
            async move {
                client.delete_bucket(&bucket).await?;
                Ok(BucketDeleteStepResult::BucketDeleted(bucket))
            },
            Message::BucketDeleteStepFinished,
        )
    }

    fn cmd_process_next_bulk_delete_step(&mut self) -> Task<Message> {
        let Some(state) = self.bulk_delete.as_mut() else {
            return Task::none();
        };
        state.deleting = true;

        if state.next_index < state.keys.len() {
            let client = self.client.clone();
            let bucket = state.bucket.clone();
            let end = (state.next_index + 1000).min(state.keys.len());
            let batch: Vec<String> = state.keys[state.next_index..end].to_vec();
            let batch_size = batch.len();
            state.next_index = end;
            return Task::perform(
                async move {
                    let failed = client.delete_objects(&bucket, &batch).await?;
                    Ok(batch_size - failed.len())
                },
                Message::BulkDeleteBatchFinished,
            );
        }

        // all done
        let deleted = state.deleted;
        let total = state.total;
        self.bulk_delete = None;
        self.selected_keys.clear();
        self.loading_objects = true;
        self.error = None;
        let summary = format!("Deleted {} of {} objects", deleted, total);
        self.error = Some(summary);
        self.cmd_fetch_objects()
    }

    fn cmd_process_next_prefix_delete_batch(&mut self) -> Task<Message> {
        let Some(state) = self.prefix_delete.as_mut() else {
            return Task::none();
        };
        state.deleting = true;

        if state.next_index < state.keys.len() {
            let client = self.client.clone();
            let bucket = state.bucket.clone();
            let end = (state.next_index + 1000).min(state.keys.len());
            let batch: Vec<String> = state.keys[state.next_index..end].to_vec();
            let batch_size = batch.len();
            state.next_index = end;
            return Task::perform(
                async move {
                    let failed = client.delete_objects(&bucket, &batch).await?;
                    Ok(batch_size - failed.len())
                },
                Message::PrefixDeleteBatchFinished,
            );
        }

        // all done
        let deleted = state.deleted;
        let total = state.total;
        self.prefix_delete = None;
        self.selected_keys.clear();
        self.loading_objects = true;
        self.error = None;
        let summary = format!("Deleted {} of {} objects under prefix", deleted, total);
        self.error = Some(summary);
        self.cmd_fetch_objects()
    }

    fn make_client_for_connection(&self, connection_id: &str) -> Result<Arc<S3Client>, String> {
        if connection_id == CURRENT_CONNECTION_ID {
            return Ok(self.client.clone());
        }
        let conn = self
            .settings
            .connections
            .iter()
            .find(|c| c.name == connection_id)
            .ok_or_else(|| format!("connection '{}' not found", connection_id))?;
        let creds = conn.resolve_keys()?;
        let client = S3Client::new(
            &conn.endpoint,
            creds.as_ref().map(|(a, s)| (a.as_str(), s.as_str())),
            &conn.region,
        )?;
        Ok(Arc::new(client))
    }
}

fn now_rfc3339() -> String {
    OffsetDateTime::now_utc()
        .format(&Rfc3339)
        .unwrap_or_else(|_| "1970-01-01T00:00:00Z".to_string())
}

impl TransferItem {
    pub(crate) fn label(&self) -> String {
        match (&self.source, &self.destination) {
            (
                TransferEndpoint::S3 { bucket, key, .. },
                TransferEndpoint::S3 {
                    bucket: dest_bucket,
                    key: dest_key,
                    ..
                },
            ) => format!("{}/{} -> {}/{}", bucket, key, dest_bucket, dest_key),
            (TransferEndpoint::Local { path }, TransferEndpoint::S3 { bucket, key, .. }) => {
                format!("{} -> {}/{}", path.display(), bucket, key)
            }
            (TransferEndpoint::S3 { bucket, key, .. }, TransferEndpoint::Local { path }) => {
                format!("{}/{} -> {}", bucket, key, path.display())
            }
            (TransferEndpoint::Local { path }, TransferEndpoint::Local { path: dest }) => {
                format!("{} -> {}", path.display(), dest.display())
            }
        }
    }
}

fn relative_key(root: &Path, path: &Path) -> Result<String, String> {
    let relative = path
        .strip_prefix(root)
        .map_err(|e| e.to_string())?
        .to_string_lossy()
        .replace('\\', "/");
    Ok(relative)
}

fn join_s3_key(prefix: &str, relative: &str) -> String {
    if prefix.is_empty() {
        relative.to_string()
    } else {
        format!("{}{}", prefix, relative)
    }
}

fn guess_content_type(path: &Path) -> String {
    mime_guess::from_path(path)
        .first_raw()
        .unwrap_or("application/octet-stream")
        .to_string()
}

pub(crate) fn prepare_import_items(
    root: PathBuf,
    bucket: &str,
    prefix: &str,
) -> Result<Vec<TransferItem>, String> {
    let mut items = Vec::new();
    for entry in walkdir::WalkDir::new(&root) {
        let entry = entry.map_err(|e| e.to_string())?;
        if entry.file_type().is_dir() {
            continue;
        }
        let path = entry.path().to_path_buf();
        let relative = relative_key(&root, &path)?;
        items.push(TransferItem {
            source: TransferEndpoint::Local { path },
            destination: TransferEndpoint::S3 {
                connection_id: CURRENT_CONNECTION_ID.to_string(),
                bucket: bucket.to_string(),
                key: join_s3_key(prefix, &relative),
            },
        });
    }
    Ok(items)
}

pub(crate) async fn prepare_export_items(
    client: Arc<S3Client>,
    bucket: &str,
    prefix: &str,
    root: &Path,
) -> Result<Vec<TransferItem>, String> {
    let listing = client.list_objects(bucket, prefix, "").await?;
    let mut items = Vec::new();
    for object in listing.objects {
        let relative = object
            .key
            .strip_prefix(prefix)
            .unwrap_or(&object.key)
            .replace('/', "\\");
        items.push(TransferItem {
            source: TransferEndpoint::S3 {
                connection_id: CURRENT_CONNECTION_ID.to_string(),
                bucket: bucket.to_string(),
                key: object.key,
            },
            destination: TransferEndpoint::Local {
                path: root.join(relative),
            },
        });
    }
    Ok(items)
}

pub(crate) async fn run_transfer_step(
    source_client: Arc<S3Client>,
    destination_client: Option<Arc<S3Client>>,
    item: TransferItem,
    overwrite_policy: OverwritePolicy,
    is_move: bool,
) -> Result<TransferStepResult, String> {
    match &item.destination {
        TransferEndpoint::S3 { bucket, key, .. } => {
            let dest_client = destination_client.ok_or("missing destination client")?;
            let exists = dest_client.head_object(bucket, key).await.is_ok();
            if exists {
                match overwrite_policy {
                    OverwritePolicy::Ask => return Ok(TransferStepResult::Conflict(item)),
                    OverwritePolicy::SkipAll => {
                        return Ok(TransferStepResult::Skipped(item.label()));
                    }
                    OverwritePolicy::OverwriteAll => {}
                }
            }
            match &item.source {
                TransferEndpoint::S3 {
                    bucket: src_bucket,
                    key: src_key,
                    ..
                } => {
                    // server-side copy when possible (same client/endpoint)
                    source_client
                        .copy_object(src_bucket, src_key, bucket, key)
                        .await?;
                    // for move: delete source after confirmed copy
                    if is_move {
                        source_client
                            .delete_object(src_bucket, src_key)
                            .await?;
                    }
                }
                TransferEndpoint::Local { path } => {
                    let data =
                        tokio::fs::read(path).await.map_err(|e| e.to_string())?;
                    let content_type = guess_content_type(path);
                    dest_client
                        .put_object(bucket, key, data, &content_type)
                        .await?;
                }
            }
            Ok(TransferStepResult::Copied(item.label()))
        }
        TransferEndpoint::Local { path } => {
            let exists = path.exists();
            if exists {
                match overwrite_policy {
                    OverwritePolicy::Ask => return Ok(TransferStepResult::Conflict(item)),
                    OverwritePolicy::SkipAll => {
                        return Ok(TransferStepResult::Skipped(item.label()));
                    }
                    OverwritePolicy::OverwriteAll => {}
                }
            }
            let TransferEndpoint::S3 { bucket, key, .. } = &item.source else {
                return Err("local to local transfer is unsupported".to_string());
            };
            let data = source_client.get_object(bucket, key).await?;
            if let Some(parent) = path.parent() {
                tokio::fs::create_dir_all(parent)
                    .await
                    .map_err(|e| e.to_string())?;
            }
            tokio::fs::write(path, data)
                .await
                .map_err(|e| e.to_string())?;
            Ok(TransferStepResult::Copied(item.label()))
        }
    }
}

/// Wildcard match supporting `*` (any sequence) and `?` (any single char).
/// If the pattern contains no wildcards, falls back to case-insensitive
/// substring match. Matching is always case-insensitive.
pub fn wildcard_match(pattern: &str, text: &str) -> bool {
    let pat_lower = pattern.to_ascii_lowercase();
    let text_lower = text.to_ascii_lowercase();

    if !pat_lower.contains('*') && !pat_lower.contains('?') {
        return text_lower.contains(&pat_lower);
    }

    let pat: Vec<char> = pat_lower.chars().collect();
    let txt: Vec<char> = text_lower.chars().collect();
    let (plen, tlen) = (pat.len(), txt.len());
    let (mut pi, mut ti) = (0, 0);
    let (mut star_pi, mut star_ti) = (usize::MAX, 0);

    while ti < tlen {
        if pi < plen && (pat[pi] == '?' || pat[pi] == txt[ti]) {
            pi += 1;
            ti += 1;
        } else if pi < plen && pat[pi] == '*' {
            star_pi = pi;
            star_ti = ti;
            pi += 1;
        } else if star_pi != usize::MAX {
            pi = star_pi + 1;
            star_ti += 1;
            ti = star_ti;
        } else {
            return false;
        }
    }

    while pi < plen && pat[pi] == '*' {
        pi += 1;
    }
    pi == plen
}

#[cfg(test)]
mod tests {
    use std::fs;

    use super::{
        App, BucketDeleteState, CURRENT_CONNECTION_ID, Message, Selection, StartupOptions,
        TransferMode, TransferState, prepare_import_items,
    };
    use crate::abixio::types::{ErasureInfo, HealResponse, ObjectInspectResponse, ShardInfo};

    fn sample_inspect(bucket: &str, key: &str) -> ObjectInspectResponse {
        ObjectInspectResponse {
            bucket: bucket.to_string(),
            key: key.to_string(),
            size: 128,
            etag: "etag".to_string(),
            content_type: "application/octet-stream".to_string(),
            created_at: 1,
            erasure: ErasureInfo {
                data: 2,
                parity: 1,
                distribution: vec![0, 1, 2],
            },
            shards: vec![ShardInfo {
                index: 0,
                disk: 0,
                status: "ok".to_string(),
                checksum: Some("abc".to_string()),
            }],
        }
    }

    fn empty_startup() -> StartupOptions {
        StartupOptions {
            endpoint: None,
            creds: None,
            auto_run_tests: false,
            test_report_path: None,
        }
    }

    #[test]
    fn ignores_stale_object_inspect_result() {
        let (mut app, _) = App::new(empty_startup());
        app.selection = Selection::Object {
            bucket: "bucket-a".to_string(),
            key: "new-key".to_string(),
        };
        app.loading_object_inspect = true;
        app.object_inspect_target = Some(("bucket-a".to_string(), "new-key".to_string()));

        let _ = app.update(Message::ObjectInspectLoaded {
            bucket: "bucket-a".to_string(),
            key: "old-key".to_string(),
            result: Ok(sample_inspect("bucket-a", "old-key")),
        });

        assert!(app.object_inspect.is_none());
        assert!(app.loading_object_inspect);
        assert_eq!(
            app.object_inspect_target,
            Some(("bucket-a".to_string(), "new-key".to_string()))
        );
    }

    #[test]
    fn applies_matching_object_inspect_result() {
        let (mut app, _) = App::new(empty_startup());
        app.selection = Selection::Object {
            bucket: "bucket-a".to_string(),
            key: "key-a".to_string(),
        };
        app.loading_object_inspect = true;
        app.object_inspect_target = Some(("bucket-a".to_string(), "key-a".to_string()));

        let inspect = sample_inspect("bucket-a", "key-a");
        let _ = app.update(Message::ObjectInspectLoaded {
            bucket: "bucket-a".to_string(),
            key: "key-a".to_string(),
            result: Ok(inspect.clone()),
        });

        assert!(!app.loading_object_inspect);
        assert!(app.object_inspect_target.is_none());
        match app.object_inspect {
            Some(Ok(saved)) => {
                assert_eq!(saved.bucket, inspect.bucket);
                assert_eq!(saved.key, inspect.key);
            }
            _ => panic!("expected matching inspect result to be stored"),
        }
    }

    #[test]
    fn ignores_stale_heal_completion() {
        let (mut app, _) = App::new(empty_startup());
        app.selection = Selection::Object {
            bucket: "bucket-a".to_string(),
            key: "new-key".to_string(),
        };
        app.healing_object = true;
        app.healing_target = Some(("bucket-a".to_string(), "new-key".to_string()));

        let _ = app.update(Message::HealObjectFinished {
            bucket: "bucket-a".to_string(),
            key: "old-key".to_string(),
            result: Ok(HealResponse {
                result: "heal complete".to_string(),
                shards_fixed: Some(1),
                error: None,
            }),
        });

        assert!(app.heal_result.is_none());
        assert!(app.healing_object);
        assert_eq!(
            app.healing_target,
            Some(("bucket-a".to_string(), "new-key".to_string()))
        );
    }

    #[test]
    fn blocks_copy_to_same_object_path() {
        let (mut app, _) = App::new(empty_startup());
        app.transfer = Some(TransferState {
            mode: TransferMode::CopyObject,
            destination_connection_id: CURRENT_CONNECTION_ID.to_string(),
            destination_bucket: "bucket-a".to_string(),
            destination_key: "key-a".to_string(),
            destination_buckets: None,
            loading_destination_buckets: false,
            local_path: None,
            source_bucket: Some("bucket-a".to_string()),
            source_key: Some("key-a".to_string()),
            source_prefix: None,
            items: Vec::new(),
            next_index: 0,
            completed: 0,
            skipped: 0,
            failed: 0,
            current_item: None,
            pending_conflict: None,
            overwrite_policy: super::OverwritePolicy::Ask,
            preparing: false,
            running: false,
            summary: None,
        });

        assert!(!app.transfer_can_start());
    }

    #[test]
    fn create_bucket_success_selects_new_bucket() {
        let (mut app, _) = App::new(empty_startup());
        app.create_bucket_modal_open = true;

        let _ = app.update(Message::CreateBucketDone {
            bucket: "bucket-a".to_string(),
            result: Ok(()),
        });

        assert!(!app.create_bucket_modal_open);
        assert_eq!(app.selected_bucket.as_deref(), Some("bucket-a"));
        assert_eq!(app.selection, Selection::Bucket("bucket-a".to_string()));
        assert!(app.loading_buckets);
        assert!(app.loading_objects);
    }

    #[test]
    fn bucket_delete_requires_exact_name() {
        let (mut app, _) = App::new(empty_startup());
        app.bucket_delete = Some(BucketDeleteState {
            bucket: "bucket-a".to_string(),
            confirm_name: "bucket".to_string(),
            preview_loading: false,
            object_keys: vec!["one".to_string()],
            total_objects: 1,
            deleted_objects: 0,
            next_index: 0,
            deleting: false,
            summary: None,
        });

        assert!(!app.bucket_delete_can_start());

        let _ = app.update(Message::BucketDeleteConfirmNameChanged(
            "bucket-a".to_string(),
        ));

        assert!(app.bucket_delete_can_start());
    }

    #[test]
    fn prepare_import_items_maps_relative_paths() {
        let root = std::env::temp_dir().join(format!(
            "abixio-ui-import-test-{}",
            std::time::SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH)
                .unwrap_or_default()
                .as_nanos()
        ));
        let nested = root.join("docs");
        fs::create_dir_all(&nested).expect("create import test dir");
        fs::write(nested.join("readme.txt"), b"hello").expect("write import test file");

        let items = prepare_import_items(root.clone(), "bucket-a", "prefix/")
            .expect("prepare import items");

        assert_eq!(items.len(), 1);
        match &items[0].destination {
            super::TransferEndpoint::S3 { bucket, key, .. } => {
                assert_eq!(bucket, "bucket-a");
                assert_eq!(key, "prefix/docs/readme.txt");
            }
            _ => panic!("expected s3 destination"),
        }

        fs::remove_dir_all(root).expect("cleanup import test dir");
    }

    #[test]
    fn wildcard_match_substring() {
        assert!(super::wildcard_match("hello", "say hello world"));
        assert!(super::wildcard_match("HELLO", "say hello world"));
        assert!(!super::wildcard_match("goodbye", "say hello world"));
    }

    #[test]
    fn wildcard_match_star() {
        assert!(super::wildcard_match("*.txt", "readme.txt"));
        assert!(super::wildcard_match("*.txt", "docs/readme.txt"));
        assert!(!super::wildcard_match("*.txt", "readme.md"));
        assert!(super::wildcard_match("docs/*", "docs/readme.txt"));
        assert!(super::wildcard_match("*read*", "docs/readme.txt"));
    }

    #[test]
    fn wildcard_match_question() {
        assert!(super::wildcard_match("?.txt", "a.txt"));
        assert!(!super::wildcard_match("?.txt", "ab.txt"));
    }

    #[test]
    fn wildcard_match_case_insensitive() {
        assert!(super::wildcard_match("*.TXT", "readme.txt"));
        assert!(super::wildcard_match("*.txt", "README.TXT"));
    }

    #[test]
    fn wildcard_match_empty() {
        assert!(super::wildcard_match("", "anything"));
        assert!(super::wildcard_match("*", "anything"));
    }
}
