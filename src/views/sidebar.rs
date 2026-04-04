use eframe::egui;

use crate::app::{App, Section};

impl App {
    pub fn sidebar(&mut self, ui: &mut egui::Ui) {
        ui.vertical_centered(|ui: &mut egui::Ui| {
            ui.add_space(8.0);

            self.nav_button(ui, "B", "Browse", Section::Browse, true);
            ui.add_space(4.0);

            if self.is_abixio {
                self.nav_button(ui, "D", "Disks", Section::Disks, true);
                ui.add_space(4.0);
                self.nav_button(ui, "C", "Config", Section::Config, true);
                ui.add_space(4.0);
                self.nav_button(ui, "H", "Healing", Section::Healing, true);
                ui.add_space(4.0);
            }

            // connections always at bottom
            ui.with_layout(
                egui::Layout::bottom_up(egui::Align::Center),
                |ui: &mut egui::Ui| {
                    ui.add_space(8.0);
                    self.nav_button(ui, "+", "Connections", Section::Connections, true);
                },
            );
        });
    }

    fn nav_button(
        &mut self,
        ui: &mut egui::Ui,
        icon: &str,
        tooltip: &str,
        section: Section,
        _enabled: bool,
    ) {
        let active = self.current_section == section;
        let btn = egui::Button::new(egui::RichText::new(icon).size(16.0).strong().color(
            if active {
                egui::Color32::WHITE
            } else {
                egui::Color32::GRAY
            },
        ))
        .min_size(egui::vec2(32.0, 32.0))
        .fill(if active {
            ui.visuals().selection.bg_fill
        } else {
            egui::Color32::TRANSPARENT
        });

        if ui.add(btn).on_hover_text(tooltip).clicked() {
            self.current_section = section;
        }
    }
}
