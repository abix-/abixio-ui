use iced::widget::{button, column, text};
use iced::{Element, Length};

use crate::app::{App, Message, Section};

impl App {
    pub fn sidebar_view(&self) -> Element<'_, Message> {
        let mut col = column![self.nav_btn("B", Section::Browse),]
            .spacing(4)
            .padding(4);

        if self.is_abixio {
            col = col.push(self.nav_btn("D", Section::Disks));
            if self.server_status.as_ref().is_some_and(|s| s.cluster.enabled) {
                col = col.push(self.nav_btn("C", Section::Cluster));
            }
            col = col.push(self.nav_btn("H", Section::Healing));
        }

        col = col.push(self.nav_btn("Y", Section::Sync));
        col = col.push(self.nav_btn("+", Section::Connections));
        col = col.push(self.nav_btn("T", Section::Testing));
        col = col.push(self.nav_btn("R", Section::Server));
        col = col.push(iced::widget::space::vertical());
        col = col.push(self.nav_btn("S", Section::Settings));

        col.height(Length::Fill).into()
    }

    fn nav_btn(&self, label: &str, section: Section) -> Element<'_, Message> {
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
