use eframe::egui;

use crate::app::App;

impl App {
    pub fn bucket_panel(&mut self, ui: &mut egui::Ui) {
        ui.heading("Buckets");
        ui.separator();

        if self.buckets_op.pending {
            ui.spinner();
        }

        if let Some(Ok(buckets)) = &self.buckets_op.data {
            let buckets = buckets.clone();
            for bucket in &buckets {
                let selected = self.selected_bucket.as_deref() == Some(&bucket.name);
                if ui.selectable_label(selected, &bucket.name).clicked() {
                    self.selected_bucket = Some(bucket.name.clone());
                    self.current_prefix.clear();
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
