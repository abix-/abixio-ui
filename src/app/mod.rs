mod handlers;
pub mod transfer_ops;
pub mod types;

use std::collections::HashSet;
use std::path::PathBuf;
use std::sync::Arc;

use iced::keyboard;
use iced::widget::{button, column, container, row, stack, text, text_editor};
use iced::{Element, Length, Subscription, Task, Theme};

use crate::abixio::client::AdminClient;
use crate::abixio::types::{
    DisksResponse, HealResponse, HealStatusResponse, ObjectInspectResponse, StatusResponse,
};
use crate::config::{self, Settings};
use crate::s3::client::{
    BucketInfo, ListObjectsResult, ObjectDetail, ObjectInfo, S3Client, VersionInfo,
};
use crate::views::testing::TestResult;

pub use transfer_ops::{
    prepare_export_items, prepare_import_items, run_transfer_step, wildcard_match,
};
pub use types::{
    BucketDeleteState, BucketDeleteStepResult, BucketDocumentKind, BucketDocumentLoadState,
    BucketDocumentState, BulkDeleteState, CURRENT_CONNECTION_ID, OverwritePolicy,
    PrefixDeleteState, StartupOptions, TransferEndpoint, TransferItem, TransferMode, TransferState,
    TransferStepResult,
};

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

    // presigned sharing
    OpenShareModal,
    CloseShareModal,
    ShareExpiryChanged(String),
    GenerateShareUrl,
    ShareUrlGenerated(Result<String, String>),

    // bucket policy / lifecycle / tags
    BucketDocumentLoaded(BucketDocumentKind, Result<Option<String>, String>),
    BucketTagsLoaded(Result<std::collections::HashMap<String, String>, String>),
    OpenBucketDocumentEditor(BucketDocumentKind),
    CancelBucketDocumentEditor(BucketDocumentKind),
    BucketDocumentEdited(BucketDocumentKind, text_editor::Action),
    SaveBucketDocument(BucketDocumentKind),
    BucketDocumentSaved(BucketDocumentKind, Result<(), String>),
    DeleteBucketDocument(BucketDocumentKind),
    BucketDocumentDeleted(BucketDocumentKind, Result<(), String>),
    BucketTagKeyChanged(String),
    BucketTagValueChanged(String),
    AddBucketTag,
    RemoveBucketTag(String),
    BucketTagsSaved(Result<(), String>),

    // object preview
    PreviewLoaded(Result<String, String>),

    // versioning
    VersioningStatusLoaded(Result<String, String>),
    VersionsLoaded(Result<Vec<VersionInfo>, String>),
    EnableVersioning,
    SuspendVersioning,
    VersioningToggled(Result<(), String>),
    DeleteVersion(String),
    VersionDeleted(Result<(), String>),
    RestoreVersion(String),
    VersionRestored(Result<String, String>),

    // tags
    TagsLoaded(Result<std::collections::HashMap<String, String>, String>),
    TagKeyChanged(String),
    TagValueChanged(String),
    AddTag,
    RemoveTag(String),
    TagsSaved(Result<(), String>),

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

    // presigned sharing
    pub share_modal_open: bool,
    pub share_url: Option<String>,
    pub share_expiry_secs: u64,

    // bucket config
    pub bucket_policy: BucketDocumentState,
    pub bucket_lifecycle: BucketDocumentState,
    pub bucket_tags: Option<Result<std::collections::HashMap<String, String>, String>>,
    pub bucket_tag_key: String,
    pub bucket_tag_value: String,

    // object preview
    pub object_preview: Option<Result<String, String>>,

    // versioning
    pub bucket_versioning: Option<Result<String, String>>,
    pub object_versions: Option<Result<Vec<VersionInfo>, String>>,
    pub loading_versions: bool,

    // tags
    pub object_tags: Option<Result<std::collections::HashMap<String, String>, String>>,
    pub loading_tags: bool,
    pub editing_tag_key: String,
    pub editing_tag_value: String,

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

        let mut perf = crate::perf::PerfStats::new();
        perf.set_s3_stats(client.stats().clone());

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
            perf,
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
            share_modal_open: false,
            share_url: None,
            share_expiry_secs: 3600,
            bucket_policy: BucketDocumentState::new(),
            bucket_lifecycle: BucketDocumentState::new(),
            bucket_tags: None,
            bucket_tag_key: String::new(),
            bucket_tag_value: String::new(),
            object_preview: None,
            bucket_versioning: None,
            object_versions: None,
            loading_versions: false,
            object_tags: None,
            loading_tags: false,
            editing_tag_key: String::new(),
            editing_tag_value: String::new(),
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
        self.perf.record_frame();

        match message {
            // -- navigation --
            Message::SelectSection(s) => {
                self.section = s;
                Task::none()
            }
            Message::SelectBucket(name) => self.handle_select_bucket(name),
            Message::NavigatePrefix(prefix) => self.handle_navigate_prefix(prefix),
            Message::SelectObject(key) => self.handle_select_object(key),
            Message::ClearSelection => self.handle_clear_selection(),

            // -- data loaded --
            Message::BucketsLoaded(r) => self.handle_buckets_loaded(r),
            Message::ObjectsLoaded(r) => self.handle_objects_loaded(r),
            Message::DetailLoaded(r) => self.handle_detail_loaded(r),
            Message::UploadDone(r) => self.handle_upload_done(r),
            Message::DeleteDone(r) => self.handle_delete_done(r),
            Message::CreateBucketDone { bucket, result } => {
                self.handle_create_bucket_done(bucket, result)
            }
            Message::DownloadDone(r) => self.handle_download_done(r),

            // -- browse actions --
            Message::Refresh => self.handle_refresh(),
            Message::RefreshAll => self.handle_refresh_all(),
            Message::Upload => self.handle_upload(),
            Message::Delete(bucket, key) => self.handle_delete(bucket, key),
            Message::Download(bucket, key) => self.handle_download(bucket, key),
            Message::ObjectFilterChanged(v) => self.handle_object_filter_changed(v),
            Message::Find => self.handle_find(),
            Message::FindComplete(r) => self.handle_find_complete(r),
            Message::ClearFind => self.handle_clear_find(),
            Message::ToggleObjectSelected(k) => self.handle_toggle_object_selected(k),
            Message::SelectAllObjects => self.handle_select_all_objects(),
            Message::ClearObjectSelection => self.handle_clear_object_selection(),
            Message::NewBucketNameChanged(v) => self.handle_new_bucket_name_changed(v),
            Message::OpenCreateBucketModal => self.handle_open_create_bucket_modal(),
            Message::CloseCreateBucketModal => self.handle_close_create_bucket_modal(),
            Message::CreateBucket => self.handle_create_bucket(),

            // -- transfer --
            Message::OpenCopyObject => self.handle_open_copy_object(),
            Message::OpenMoveObject => self.handle_open_move_object(),
            Message::OpenRenameObject => self.handle_open_rename_object(),
            Message::OpenImportFolder => self.handle_open_import_folder(),
            Message::OpenExportPrefix => self.handle_open_export_prefix(),
            Message::CloseTransferModal => self.handle_close_transfer_modal(),
            Message::TransferDestinationConnectionChanged(id) => {
                self.handle_transfer_destination_connection_changed(id)
            }
            Message::TransferDestinationBucketChanged(b) => {
                self.handle_transfer_destination_bucket_changed(b)
            }
            Message::TransferDestinationKeyChanged(k) => {
                self.handle_transfer_destination_key_changed(k)
            }
            Message::TransferDestinationBucketsLoaded(r) => {
                self.handle_transfer_destination_buckets_loaded(r)
            }
            Message::StartTransfer => self.handle_start_transfer(),
            Message::TransferPrepared(r) => self.handle_transfer_prepared(r),
            Message::TransferStepFinished(r) => self.handle_transfer_step_finished(r),
            Message::TransferConflictOverwrite => self.handle_transfer_conflict_overwrite(),
            Message::TransferConflictSkip => self.handle_transfer_conflict_skip(),
            Message::TransferConflictOverwriteAll => self.handle_transfer_conflict_overwrite_all(),
            Message::TransferConflictSkipAll => self.handle_transfer_conflict_skip_all(),

            // -- delete --
            Message::OpenBulkDeleteModal => self.handle_open_bulk_delete_modal(),
            Message::CloseBulkDeleteModal => self.handle_close_bulk_delete_modal(),
            Message::ConfirmBulkDelete => self.handle_confirm_bulk_delete(),
            Message::BulkDeleteBatchFinished(r) => self.handle_bulk_delete_batch_finished(r),
            Message::OpenPrefixDeleteModal(p) => self.handle_open_prefix_delete_modal(p),
            Message::ClosePrefixDeleteModal => self.handle_close_prefix_delete_modal(),
            Message::PrefixDeleteListLoaded(r) => self.handle_prefix_delete_list_loaded(r),
            Message::ConfirmPrefixDelete => self.handle_confirm_prefix_delete(),
            Message::PrefixDeleteBatchFinished(r) => self.handle_prefix_delete_batch_finished(r),
            Message::OpenDeleteBucketModal => self.handle_open_delete_bucket_modal(),
            Message::CloseDeleteBucketModal => self.handle_close_delete_bucket_modal(),
            Message::BucketDeletePreviewLoaded { bucket, result } => {
                self.handle_bucket_delete_preview_loaded(bucket, result)
            }
            Message::BucketDeleteConfirmNameChanged(v) => {
                self.handle_bucket_delete_confirm_name_changed(v)
            }
            Message::ConfirmDeleteBucket => self.handle_confirm_delete_bucket(),
            Message::BucketDeleteStepFinished(r) => self.handle_bucket_delete_step_finished(r),

            // -- connection manager --
            Message::ConnectTo(name) => self.handle_connect_to(name),
            Message::AddConnection => self.handle_add_connection(),
            Message::EditConnection(name) => self.handle_edit_connection(name),
            Message::RemoveConnection(name) => self.handle_remove_connection(name),
            Message::TestConnection(name) => self.handle_test_connection(name),
            Message::TestConnectionResult(name, result) => {
                self.handle_test_connection_result(name, result)
            }
            Message::NewConnNameChanged(v) => self.handle_new_conn_name_changed(v),
            Message::NewConnEndpointChanged(v) => self.handle_new_conn_endpoint_changed(v),
            Message::NewConnRegionChanged(v) => self.handle_new_conn_region_changed(v),
            Message::NewConnAccessKeyChanged(v) => self.handle_new_conn_access_key_changed(v),
            Message::NewConnSecretKeyChanged(v) => self.handle_new_conn_secret_key_changed(v),

            // -- admin --
            Message::AbixioDetected(status) => self.handle_abixio_detected(status),
            Message::DisksLoaded(r) => self.handle_disks_loaded(r),
            Message::HealStatusLoaded(r) => self.handle_heal_status_loaded(r),
            Message::ObjectInspectLoaded {
                bucket,
                key,
                result,
            } => self.handle_object_inspect_loaded(bucket, key, result),
            Message::RefreshDisks => self.handle_refresh_disks(),
            Message::RefreshHealStatus => self.handle_refresh_heal_status(),
            Message::RefreshObjectInspect => self.handle_refresh_object_inspect(),
            Message::OpenHealConfirm => self.handle_open_heal_confirm(),
            Message::CancelHealConfirm => self.handle_cancel_heal_confirm(),
            Message::ConfirmHealObject => self.handle_confirm_heal_object(),
            Message::HealObjectFinished {
                bucket,
                key,
                result,
            } => self.handle_heal_object_finished(bucket, key, result),

            // -- detail panel --
            Message::OpenShareModal => self.handle_open_share_modal(),
            Message::CloseShareModal => self.handle_close_share_modal(),
            Message::ShareExpiryChanged(s) => self.handle_share_expiry_changed(s),
            Message::GenerateShareUrl => self.handle_generate_share_url(),
            Message::ShareUrlGenerated(r) => self.handle_share_url_generated(r),
            Message::BucketDocumentLoaded(kind, r) => self.handle_bucket_document_loaded(kind, r),
            Message::BucketTagsLoaded(r) => self.handle_bucket_tags_loaded(r),
            Message::OpenBucketDocumentEditor(kind) => {
                self.handle_open_bucket_document_editor(kind)
            }
            Message::CancelBucketDocumentEditor(kind) => {
                self.handle_cancel_bucket_document_editor(kind)
            }
            Message::BucketDocumentEdited(kind, action) => {
                self.handle_bucket_document_edited(kind, action)
            }
            Message::SaveBucketDocument(kind) => self.handle_save_bucket_document(kind),
            Message::BucketDocumentSaved(kind, r) => self.handle_bucket_document_saved(kind, r),
            Message::DeleteBucketDocument(kind) => self.handle_delete_bucket_document(kind),
            Message::BucketDocumentDeleted(kind, r) => self.handle_bucket_document_deleted(kind, r),
            Message::BucketTagKeyChanged(s) => self.handle_bucket_tag_key_changed(s),
            Message::BucketTagValueChanged(s) => self.handle_bucket_tag_value_changed(s),
            Message::AddBucketTag => self.handle_add_bucket_tag(),
            Message::RemoveBucketTag(k) => self.handle_remove_bucket_tag(k),
            Message::BucketTagsSaved(r) => self.handle_bucket_tags_saved(r),
            Message::PreviewLoaded(r) => self.handle_preview_loaded(r),
            Message::VersioningStatusLoaded(r) => self.handle_versioning_status_loaded(r),
            Message::VersionsLoaded(r) => self.handle_versions_loaded(r),
            Message::EnableVersioning => self.handle_enable_versioning(),
            Message::SuspendVersioning => self.handle_suspend_versioning(),
            Message::VersioningToggled(r) => self.handle_versioning_toggled(r),
            Message::DeleteVersion(vid) => self.handle_delete_version(vid),
            Message::VersionDeleted(r) => self.handle_version_deleted(r),
            Message::RestoreVersion(vid) => self.handle_restore_version(vid),
            Message::VersionRestored(r) => self.handle_version_restored(r),
            Message::TagsLoaded(r) => self.handle_tags_loaded(r),
            Message::TagKeyChanged(s) => self.handle_tag_key_changed(s),
            Message::TagValueChanged(s) => self.handle_tag_value_changed(s),
            Message::AddTag => self.handle_add_tag(),
            Message::RemoveTag(k) => self.handle_remove_tag(k),
            Message::TagsSaved(r) => self.handle_tags_saved(r),

            // -- settings --
            Message::SetTheme(t) => {
                self.theme = t;
                Task::none()
            }
            Message::DismissError => {
                self.error = None;
                Task::none()
            }

            // -- testing --
            Message::RunTests => self.handle_run_tests(),
            Message::TestsComplete(r) => self.handle_tests_complete(r),
            Message::AutoStartTests => self.handle_auto_start_tests(),
            Message::TestReportWritten(r) => self.handle_test_report_written(r),
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
        let with_prefix = if self.prefix_delete.is_some() {
            stack![with_bulk, self.prefix_delete_modal()].into()
        } else {
            with_bulk
        };
        if self.share_modal_open {
            stack![with_prefix, self.share_modal()].into()
        } else {
            with_prefix
        }
    }

    // -- helpers that stay in mod.rs --

    pub(crate) fn current_selected_object(&self) -> Option<(String, String)> {
        match &self.selection {
            Selection::Object { bucket, key } => Some((bucket.clone(), key.clone())),
            _ => None,
        }
    }

    pub(crate) fn current_selected_bucket(&self) -> Option<String> {
        match &self.selection {
            Selection::Bucket(bucket) => Some(bucket.clone()),
            Selection::Object { bucket, .. } => Some(bucket.clone()),
            Selection::None => self.selected_bucket.clone(),
        }
    }

    pub(crate) fn selected_object_matches(&self, bucket: &str, key: &str) -> bool {
        matches!(
            &self.selection,
            Selection::Object {
                bucket: selected_bucket,
                key: selected_key,
            } if selected_bucket == bucket && selected_key == key
        )
    }

    pub(crate) fn make_client_for_connection(
        &self,
        connection_id: &str,
    ) -> Result<Arc<S3Client>, String> {
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

    pub(crate) fn bucket_document_state(&self, kind: BucketDocumentKind) -> &BucketDocumentState {
        match kind {
            BucketDocumentKind::Policy => &self.bucket_policy,
            BucketDocumentKind::Lifecycle => &self.bucket_lifecycle,
        }
    }

    pub(crate) fn bucket_document_state_mut(
        &mut self,
        kind: BucketDocumentKind,
    ) -> &mut BucketDocumentState {
        match kind {
            BucketDocumentKind::Policy => &mut self.bucket_policy,
            BucketDocumentKind::Lifecycle => &mut self.bucket_lifecycle,
        }
    }

    pub(crate) fn reset_bucket_document_states(&mut self) {
        self.bucket_policy.reset();
        self.bucket_lifecycle.reset();
    }
}

