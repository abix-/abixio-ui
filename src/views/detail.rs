use eframe::egui;

use crate::app::{App, Selection};

impl App {
    pub fn detail_panel(&mut self, ui: &mut egui::Ui) {
        let selection = self.selection.clone();

        match &selection {
            Selection::None => {}
            Selection::Bucket(name) => {
                ui.heading(name);
                ui.separator();
                ui.label("Bucket");
                ui.add_space(8.0);

                // show object count if we have listing data
                if let Some(Ok(result)) = &self.objects_op.data {
                    ui.label(format!(
                        "{} objects",
                        result.objects.len() + result.common_prefixes.len()
                    ));
                }

                ui.add_space(16.0);
                // TODO: delete bucket button
            }
            Selection::Object { bucket, key, info } => {
                // show short name
                let short = key.rsplit('/').next().unwrap_or(key);
                ui.heading(short);
                ui.separator();

                if let Some(obj) = info {
                    ui.label(format!("Size: {}", format_size(obj.size)));
                    ui.label(format!("Modified: {}", &obj.last_modified));
                    ui.label(format!("ETag: {}", &obj.etag));
                } else {
                    ui.label(format!("Key: {}", key));
                }

                ui.add_space(8.0);
                ui.label(format!("Bucket: {}", bucket));

                if self.is_abixio {
                    ui.add_space(12.0);
                    ui.separator();
                    ui.strong("Shards");
                    ui.label("Connect to AbixIO for shard details");
                    // TODO: fetch from /_abixio/object-info
                }

                ui.add_space(16.0);

                if ui.button("Download").clicked() {
                    // TODO: implement download
                }

                let bucket_clone = bucket.clone();
                let key_clone = key.clone();
                if ui.button("Delete").clicked() {
                    self.delete_object(ui.ctx(), &bucket_clone, &key_clone);
                }
            }
        }

        ui.add_space(8.0);
        ui.separator();
        if ui.small_button("Close").clicked() {
            self.selection = Selection::None;
        }
    }
}

fn format_size(bytes: u64) -> String {
    if bytes < 1024 {
        format!("{} B", bytes)
    } else if bytes < 1024 * 1024 {
        format!("{:.1} KB", bytes as f64 / 1024.0)
    } else if bytes < 1024 * 1024 * 1024 {
        format!("{:.1} MB", bytes as f64 / (1024.0 * 1024.0))
    } else {
        format!("{:.1} GB", bytes as f64 / (1024.0 * 1024.0 * 1024.0))
    }
}
