use iced::widget::{button, column, row, text};
use iced::{Element, Length};

use crate::app::{App, AppTheme, Message};

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
                text("Requests (5m)").size(10),
                text(format!("{}", p.requests_5m())).size(10)
            ]
            .spacing(8),
            row![
                text("Requests (total)").size(10),
                text(format!("{}", p.total_requests)).size(10)
            ]
            .spacing(8),
            text("Disk I/O").size(11),
            row![text("Writes").size(10), text("0 (no caching)").size(10)].spacing(8),
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
