use iced::widget::{button, column, row, scrollable, text, text_input};
use iced::{Element, Length};

use crate::app::{App, Message};

impl App {
    pub fn connections_view(&self) -> Element<'_, Message> {
        let mut layout = column![].spacing(8).padding(12).width(Length::Fill);

        layout = layout.push(text("Connections").size(18));
        layout = layout.push(iced::widget::rule::horizontal(1));

        if self.settings.connections.is_empty() {
            layout = layout.push(text("No connections saved. Add one below.").size(12));
        } else {
            let mut list = column![].spacing(4);
            for conn in &self.settings.connections {
                let is_active = self.active_connection.as_deref() == Some(&conn.name);
                let has_keys = conn.resolve_keys().ok().flatten().is_some();
                let auth_label = if has_keys { "authenticated" } else { "anonymous" };
                let status = if is_active { " [connected]" } else { "" };
                let label = format!(
                    "{} - {} ({}, {}){}", conn.name, conn.endpoint, conn.region, auth_label, status
                );

                let mut r = row![text(label).size(11)].spacing(8);

                if !is_active {
                    r = r.push(
                        button(text("connect").size(10))
                            .style(button::primary)
                            .on_press(Message::ConnectTo(conn.name.clone())),
                    );
                }
                r = r.push(
                    button(text("delete").size(10))
                        .style(button::text)
                        .on_press(Message::RemoveConnection(conn.name.clone())),
                );

                list = list.push(r);
            }
            layout = layout.push(scrollable(list).height(Length::Shrink));
        }

        layout = layout.push(iced::widget::rule::horizontal(1));

        // add connection form
        layout = layout.push(text("Add connection").size(13));
        layout = layout.push(
            row![
                text_input("name", &self.new_conn_name)
                    .on_input(Message::NewConnNameChanged)
                    .size(11)
                    .width(120),
                text_input("http://endpoint:9000", &self.new_conn_endpoint)
                    .on_input(Message::NewConnEndpointChanged)
                    .size(11)
                    .width(200),
                text_input("region", &self.new_conn_region)
                    .on_input(Message::NewConnRegionChanged)
                    .size(11)
                    .width(100),
            ]
            .spacing(4),
        );
        layout = layout.push(
            row![
                text_input("access key (optional)", &self.new_conn_access_key)
                    .on_input(Message::NewConnAccessKeyChanged)
                    .size(11)
                    .width(200),
                text_input("secret key (optional)", &self.new_conn_secret_key)
                    .on_input(Message::NewConnSecretKeyChanged)
                    .secure(true)
                    .size(11)
                    .width(200),
                button(text("add").size(10))
                    .style(button::primary)
                    .on_press(Message::AddConnection),
            ]
            .spacing(4),
        );

        layout = layout.push(iced::widget::rule::horizontal(1));
        layout = layout.push(text("Leave access key and secret key empty for anonymous access. Keys are stored in the OS keychain, never on disk.").size(10));

        layout.into()
    }
}
