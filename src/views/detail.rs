use iced::widget::{button, column, container, row, scrollable, space, text, text_editor};
use iced::{Element, Length};

use crate::app::{
    App, BucketDocumentKind, BucketDocumentLoadState, BucketDocumentState, Message, Selection,
};

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

                col = col.push(section("Versioning"));
                match &self.bucket_versioning {
                    Some(Ok(status)) if status == "Enabled" => {
                        col = col.push(meta_row("Status", "Enabled"));
                        col = col.push(
                            button(text("Suspend Versioning").size(11))
                                .on_press(Message::SuspendVersioning),
                        );
                    }
                    Some(Ok(status)) if status == "Suspended" => {
                        col = col.push(meta_row("Status", "Suspended"));
                        col = col.push(
                            button(text("Enable Versioning").size(11))
                                .on_press(Message::EnableVersioning),
                        );
                    }
                    Some(Ok(_)) | None => {
                        col = col.push(meta_row("Status", "Disabled"));
                        col = col.push(
                            button(text("Enable Versioning").size(11))
                                .on_press(Message::EnableVersioning),
                        );
                    }
                    Some(Err(e)) => {
                        col = col.push(text(format!("Error: {}", e)).size(10));
                    }
                }

                // bucket tags
                col = col.push(section("Bucket Tags"));
                if let Some(Ok(tags)) = &self.bucket_tags {
                    if tags.is_empty() {
                        col = col.push(text("No tags").size(10));
                    } else {
                        let mut sorted: Vec<_> = tags.keys().collect();
                        sorted.sort();
                        for k in sorted {
                            let v = &tags[k];
                            let kc = k.clone();
                            col = col.push(
                                row![
                                    text(k).size(10).width(Length::FillPortion(2)),
                                    text(v).size(10).width(Length::FillPortion(2)),
                                    button(text("x").size(9))
                                        .on_press(Message::RemoveBucketTag(kc))
                                        .style(button::text),
                                ]
                                .spacing(4),
                            );
                        }
                    }
                    col = col.push(
                        row![
                            iced::widget::text_input("key", &self.bucket_tag_key)
                                .on_input(Message::BucketTagKeyChanged)
                                .size(10)
                                .width(Length::FillPortion(2)),
                            iced::widget::text_input("value", &self.bucket_tag_value)
                                .on_input(Message::BucketTagValueChanged)
                                .size(10)
                                .width(Length::FillPortion(2)),
                            if self.bucket_tag_key.trim().is_empty() {
                                button(text("Add").size(9))
                            } else {
                                button(text("Add").size(9)).on_press(Message::AddBucketTag)
                            },
                        ]
                        .spacing(4),
                    );
                } else if let Some(Err(_)) = &self.bucket_tags {
                    col = col.push(text("No tags").size(10));
                }

                col = col.push(bucket_document_section(
                    BucketDocumentKind::Policy,
                    &self.bucket_policy,
                ));
                col = col.push(bucket_document_section(
                    BucketDocumentKind::Lifecycle,
                    &self.bucket_lifecycle,
                ));

                if self.is_abixio {
                    col = col.push(section("AbixIO"));
                    match &self.bucket_ftt {
                        Some(Ok(ftt)) => {
                            col = col.push(meta_row("FTT", &ftt.to_string()));
                        }
                        Some(Err(e)) => {
                            col = col.push(text(format!("FTT error: {}", e)).size(10));
                        }
                        None => {
                            col = col.push(text("Loading FTT...").size(10));
                        }
                    }
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

                    col = col.push(section("Tags"));
                    if self.loading_tags {
                        col = col.push(text("Loading...").size(11));
                    } else if let Some(Ok(tags)) = &self.object_tags {
                        if tags.is_empty() {
                            col = col.push(text("No tags").size(10));
                        } else {
                            let mut sorted_keys: Vec<&String> = tags.keys().collect();
                            sorted_keys.sort();
                            for tag_key in sorted_keys {
                                let tag_value = &tags[tag_key];
                                let k = tag_key.clone();
                                col = col.push(
                                    row![
                                        text(tag_key).size(10).width(Length::FillPortion(2)),
                                        text(tag_value).size(10).width(Length::FillPortion(2)),
                                        button(text("x").size(9))
                                            .on_press(Message::RemoveTag(k))
                                            .style(button::text),
                                    ]
                                    .spacing(4),
                                );
                            }
                        }
                        if tags.len() < 10 {
                            col = col.push(
                                row![
                                    iced::widget::text_input("key", &self.editing_tag_key)
                                        .on_input(Message::TagKeyChanged)
                                        .size(10)
                                        .width(Length::FillPortion(2)),
                                    iced::widget::text_input("value", &self.editing_tag_value)
                                        .on_input(Message::TagValueChanged)
                                        .size(10)
                                        .width(Length::FillPortion(2)),
                                    if self.editing_tag_key.trim().is_empty() {
                                        button(text("Add").size(9))
                                    } else {
                                        button(text("Add").size(9)).on_press(Message::AddTag)
                                    },
                                ]
                                .spacing(4),
                            );
                        }
                    } else if let Some(Err(e)) = &self.object_tags {
                        col = col.push(text(format!("Tags error: {}", e)).size(10));
                    }

                    col = col.push(section("Versions"));
                    if self.loading_versions {
                        col = col.push(text("Loading...").size(11));
                    } else if let Some(Ok(versions)) = &self.object_versions {
                        let obj_versions: Vec<_> =
                            versions.iter().filter(|v| v.key == *key).collect();
                        if obj_versions.is_empty() {
                            col = col.push(text("No versions").size(10));
                        } else {
                            for v in &obj_versions {
                                let vid = v.version_id.clone();
                                let vid_short = if vid.len() > 8 {
                                    format!("{}...", &vid[..8])
                                } else {
                                    vid.clone()
                                };
                                let label = if v.is_delete_marker {
                                    format!("{} (delete marker)", vid_short)
                                } else if v.is_latest {
                                    format!("{} (latest) {}B", vid_short, v.size)
                                } else {
                                    format!("{} {}B", vid_short, v.size)
                                };
                                let mut ver_row =
                                    row![text(label).size(10).width(Length::FillPortion(3)),]
                                        .spacing(4);
                                if !v.is_delete_marker && !v.is_latest {
                                    let vid_restore = vid.clone();
                                    ver_row = ver_row.push(
                                        button(text("Restore").size(9))
                                            .on_press(Message::RestoreVersion(vid_restore))
                                            .style(button::text),
                                    );
                                }
                                let vid_del = vid.clone();
                                ver_row = ver_row.push(
                                    button(text("x").size(9))
                                        .on_press(Message::DeleteVersion(vid_del))
                                        .style(button::text),
                                );
                                col = col.push(ver_row);
                            }
                        }
                    } else if let Some(Err(e)) = &self.object_versions {
                        col = col.push(text(format!("Versions error: {}", e)).size(10));
                    }

                    // preview (first 4KB of text objects)
                    // preview (first 4KB of text objects)
                    if let Some(Ok(preview)) = &self.object_preview
                        && !preview.is_empty()
                    {
                        col = col.push(section("Preview"));
                        col = col.push(
                            text(if preview.len() > 500 {
                                format!("{}...", &preview[..500])
                            } else {
                                preview.clone()
                            })
                            .size(9),
                        );
                    }

                    col = col.push(section("Actions"));
                    col = col.push(
                        row![
                            button(text("Download").size(11))
                                .on_press(Message::Download(bucket.clone(), key.clone())),
                            button(text("Share").size(11)).on_press(Message::OpenShareModal),
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
                            if inspect.erasure.epoch_id > 0 {
                                col = col.push(meta_row(
                                    "Epoch",
                                    &inspect.erasure.epoch_id.to_string(),
                                ));
                            }
                            if !inspect.erasure.set_id.is_empty() {
                                col = col.push(meta_row("Volume Pool", &inspect.erasure.set_id));
                            }
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
                                let location = if !shard.node_id.is_empty() {
                                    format!(
                                        "disk {} / {} / {} ({})",
                                        shard.disk, shard.node_id, shard.volume_id, shard.status
                                    )
                                } else {
                                    format!("disk {} ({})", shard.disk, shard.status)
                                };
                                col = col.push(meta_row(
                                    &format!("Shard {}", shard.index),
                                    &location,
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

    pub fn share_modal(&self) -> Element<'_, Message> {
        if !self.share_modal_open {
            return container(text("")).width(Length::Shrink).into();
        }

        let expiry_options = vec![
            ("1 hour", "3600"),
            ("6 hours", "21600"),
            ("24 hours", "86400"),
            ("7 days", "604800"),
        ];

        let mut body = column![
            text("Share Object").size(16),
            text("Generate a presigned download URL.").size(11),
        ]
        .spacing(8)
        .padding(12);

        body = body.push(text("Expiry:").size(11));
        for (label, secs) in &expiry_options {
            let is_selected = self.share_expiry_secs.to_string() == *secs;
            let secs_str = secs.to_string();
            body = body.push(
                button(
                    text(if is_selected {
                        format!("> {}", label)
                    } else {
                        label.to_string()
                    })
                    .size(10),
                )
                .on_press(Message::ShareExpiryChanged(secs_str))
                .style(if is_selected {
                    button::primary
                } else {
                    button::text
                }),
            );
        }

        body = body.push(button(text("Generate URL").size(11)).on_press(Message::GenerateShareUrl));

        if let Some(url) = &self.share_url {
            body = body.push(
                iced::widget::text_input("", url)
                    .size(9)
                    .width(Length::Fill),
            );
        }

        body = body.push(button(text("Close").size(11)).on_press(Message::CloseShareModal));

        modal_card(body, 420)
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
            body = body.push(text(format!("  and {} more...", state.keys.len() - 10)).size(10));
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
                body = body.push(text(format!("  and {} more...", state.keys.len() - 10)).size(10));
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

fn bucket_document_section<'a>(
    kind: BucketDocumentKind,
    state: &'a BucketDocumentState,
) -> Element<'a, Message> {
    let mut col = column![section(kind.title())].spacing(4);

    if state.editing {
        if matches!(state.loaded, Some(BucketDocumentLoadState::Absent) | None) {
            col = col.push(text("Start from this example:").size(10));
            col = col.push(text(kind.example()).size(9));
        }

        col = col.push(
            text_editor(&state.editor)
                .placeholder(kind.example())
                .on_action(move |action| Message::BucketDocumentEdited(kind, action))
                .height(Length::Fixed(180.0))
                .padding(8)
                .size(10),
        );

        if let Some(error) = &state.error {
            col = col.push(text(error).size(10));
        }

        let save_button = if state.saving {
            button(text("Saving...").size(11))
        } else {
            button(text("Save").size(11)).on_press(Message::SaveBucketDocument(kind))
        };

        col = col.push(
            row![
                save_button,
                button(text("Cancel").size(11)).on_press(Message::CancelBucketDocumentEditor(kind)),
            ]
            .spacing(4),
        );

        return col.into();
    }

    match &state.loaded {
        None => {
            col = col.push(text("Loading...").size(10));
        }
        Some(BucketDocumentLoadState::Absent) => {
            col = col.push(text(kind.empty_label()).size(10));
            col = col.push(
                button(text(kind.create_label()).size(11))
                    .on_press(Message::OpenBucketDocumentEditor(kind)),
            );
        }
        Some(BucketDocumentLoadState::Loaded(text_value)) => {
            col = col.push(text(text_value).size(9));
            if let Some(error) = &state.error {
                col = col.push(text(error).size(10));
            }
            col = col.push(
                row![
                    button(text(kind.edit_label()).size(11))
                        .on_press(Message::OpenBucketDocumentEditor(kind)),
                    button(text(kind.delete_label()).size(11))
                        .on_press(Message::DeleteBucketDocument(kind)),
                ]
                .spacing(4),
            );
        }
        Some(BucketDocumentLoadState::Error(error)) => {
            col = col.push(text(format!("Error: {}", error)).size(10));
            col = col.push(
                button(text(kind.create_label()).size(11))
                    .on_press(Message::OpenBucketDocumentEditor(kind)),
            );
        }
    }

    col.into()
}
