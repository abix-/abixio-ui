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

        // -- performance --
        ui.strong("Performance");
        ui.separator();
        ui.add_space(4.0);

        let p = &self.perf;

        // rendering section
        ui.label(
            egui::RichText::new("Rendering")
                .size(11.0)
                .color(LABEL_COLOR),
        );
        egui::Grid::new("perf_render")
            .min_col_width(120.0)
            .show(ui, |ui: &mut egui::Ui| {
                perf_row(ui, "FPS (current)", &format!("{:.0}", p.current_fps()));
                perf_row(ui, "FPS (5m avg)", &format!("{:.0}", p.avg_fps()));
                perf_row(ui, "Frame time", &format!("{:.1} ms", p.current_frame_ms()));
                perf_row(
                    ui,
                    "Frame time (5m avg)",
                    &format!("{:.1} ms", p.avg_frame_ms()),
                );
                perf_row(
                    ui,
                    "Frame time (5m max)",
                    &format!("{:.1} ms", p.max_frame_ms()),
                );
                perf_row(ui, "Total frames", &p.total_frames().to_string());
                perf_row(ui, "Repaints (5m)", &p.repaints_5m().to_string());
            });

        ui.add_space(8.0);

        // network section
        ui.label(egui::RichText::new("Network").size(11.0).color(LABEL_COLOR));
        egui::Grid::new("perf_network")
            .min_col_width(120.0)
            .show(ui, |ui: &mut egui::Ui| {
                perf_row(ui, "Requests (5m)", &p.requests_5m().to_string());
                perf_row(ui, "Requests (total)", &p.total_requests.to_string());
                perf_row(ui, "Bytes in (5m)", &format_bytes(p.bytes_in_5m()));
                perf_row(ui, "Bytes out (5m)", &format_bytes(p.bytes_out_5m()));
                perf_row(
                    ui,
                    "Bytes in (total)",
                    &format_bytes(p.total_bytes_in as f64),
                );
                perf_row(
                    ui,
                    "Bytes out (total)",
                    &format_bytes(p.total_bytes_out as f64),
                );
            });

        ui.add_space(8.0);

        // disk section
        ui.label(
            egui::RichText::new("Disk I/O")
                .size(11.0)
                .color(LABEL_COLOR),
        );
        egui::Grid::new("perf_disk")
            .min_col_width(120.0)
            .show(ui, |ui: &mut egui::Ui| {
                perf_row(ui, "Writes", "0 (no local caching)");
                perf_row(ui, "Reads", "0 (no local caching)");
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

fn perf_row(ui: &mut egui::Ui, label: &str, value: &str) {
    ui.label(egui::RichText::new(label).size(11.0).color(LABEL_COLOR));
    ui.label(egui::RichText::new(value).size(11.0));
    ui.end_row();
}

fn format_bytes(bytes: f64) -> String {
    if bytes < 1024.0 {
        format!("{:.0} B", bytes)
    } else if bytes < 1024.0 * 1024.0 {
        format!("{:.1} KB", bytes / 1024.0)
    } else if bytes < 1024.0 * 1024.0 * 1024.0 {
        format!("{:.1} MB", bytes / (1024.0 * 1024.0))
    } else {
        format!("{:.1} GB", bytes / (1024.0 * 1024.0 * 1024.0))
    }
}
