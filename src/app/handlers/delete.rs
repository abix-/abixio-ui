use iced::Task;

use crate::s3::client::ObjectInfo;

use super::super::types::{
    BucketDeleteState, BucketDeleteStepResult, BulkDeleteState, PrefixDeleteState,
};
use super::super::{App, Message, Selection};

impl App {
    pub(crate) fn handle_open_bulk_delete_modal(&mut self) -> Task<Message> {
        let bucket = match &self.selected_bucket {
            Some(b) => b.clone(),
            None => return Task::none(),
        };
        if self.selected_keys.is_empty() {
            return Task::none();
        }
        let keys: Vec<String> = self.selected_keys.iter().cloned().collect();
        let total = keys.len();
        self.bulk_delete = Some(BulkDeleteState {
            bucket,
            keys,
            total,
            deleted: 0,
            next_index: 0,
            deleting: false,
            summary: None,
        });
        Task::none()
    }

    pub(crate) fn handle_close_bulk_delete_modal(&mut self) -> Task<Message> {
        self.bulk_delete = None;
        Task::none()
    }

    pub(crate) fn handle_confirm_bulk_delete(&mut self) -> Task<Message> {
        self.cmd_process_next_bulk_delete_step()
    }

    pub(crate) fn handle_bulk_delete_batch_finished(
        &mut self,
        result: Result<usize, String>,
    ) -> Task<Message> {
        let Some(state) = self.bulk_delete.as_mut() else {
            return Task::none();
        };
        match result {
            Ok(count) => {
                state.deleted += count;
                state.summary = Some(format!("Deleting: {}/{} done", state.deleted, state.total));
                self.cmd_process_next_bulk_delete_step()
            }
            Err(error) => {
                state.deleting = false;
                state.summary = Some(format!(
                    "Stopped after {}/{}: {}",
                    state.deleted, state.total, error
                ));
                self.error = Some(format!("Bulk delete failed: {}", error));
                Task::none()
            }
        }
    }

    pub(crate) fn handle_open_prefix_delete_modal(&mut self, prefix: String) -> Task<Message> {
        let bucket = match &self.selected_bucket {
            Some(b) => b.clone(),
            None => return Task::none(),
        };
        self.prefix_delete = Some(PrefixDeleteState {
            bucket: bucket.clone(),
            prefix: prefix.clone(),
            keys: Vec::new(),
            loading: true,
            total: 0,
            deleted: 0,
            next_index: 0,
            deleting: false,
            summary: None,
        });
        let client = self.client.clone();
        Task::perform(
            async move {
                let result = client.list_objects_recursive(&bucket, &prefix).await?;
                Ok(result.objects.into_iter().map(|o| o.key).collect())
            },
            Message::PrefixDeleteListLoaded,
        )
    }

    pub(crate) fn handle_close_prefix_delete_modal(&mut self) -> Task<Message> {
        self.prefix_delete = None;
        Task::none()
    }

    pub(crate) fn handle_prefix_delete_list_loaded(
        &mut self,
        result: Result<Vec<String>, String>,
    ) -> Task<Message> {
        let Some(state) = self.prefix_delete.as_mut() else {
            return Task::none();
        };
        state.loading = false;
        match result {
            Ok(keys) => {
                state.total = keys.len();
                state.keys = keys;
            }
            Err(e) => {
                state.summary = Some(format!("Failed to list: {}", e));
            }
        }
        Task::none()
    }

    pub(crate) fn handle_confirm_prefix_delete(&mut self) -> Task<Message> {
        self.cmd_process_next_prefix_delete_batch()
    }

    pub(crate) fn handle_prefix_delete_batch_finished(
        &mut self,
        result: Result<usize, String>,
    ) -> Task<Message> {
        let Some(state) = self.prefix_delete.as_mut() else {
            return Task::none();
        };
        match result {
            Ok(count) => {
                state.deleted += count;
                state.summary = Some(format!("Deleting: {}/{} done", state.deleted, state.total));
                self.cmd_process_next_prefix_delete_batch()
            }
            Err(error) => {
                state.deleting = false;
                state.summary = Some(format!(
                    "Stopped after {}/{}: {}",
                    state.deleted, state.total, error
                ));
                self.error = Some(format!("Prefix delete failed: {}", error));
                Task::none()
            }
        }
    }