#[cfg(test)]
mod tests {
    use iced::widget::text_editor;

    use super::types::{
        BucketDeleteState, BucketDocumentLoadState, CURRENT_CONNECTION_ID, OverwritePolicy,
        TransferMode, TransferState,
    };
    use super::{App, BucketDocumentKind, Message, Selection, StartupOptions};
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
            overwrite_policy: OverwritePolicy::Ask,
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
    fn opening_one_bucket_document_editor_closes_the_other() {
        let (mut app, _) = App::new(empty_startup());
        app.bucket_policy
            .set_loaded(BucketDocumentLoadState::Loaded(
                "{\"Version\":\"2012-10-17\"}".to_string(),
            ));
        app.bucket_lifecycle
            .set_loaded(BucketDocumentLoadState::Loaded(
                "<LifecycleConfiguration />".to_string(),
            ));

        let _ = app.update(Message::OpenBucketDocumentEditor(
            BucketDocumentKind::Policy,
        ));
        assert!(app.bucket_policy.editing);
        assert!(!app.bucket_lifecycle.editing);

        let _ = app.update(Message::OpenBucketDocumentEditor(
            BucketDocumentKind::Lifecycle,
        ));
        assert!(!app.bucket_policy.editing);
        assert!(app.bucket_lifecycle.editing);
    }

