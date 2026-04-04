use iced::widget::{button, column, text};
use iced::{Element, Length};

use crate::app::{App, Message, Section};

impl App {
    pub fn sidebar_view(&self) -> Element<Message> {
        column![
            self.nav_btn("B", Section::Browse),
            self.nav_btn("+", Section::Connections),
            iced::widget::space::vertical(),
            self.nav_btn("S", Section::Settings),
        ]
        .spacing(4)
        .padding(4)
        .height(Length::Fill)
        .into()
    }

    fn nav_btn(&self, label: &str, section: Section) -> Element<Message> {
        let is_active = self.section == section;
        button(text(label.to_string()).size(14).center())
            .width(32)
            .height(32)
            .style(if is_active {
                button::primary
            } else {
                button::text
            })
            .on_press(Message::SelectSection(section))
            .into()
    }
}
