use std::sync::Arc;

use eframe::egui;

use crate::async_op::AsyncOp;
use crate::s3::client::{BucketInfo, ListObjectsResult, ObjectDetail, ObjectInfo, S3Client};

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
    pub detail_op: AsyncOp<ObjectDetail>,

    // ui state
    pub selected_bucket: Option<String>,
    pub current_prefix: String,
    pub new_bucket_name: String,
}

impl App {
    pub fn new(cc: &eframe::CreationContext<'_>, endpoint: &str) -> Self {
        apply_theme(&cc.egui_ctx);

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
            detail_op: AsyncOp::new(),
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

    pub fn fetch_detail(&mut self, ctx: &egui::Context, bucket: &str, key: &str) {
        let client = self.client.clone();
        let bucket = bucket.to_string();
        let key = key.to_string();
        self.detail_op
            .request(ctx, async move { client.head_object(&bucket, &key).await });
    }

    pub fn download_object(&mut self, ctx: &egui::Context, bucket: &str, key: &str) {
        let filename = key.rsplit('/').next().unwrap_or(key).to_string();
        let save_path = rfd::FileDialog::new().set_file_name(&filename).save_file();
        let save_path = match save_path {
            Some(p) => p,
            None => return,
        };

        let client = self.client.clone();
        let bucket = bucket.to_string();
        let key = key.to_string();
        // reuse upload_op for download (it's AsyncOp<String>, returns path)
        self.upload_op.request(ctx, async move {
            let data = client.get_object(&bucket, &key).await?;
            std::fs::write(&save_path, &data).map_err(|e| e.to_string())?;
            Ok(save_path.to_string_lossy().to_string())
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
        self.detail_op.poll();

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
            || self.detail_op.pending
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

// -- custom theme --
// Red = errors/destructive only. Teal accent for selection/active.
// High contrast: light text on dark backgrounds.

const BG_DEEP: egui::Color32 = egui::Color32::from_rgb(0x12, 0x12, 0x1a);
const BG_PANEL: egui::Color32 = egui::Color32::from_rgb(0x1a, 0x1c, 0x2e);
const BG_WIDGET: egui::Color32 = egui::Color32::from_rgb(0x24, 0x28, 0x3e);
const ACCENT: egui::Color32 = egui::Color32::from_rgb(0x2d, 0xd4, 0xbf); // teal
const TEXT_PRIMARY: egui::Color32 = egui::Color32::from_rgb(0xee, 0xee, 0xee);
const TEXT_MUTED: egui::Color32 = egui::Color32::from_rgb(0x88, 0x99, 0xaa);
const SEPARATOR: egui::Color32 = egui::Color32::from_rgb(0x2a, 0x2e, 0x42);
const LINK: egui::Color32 = egui::Color32::from_rgb(0x5c, 0xb8, 0xff); // bright blue
pub const ERROR_COLOR: egui::Color32 = egui::Color32::from_rgb(0xe0, 0x40, 0x40); // red = errors only
pub const LABEL_COLOR: egui::Color32 = TEXT_MUTED;
pub const VALUE_COLOR: egui::Color32 = TEXT_PRIMARY;

fn apply_theme(ctx: &egui::Context) {
    // force dark mode
    ctx.set_theme(egui::Theme::Dark);

    // customize dark visuals
    let mut visuals = egui::Visuals::dark();

    visuals.panel_fill = BG_PANEL;
    visuals.window_fill = BG_PANEL;
    visuals.extreme_bg_color = BG_DEEP;
    visuals.faint_bg_color = egui::Color32::from_rgb(0x1e, 0x20, 0x30);
    visuals.override_text_color = Some(TEXT_PRIMARY);
    visuals.hyperlink_color = LINK;

    visuals.selection.bg_fill = egui::Color32::from_rgb(0x1a, 0x6b, 0x5e);
    visuals.selection.stroke = egui::Stroke::new(1.0, ACCENT);

    visuals.widgets.noninteractive.bg_stroke = egui::Stroke::new(1.0, SEPARATOR);
    visuals.widgets.noninteractive.bg_fill = BG_PANEL;
    visuals.widgets.inactive.bg_fill = BG_WIDGET;
    visuals.widgets.hovered.bg_fill = egui::Color32::from_rgb(0x2e, 0x34, 0x50);
    visuals.widgets.active.bg_fill = egui::Color32::from_rgb(0x1a, 0x6b, 0x5e);

    ctx.set_visuals_of(egui::Theme::Dark, visuals);
}
