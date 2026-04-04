use std::sync::Arc;

use iced::keyboard;
use iced::widget::{button, column, container, row, stack, text};
use iced::{Element, Length, Subscription, Task, Theme};

use crate::abixio::client::AdminClient;
use crate::abixio::types::{
    DisksResponse, HealResponse, HealStatusResponse, ObjectInspectResponse, StatusResponse,
};
use crate::config::{self, Connection, Settings};
use crate::s3::client::{BucketInfo, ListObjectsResult, ObjectDetail, S3Client};
use crate::views::testing::TestResult;

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
    CreateBucketDone(Result<(), String>),
    DownloadDone(Result<String, String>),

    Refresh,
    RefreshAll,
    Upload,
    Delete(String, String),
    Download(String, String),
    CreateBucket,
    SetTheme(AppTheme),
    NewBucketNameChanged(String),
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

    // testing
    RunTests,
    TestsComplete(Vec<TestResult>),
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

    // connection form
    pub new_conn_name: String,
    pub new_conn_endpoint: String,
    pub new_conn_region: String,
    pub new_conn_access_key: String,
    pub new_conn_secret_key: String,

    // testing
    pub test_results: Vec<TestResult>,
    pub test_running: bool,
    pub test_progress: String,
}

impl App {
    pub fn new(endpoint: Option<String>, creds: Option<(String, String)>) -> (Self, Task<Message>) {
        let settings = config::load().unwrap_or_default();

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

        let app = Self {
            client: client.clone(),
            endpoint: start_endpoint,
            section,
            selection: Selection::None,
            theme: AppTheme::Dark,
            buckets: None,
            objects: None,
            detail: None,
            selected_bucket: None,
            current_prefix: String::new(),
            new_bucket_name: String::new(),
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
            new_conn_name: String::new(),
            new_conn_endpoint: String::new(),
            new_conn_region: "us-east-1".to_string(),
            new_conn_access_key: String::new(),
            new_conn_secret_key: String::new(),
            test_results: Vec::new(),
            test_running: false,
            test_progress: String::new(),
        };

        let task = if loading_buckets {
            let c = client.clone();
            Task::perform(
                async move { c.list_buckets().await },
                Message::BucketsLoaded,
            )
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
                self.selection = Selection::Bucket(name);
                self.clear_object_admin_state();
                self.loading_objects = true;
                self.cmd_fetch_objects()
            }
            Message::NavigatePrefix(prefix) => {
                self.current_prefix = prefix;
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
            Message::CreateBucketDone(Ok(())) => {
                self.loading_buckets = true;
                self.cmd_fetch_buckets()
            }
            Message::CreateBucketDone(Err(e)) => {
                self.error = Some(format!("Create bucket failed: {}", e));
                Task::none()
            }
            Message::DownloadDone(Ok(_)) => Task::none(),
            Message::DownloadDone(Err(e)) => {
                self.error = Some(format!("Download failed: {}", e));
                Task::none()
            }

            Message::Refresh => {
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
            Message::CreateBucket => {
                if self.new_bucket_name.is_empty() {
                    return Task::none();
                }
                let client = self.client.clone();
                let name = self.new_bucket_name.clone();
                self.new_bucket_name.clear();
                Task::perform(
                    async move { client.create_bucket(&name).await },
                    Message::CreateBucketDone,
                )
            }
            Message::SetTheme(t) => {
                self.theme = t;
                Task::none()
            }
            Message::NewBucketNameChanged(val) => {
                self.new_bucket_name = val;
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
                    return Task::batch(vec![
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
                    ]);
                } else {
                    self.is_abixio = false;
                    self.server_status = None;
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
            Message::RunTests => {
                if self.test_running || self.endpoint.is_empty() {
                    return Task::none();
                }
                self.test_running = true;
                self.test_results.clear();
                self.test_progress = "running tests...".to_string();
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
            Message::TestsComplete(results) => {
                self.test_running = false;
                let passed = results.iter().filter(|r| r.passed).count();
                let total = results.len();
                self.test_progress = format!("done: {}/{} passed", passed, total);
                self.test_results = results;
                Task::none()
            }
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

        if matches!(self.selection, Selection::Object { .. }) {
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
        if self.heal_confirm_target.is_some() {
            stack![base, self.heal_confirm_modal()].into()
        } else {
            base
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
}

#[cfg(test)]
mod tests {
    use super::{App, Message, Selection};
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

    #[test]
    fn ignores_stale_object_inspect_result() {
        let (mut app, _) = App::new(None, None);
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
        let (mut app, _) = App::new(None, None);
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
        let (mut app, _) = App::new(None, None);
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
}
