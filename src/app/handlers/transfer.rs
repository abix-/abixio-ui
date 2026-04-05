use iced::Task;

use crate::s3::client::BucketInfo;

use super::super::transfer_ops::{prepare_export_items, prepare_import_items, run_transfer_step};
use super::super::types::{
    CURRENT_CONNECTION_ID, OverwritePolicy, TransferEndpoint, TransferItem, TransferMode,
    TransferState, TransferStepResult,
};
use super::super::{App, Message};

impl App {
    pub(crate) fn handle_open_copy_object(&mut self) -> Task<Message> {
        let Some((bucket, key)) = self.current_selected_object() else {
            return Task::none();
        };
        let destination_connection_id = self.current_connection_id();
        let destination_buckets = if destination_connection_id == CURRENT_CONNECTION_ID {
            self.buckets.clone()
        } else {
            None
        };
        self.transfer = Some(TransferState {
            mode: TransferMode::CopyObject,
            destination_connection_id: destination_connection_id.clone(),
            destination_bucket: bucket.clone(),
            destination_key: key.clone(),
            destination_buckets,
            loading_destination_buckets: false,
            local_path: None,
            source_bucket: Some(bucket),
            source_key: Some(key),
            source_prefix: None,
            items: Vec::new(),
            next_index: 0,
            completed: 0,
            skipped: 0,
            failed: 0,
            current_item: None,
            pending_conflict: None,
            overwrite_policy: OverwritePolicy::Ask,
            preparing: false,
            running: false,
            summary: None,
        });
        if destination_connection_id == CURRENT_CONNECTION_ID {
            Task::none()
        } else {
            if let Some(transfer) = self.transfer.as_mut() {
                transfer.loading_destination_buckets = true;
            }
            self.cmd_fetch_transfer_buckets(&destination_connection_id)
        }
    }

    pub(crate) fn handle_open_move_object(&mut self) -> Task<Message> {
        let Some((bucket, key)) = self.current_selected_object() else {
            return Task::none();
        };
        self.transfer = Some(TransferState {
            mode: TransferMode::MoveObject,
            destination_connection_id: CURRENT_CONNECTION_ID.to_string(),
            destination_bucket: bucket.clone(),
            destination_key: key.clone(),
            destination_buckets: self.buckets.clone(),
            loading_destination_buckets: false,
            local_path: None,
            source_bucket: Some(bucket),
            source_key: Some(key),
            source_prefix: None,
            items: Vec::new(),
            next_index: 0,
            completed: 0,
            skipped: 0,
            failed: 0,
            current_item: None,
            pending_conflict: None,
            overwrite_policy: OverwritePolicy::Ask,
            preparing: false,
            running: false,
            summary: None,
        });
        Task::none()
    }

    pub(crate) fn handle_open_rename_object(&mut self) -> Task<Message> {
        let Some((bucket, key)) = self.current_selected_object() else {
            return Task::none();
        };
        self.transfer = Some(TransferState {
            mode: TransferMode::MoveObject,
            destination_connection_id: CURRENT_CONNECTION_ID.to_string(),
            destination_bucket: bucket.clone(),
            destination_key: key.clone(),
            destination_buckets: self.buckets.clone(),
            loading_destination_buckets: false,
            local_path: None,
            source_bucket: Some(bucket),
            source_key: Some(key),
            source_prefix: None,
            items: Vec::new(),
            next_index: 0,
            completed: 0,
            skipped: 0,
            failed: 0,
            current_item: None,
            pending_conflict: None,
            overwrite_policy: OverwritePolicy::Ask,
            preparing: false,
            running: false,
            summary: None,
        });
        Task::none()
    }

    pub(crate) fn handle_open_import_folder(&mut self) -> Task<Message> {
        let Some(bucket) = self.selected_bucket.clone() else {
            return Task::none();
        };
        let Some(path) = rfd::FileDialog::new().pick_folder() else {
            return Task::none();
        };
        self.transfer = Some(TransferState {
            mode: TransferMode::ImportFolder,
            destination_connection_id: self.current_connection_id(),
            destination_bucket: bucket,
            destination_key: self.current_prefix.clone(),
            destination_buckets: self.buckets.clone(),
            loading_destination_buckets: false,
            local_path: Some(path),
            source_bucket: None,
            source_key: None,
            source_prefix: None,
            items: Vec::new(),
            next_index: 0,
            completed: 0,
            skipped: 0,
            failed: 0,
            current_item: None,
            pending_conflict: None,
            overwrite_policy: OverwritePolicy::Ask,
            preparing: false,
            running: false,
            summary: None,
        });
        Task::none()
    }

