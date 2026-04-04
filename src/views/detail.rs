use eframe::egui;

use crate::app::{App, LABEL_COLOR, Selection, VALUE_COLOR};

impl App {
    pub fn detail_panel(&mut self, ui: &mut egui::Ui) {
        let selection = self.selection.clone();

        egui::ScrollArea::vertical().show(ui, |ui: &mut egui::Ui| {
            match &selection {
                Selection::None => {}
                Selection::Bucket(name) => self.detail_bucket(ui, name),
                Selection::Object { bucket, key, .. } => {
                    self.detail_object(ui, &bucket.clone(), &key.clone())
                }
            }

            ui.add_space(12.0);
            ui.separator();
            if ui.small_button("Close [ESC]").clicked() {
                self.selection = Selection::None;
            }
        });
    }

    fn detail_bucket(&mut self, ui: &mut egui::Ui, name: &str) {
        ui.add_space(8.0);
        ui.heading(name);
        ui.label(egui::RichText::new("Bucket").size(11.0).color(LABEL_COLOR));
        ui.add_space(12.0);

        if let Some(Ok(result)) = &self.objects_op.data {
            section_header(ui, "Contents");
            meta_row(ui, "Objects", &result.objects.len().to_string());
            meta_row(ui, "Prefixes", &result.common_prefixes.len().to_string());
        }
    }

    fn detail_object(&mut self, ui: &mut egui::Ui, bucket: &str, key: &str) {
        // filename
        let short = key.rsplit('/').next().unwrap_or(key);
        ui.add_space(8.0);
        ui.label(egui::RichText::new(short).size(18.0).strong());

        // full path
        ui.label(
            egui::RichText::new(format!("{} / {}", bucket, key))
                .size(11.0)
                .color(LABEL_COLOR),
        );

        ui.add_space(12.0);

        // check if we have detail data from HEAD
        if self.detail_op.pending {
            ui.spinner();
            return;
        }

        if let Some(Ok(detail)) = &self.detail_op.data {
            let detail = detail.clone();

            // -- overview --
            section_header(ui, "Overview");
            meta_row(ui, "Size", &format_size(detail.size));
            meta_row(ui, "Type", &detail.content_type);
            meta_row(ui, "Modified", &detail.last_modified);
            meta_row(ui, "ETag", &detail.etag);

            ui.add_space(8.0);

            // -- storage --
            section_header(ui, "Storage");
            meta_row(ui, "Bucket", bucket);
            meta_row(ui, "Key", key);
            let prefix = if key.contains('/') {
                key.rsplit_once('/')
                    .map(|(p, _)| format!("{}/", p))
                    .unwrap_or_default()
            } else {
                "(root)".to_string()
            };
            meta_row(ui, "Prefix", &prefix);

            ui.add_space(8.0);

            // -- all HTTP headers --
            section_header(ui, "HTTP Headers");
            for (name, value) in &detail.headers {
                meta_row(ui, name, value);
            }

            ui.add_space(8.0);

            // -- AbixIO shards --
            if self.is_abixio {
                section_header(ui, "Erasure Shards");
                ui.label(
                    egui::RichText::new("Requires /_abixio/object-info endpoint")
                        .size(11.0)
                        .color(LABEL_COLOR),
                );
            }
        } else if let Some(Err(e)) = &self.detail_op.data {
            ui.colored_label(crate::app::ERROR_COLOR, format!("Error: {}", e));
        } else {
            // no data yet and not pending -- shouldn't happen, but show basic info
            ui.label("Loading...");
        }

        ui.add_space(12.0);

        // -- actions --
        section_header(ui, "Actions");
        ui.horizontal(|ui: &mut egui::Ui| {
            if ui.button("Download").clicked() {
                self.download_object(ui.ctx(), bucket, key);
            }
            if ui
                .button(egui::RichText::new("Delete").color(crate::app::ERROR_COLOR))
                .clicked()
            {
                self.delete_object(ui.ctx(), bucket, key);
            }
        });
    }
}

fn section_header(ui: &mut egui::Ui, text: &str) {
    ui.add_space(4.0);
    ui.label(
        egui::RichText::new(text)
            .size(12.0)
            .strong()
            .color(LABEL_COLOR),
    );
    ui.separator();
}

fn meta_row(ui: &mut egui::Ui, label: &str, value: &str) {
    ui.horizontal(|ui: &mut egui::Ui| {
        ui.label(
            egui::RichText::new(format!("{}  ", label))
                .size(12.0)
                .color(LABEL_COLOR),
        );
        ui.label(egui::RichText::new(value).size(12.0).color(VALUE_COLOR));
    });
}

fn format_size(bytes: u64) -> String {
    if bytes < 1024 {
        format!("{} B", bytes)
    } else if bytes < 1024 * 1024 {
        format!("{:.1} KB ({} bytes)", bytes as f64 / 1024.0, bytes)
    } else if bytes < 1024 * 1024 * 1024 {
        format!(
            "{:.1} MB ({} bytes)",
            bytes as f64 / (1024.0 * 1024.0),
            bytes
        )
    } else {
        format!(
            "{:.1} GB ({} bytes)",
            bytes as f64 / (1024.0 * 1024.0 * 1024.0),
            bytes
        )
    }
}
