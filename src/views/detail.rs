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
                col = col.push(section("Overview"));
                col = col.push(meta_row("Bucket", name));
                col = col.push(meta_row("Prefix", &self.current_prefix));

                if let Some(Ok(objects)) = &self.objects {
                    col = col.push(section("Contents"));
                    col = col.push(meta_row(
                        "Folders",
                        &objects.common_prefixes.len().to_string(),
                    ));
                    col = col.push(meta_row("Objects", &objects.objects.len().to_string()));
                }

                col = col.push(section("Actions"));
                col = col.push(
                    row![
                        button(text("Refresh").size(11)).on_press(Message::Refresh),
                        button(text("Delete Bucket").size(11))
                            .on_press(Message::OpenDeleteBucketModal),
                    ]
                    .spacing(4),
                );
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
                            button(text("Move").size(11)).on_press(Message::OpenMoveObject),
                            button(text("Rename").size(11)).on_press(Message::OpenRenameObject),
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

    pub fn create_bucket_modal(&self) -> Element<'_, Message> {
        let create_button = if self.new_bucket_name.trim().is_empty() {
            button(text("Create Bucket").size(11))
        } else {
            button(text("Create Bucket").size(11)).on_press(Message::CreateBucket)
        };

        let mut body = column![
            text("Create Bucket").size(16),
            text("Create a new bucket on the current connection.").size(11),
            text_input_row(
                "Bucket name",
                &self.new_bucket_name,
                Message::NewBucketNameChanged
            ),
        ]
        .spacing(8)
        .padding(12);

        if let Some(error) = &self.create_bucket_modal_error {
            body = body.push(text(error).size(11));
        }

        body = body.push(
            row![
                button(text("Cancel").size(11)).on_press(Message::CloseCreateBucketModal),
                create_button,
            ]
            .spacing(8),
        );

        modal_card(body, 360)
    }

    pub fn bucket_delete_modal(&self) -> Element<'_, Message> {
        let Some(state) = &self.bucket_delete else {
            return container(text("")).width(Length::Shrink).into();
        };

        let delete_button = if self.bucket_delete_can_start() {
            button(text("Delete Bucket").size(11)).on_press(Message::ConfirmDeleteBucket)
        } else {
            button(text("Delete Bucket").size(11))
        };

        let mut body = column![
            text("Delete Bucket").size(16),
            text("This will delete the bucket and all objects in it.").size(11),
            meta_row("Bucket", &state.bucket),
        ]
        .spacing(8)
        .padding(12);

        if state.preview_loading {
            body = body.push(text("Loading bucket contents...").size(11));
        } else {
            body = body.push(meta_row("Objects", &state.total_objects.to_string()));
            body = body.push(meta_row("Deleted", &state.deleted_objects.to_string()));
        }

        if let Some(summary) = &state.summary {
            body = body.push(text(summary).size(11));
        }

        body = body.push(text("Type the bucket name to enable deletion.").size(11));
        body = body.push(text_input_row(
            "Bucket name",
            &state.confirm_name,
            Message::BucketDeleteConfirmNameChanged,
        ));
        body = body.push(
            row![
                if state.deleting {
                    button(text("Cancel").size(11))
                } else {
                    button(text("Cancel").size(11)).on_press(Message::CloseDeleteBucketModal)
                },
                delete_button,
            ]
            .spacing(8),
        );

        modal_card(body, 420)
    }

    pub fn bulk_delete_modal(&self) -> Element<'_, Message> {
        let Some(state) = &self.bulk_delete else {
            return container(text("")).width(Length::Shrink).into();
        };

        let delete_button = if !state.deleting {
            button(text("Delete").size(11)).on_press(Message::ConfirmBulkDelete)
        } else {
            button(text("Deleting...").size(11))
        };

        let mut body = column![
            text("Delete Selected Objects").size(16),
            text(format!(
                "Delete {} objects from {}?",
                state.total, state.bucket
            ))
            .size(11),
        ]
        .spacing(8)
        .padding(12);

        // show first 10 keys
        let show_count = state.keys.len().min(10);
        for key in &state.keys[..show_count] {
            body = body.push(text(format!("  {}", key)).size(10));
        }
        if state.keys.len() > 10 {
            body = body.push(
                text(format!("  and {} more...", state.keys.len() - 10)).size(10),
            );
        }

        if let Some(summary) = &state.summary {
            body = body.push(text(summary).size(11));
        }

        body = body.push(
            row![
                if state.deleting {
                    button(text("Cancel").size(11))
                } else {
                    button(text("Cancel").size(11)).on_press(Message::CloseBulkDeleteModal)
                },
                delete_button,
            ]
            .spacing(8),
        );

        modal_card(body, 420)
    }

    pub fn prefix_delete_modal(&self) -> Element<'_, Message> {
        let Some(state) = &self.prefix_delete else {
            return container(text("")).width(Length::Shrink).into();
        };

        let mut body = column![
            text("Delete Prefix").size(16),
            meta_row("Bucket", &state.bucket),
            meta_row("Prefix", &state.prefix),
        ]
        .spacing(8)
        .padding(12);

        if state.loading {
            body = body.push(text("Listing objects...").size(11));
        } else {
            body = body.push(meta_row("Objects", &state.total.to_string()));

            // show first 10 keys
            let show_count = state.keys.len().min(10);
            for key in &state.keys[..show_count] {
                body = body.push(text(format!("  {}", key)).size(10));
            }
            if state.keys.len() > 10 {
                body = body.push(
                    text(format!("  and {} more...", state.keys.len() - 10)).size(10),
                );
            }
        }

        if let Some(summary) = &state.summary {
            body = body.push(text(summary).size(11));
        }

        let delete_button = if !state.loading && !state.deleting && !state.keys.is_empty() {
            button(text("Delete").size(11)).on_press(Message::ConfirmPrefixDelete)
        } else if state.deleting {
            button(text("Deleting...").size(11))
        } else {
            button(text("Delete").size(11))
        };

        body = body.push(
            row![
                if state.deleting {
                    button(text("Cancel").size(11))
                } else {
                    button(text("Cancel").size(11)).on_press(Message::ClosePrefixDeleteModal)
                },
                delete_button,
            ]
            .spacing(8),
        );

        modal_card(body, 420)
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

fn text_input_row(
    placeholder: &str,
    value: &str,
    on_input: fn(String) -> Message,
) -> Element<'static, Message> {
    iced::widget::text_input(placeholder, value)
        .on_input(on_input)
        .size(11)
        .into()
}

fn modal_card<'a>(content: iced::widget::Column<'a, Message>, width: u16) -> Element<'a, Message> {
    let card = container(content).width(width as u32);

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
