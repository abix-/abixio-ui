use iced::widget::{button, column, container, row, scrollable, space, text};
use iced::{Element, Length};

use crate::app::{App, Message, Selection};

impl App {
    pub fn detail_view(&self) -> Element<'_, Message> {
        let mut col = column![].spacing(4).padding(8);

        match &self.selection {
            Selection::None => {}
            Selection::Bucket(name) => {
                col = col.push(text(name).size(16));
                col = col.push(text("Bucket").size(11));
                col = col.push(iced::widget::rule::horizontal(1));
            }
            Selection::Object { bucket, key } => {
                let short = key.rsplit('/').next().unwrap_or(key);
                col = col.push(text(short).size(16));
                col = col.push(text(format!("{} / {}", bucket, key)).size(10));
                col = col.push(iced::widget::rule::horizontal(1));

                if self.loading_detail {
                    col = col.push(text("Loading...").size(11));
                } else if let Some(Ok(detail)) = &self.detail {
                    col = col.push(section("Overview"));
                    col = col.push(meta_row("Size", &format_size(detail.size)));
                    col = col.push(meta_row("Type", &detail.content_type));
                    col = col.push(meta_row("Modified", &detail.last_modified));
                    col = col.push(meta_row("ETag", &detail.etag));

                    col = col.push(section("Storage"));
                    col = col.push(meta_row("Bucket", bucket));
                    col = col.push(meta_row("Key", key));

                    col = col.push(section("HTTP Headers"));
                    for (name, value) in &detail.headers {
                        col = col.push(meta_row(name, value));
                    }

                    col = col.push(section("Actions"));
                    col = col.push(
                        row![
                            button(text("Download").size(11))
                                .on_press(Message::Download(bucket.clone(), key.clone())),
                            button(text("Copy").size(11)).on_press(Message::OpenCopyObject),
                            button(text("Delete").size(11))
                                .on_press(Message::Delete(bucket.clone(), key.clone())),
                        ]
                        .spacing(4),
                    );

                    if self.is_abixio {
                        col = col.push(section("AbixIO"));

                        if self.loading_object_inspect {
                            col = col.push(text("Loading shard inspection...").size(11));
                        } else if let Some(Ok(inspect)) = &self.object_inspect {
                            col = col.push(meta_row(
                                "Erasure",
                                &format!(
                                    "{} data + {} parity",
                                    inspect.erasure.data, inspect.erasure.parity
                                ),
                            ));
                            col = col.push(meta_row(
                                "Distribution",
                                &inspect
                                    .erasure
                                    .distribution
                                    .iter()
                                    .map(|disk| disk.to_string())
                                    .collect::<Vec<_>>()
                                    .join(", "),
                            ));
                            col = col.push(text("Shards").size(11));

                            for shard in &inspect.shards {
                                col = col.push(meta_row(
                                    &format!("Shard {}", shard.index),
                                    &format!("disk {} ({})", shard.disk, shard.status),
                                ));
                                col = col.push(meta_row(
                                    "Checksum",
                                    shard.checksum.as_deref().unwrap_or("-"),
                                ));
                            }
                        } else if let Some(Err(error)) = &self.object_inspect {
                            col = col.push(text(format!("Inspect error: {}", error)).size(11));
                        } else {
                            col = col.push(text("Shard inspection not loaded.").size(11));
                        }

                        if let Some(result) = &self.heal_result {
                            col = col.push(text(result).size(11));
                        }

                        let refresh_button = if self.loading_object_inspect {
                            button(text("Refresh Inspect").size(11))
                        } else {
                            button(text("Refresh Inspect").size(11))
                                .on_press(Message::RefreshObjectInspect)
                        };
                        let heal_button = if self.healing_object {
                            button(text("Heal Object").size(11))
                        } else {
                            button(text("Heal Object").size(11)).on_press(Message::OpenHealConfirm)
                        };

                        col = col.push(row![refresh_button, heal_button].spacing(4));
                    }
                } else if let Some(Err(e)) = &self.detail {
                    col = col.push(text(format!("Error: {}", e)).size(11));
                }
            }
        }

        col = col.push(iced::widget::rule::horizontal(1));
        col = col.push(
            button(text("Close [ESC]").size(10))
                .style(button::text)
                .on_press(Message::ClearSelection),
        );

        scrollable(col).height(Length::Fill).into()
    }

    pub fn heal_confirm_modal(&self) -> Element<'_, Message> {
        let Some((bucket, key)) = &self.heal_confirm_target else {
            return container(text("")).width(Length::Shrink).into();
        };

        let card = container(
            column![
                text("Confirm Heal").size(16),
                text("This will ask AbixIO to heal the selected object.").size(11),
                meta_row("Bucket", bucket),
                meta_row("Key", key),
                row![
                    button(text("Cancel").size(11)).on_press(Message::CancelHealConfirm),
                    button(text("Heal Object").size(11)).on_press(Message::ConfirmHealObject),
                ]
                .spacing(8),
            ]
            .spacing(8)
            .padding(12),
        )
        .width(360);

        container(
            row![
                space::horizontal().width(Length::Fill),
                column![
                    space::vertical().height(Length::Fill),
                    card,
                    space::vertical().height(Length::Fill),
                ],
                space::horizontal().width(Length::Fill),
            ]
            .width(Length::Fill)
            .height(Length::Fill),
        )
        .width(Length::Fill)
        .height(Length::Fill)
        .into()
    }
}

fn section(label: &str) -> Element<'static, Message> {
    column![
        text(label.to_string()).size(11),
        iced::widget::rule::horizontal(1)
    ]
    .spacing(2)
    .padding(4)
    .into()
}

fn meta_row(label: &str, value: &str) -> Element<'static, Message> {
    row![
        text(label.to_string())
            .size(10)
            .width(Length::FillPortion(2)),
        text(value.to_string())
            .size(10)
            .width(Length::FillPortion(3)),
    ]
    .spacing(4)
    .into()
}

fn format_size(bytes: u64) -> String {
    if bytes < 1024 {
        format!("{} B", bytes)
    } else if bytes < 1024 * 1024 {
        format!("{:.1} KB ({} bytes)", bytes as f64 / 1024.0, bytes)
    } else {
        format!(
            "{:.1} MB ({} bytes)",
            bytes as f64 / (1024.0 * 1024.0),
            bytes
        )
    }
}