    pub(crate) fn handle_open_delete_bucket_modal(&mut self) -> Task<Message> {
        let Some(bucket) = self.current_selected_bucket() else {
            return Task::none();
        };
        self.bucket_delete = Some(BucketDeleteState {
            bucket: bucket.clone(),
            confirm_name: String::new(),
            preview_loading: true,
            object_keys: Vec::new(),
            total_objects: 0,
            deleted_objects: 0,
            next_index: 0,
            deleting: false,
            summary: None,
        });
        let client = self.client.clone();
        Task::perform(
            async move {
                let result = client
                    .list_objects(&bucket, "", "")
                    .await
                    .map(|listing| listing.objects);
                (bucket, result)
            },
            |(bucket, result)| Message::BucketDeletePreviewLoaded { bucket, result },
        )
    }

    pub(crate) fn handle_close_delete_bucket_modal(&mut self) -> Task<Message> {
        if self
            .bucket_delete
            .as_ref()
            .is_some_and(|state| state.deleting)
        {
            return Task::none();
        }
        self.bucket_delete = None;
        Task::none()
    }

    pub(crate) fn handle_bucket_delete_preview_loaded(
        &mut self,
        bucket: String,
        result: Result<Vec<ObjectInfo>, String>,
    ) -> Task<Message> {
        let Some(state) = self.bucket_delete.as_mut() else {
            return Task::none();
        };
        if state.bucket != bucket {
            return Task::none();
        }
        state.preview_loading = false;
        match result {
            Ok(objects) => {
                state.total_objects = objects.len();
                state.object_keys = objects.into_iter().map(|object| object.key).collect();
                state.summary = Some(if state.total_objects == 0 {
                    "Bucket is empty.".to_string()
                } else {
                    format!(
                        "Bucket contains {} object(s). Delete will remove them recursively.",
                        state.total_objects
                    )
                });
            }
            Err(error) => {
                state.summary = Some(format!("Preview failed: {}", error));
            }
        }
        Task::none()
    }

    pub(crate) fn handle_bucket_delete_confirm_name_changed(
        &mut self,
        value: String,
    ) -> Task<Message> {
        if let Some(state) = self.bucket_delete.as_mut() {
            state.confirm_name = value;
        }
        Task::none()
    }

    pub(crate) fn handle_confirm_delete_bucket(&mut self) -> Task<Message> {
        self.cmd_process_next_bucket_delete_step()
    }

    pub(crate) fn handle_bucket_delete_step_finished(
        &mut self,
        result: Result<BucketDeleteStepResult, String>,
    ) -> Task<Message> {
        let Some(state) = self.bucket_delete.as_mut() else {
            return Task::none();
        };
        match result {
            Ok(BucketDeleteStepResult::ObjectDeleted(label)) => {
                state.deleted_objects += 1;
                state.next_index += 1;
                state.summary = Some(format!(
                    "Deleting objects: {}/{} complete. Last: {}",
                    state.deleted_objects, state.total_objects, label
                ));
                self.cmd_process_next_bucket_delete_step()
            }
            Ok(BucketDeleteStepResult::BucketDeleted(bucket)) => {
                self.bucket_delete = None;
                if self.selected_bucket.as_deref() == Some(&bucket) {
                    self.selected_bucket = None;
                    self.selection = Selection::None;
                    self.current_prefix.clear();
                    self.object_filter.clear();
                    self.selected_keys.clear();
                    self.find_results = None;
                    self.objects = None;
                    self.detail = None;
                    self.clear_object_admin_state();
                }
                self.loading_buckets = true;
                Task::batch(vec![self.cmd_fetch_buckets(), Task::none()])
            }
            Err(error) => {
                state.deleting = false;
                state.summary = Some(format!(
                    "Delete stopped after {} of {} objects: {}",
                    state.deleted_objects, state.total_objects, error
                ));
                self.error = Some(format!("Delete bucket failed: {}", error));
                if self.selected_bucket.as_deref() == Some(&state.bucket) {
                    self.loading_objects = true;
                    Task::batch(vec![self.cmd_fetch_buckets(), self.cmd_fetch_objects()])
                } else {
                    self.loading_buckets = true;
                    self.cmd_fetch_buckets()
                }
            }
        }
    }

