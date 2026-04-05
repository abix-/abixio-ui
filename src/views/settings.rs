use iced::widget::{button, column, row, text};
use iced::{Element, Length};

use crate::app::{App, AppTheme, Message};

fn format_bytes(n: u64) -> String {
    if n < 1024 {
        format!("{} B", n)
    } else if n < 1024 * 1024 {
        format!("{:.1} KB", n as f64 / 1024.0)
    } else if n < 1024 * 1024 * 1024 {
        format!("{:.1} MB", n as f64 / (1024.0 * 1024.0))
    } else {
        format!("{:.2} GB", n as f64 / (1024.0 * 1024.0 * 1024.0))
    }
}

impl App {
    pub fn settings_view(&self) -> Element<'_, Message> {
        let p = &self.perf;

        column![
            text("Settings").size(18),
            iced::widget::rule::horizontal(1),
            // appearance
            text("Appearance").size(13),
            row![
                text("Theme").size(11),
                button(text("Dark").size(11))
                    .style(if self.theme == AppTheme::Dark {
                        button::primary
                    } else {
                        button::secondary
                    })
                    .on_press(Message::SetTheme(AppTheme::Dark)),
                button(text("Light").size(11))
                    .style(if self.theme == AppTheme::Light {
                        button::primary
                    } else {
                        button::secondary
                    })
                    .on_press(Message::SetTheme(AppTheme::Light)),
            ]
            .spacing(8),
            iced::widget::rule::horizontal(1),
            // connection
            text("Connection").size(13),
            row![
                text("Active").size(11),
                text(self.active_connection.as_deref().unwrap_or("none")).size(11)
            ]
            .spacing(8),
            row![text("Endpoint").size(11), text(&self.endpoint).size(11)].spacing(8),
            iced::widget::rule::horizontal(1),
            // performance
            text("Performance").size(13),
            text("Updates").size(11),
            row![
                text("Updates/sec (current)").size(10),
                text(format!("{:.0}", p.current_fps())).size(10)
            ]
            .spacing(8),
            row![
                text("Updates/sec (5m avg)").size(10),
                text(format!("{:.0}", p.avg_fps())).size(10)
            ]
            .spacing(8),
            row![
                text("Update time").size(10),
                text(format!("{:.1} ms", p.current_frame_ms())).size(10)
            ]
            .spacing(8),
            row![
                text("Total updates").size(10),
                text(format!("{}", p.total_frames())).size(10)
            ]
            .spacing(8),
            row![
                text("Updates (5m)").size(10),
                text(format!("{}", p.repaints_5m())).size(10)
            ]
            .spacing(8),
            text("Network").size(11),
            row![
                text("Requests (total)").size(10),
                text(format!("{}", p.total_requests())).size(10)
            ]
            .spacing(8),
            row![
                text("Bytes sent").size(10),
                text(format_bytes(p.total_bytes_out())).size(10)
            ]
            .spacing(8),
            row![
                text("Bytes received").size(10),
                text(format_bytes(p.total_bytes_in())).size(10)
            ]
            .spacing(8),
            iced::widget::rule::horizontal(1),
            text("About").size(13),
            text(format!("abixio-ui v{}", env!("CARGO_PKG_VERSION"))).size(11),
        ]
        .spacing(4)
        .padding(12)
        .width(Length::Fill)
        .into()
    }
}

#[cfg(test)]
mod tests {
    use super::format_bytes;

    #[test]
    fn format_bytes_zero() {
        assert_eq!(format_bytes(0), "0 B");
    }

    #[test]
    fn format_bytes_small() {
        assert_eq!(format_bytes(512), "512 B");
    }

    #[test]
    fn format_bytes_one_byte() {
        assert_eq!(format_bytes(1), "1 B");
    }

    #[test]
    fn format_bytes_just_under_1kb() {
        assert_eq!(format_bytes(1023), "1023 B");
    }

    #[test]
    fn format_bytes_exactly_1kb() {
        assert_eq!(format_bytes(1024), "1.0 KB");
    }

    #[test]
    fn format_bytes_1_5kb() {
        assert_eq!(format_bytes(1536), "1.5 KB");
    }

    #[test]
    fn format_bytes_just_under_1mb() {
        assert_eq!(format_bytes(1024 * 1024 - 1), "1024.0 KB");
    }

    #[test]
    fn format_bytes_exactly_1mb() {
        assert_eq!(format_bytes(1024 * 1024), "1.0 MB");
    }

    #[test]
    fn format_bytes_2mb() {
        assert_eq!(format_bytes(2 * 1024 * 1024), "2.0 MB");
    }

    #[test]
    fn format_bytes_exactly_1gb() {
        assert_eq!(format_bytes(1024 * 1024 * 1024), "1.00 GB");
    }

    #[test]
    fn format_bytes_3gb() {
        assert_eq!(format_bytes(3 * 1024 * 1024 * 1024), "3.00 GB");
    }
}
