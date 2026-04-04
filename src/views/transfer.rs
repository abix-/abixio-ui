use iced::widget::{
    button, column, container, pick_list, row, scrollable, space, text, text_input,
};
use iced::{Element, Length};

use crate::app::{App, Message, TransferMode};

impl App {
    pub fn transfer_modal(&self) -> Element<'_, Message> {
        let Some(transfer) = &self.transfer else {
            return container(text("")).width(Length::Shrink).into();
        };

        let mut body = column![text(self.transfer_title()).size(16)]
            .spacing(8)
            .padding(12);

        match transfer.mode {
            TransferMode::CopyObject | TransferMode::MoveObject => {
                body = body.push(text(self.transfer_source_summary()).size(11));
                let connection_options = self.available_connection_options();
                body = body.push(text("Destination connection").size(11));
                body = body.push(
                    pick_list(
                        connection_options,
                        self.selected_transfer_connection_label(),
                        |label| {
                            if label == "Current connection" {
                                Message::TransferDestinationConnectionChanged(
                                    crate::app::CURRENT_CONNECTION_ID.to_string(),
                                )
                            } else {
                                Message::TransferDestinationConnectionChanged(label)
                            }
                        },
                    )
                    .placeholder("Select connection"),
                );
                body = body.push(text("Destination bucket").size(11));
                if transfer.loading_destination_buckets {
                    body = body.push(text("Loading buckets...").size(11));
                } else {
                    let bucket_options = self.transfer_bucket_options();
                    body = body.push(
                        pick_list(
                            bucket_options,
                            if transfer.destination_bucket.is_empty() {
                                None
                            } else {
                                Some(transfer.destination_bucket.clone())
                            },
                            Message::TransferDestinationBucketChanged,
                        )
                        .placeholder("Select bucket"),
                    );
                }
                body = body.push(text("Destination key").size(11));
                body = body.push(
                    text_input("object key", &transfer.destination_key)
                        .on_input(Message::TransferDestinationKeyChanged),
                );
            }
            TransferMode::ImportFolder => {
                body = body.push(text(self.transfer_source_summary()).size(11));
                body = body.push(text(format!(
                    "Destination: {}/{}",
                    transfer.destination_bucket, transfer.destination_key
                )));
            }
            TransferMode::ExportPrefix => {
                body = body.push(text(self.transfer_source_summary()).size(11));
                if let Some(path) = &transfer.local_path {
                    body =
                        body.push(text(format!("Destination folder: {}", path.display())).size(11));
                }
            }
        }

        if let Some(conflict) = &transfer.pending_conflict {
            body = body.push(iced::widget::rule::horizontal(1));
            body = body.push(text("Destination already exists.").size(12));
            body = body.push(text(conflict.label()).size(10));
            body = body.push(
                row![
                    button(text("Overwrite").size(11)).on_press(Message::TransferConflictOverwrite),
                    button(text("Skip").size(11)).on_press(Message::TransferConflictSkip),
                    button(text("Overwrite All").size(11))
                        .on_press(Message::TransferConflictOverwriteAll),
                    button(text("Skip All").size(11)).on_press(Message::TransferConflictSkipAll),
                ]
                .spacing(6),
            );
        } else if transfer.running || transfer.preparing || transfer.summary.is_some() {
            body = body.push(iced::widget::rule::horizontal(1));
            if transfer.preparing {
                body = body.push(text("Preparing transfer...").size(11));
            }
            if let Some(item) = &transfer.current_item {
                body = body.push(text(format!("Current: {}", item)).size(11));
            }
            body = body.push(text(format!(
                "Copied: {}  Skipped: {}  Failed: {}",
                transfer.completed, transfer.skipped, transfer.failed
            )));
            if let Some(summary) = &transfer.summary {
                body = body.push(text(summary).size(11));
            }
        }

        body = body.push(iced::widget::rule::horizontal(1));
        let close_button = if transfer.running || transfer.preparing {
            button(text("Close").size(11))
        } else {
            button(text("Close").size(11)).on_press(Message::CloseTransferModal)
        };
        let start_button = if self.transfer_can_start() {
            button(text("Start Copy").size(11)).on_press(Message::StartTransfer)
        } else {
            button(text("Start Copy").size(11))
        };
        body = body.push(row![close_button, start_button].spacing(8));

        let card = container(scrollable(body).height(Length::Shrink)).width(420);

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

    fn transfer_title(&self) -> &'static str {
        match self.transfer.as_ref().map(|t| t.mode) {
            Some(TransferMode::CopyObject) => "Copy Object",
            Some(TransferMode::MoveObject) => "Move / Rename Object",
            Some(TransferMode::ImportFolder) => "Import Folder",
            Some(TransferMode::ExportPrefix) => "Export Prefix",
            None => "Transfer",
        }
    }

    fn transfer_source_summary(&self) -> String {
        let Some(transfer) = &self.transfer else {
            return String::new();
        };
        match transfer.mode {
            TransferMode::CopyObject | TransferMode::MoveObject => format!(
                "Source: {}/{}",
                transfer.source_bucket.clone().unwrap_or_default(),
                transfer.source_key.clone().unwrap_or_default()
            ),
            TransferMode::ImportFolder => format!(
                "Source folder: {}",
                transfer
                    .local_path
                    .as_ref()
                    .map(|p| p.display().to_string())
                    .unwrap_or_default()
            ),
            TransferMode::ExportPrefix => format!(
                "Source prefix: {}/{}",
                transfer.source_bucket.clone().unwrap_or_default(),
                transfer.source_prefix.clone().unwrap_or_default()
            ),
        }
    }

    fn transfer_bucket_options(&self) -> Vec<String> {
        self.transfer
            .as_ref()
            .and_then(|t| t.destination_buckets.as_ref())
            .and_then(|result| result.as_ref().ok().cloned())
            .unwrap_or_default()
            .into_iter()
            .map(|bucket| bucket.name)
            .collect()
    }
}
