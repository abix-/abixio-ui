use iced::widget::{button, column, container, row, scrollable, text};
use iced::{Element, Length};

use crate::app::{App, Message, Selection};

impl App {
    pub fn object_list_view(&self) -> Element<Message> {
        let bucket = match &self.selected_bucket {
            Some(b) => b.clone(),
            None => {
                return container(text("Select a bucket").size(14))
                    .padding(20)
                    .width(Length::Fill)
                    .height(Length::Fill)
                    .into();
            }
        };

        // breadcrumb bar
        let mut breadcrumbs = row![
            button(text(bucket.clone()).size(12))
                .style(button::text)
                .on_press(Message::NavigatePrefix(String::new())),
        ]
        .spacing(2);

        if !self.current_prefix.is_empty() {
            let parts: Vec<&str> = self
                .current_prefix
                .trim_end_matches('/')
                .split('/')
                .collect();
            for (i, part) in parts.iter().enumerate() {
                let prefix = parts[..=i].join("/") + "/";
                breadcrumbs = breadcrumbs.push(text("/").size(12));
                breadcrumbs = breadcrumbs.push(
                    button(text(*part).size(12))
                        .style(button::text)
                        .on_press(Message::NavigatePrefix(prefix)),
                );
            }
        }

        let actions = row![
            button(text("Upload").size(11)).on_press(Message::Upload),
            button(text("Refresh").size(11)).on_press(Message::Refresh),
        ]
        .spacing(4);

        let toolbar = row![breadcrumbs, iced::widget::space::horizontal(), actions]
            .spacing(8)
            .padding(4);

        let mut content = column![toolbar, iced::widget::rule::horizontal(1)].spacing(4);

        if self.loading_objects {
            content = content.push(text("Loading...").size(12));
        } else if let Some(Ok(result)) = &self.objects {
            // folders
            for cp in &result.common_prefixes {
                let display = cp.strip_prefix(&self.current_prefix).unwrap_or(cp);
                let prefix = cp.clone();
                content = content.push(
                    button(text(format!("  {}", display)).size(12))
                        .width(Length::Fill)
                        .style(button::text)
                        .on_press(Message::NavigatePrefix(prefix)),
                );
            }

            // header
            content = content.push(
                row![
                    text("Name").size(11).width(Length::FillPortion(4)),
                    text("Size").size(11).width(Length::FillPortion(1)),
                    text("Modified").size(11).width(Length::FillPortion(2)),
                ]
                .spacing(8)
                .padding([2, 4]),
            );

            // objects
            for obj in &result.objects {
                let display_key = obj
                    .key
                    .strip_prefix(&self.current_prefix)
                    .unwrap_or(&obj.key);
                let is_selected = matches!(
                    &self.selection,
                    Selection::Object { key, .. } if *key == obj.key
                );
                let key = obj.key.clone();
                content = content.push(
                    button(
                        row![
                            text(display_key).size(11).width(Length::FillPortion(4)),
                            text(format_size(obj.size))
                                .size(11)
                                .width(Length::FillPortion(1)),
                            text(&obj.last_modified)
                                .size(11)
                                .width(Length::FillPortion(2)),
                        ]
                        .spacing(8),
                    )
                    .width(Length::Fill)
                    .style(if is_selected {
                        button::primary
                    } else {
                        button::text
                    })
                    .on_press(Message::SelectObject(key)),
                );
            }
        } else if let Some(Err(e)) = &self.objects {
            content = content.push(text(format!("Error: {}", e)).size(11));
        }

        scrollable(content.padding(4))
            .width(Length::Fill)
            .height(Length::Fill)
            .into()
    }
}

fn format_size(bytes: u64) -> String {
    if bytes < 1024 {
        format!("{} B", bytes)
    } else if bytes < 1024 * 1024 {
        format!("{:.1} KB", bytes as f64 / 1024.0)
    } else if bytes < 1024 * 1024 * 1024 {
        format!("{:.1} MB", bytes as f64 / (1024.0 * 1024.0))
    } else {
        format!("{:.1} GB", bytes as f64 / (1024.0 * 1024.0 * 1024.0))
    }
}
