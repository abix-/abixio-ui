use iced::widget::{button, checkbox, column, row, scrollable, text, text_input};
use iced::{Element, Length};

use crate::app::{App, Message};

impl App {
    pub fn server_view(&self) -> Element<'_, Message> {
        let mut layout = column![].spacing(8).padding(12).width(Length::Fill);

        layout = layout.push(text("Server").size(18));
        layout = layout.push(iced::widget::rule::horizontal(1));

        // binary path
        layout = layout.push(text("Binary").size(13));
        layout = layout.push(
            row![
                text_input("auto-detect", &self.server_config.binary_path)
                    .on_input(Message::ServerBinaryPathChanged)
                    .size(10)
                    .width(Length::Fill),
            ]
            .spacing(4),
        );
        if let Some(path) = &self.server_binary_found {
            layout = layout.push(
                text(format!("found: {}", path.display()))
                    .size(9),
            );
        } else {
            layout = layout
                .push(text("not found. install abixio or set the path above.").size(9));
        }

        layout = layout.push(iced::widget::rule::horizontal(1));

        // volumes
        layout = layout.push(
            row![
                text("Volumes").size(13),
                button(text("+").size(11)).on_press(Message::ServerAddVolume),
            ]
            .spacing(8),
        );
        for (i, vol) in self.server_config.volumes.iter().enumerate() {
            layout = layout.push(
                row![
                    text_input("path", vol)
                        .on_input(move |v| Message::ServerVolumeChanged(i, v))
                        .size(10)
                        .width(Length::Fill),
                    button(text("...").size(10)).on_press(Message::ServerPickVolume(i)),
                    button(text("x").size(10))
                        .on_press(Message::ServerRemoveVolume(i))
                        .style(button::text),
                ]
                .spacing(4),
            );
        }
        if self.server_config.volumes.is_empty() {
            layout = layout.push(text("no volumes configured. add at least one.").size(9));
        }

        layout = layout.push(iced::widget::rule::horizontal(1));

        // options
        layout = layout.push(text("Options").size(13));
        layout = layout.push(
            row![
                text("Listen").size(11),
                text_input(":10000", &self.server_config.listen)
                    .on_input(Message::ServerListenChanged)
                    .size(10)
                    .width(120),
            ]
            .spacing(8),
        );
        layout = layout.push(
            row![
                text("Scan interval").size(11),
                text_input("10m", &self.server_config.scan_interval)
                    .on_input(Message::ServerScanIntervalChanged)
                    .size(10)
                    .width(80),
            ]
            .spacing(8),
        );
        layout = layout.push(
            row![
                text("Heal interval").size(11),
                text_input("24h", &self.server_config.heal_interval)
                    .on_input(Message::ServerHealIntervalChanged)
                    .size(10)
                    .width(80),
            ]
            .spacing(8),
        );
        layout = layout.push(
            row![
                text("MRF workers").size(11),
                text_input("2", &self.server_config.mrf_workers.to_string())
                    .on_input(Message::ServerMrfWorkersChanged)
                    .size(10)
                    .width(60),
            ]
            .spacing(8),
        );
        layout = layout.push(
            row![
                checkbox(self.server_config.no_auth)
                    .on_toggle(Message::ServerNoAuthToggled)
                    .size(14),
                text("No auth").size(11),
            ]
            .spacing(4),
        );
        layout = layout.push(
            row![
                checkbox(self.server_config.auto_connect)
                    .on_toggle(Message::ServerAutoConnectToggled)
                    .size(14),
                text("Auto-connect after launch").size(11),
            ]
            .spacing(4),
        );

        layout = layout.push(iced::widget::rule::horizontal(1));

        // actions
        let start_enabled = self.server_binary_found.is_some()
            && !self.server_running
            && self.server_config.volumes.iter().any(|v| !v.trim().is_empty());

        layout = layout.push(
            row![
                if start_enabled {
                    button(text("Start Server").size(11)).on_press(Message::StartServer)
                } else {
                    button(text("Start Server").size(11))
                },
                if self.server_running {
                    button(text("Stop Server").size(11)).on_press(Message::StopServer)
                } else {
                    button(text("Stop Server").size(11))
                },
                button(text("Save Config").size(11)).on_press(Message::ServerSaveConfig),
            ]
            .spacing(8),
        );

        if self.server_running {
            layout = layout.push(text("server running").size(10));
        }

        // log output
        if !self.server_log.is_empty() {
            layout = layout.push(iced::widget::rule::horizontal(1));
            layout = layout.push(text("Log").size(13));

            let log_text = self.server_log.join("\n");
            let log_view = scrollable(text(log_text).size(9).width(Length::Fill))
                .height(Length::Fill);
            layout = layout.push(log_view);
        }

        layout.height(Length::Fill).into()
    }
}
