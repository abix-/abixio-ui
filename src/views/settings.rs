use eframe::egui;

use crate::app::{App, AppSettings, LABEL_COLOR, ThemeChoice, apply_theme_choice};

impl App {
    pub fn settings_view(&mut self, ui: &mut egui::Ui) {
        ui.heading("Settings");
        ui.add_space(12.0);

        // -- appearance --
        ui.strong("Appearance");
        ui.separator();
        ui.add_space(4.0);

        ui.horizontal(|ui: &mut egui::Ui| {
            ui.label(egui::RichText::new("Theme").color(LABEL_COLOR));
            ui.add_space(8.0);

            let mut changed = false;

            if ui
                .selectable_label(self.settings.theme == ThemeChoice::Dark, "Dark")
                .clicked()
            {
                self.settings.theme = ThemeChoice::Dark;
                changed = true;
            }
            if ui
                .selectable_label(self.settings.theme == ThemeChoice::Light, "Light")
                .clicked()
            {
                self.settings.theme = ThemeChoice::Light;
                changed = true;
            }
            if ui
                .selectable_label(self.settings.theme == ThemeChoice::System, "System")
                .clicked()
            {
                self.settings.theme = ThemeChoice::System;
                changed = true;
            }

            if changed {
                apply_theme_choice(ui.ctx(), self.settings.theme);
            }
        });

        ui.add_space(16.0);

        // -- connection --
        ui.strong("Connection");
        ui.separator();
        ui.add_space(4.0);

        ui.horizontal(|ui: &mut egui::Ui| {
            ui.label(egui::RichText::new("Endpoint").color(LABEL_COLOR));
            ui.add_space(8.0);
            ui.label(&self.endpoint);
        });

        ui.horizontal(|ui: &mut egui::Ui| {
            ui.label(egui::RichText::new("Server type").color(LABEL_COLOR));
            ui.add_space(8.0);
            if self.is_abixio {
                ui.label("AbixIO");
            } else {
                ui.label("Generic S3");
            }
        });

        ui.add_space(16.0);

        // -- about --
        ui.strong("About");
        ui.separator();
        ui.add_space(4.0);
        ui.label(format!("abixio-ui v{}", env!("CARGO_PKG_VERSION")));
        ui.label(
            egui::RichText::new("native desktop s3 manager")
                .size(11.0)
                .color(LABEL_COLOR),
        );
        ui.hyperlink_to("github", "https://github.com/abix-/abixio-ui");
    }
}
