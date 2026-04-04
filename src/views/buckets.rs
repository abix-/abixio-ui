use eframe::egui;

use crate::app::{App, Selection};

impl App {
    /// The full browse view: bucket sidebar + object table
    pub fn browse_view(&mut self, ui: &mut egui::Ui) {
        egui::SidePanel::left("buckets")
            .default_width(160.0)
            .show_inside(ui, |ui: &mut egui::Ui| {
                self.bucket_panel(ui);
            });

        egui::CentralPanel::default().show_inside(ui, |ui: &mut egui::Ui| {
            self.object_panel(ui);
        });
    }

    fn bucket_panel(&mut self, ui: &mut egui::Ui) {
        ui.strong("Buckets");
        ui.separator();

        if self.buckets_op.pending {
            ui.spinner();
        }

        if let Some(Ok(buckets)) = &self.buckets_op.data {
            let buckets = buckets.clone();
            for bucket in &buckets {
                let is_selected = self.selected_bucket.as_deref() == Some(&bucket.name);
                let resp = ui.selectable_label(is_selected, &bucket.name);
                if resp.clicked() {
                    self.selected_bucket = Some(bucket.name.clone());
                    self.current_prefix.clear();
                    self.selection = Selection::Bucket(bucket.name.clone());
                    self.fetch_objects(ui.ctx());
                }
            }
        }

        if let Some(Err(e)) = &self.buckets_op.data {
            ui.colored_label(egui::Color32::RED, format!("Error: {}", e));
        }

        ui.separator();

        ui.horizontal(|ui: &mut egui::Ui| {
            ui.text_edit_singleline(&mut self.new_bucket_name);
            if ui.button("+").clicked() && !self.new_bucket_name.is_empty() {
                let name = self.new_bucket_name.clone();
                self.new_bucket_name.clear();
                self.create_bucket(ui.ctx(), &name);
            }
        });
    }
}
