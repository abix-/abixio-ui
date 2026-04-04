use std::sync::Arc;

use eframe::egui;

use crate::async_op::AsyncOp;
use crate::s3::client::{BucketInfo, ListObjectsResult, ObjectInfo, S3Client};

#[derive(PartialEq, Clone, Copy)]
pub enum Section {
    Browse,
    Disks,
    Config,
    Healing,
    Connections,
}

#[derive(Clone, PartialEq)]
pub enum Selection {
    None,
    Bucket(String),
    Object {
        bucket: String,
        key: String,
        info: Option<ObjectInfo>,
    },
}

pub struct App {
    pub client: Arc<S3Client>,
    pub endpoint: String,

    // navigation
    pub current_section: Section,
    pub selection: Selection,
    pub is_abixio: bool,

    // async ops
    pub buckets_op: AsyncOp<Vec<BucketInfo>>,
    pub objects_op: AsyncOp<ListObjectsResult>,
    pub upload_op: AsyncOp<String>,
    pub delete_op: AsyncOp<()>,
    pub create_bucket_op: AsyncOp<()>,

    // ui state
    pub selected_bucket: Option<String>,
    pub current_prefix: String,
    pub new_bucket_name: String,
}

impl App {
    pub fn new(cc: &eframe::CreationContext<'_>, endpoint: &str) -> Self {
        cc.egui_ctx.set_visuals(egui::Visuals::dark());

        let client = Arc::new(S3Client::new(endpoint));
        let mut app = Self {
            client,
            endpoint: endpoint.to_string(),
            current_section: Section::Browse,
            selection: Selection::None,
            is_abixio: false,
            buckets_op: AsyncOp::new(),
            objects_op: AsyncOp::new(),
            upload_op: AsyncOp::new(),
            delete_op: AsyncOp::new(),
            create_bucket_op: AsyncOp::new(),
            selected_bucket: None,
            current_prefix: String::new(),
            new_bucket_name: String::new(),
        };
        app.fetch_buckets(&cc.egui_ctx);
        app
    }

    pub fn fetch_buckets(&mut self, ctx: &egui::Context) {
        let client = self.client.clone();
        self.buckets_op
            .request(ctx, async move { client.list_buckets().await });
    }

    pub fn fetch_objects(&mut self, ctx: &egui::Context) {
        let client = self.client.clone();
        let bucket = match &self.selected_bucket {
            Some(b) => b.clone(),
            None => return,
        };
        let prefix = self.current_prefix.clone();
        self.objects_op.request(ctx, async move {
            client.list_objects(&bucket, &prefix, "/").await
        });
    }

    pub fn create_bucket(&mut self, ctx: &egui::Context, name: &str) {
        let client = self.client.clone();
        let name = name.to_string();
        self.create_bucket_op
            .request(ctx, async move { client.create_bucket(&name).await });
    }

    pub fn upload_file(&mut self, ctx: &egui::Context) {
        let file = rfd::FileDialog::new().pick_file();
        let file = match file {
            Some(f) => f,
            None => return,
        };

        let client = self.client.clone();
        let bucket = match &self.selected_bucket {
            Some(b) => b.clone(),
            None => return,
        };
        let prefix = self.current_prefix.clone();
        let filename = file
            .file_name()
            .map(|n| n.to_string_lossy().to_string())
            .unwrap_or_else(|| "upload".to_string());
        let key = format!("{}{}", prefix, filename);

        self.upload_op.request(ctx, async move {
            let data = std::fs::read(&file).map_err(|e| e.to_string())?;
            client
                .put_object(&bucket, &key, data, "application/octet-stream")
                .await
        });
    }

    pub fn delete_object(&mut self, ctx: &egui::Context, bucket: &str, key: &str) {
        let client = self.client.clone();
        let bucket = bucket.to_string();
        let key = key.to_string();
        self.delete_op.request(
            ctx,
            async move { client.delete_object(&bucket, &key).await },
        );
    }
}

impl eframe::App for App {
    fn logic(&mut self, ctx: &egui::Context, _frame: &mut eframe::Frame) {
        self.buckets_op.poll();
        self.objects_op.poll();
        self.upload_op.poll();
        self.delete_op.poll();
        self.create_bucket_op.poll();

        if let Some(Ok(())) = self.create_bucket_op.data.take() {
            self.fetch_buckets(ctx);
        }
        if let Some(Ok(_)) = self.upload_op.data.take() {
            self.fetch_objects(ctx);
        }
        if let Some(Ok(())) = self.delete_op.data.take() {
            self.selection = Selection::None;
            self.fetch_objects(ctx);
        }

        if self.buckets_op.pending
            || self.objects_op.pending
            || self.upload_op.pending
            || self.delete_op.pending
            || self.create_bucket_op.pending
        {
            ctx.request_repaint();
        }
    }

    fn ui(&mut self, ui: &mut egui::Ui, _frame: &mut eframe::Frame) {
        // ESC clears selection
        if ui.input(|i| i.key_pressed(egui::Key::Escape)) {
            self.selection = Selection::None;
        }

        // top bar
        egui::TopBottomPanel::top("top_bar").show_inside(ui, |ui: &mut egui::Ui| {
            ui.horizontal(|ui: &mut egui::Ui| {
                ui.strong("abixio-ui");
                ui.separator();
                ui.label(&self.endpoint);
                if self.is_abixio {
                    ui.colored_label(egui::Color32::from_rgb(100, 200, 100), "AbixIO");
                }
            });
        });

        // left icon sidebar
        egui::SidePanel::left("nav")
            .exact_width(40.0)
            .resizable(false)
            .show_inside(ui, |ui: &mut egui::Ui| {
                self.sidebar(ui);
            });

        // right detail panel (only when something selected)
        if !matches!(self.selection, Selection::None) {
            egui::SidePanel::right("detail")
                .default_width(280.0)
                .show_inside(ui, |ui: &mut egui::Ui| {
                    self.detail_panel(ui);
                });
        }

        // center content
        egui::CentralPanel::default().show_inside(ui, |ui: &mut egui::Ui| {
            match self.current_section {
                Section::Browse => self.browse_view(ui),
                Section::Disks => {
                    ui.heading("Disk Health");
                    ui.label("Coming soon -- requires AbixIO management API");
                }
                Section::Config => {
                    ui.heading("Server Config");
                    ui.label("Coming soon -- requires AbixIO management API");
                }
                Section::Healing => {
                    ui.heading("Healing Status");
                    ui.label("Coming soon -- requires AbixIO management API");
                }
                Section::Connections => {
                    ui.heading("Connections");
                    ui.label("Coming soon -- connection manager");
                }
            }
        });
    }
}