    #[test]
    fn cancel_bucket_document_editor_restores_loaded_text() {
        let (mut app, _) = App::new(empty_startup());
        app.bucket_policy
            .set_loaded(BucketDocumentLoadState::Loaded(
                "{\"Version\":\"2012-10-17\"}".to_string(),
            ));

        let _ = app.update(Message::OpenBucketDocumentEditor(
            BucketDocumentKind::Policy,
        ));
        app.bucket_policy.editor = text_editor::Content::with_text("{\"Version\":\"custom\"}");

        let _ = app.update(Message::CancelBucketDocumentEditor(
            BucketDocumentKind::Policy,
        ));

        assert!(!app.bucket_policy.editing);
        assert_eq!(
            app.bucket_policy.editor.text(),
            "{\"Version\":\"2012-10-17\"}"
        );
    }

    #[test]
    fn empty_bucket_document_save_is_blocked_locally() {
        let (mut app, _) = App::new(empty_startup());
        app.selected_bucket = Some("bucket-a".to_string());
        app.selection = Selection::Bucket("bucket-a".to_string());
        app.bucket_policy
            .set_loaded(BucketDocumentLoadState::Absent);

        let _ = app.update(Message::OpenBucketDocumentEditor(
            BucketDocumentKind::Policy,
        ));
        app.bucket_policy.editor = text_editor::Content::with_text("   ");

        let _ = app.update(Message::SaveBucketDocument(BucketDocumentKind::Policy));

        assert_eq!(
            app.bucket_policy.error.as_deref(),
            Some("Policy JSON cannot be empty.")
        );
        assert!(!app.bucket_policy.saving);
    }

    #[test]
    fn selecting_bucket_resets_bucket_document_editors() {
        let (mut app, _) = App::new(empty_startup());
        app.bucket_policy
            .set_loaded(BucketDocumentLoadState::Loaded(
                "{\"Version\":\"2012-10-17\"}".to_string(),
            ));
        app.bucket_lifecycle
            .set_loaded(BucketDocumentLoadState::Loaded(
                "<LifecycleConfiguration />".to_string(),
            ));
        let _ = app.update(Message::OpenBucketDocumentEditor(
            BucketDocumentKind::Policy,
        ));
        app.bucket_policy.error = Some("invalid".to_string());

        let _ = app.update(Message::SelectBucket("bucket-b".to_string()));

        assert!(!app.bucket_policy.editing);
        assert!(app.bucket_policy.loaded.is_none());
        assert!(app.bucket_policy.error.is_none());
        assert!(app.bucket_lifecycle.loaded.is_none());
    }
}
