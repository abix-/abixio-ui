use iced::widget::{button, column, row, text};
use iced::{Element, Length};

use crate::app::{App, Message};

impl App {
    pub fn disks_view(&self) -> Element<'_, Message> {
        let mut layout = column![].spacing(8).padding(12).width(Length::Fill);

        layout = layout.push(
            row![
                text("Disks").size(18),
                button(text("refresh").size(10))
                    .style(button::secondary)
                    .on_press(Message::RefreshDisks),
            ]
            .spacing(8),
        );
        layout = layout.push(iced::widget::rule::horizontal(1));

        match &self.disks_data {
            None => {
                layout = layout.push(text("Loading...").size(12));
            }
            Some(Err(e)) => {
                layout = layout.push(text(format!("Error: {}", e)).size(12));
            }
            Some(Ok(data)) => {
                // header row
                layout = layout.push(
                    row![
                        text("#").size(10).width(30),
                        text("Path").size(10).width(200),
                        text("Status").size(10).width(60),
                        text("Total").size(10).width(80),
                        text("Used").size(10).width(80),
                        text("Free").size(10).width(80),
                        text("Buckets").size(10).width(60),
                        text("Objects").size(10).width(60),
                    ]
                    .spacing(4),
                );
                layout = layout.push(iced::widget::rule::horizontal(1));

                for disk in &data.disks {
                    let status_label = if disk.online { "online" } else { "OFFLINE" };
                    layout = layout.push(
                        row![
                            text(format!("{}", disk.index)).size(10).width(30),
                            text(&disk.path).size(10).width(200),
                            text(status_label).size(10).width(60),
                            text(human_bytes(disk.total_bytes)).size(10).width(80),
                            text(human_bytes(disk.used_bytes)).size(10).width(80),
                            text(human_bytes(disk.free_bytes)).size(10).width(80),
                            text(format!("{}", disk.bucket_count)).size(10).width(60),
                            text(format!("{}", disk.object_count)).size(10).width(60),
                        ]
                        .spacing(4),
                    );
                }

                // summary
                if !data.disks.is_empty() {
                    layout = layout.push(iced::widget::rule::horizontal(1));
                    let total: u64 = data.disks.iter().map(|d| d.total_bytes).sum();
                    let used: u64 = data.disks.iter().map(|d| d.used_bytes).sum();
                    let free: u64 = data.disks.iter().map(|d| d.free_bytes).sum();
                    let online = data.disks.iter().filter(|d| d.online).count();
                    layout = layout.push(
                        text(format!(
                            "{} disks ({} online) | {} total, {} used, {} free",
                            data.disks.len(),
                            online,
                            human_bytes(total),
                            human_bytes(used),
                            human_bytes(free),
                        ))
                        .size(11),
                    );
                }
            }
        }

        layout.into()
    }
}

fn human_bytes(bytes: u64) -> String {
    const KB: u64 = 1024;
    const MB: u64 = 1024 * KB;
    const GB: u64 = 1024 * MB;
    const TB: u64 = 1024 * GB;

    if bytes >= TB {
        format!("{:.1} TB", bytes as f64 / TB as f64)
    } else if bytes >= GB {
        format!("{:.1} GB", bytes as f64 / GB as f64)
    } else if bytes >= MB {
        format!("{:.1} MB", bytes as f64 / MB as f64)
    } else if bytes >= KB {
        format!("{:.1} KB", bytes as f64 / KB as f64)
    } else {
        format!("{} B", bytes)
    }
}