    pub(crate) fn handle_open_export_prefix(&mut self) -> Task<Message> {
        let Some(bucket) = self.selected_bucket.clone() else {
            return Task::none();
        };
        let Some(path) = rfd::FileDialog::new().pick_folder() else {
            return Task::none();
        };
        self.transfer = Some(TransferState {
            mode: TransferMode::ExportPrefix,
            destination_connection_id: self.current_connection_id(),
            destination_bucket: bucket.clone(),
            destination_key: self.current_prefix.clone(),
            destination_buckets: None,
            loading_destination_buckets: false,
            local_path: Some(path),
            source_bucket: Some(bucket),
            source_key: None,
            source_prefix: Some(self.current_prefix.clone()),
            items: Vec::new(),
            next_index: 0,
            completed: 0,
            skipped: 0,
            failed: 0,
            current_item: None,
            pending_conflict: None,
            overwrite_policy: OverwritePolicy::Ask,
            preparing: false,
            running: false,
            summary: None,
        });
        Task::none()
    }

    pub(crate) fn handle_close_transfer_modal(&mut self) -> Task<Message> {
        if self.transfer.as_ref().is_some_and(|t| t.running) {
            return Task::none();
        }
        self.transfer = None;
        Task::none()
    }

    pub(crate) fn handle_transfer_destination_connection_changed(
        &mut self,
        connection_id: String,
    ) -> Task<Message> {
        let Some(transfer) = self.transfer.as_mut() else {
            return Task::none();
        };
        transfer.destination_connection_id = connection_id.clone();
        transfer.destination_buckets = None;
        transfer.loading_destination_buckets = true;
        transfer.destination_bucket.clear();
        transfer.summary = None;
        self.cmd_fetch_transfer_buckets(&connection_id)
    }

    pub(crate) fn handle_transfer_destination_bucket_changed(
        &mut self,
        bucket: String,
    ) -> Task<Message> {
        if let Some(transfer) = self.transfer.as_mut() {
            transfer.destination_bucket = bucket;
            transfer.summary = None;
        }
        Task::none()
    }

    pub(crate) fn handle_transfer_destination_key_changed(&mut self, key: String) -> Task<Message> {
        if let Some(transfer) = self.transfer.as_mut() {
            transfer.destination_key = key;
            transfer.summary = None;
        }
        Task::none()
    }

    pub(crate) fn handle_transfer_destination_buckets_loaded(
        &mut self,
        result: Result<Vec<BucketInfo>, String>,
    ) -> Task<Message> {
        if let Some(transfer) = self.transfer.as_mut() {
            transfer.loading_destination_buckets = false;
            transfer.destination_buckets = Some(result);
        }
        Task::none()
    }

    pub(crate) fn handle_start_transfer(&mut self) -> Task<Message> {
        if !self.transfer_can_start() {
            return Task::none();
        }
        let Some(transfer) = self.transfer.as_mut() else {
            return Task::none();
        };
        transfer.preparing = true;
        transfer.summary = None;
        match transfer.mode {
            TransferMode::CopyObject | TransferMode::MoveObject => {
                let item = TransferItem {
                    source: TransferEndpoint::S3 {
                        connection_id: CURRENT_CONNECTION_ID.to_string(),
                        bucket: transfer.source_bucket.clone().unwrap_or_default(),
                        key: transfer.source_key.clone().unwrap_or_default(),
                    },
                    destination: TransferEndpoint::S3 {
                        connection_id: transfer.destination_connection_id.clone(),
                        bucket: transfer.destination_bucket.clone(),
                        key: transfer.destination_key.clone(),
                    },
                };
                Task::perform(async move { Ok(vec![item]) }, Message::TransferPrepared)
            }
            TransferMode::ImportFolder => {
                let root = transfer.local_path.clone().unwrap_or_default();
                let bucket = transfer.destination_bucket.clone();
                let prefix = transfer.destination_key.clone();
                Task::perform(
                    async move { prepare_import_items(root, &bucket, &prefix) },
                    Message::TransferPrepared,
                )
            }
            TransferMode::ExportPrefix => {
                let client = self.client.clone();
                let bucket = transfer.source_bucket.clone().unwrap_or_default();
                let prefix = transfer.source_prefix.clone().unwrap_or_default();
                let root = transfer.local_path.clone().unwrap_or_default();
                Task::perform(
                    async move { prepare_export_items(client, &bucket, &prefix, &root).await },
                    Message::TransferPrepared,
                )
            }
        }
    }