    // -- helpers --

    pub(crate) fn bucket_delete_can_start(&self) -> bool {
        let Some(state) = &self.bucket_delete else {
            return false;
        };
        !state.preview_loading
            && !state.deleting
            && state.confirm_name == state.bucket
            && !state.bucket.is_empty()
    }

    fn cmd_process_next_bucket_delete_step(&mut self) -> Task<Message> {
        let Some(state) = self.bucket_delete.as_mut() else {
            return Task::none();
        };
        if state.preview_loading || state.confirm_name != state.bucket {
            return Task::none();
        }
        state.deleting = true;

        if state.next_index < state.object_keys.len() {
            let client = self.client.clone();
            let bucket = state.bucket.clone();
            let key = state.object_keys[state.next_index].clone();
            return Task::perform(
                async move {
                    client.delete_object(&bucket, &key).await?;
                    Ok(BucketDeleteStepResult::ObjectDeleted(key))
                },
                Message::BucketDeleteStepFinished,
            );
        }

        let client = self.client.clone();
        let bucket = state.bucket.clone();
        Task::perform(
            async move {
                client.delete_bucket(&bucket).await?;
                Ok(BucketDeleteStepResult::BucketDeleted(bucket))
            },
            Message::BucketDeleteStepFinished,
        )
    }

    fn cmd_process_next_bulk_delete_step(&mut self) -> Task<Message> {
        let Some(state) = self.bulk_delete.as_mut() else {
            return Task::none();
        };
        state.deleting = true;

        if state.next_index < state.keys.len() {
            let client = self.client.clone();
            let bucket = state.bucket.clone();
            let end = (state.next_index + 1000).min(state.keys.len());
            let batch: Vec<String> = state.keys[state.next_index..end].to_vec();
            let batch_size = batch.len();
            state.next_index = end;
            return Task::perform(
                async move {
                    let failed = client.delete_objects(&bucket, &batch).await?;
                    Ok(batch_size - failed.len())
                },
                Message::BulkDeleteBatchFinished,
            );
        }

        // all done
        let deleted = state.deleted;
        let total = state.total;
        self.bulk_delete = None;
        self.selected_keys.clear();
        self.loading_objects = true;
        self.error = None;
        let summary = format!("Deleted {} of {} objects", deleted, total);
        self.error = Some(summary);
        self.cmd_fetch_objects()
    }

    fn cmd_process_next_prefix_delete_batch(&mut self) -> Task<Message> {
        let Some(state) = self.prefix_delete.as_mut() else {
            return Task::none();
        };
        state.deleting = true;

        if state.next_index < state.keys.len() {
            let client = self.client.clone();
            let bucket = state.bucket.clone();
            let end = (state.next_index + 1000).min(state.keys.len());
            let batch: Vec<String> = state.keys[state.next_index..end].to_vec();
            let batch_size = batch.len();
            state.next_index = end;
            return Task::perform(
                async move {
                    let failed = client.delete_objects(&bucket, &batch).await?;
                    Ok(batch_size - failed.len())
                },
                Message::PrefixDeleteBatchFinished,
            );
        }

        // all done
        let deleted = state.deleted;
        let total = state.total;
        self.prefix_delete = None;
        self.selected_keys.clear();
        self.loading_objects = true;
        self.error = None;
        let summary = format!("Deleted {} of {} objects under prefix", deleted, total);
        self.error = Some(summary);
        self.cmd_fetch_objects()
    }
}
