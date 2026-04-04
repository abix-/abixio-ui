use iced::widget::{button, column, row, text};
use iced::{Element, Length};

use crate::app::{App, Message};

impl App {
    pub fn healing_view(&self) -> Element<'_, Message> {
        let mut layout = column![].spacing(8).padding(12).width(Length::Fill);

        layout = layout.push(
            row![
                text("Healing").size(18),
                button(text("refresh").size(10))
                    .style(button::secondary)
                    .on_press(Message::RefreshHealStatus),
            ]
            .spacing(8),
        );
        layout = layout.push(iced::widget::rule::horizontal(1));

        match &self.heal_data {
            None => {
                layout = layout.push(text("Loading...").size(12));
            }
            Some(Err(e)) => {
                layout = layout.push(text(format!("Error: {}", e)).size(12));
            }
            Some(Ok(data)) => {
                // MRF section
                layout = layout.push(text("MRF Queue").size(14));
                layout = layout.push(
                    row![
                        text("Pending items").size(11),
                        text(format!("{}", data.mrf_pending)).size(11),
                    ]
                    .spacing(8),
                );
                layout = layout.push(
                    row![
                        text("Workers").size(11),
                        text(format!("{}", data.mrf_workers)).size(11),
                    ]
                    .spacing(8),
                );

                layout = layout.push(iced::widget::rule::horizontal(1));

                // Scanner section
                layout = layout.push(text("Integrity Scanner").size(14));
                let s = &data.scanner;
                layout = layout.push(
                    row![
                        text("Status").size(11),
                        text(if s.running { "running" } else { "stopped" }).size(11),
                    ]
                    .spacing(8),
                );
                layout = layout.push(
                    row![
                        text("Scan interval").size(11),
                        text(&s.scan_interval).size(11),
                    ]
                    .spacing(8),
                );
                layout = layout.push(
                    row![
                        text("Heal interval").size(11),
                        text(&s.heal_interval).size(11),
                    ]
                    .spacing(8),
                );
                layout = layout.push(
                    row![
                        text("Objects scanned").size(11),
                        text(format!("{}", s.objects_scanned)).size(11),
                    ]
                    .spacing(8),
                );
                layout = layout.push(
                    row![
                        text("Objects healed").size(11),
                        text(format!("{}", s.objects_healed)).size(11),
                    ]
                    .spacing(8),
                );
                if s.last_scan_started > 0 {
                    layout = layout.push(
                        row![
                            text("Last scan").size(11),
                            text(format!("{}s ago", s.last_scan_started)).size(11),
                        ]
                        .spacing(8),
                    );
                }
                if s.last_scan_duration_secs > 0 {
                    layout = layout.push(
                        row![
                            text("Last scan duration").size(11),
                            text(format!("{}s", s.last_scan_duration_secs)).size(11),
                        ]
                        .spacing(8),
                    );
                }
            }
        }

        // server info from status probe
        if let Some(status) = &self.server_status {
            layout = layout.push(iced::widget::rule::horizontal(1));
            layout = layout.push(text("Server").size(14));
            layout = layout
                .push(row![text("Version").size(11), text(&status.version).size(11),].spacing(8));
            layout = layout.push(
                row![
                    text("Uptime").size(11),
                    text(format_uptime(status.uptime_secs)).size(11),
                ]
                .spacing(8),
            );
            layout = layout.push(
                row![
                    text("Erasure").size(11),
                    text(format!(
                        "{} data + {} parity ({} disks)",
                        status.data_shards, status.parity_shards, status.total_disks
                    ))
                    .size(11),
                ]
                .spacing(8),
            );
        }

        layout.into()
    }
}

fn format_uptime(secs: u64) -> String {
    let days = secs / 86400;
    let hours = (secs % 86400) / 3600;
    let mins = (secs % 3600) / 60;
    if days > 0 {
        format!("{}d {}h {}m", days, hours, mins)
    } else if hours > 0 {
        format!("{}h {}m", hours, mins)
    } else {
        format!("{}m", mins)
    }
}
