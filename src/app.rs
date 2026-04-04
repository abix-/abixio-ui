use std::sync::Arc;

use iced::keyboard;
use iced::widget::{button, column, container, row, scrollable, text, text_input};
use iced::{Element, Length, Subscription, Task, Theme};

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
}

impl App {
    pub fn new(endpoint: String) -> (Self, Task<Message>) {
        let client = Arc::new(S3Client::new(&endpoint));
        let app = Self {
            client: client.clone(),
            endpoint,
            section: Section::Browse,
            selection: Selection::None,
            theme: AppTheme::Dark,
            buckets: None,
            objects: None,
            detail: None,
            selected_bucket: None,
            current_prefix: String::new(),
            new_bucket_name: String::new(),
            loading_buckets: true,
            loading_objects: false,
            loading_detail: false,
            error: None,
            perf: crate::perf::PerfStats::new(),
        };
        let task = {
            let c = client.clone();
            Task::perform(
                async move { c.list_buckets().await },
                Message::BucketsLoaded,
            )
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
                        let data = std::fs::read(&file).map_err(|e| e.to_string())?;
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
                        std::fs::write(&save_path, &data).map_err(|e| e.to_string())?;
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
        }
    }

    pub fn view(&self) -> Element<Message> {
        let sidebar = self.sidebar_view();
        let content: Element<Message> = match self.section {
            Section::Browse => self.browse_view(),
            Section::Settings => self.settings_view(),
            _ => container(text("Coming soon").size(14)).padding(20).into(),
        };

        let mut main_row = row![container(sidebar).width(40), content,];

        if matches!(self.selection, Selection::Object { .. }) {
            main_row = main_row.push(container(self.detail_view()).width(280));
        }

        let top_bar = container(
            row![
                text("abixio-ui").size(14),
                text(" | ").size(14),
                text(&self.endpoint).size(12),
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
