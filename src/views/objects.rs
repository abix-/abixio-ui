use iced::widget::{button, checkbox, column, container, row, scrollable, text, text_input};
use iced::{Element, Length};

use crate::app::{App, Message, Selection};

impl App {
    pub fn object_list_view(&self) -> Element<'_, Message> {
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

        let filter_input = text_input("filter...", &self.object_filter)
            .on_input(Message::ObjectFilterChanged)
            .size(11)
            .width(140);

        let mut find_btn = button(text("Find").size(11));
        if !self.object_filter.is_empty() && !self.finding {
            find_btn = find_btn.on_press(Message::Find);
        }

        let mut actions = row![
            filter_input,
            find_btn,
        ]
        .spacing(4);

        // selection controls
        let sel_count = self.selected_keys.len();
        if sel_count > 0 {
            actions = actions.push(
                button(text(format!("Delete {} selected", sel_count)).size(11))
                    .on_press(Message::OpenBulkDeleteModal),
            );
            actions = actions.push(
                button(text("Clear sel").size(11))
                    .on_press(Message::ClearObjectSelection),
            );
        } else {
            actions = actions.push(
                button(text("Select All").size(11))
                    .on_press(Message::SelectAllObjects),
            );
        }

        actions = actions
            .push(button(text("Upload").size(11)).on_press(Message::Upload))
            .push(button(text("Import Folder").size(11)).on_press(Message::OpenImportFolder))
            .push(button(text("Export Prefix").size(11)).on_press(Message::OpenExportPrefix))
            .push(button(text("Refresh").size(11)).on_press(Message::Refresh));

        let toolbar = row![breadcrumbs, iced::widget::space::horizontal(), actions]
            .spacing(8)
            .padding(4);

        let mut content = column![toolbar, iced::widget::rule::horizontal(1)].spacing(4);

        // find results mode
        if let Some(find_result) = &self.find_results {
            content = content.push(
                row![
                    button(text("Clear find").size(11)).on_press(Message::ClearFind),
                    text(match find_result {
                        Ok(r) => format!("{} results", r.objects.len()),
                        Err(e) => format!("Error: {}", e),
                    })
                    .size(11),
                ]
                .spacing(8),
            );

            if let Ok(result) = find_result {
                // header
                content = content.push(
                    row![
                        text("").size(11).width(20),
                        text("Key").size(11).width(Length::FillPortion(4)),
                        text("Size").size(11).width(Length::FillPortion(1)),
                        text("Modified").size(11).width(Length::FillPortion(2)),
                    ]
                    .spacing(8)
                    .padding([2, 4]),
                );

                for obj in &result.objects {
                    let is_selected = matches!(
                        &self.selection,
                        Selection::Object { key, .. } if *key == obj.key
                    );
                    let key = obj.key.clone();
                    let key_for_check = obj.key.clone();
                    let checked = self.selected_keys.contains(&obj.key);
                    content = content.push(
                        row![
                            checkbox(checked)
                                .on_toggle(move |_| Message::ToggleObjectSelected(
                                    key_for_check.clone()
                                ))
                                .size(14),
                            button(
                                row![
                                    text(&obj.key).size(11).width(Length::FillPortion(4)),
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
                        ]
                        .spacing(4),
                    );
                }
            }

            return scrollable(content.padding(4))
                .width(Length::Fill)
                .height(Length::Fill)
                .into();
        }

        if self.finding {
            content = content.push(text("Searching...").size(12));
        } else if self.loading_objects {
            content = content.push(text("Loading...").size(12));
        } else if let Some(Ok(result)) = &self.objects {
            let filter = self.object_filter.to_ascii_lowercase();
            let has_filter = !filter.is_empty();

            // folders (no checkboxes on folders)
            let mut folder_count = 0;
            for cp in &result.common_prefixes {
                let display = cp.strip_prefix(&self.current_prefix).unwrap_or(cp);
                if has_filter && !display.to_ascii_lowercase().contains(&filter) {
                    continue;
                }
                folder_count += 1;
                let prefix = cp.clone();
                let prefix_del = cp.clone();
                content = content.push(
                    row![
                        button(text(format!("  {}", display)).size(12))
                            .width(Length::Fill)
                            .style(button::text)
                            .on_press(Message::NavigatePrefix(prefix)),
                        button(text("Del").size(10))
                            .on_press(Message::OpenPrefixDeleteModal(prefix_del)),
                    ]
                    .spacing(4),
                );
            }

            // header
            content = content.push(
                row![
                    text("").size(11).width(20),
                    text("Name").size(11).width(Length::FillPortion(4)),
                    text("Size").size(11).width(Length::FillPortion(1)),
                    text("Modified").size(11).width(Length::FillPortion(2)),
                ]
                .spacing(8)
                .padding([2, 4]),
            );

            // objects with checkboxes
            let mut obj_shown = 0;
            for obj in &result.objects {
                let display_key = obj
                    .key
                    .strip_prefix(&self.current_prefix)
                    .unwrap_or(&obj.key);
                if has_filter && !display_key.to_ascii_lowercase().contains(&filter) {
                    continue;
                }
                obj_shown += 1;
                let is_selected = matches!(
                    &self.selection,
                    Selection::Object { key, .. } if *key == obj.key
                );
                let key = obj.key.clone();
                let key_for_check = obj.key.clone();
                let checked = self.selected_keys.contains(&obj.key);
                content = content.push(
                    row![
                        checkbox(checked)
                            .on_toggle(move |_| Message::ToggleObjectSelected(
                                key_for_check.clone()
                            ))
                            .size(14),
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
                    ]
                    .spacing(4),
                );
            }

            if has_filter {
                let total_folders = result.common_prefixes.len();
                let total_objects = result.objects.len();
                content = content.push(
                    text(format!(
                        "showing {} of {} folders, {} of {} objects",
                        folder_count, total_folders, obj_shown, total_objects
                    ))
                    .size(10),
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