    pub(crate) fn handle_transfer_prepared(
        &mut self,
        result: Result<Vec<TransferItem>, String>,
    ) -> Task<Message> {
        let Some(transfer) = self.transfer.as_mut() else {
            return Task::none();
        };
        transfer.preparing = false;
        match result {
            Ok(items) => {
                transfer.items = items;
                transfer.next_index = 0;
                transfer.completed = 0;
                transfer.skipped = 0;
                transfer.failed = 0;
                transfer.running = true;
                transfer.pending_conflict = None;
                transfer.current_item = None;
                if transfer.items.is_empty() {
                    transfer.running = false;
                    transfer.summary = Some("Nothing to copy.".to_string());
                    Task::none()
                } else {
                    self.cmd_process_next_transfer_step()
                }
            }
            Err(error) => {
                transfer.running = false;
                transfer.summary = Some(format!("Transfer preparation failed: {}", error));
                Task::none()
            }
        }
    }

    pub(crate) fn handle_transfer_step_finished(
        &mut self,
        result: Result<TransferStepResult, String>,
    ) -> Task<Message> {
        let Some(transfer) = self.transfer.as_mut() else {
            return Task::none();
        };
        match result {
            Ok(TransferStepResult::Conflict(item)) => {
                transfer.pending_conflict = Some(item);
                transfer.current_item = None;
                transfer.running = false;
                Task::none()
            }
            Ok(TransferStepResult::Copied(label)) => {
                transfer.completed += 1;
                transfer.next_index += 1;
                transfer.current_item = Some(label);
                self.cmd_process_next_transfer_step()
            }
            Ok(TransferStepResult::Skipped(label)) => {
                transfer.skipped += 1;
                transfer.next_index += 1;
                transfer.current_item = Some(label);
                self.cmd_process_next_transfer_step()
            }
            Err(error) => {
                transfer.failed += 1;
                transfer.next_index += 1;
                self.error = Some(format!("Transfer failed: {}", error));
                self.cmd_process_next_transfer_step()
            }
        }
    }

    pub(crate) fn handle_transfer_conflict_overwrite(&mut self) -> Task<Message> {
        self.resolve_transfer_conflict(false, false)
    }

    pub(crate) fn handle_transfer_conflict_skip(&mut self) -> Task<Message> {
        self.resolve_transfer_conflict(true, false)
    }

    pub(crate) fn handle_transfer_conflict_overwrite_all(&mut self) -> Task<Message> {
        self.resolve_transfer_conflict(false, true)
    }

    pub(crate) fn handle_transfer_conflict_skip_all(&mut self) -> Task<Message> {
        self.resolve_transfer_conflict(true, true)
    }

    // -- helpers --

    pub fn transfer_can_start(&self) -> bool {
        let Some(transfer) = &self.transfer else {
            return false;
        };
        if transfer.preparing || transfer.running || transfer.loading_destination_buckets {
            return false;
        }
        match transfer.mode {
            TransferMode::CopyObject | TransferMode::MoveObject => {
                !transfer.destination_bucket.is_empty()
                    && !transfer.destination_key.is_empty()
                    && !self.transfer_points_to_same_object(transfer)
            }
            TransferMode::ImportFolder | TransferMode::ExportPrefix => {
                transfer.local_path.is_some()
            }
        }
    }

    fn transfer_points_to_same_object(&self, transfer: &TransferState) -> bool {
        matches!(
            (&transfer.source_bucket, &transfer.source_key),
            (Some(bucket), Some(key))
                if transfer.destination_connection_id == CURRENT_CONNECTION_ID
                    && transfer.destination_bucket == *bucket
                    && transfer.destination_key == *key
        )
    }

    pub(crate) fn cmd_fetch_transfer_buckets(&self, connection_id: &str) -> Task<Message> {
        let connection_id = connection_id.to_string();
        let result = self.make_client_for_connection(&connection_id);
        match result {
            Ok(client) => Task::perform(
                async move { client.list_buckets().await },
                Message::TransferDestinationBucketsLoaded,
            ),
            Err(error) => Task::perform(
                async move { Err(error) },
                Message::TransferDestinationBucketsLoaded,
            ),
        }
    }

