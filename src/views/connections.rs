use iced::widget::{button, column, row, scrollable, text, text_input};
use iced::{Element, Length};

use crate::app::{App, Message};

impl App {
    pub fn connections_view(&self) -> Element<'_, Message> {
        let mut layout = column![].spacing(8).padding(12).width(Length::Fill);

        // -- connections section --
        layout = layout.push(text("Connections").size(18));
        layout = layout.push(iced::widget::rule::horizontal(1));

        if self.connections.is_empty() {
            layout = layout.push(text("No connections saved. Add one below.").size(12));
        } else {
            let mut list = column![].spacing(4);
            for conn in &self.connections {
                let is_active = self.active_connection.as_deref() == Some(&conn.name);
                let cred_label = conn
                    .credential
                    .as_deref()
                    .unwrap_or("anonymous");
                let status = if is_active { " [connected]" } else { "" };
                let label = format!("{} - {} ({}){}", conn.name, conn.endpoint, cred_label, status);

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
                text_input("credential (optional)", &self.new_conn_credential)
                    .on_input(Message::NewConnCredentialChanged)
                    .size(11)
                    .width(150),
                button(text("add").size(10))
                    .style(button::primary)
                    .on_press(Message::AddConnection),
            ]
            .spacing(4),
        );

        layout = layout.push(iced::widget::rule::horizontal(1));

        // -- credentials section --
        layout = layout.push(text("Credentials").size(18));
        layout = layout.push(iced::widget::rule::horizontal(1));

        if self.credentials.is_empty() {
            layout = layout.push(text("No credentials saved. Add one below.").size(12));
        } else {
            let mut list = column![].spacing(4);
            for cred in &self.credentials {
                let label = format!(
                    "{} - {} ({})",
                    cred.name, cred.access_key_id, cred.region
                );
                let r = row![
                    text(label).size(11),
                    button(text("delete").size(10))
                        .style(button::text)
                        .on_press(Message::RemoveCredential(cred.name.clone())),
                ]
                .spacing(8);
                list = list.push(r);
            }
            layout = layout.push(scrollable(list).height(Length::Shrink));
        }

        // add credential form
        layout = layout.push(text("Add credential").size(13));
        layout = layout.push(
            row![
                text_input("name", &self.new_cred_name)
                    .on_input(Message::NewCredNameChanged)
                    .size(11)
                    .width(120),
                text_input("access key id", &self.new_cred_access_key)
                    .on_input(Message::NewCredAccessKeyChanged)
                    .size(11)
                    .width(150),
                text_input("secret key", &self.new_cred_secret_key)
                    .on_input(Message::NewCredSecretKeyChanged)
                    .secure(true)
                    .size(11)
                    .width(150),
                text_input("region", &self.new_cred_region)
                    .on_input(Message::NewCredRegionChanged)
                    .size(11)
                    .width(100),
                button(text("add").size(10))
                    .style(button::primary)
                    .on_press(Message::AddCredential),
            ]
            .spacing(4),
        );

        layout.into()
    }
}
