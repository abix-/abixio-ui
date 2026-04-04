use std::sync::Arc;

use iced::keyboard;
use iced::widget::{button, column, container, row, text};
use iced::{Element, Length, Subscription, Task, Theme};

use crate::config::{self, Connection, Settings};
use crate::s3::client::{BucketInfo, ListObjectsResult, ObjectDetail, S3Client};

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
}

#[derive(Debug, Clone, Copy, PartialEq)]
pub enum Section {
    Browse,
    Disks,
    Config,
    Healing,
    Connections,
    Settings,
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

    // connection form
    pub new_conn_name: String,
    pub new_conn_endpoint: String,
    pub new_conn_region: String,
    pub new_conn_access_key: String,
    pub new_conn_secret_key: String,
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
                    let fallback = S3Client::anonymous("http://localhost:10000")
                        .expect("fallback client");
                    (Arc::new(fallback), String::new(), Section::Connections, false)
                }
            }
        } else {
            let fallback =
                S3Client::anonymous("http://localhost:10000").expect("fallback client");
            (Arc::new(fallback), String::new(), Section::Connections, false)
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
            new_conn_name: String::new(),
            new_conn_endpoint: String::new(),
            new_conn_region: "us-east-1".to_string(),
            new_conn_access_key: String::new(),
            new_conn_secret_key: String::new(),
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
                self.loading_objects = true;
                self.cmd_fetch_objects()
            }
            Message::NavigatePrefix(prefix) => {
                self.current_prefix = prefix;
                self.selection = Selection::None;
                self.loading_objects = true;
                self.cmd_fetch_objects()
            }
            Message::SelectObject(key) => {
                let bucket = self.selected_bucket.clone().unwrap_or_default();
                self.selection = Selection::Object {
                    bucket: bucket.clone(),
                    key: key.clone(),
                };
                self.loading_detail = true;
                self.cmd_fetch_detail(&bucket, &key)
            }
            Message::ClearSelection => {
                self.selection = Selection::None;
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
                        self.loading_buckets = true;
                        self.cmd_fetch_buckets()
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
                    async move {
                        client.list_buckets().await.map(|_| ())
                    },
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
        }
    }

    pub fn view(&self) -> Element<Message> {
        let sidebar = self.sidebar_view();
        let content: Element<Message> = match self.section {
            Section::Browse => self.browse_view(),
            Section::Connections => self.connections_view(),
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

        layout.into()
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
}