    pub(crate) fn cmd_process_next_transfer_step(&mut self) -> Task<Message> {
        let Some(transfer) = self.transfer.as_mut() else {
            return Task::none();
        };
        if transfer.next_index >= transfer.items.len() {
            transfer.running = false;
            transfer.pending_conflict = None;
            transfer.current_item = None;
            transfer.summary = Some(format!(
                "Done. Copied: {}. Skipped: {}. Failed: {}.",
                transfer.completed, transfer.skipped, transfer.failed
            ));
            let should_refresh = matches!(
                transfer.mode,
                TransferMode::ImportFolder | TransferMode::CopyObject | TransferMode::MoveObject
            ) && transfer.destination_connection_id == CURRENT_CONNECTION_ID;
            return if should_refresh {
                self.loading_objects = true;
                self.cmd_fetch_objects()
            } else {
                Task::none()
            };
        }
        transfer.running = true;
        transfer.pending_conflict = None;
        let item = transfer.items[transfer.next_index].clone();
        transfer.current_item = Some(item.label());
        let overwrite_policy = transfer.overwrite_policy;
        let is_move = transfer.mode == TransferMode::MoveObject;
        let source_client = self.client.clone();
        let destination_client = match &item.destination {
            TransferEndpoint::S3 { connection_id, .. } => match self
                .make_client_for_connection(connection_id)
            {
                Ok(client) => Some(client),
                Err(error) => {
                    return Task::perform(async move { Err(error) }, Message::TransferStepFinished);
                }
            },
            TransferEndpoint::Local { .. } => None,
        };
        Task::perform(
            async move {
                run_transfer_step(
                    source_client,
                    destination_client,
                    item,
                    overwrite_policy,
                    is_move,
                )
                .await
            },
            Message::TransferStepFinished,
        )
    }

    fn resolve_transfer_conflict(&mut self, skip: bool, remember: bool) -> Task<Message> {
        let Some(transfer) = self.transfer.as_mut() else {
            return Task::none();
        };
        let Some(item) = transfer.pending_conflict.take() else {
            return Task::none();
        };
        if remember {
            transfer.overwrite_policy = if skip {
                OverwritePolicy::SkipAll
            } else {
                OverwritePolicy::OverwriteAll
            };
        }
        if skip {
            transfer.skipped += 1;
            transfer.next_index += 1;
            transfer.current_item = Some(item.label());
            self.cmd_process_next_transfer_step()
        } else {
            transfer.running = true;
            let source_client = self.client.clone();
            let destination_client = match &item.destination {
                TransferEndpoint::S3 { connection_id, .. } => {
                    match self.make_client_for_connection(connection_id) {
                        Ok(client) => Some(client),
                        Err(error) => {
                            return Task::perform(
                                async move { Err(error) },
                                Message::TransferStepFinished,
                            );
                        }
                    }
                }
                TransferEndpoint::Local { .. } => None,
            };
            Task::perform(
                async move {
                    run_transfer_step(
                        source_client,
                        destination_client,
                        item,
                        OverwritePolicy::OverwriteAll,
                        false,
                    )
                    .await
                },
                Message::TransferStepFinished,
            )
        }
    }

    pub fn current_connection_id(&self) -> String {
        self.active_connection
            .clone()
            .unwrap_or_else(|| CURRENT_CONNECTION_ID.to_string())
    }

    pub fn current_connection_label(&self) -> String {
        self.active_connection
            .clone()
            .unwrap_or_else(|| "Current connection".to_string())
    }

    pub fn available_connection_options(&self) -> Vec<String> {
        let mut options = vec![self.current_connection_label()];
        for conn in &self.settings.connections {
            if self.active_connection.as_deref() != Some(&conn.name) {
                options.push(conn.name.clone());
            }
        }
        options
    }

    pub fn selected_transfer_connection_label(&self) -> Option<String> {
        let transfer = self.transfer.as_ref()?;
        if transfer.destination_connection_id == CURRENT_CONNECTION_ID {
            Some(self.current_connection_label())
        } else {
            Some(transfer.destination_connection_id.clone())
        }
    }
}
