use std::sync::Arc;

use eframe::egui;

use crate::async_op::AsyncOp;
use crate::s3::client::{BucketInfo, ListObjectsResult, S3Client};

pub struct App {
    client: Arc<S3Client>,
    pub endpoint: String,

    // async ops
    pub buckets_op: AsyncOp<Vec<BucketInfo>>,
    pub objects_op: AsyncOp<ListObjectsResult>,
    upload_op: AsyncOp<String>,
    delete_op: AsyncOp<()>,
    create_bucket_op: AsyncOp<()>,

    // ui state
    pub selected_bucket: Option<String>,
    pub current_prefix: String,
    pub new_bucket_name: String,
}

impl App {
    pub fn new(cc: &eframe::CreationContext<'_>, endpoint: &str) -> Self {
        let client = Arc::new(S3Client::new(endpoint));
        let mut app = Self {
            client,
            endpoint: endpoint.to_string(),
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
        // poll all async ops
        self.buckets_op.poll();
        self.objects_op.poll();
        self.upload_op.poll();
        self.delete_op.poll();
        self.create_bucket_op.poll();

        // if create/upload/delete just completed, refresh
        if let Some(Ok(())) = self.create_bucket_op.data.take() {
            self.fetch_buckets(ctx);
        }
        if let Some(Ok(_)) = self.upload_op.data.take() {
            self.fetch_objects(ctx);
        }
        if let Some(Ok(())) = self.delete_op.data.take() {
            self.fetch_objects(ctx);
        }

        // request repaint only if something is pending
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
        let ctx = ui.ctx().clone();

        egui::TopBottomPanel::top("top_bar").show_inside(ui, |ui: &mut egui::Ui| {
            ui.horizontal(|ui: &mut egui::Ui| {
                ui.label("Endpoint:");
                ui.label(&self.endpoint);
                ui.with_layout(
                    egui::Layout::right_to_left(egui::Align::Center),
                    |ui: &mut egui::Ui| {
                        if ui.button("Refresh All").clicked() {
                            self.fetch_buckets(&ctx);
                        }
                    },
                );
            });
        });

        egui::SidePanel::left("bucket_panel")
            .default_width(200.0)
            .show_inside(ui, |ui: &mut egui::Ui| {
                self.bucket_panel(ui);
            });

        egui::CentralPanel::default().show_inside(ui, |ui: &mut egui::Ui| {
            self.object_panel(ui);
        });
    }
}
