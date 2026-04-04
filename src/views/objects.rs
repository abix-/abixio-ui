use eframe::egui;

use crate::app::{App, Selection};
use crate::s3::client::ObjectInfo;

impl App {
    pub fn object_panel(&mut self, ui: &mut egui::Ui) {
        let bucket = match &self.selected_bucket {
            Some(b) => b.clone(),
            None => {
                ui.centered_and_justified(|ui: &mut egui::Ui| {
                    ui.label("Select a bucket");
                });
                return;
            }
        };

        // breadcrumb + actions bar
        let prefix = self.current_prefix.clone();
        let mut nav_to: Option<String> = None;

        ui.horizontal(|ui: &mut egui::Ui| {
            if ui.link(&bucket).clicked() {
                nav_to = Some(String::new());
            }
            if !prefix.is_empty() {
                ui.label("/");
                let parts: Vec<&str> = prefix.trim_end_matches('/').split('/').collect();
                for (i, part) in parts.iter().enumerate() {
                    if ui.link(*part).clicked() {
                        nav_to = Some(parts[..=i].join("/") + "/");
                    }
                    if i < parts.len() - 1 {
                        ui.label("/");
                    }
                }
            }

            ui.with_layout(
                egui::Layout::right_to_left(egui::Align::Center),
                |ui: &mut egui::Ui| {
                    if ui.button("Refresh").clicked() {
                        nav_to = Some(prefix.clone());
                    }
                    if ui.button("Upload").clicked() {
                        self.upload_file(ui.ctx());
                    }
                },
            );
        });

        if let Some(new_prefix) = nav_to {
            self.current_prefix = new_prefix;
            self.selection = Selection::None;
            self.fetch_objects(ui.ctx());
        }

        ui.separator();

        if self.objects_op.pending {
            ui.spinner();
            return;
        }

        if let Some(Err(e)) = &self.objects_op.data {
            ui.colored_label(egui::Color32::RED, format!("Error: {}", e));
            return;
        }

        let result = match &self.objects_op.data {
            Some(Ok(r)) => r.clone(),
            _ => return,
        };

        let current_prefix = self.current_prefix.clone();

        // folders (common prefixes)
        let mut folder_nav: Option<String> = None;
        for cp in &result.common_prefixes {
            let display = cp.strip_prefix(&current_prefix).unwrap_or(cp);
            ui.horizontal(|ui: &mut egui::Ui| {
                ui.label("  ");
                if ui.link(format!("{}", display)).clicked() {
                    folder_nav = Some(cp.clone());
                }
            });
        }

        if let Some(p) = folder_nav {
            self.current_prefix = p;
            self.selection = Selection::None;
            self.fetch_objects(ui.ctx());
            return;
        }

        // objects table
        let mut clicked_obj: Option<ObjectInfo> = None;

        egui::Grid::new("objects_grid")
            .striped(true)
            .min_col_width(80.0)
            .show(ui, |ui: &mut egui::Ui| {
                ui.strong("Name");
                ui.strong("Size");
                ui.strong("Modified");
                ui.end_row();

                for obj in &result.objects {
                    let display_key = obj.key.strip_prefix(&current_prefix).unwrap_or(&obj.key);

                    let is_selected = matches!(
                        &self.selection,
                        Selection::Object { key, .. } if *key == obj.key
                    );

                    let resp = ui.selectable_label(is_selected, display_key);
                    if resp.clicked() {
                        clicked_obj = Some(obj.clone());
                    }

                    ui.label(format_size(obj.size));
                    ui.label(&obj.last_modified);
                    ui.end_row();
                }
            });

        if let Some(obj) = clicked_obj {
            self.fetch_detail(ui.ctx(), &bucket, &obj.key);
            self.selection = Selection::Object {
                bucket,
                key: obj.key.clone(),
                info: Some(obj),
            };
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
