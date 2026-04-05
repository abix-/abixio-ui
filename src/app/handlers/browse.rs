use iced::Task;

use crate::s3::client::{BucketInfo, ListObjectsResult, ObjectDetail};

use super::super::transfer_ops::wildcard_match;
use super::super::{App, BucketDocumentKind, Message, Selection};

impl App {
    pub(crate) fn handle_select_bucket(&mut self, name: String) -> Task<Message> {
        self.selected_bucket = Some(name.clone());
        self.current_prefix.clear();
        self.object_filter.clear();
        self.selected_keys.clear();
        self.find_results = None;
        self.selection = Selection::Bucket(name.clone());
        self.clear_object_admin_state();
        self.loading_objects = true;
        self.reset_bucket_document_states();
        self.bucket_tags = None;
        Task::batch(vec![
            self.cmd_fetch_objects(),
            self.cmd_fetch_versioning_status(&name),
            self.cmd_fetch_bucket_document(BucketDocumentKind::Policy, &name),
            self.cmd_fetch_bucket_document(BucketDocumentKind::Lifecycle, &name),
            self.cmd_fetch_bucket_tags(&name),
        ])
    }

    pub(crate) fn handle_navigate_prefix(&mut self, prefix: String) -> Task<Message> {
        self.current_prefix = prefix;
        self.object_filter.clear();
        self.selected_keys.clear();
        self.find_results = None;
        self.selection = Selection::None;
        self.clear_object_admin_state();
        self.loading_objects = true;
        self.cmd_fetch_objects()
    }

    pub(crate) fn handle_select_object(&mut self, key: String) -> Task<Message> {
        let bucket = self.selected_bucket.clone().unwrap_or_default();
        self.clear_object_admin_state();
        self.selection = Selection::Object {
            bucket: bucket.clone(),
            key: key.clone(),
        };
        self.loading_detail = true;
        self.loading_tags = true;
        self.loading_versions = true;
        self.object_tags = None;
        self.object_versions = None;
        if self.is_abixio && self.admin_client.is_some() {
            self.loading_object_inspect = true;
            self.object_inspect_target = Some((bucket.clone(), key.clone()));
            Task::batch(vec![
                self.cmd_fetch_detail(&bucket, &key),
                self.cmd_fetch_object_inspect(&bucket, &key),
                self.cmd_fetch_tags(&bucket, &key),
                self.cmd_fetch_versions(&bucket, &key),
                self.cmd_fetch_preview(&bucket, &key),
            ])
        } else {
            Task::batch(vec![
                self.cmd_fetch_detail(&bucket, &key),
                self.cmd_fetch_tags(&bucket, &key),
                self.cmd_fetch_versions(&bucket, &key),
                self.cmd_fetch_preview(&bucket, &key),
            ])
        }
    }

    pub(crate) fn handle_clear_selection(&mut self) -> Task<Message> {
        self.selection = Selection::None;
        self.clear_object_admin_state();
        self.bucket_policy.cancel_editing();
        self.bucket_lifecycle.cancel_editing();
        Task::none()
    }

    pub(crate) fn handle_buckets_loaded(
        &mut self,
        r: Result<Vec<BucketInfo>, String>,
    ) -> Task<Message> {
        self.loading_buckets = false;
        self.buckets = Some(r);
        Task::none()
    }

    pub(crate) fn handle_objects_loaded(
        &mut self,
        r: Result<ListObjectsResult, String>,
    ) -> Task<Message> {
        self.loading_objects = false;
        self.objects = Some(r);
        Task::none()
    }

    pub(crate) fn handle_detail_loaded(
        &mut self,
        r: Result<ObjectDetail, String>,
    ) -> Task<Message> {
        self.loading_detail = false;
        self.detail = Some(r);
        Task::none()
    }

    pub(crate) fn handle_upload_done(&mut self, r: Result<String, String>) -> Task<Message> {
        match r {
            Ok(_) => {
                self.loading_objects = true;
                self.cmd_fetch_objects()
            }
            Err(e) => {
                self.error = Some(format!("Upload failed: {}", e));
                Task::none()
            }
        }
    }

    pub(crate) fn handle_delete_done(&mut self, r: Result<(), String>) -> Task<Message> {
        match r {
            Ok(()) => {
                self.selection = Selection::None;
                self.clear_object_admin_state();
                self.reset_bucket_document_states();
                self.loading_objects = true;
                self.cmd_fetch_objects()
            }
            Err(e) => {
                self.error = Some(format!("Delete failed: {}", e));
                Task::none()
            }
        }
    }

    pub(crate) fn handle_create_bucket_done(
        &mut self,
        bucket: String,
        result: Result<(), String>,
    ) -> Task<Message> {
        match result {
            Ok(()) => {
                self.create_bucket_modal_open = false;
                self.create_bucket_modal_error = None;
                self.selected_bucket = Some(bucket.clone());
                self.selection = Selection::Bucket(bucket);
                self.current_prefix.clear();
                self.object_filter.clear();
                self.selected_keys.clear();
                self.find_results = None;
                self.objects = None;
                self.detail = None;
                self.reset_bucket_document_states();
                self.loading_buckets = true;
                self.loading_objects = true;
                Task::batch(vec![self.cmd_fetch_buckets(), self.cmd_fetch_objects()])
            }
            Err(e) => {
                self.create_bucket_modal_error = Some(e.clone());
                self.error = Some(format!("Create bucket failed: {}", e));
                Task::none()
            }
        }
    }

    pub(crate) fn handle_download_done(&mut self, r: Result<String, String>) -> Task<Message> {
        match r {
            Ok(_) => Task::none(),
            Err(e) => {
                self.error = Some(format!("Download failed: {}", e));
                Task::none()
            }
        }
    }

    pub(crate) fn handle_refresh(&mut self) -> Task<Message> {
        self.find_results = None;
        self.selected_keys.clear();
        self.loading_objects = true;
        self.cmd_fetch_objects()
    }

    pub(crate) fn handle_refresh_all(&mut self) -> Task<Message> {
        self.loading_buckets = true;
        self.cmd_fetch_buckets()
    }

    pub(crate) fn handle_upload(&mut self) -> Task<Message> {
        let file = rfd::FileDialog::new().pick_file();
        let file = match file {
            Some(f) => f,
            None => return Task::none(),
        };
        let client = self.client.clone();
        let bucket = match &self.selected_bucket {
            Some(b) => b.clone(),
            None => return Task::none(),
        };
        let prefix = self.current_prefix.clone();
        let filename = file
            .file_name()
            .map(|n| n.to_string_lossy().to_string())
            .unwrap_or_else(|| "upload".to_string());
        let key = format!("{}{}", prefix, filename);
        Task::perform(
            async move {
                client
                    .upload_file(&bucket, &key, &file, "application/octet-stream")
                    .await
            },
            Message::UploadDone,
        )
    }

    pub(crate) fn handle_delete(&mut self, bucket: String, key: String) -> Task<Message> {
        let client = self.client.clone();
        Task::perform(
            async move { client.delete_object(&bucket, &key).await },
            Message::DeleteDone,
        )
    }

    pub(crate) fn handle_download(&mut self, bucket: String, key: String) -> Task<Message> {
        let filename = key.rsplit('/').next().unwrap_or(&key).to_string();
        let save_path = rfd::FileDialog::new().set_file_name(&filename).save_file();
        let save_path = match save_path {
            Some(p) => p,
            None => return Task::none(),
        };
        let client = self.client.clone();
        Task::perform(
            async move {
                let data = client.get_object(&bucket, &key).await?;
                tokio::fs::write(&save_path, &data)
                    .await
                    .map_err(|e| e.to_string())?;
                Ok(save_path.to_string_lossy().to_string())
            },
            Message::DownloadDone,
        )
    }

    pub(crate) fn handle_object_filter_changed(&mut self, value: String) -> Task<Message> {
        self.object_filter = value;
        Task::none()
    }

    pub(crate) fn handle_find(&mut self) -> Task<Message> {
        let bucket = match &self.selected_bucket {
            Some(b) => b.clone(),
            None => return Task::none(),
        };
        if self.object_filter.is_empty() {
            return Task::none();
        }
        self.finding = true;
        self.find_results = None;
        let client = self.client.clone();
        let prefix = self.current_prefix.clone();
        let pattern = self.object_filter.clone();
        Task::perform(
            async move {
                let result = client.list_objects_recursive(&bucket, &prefix).await?;
                let filtered: Vec<_> = result
                    .objects
                    .into_iter()
                    .filter(|obj| wildcard_match(&pattern, &obj.key))
                    .collect();
                Ok(ListObjectsResult {
                    objects: filtered,
                    common_prefixes: Vec::new(),
                    is_truncated: result.is_truncated,
                })
            },
            Message::FindComplete,
        )
    }

    pub(crate) fn handle_find_complete(
        &mut self,
        r: Result<ListObjectsResult, String>,
    ) -> Task<Message> {
        self.finding = false;
        self.find_results = Some(r);
        Task::none()
    }

    pub(crate) fn handle_clear_find(&mut self) -> Task<Message> {
        self.find_results = None;
        self.selected_keys.clear();
        Task::none()
    }

    pub(crate) fn handle_toggle_object_selected(&mut self, key: String) -> Task<Message> {
        if !self.selected_keys.remove(&key) {
            self.selected_keys.insert(key);
        }
        Task::none()
    }

    pub(crate) fn handle_select_all_objects(&mut self) -> Task<Message> {
        if let Some(Ok(result)) = &self.objects {
            let filter = self.object_filter.to_ascii_lowercase();
            for obj in &result.objects {
                let display = obj
                    .key
                    .strip_prefix(&self.current_prefix)
                    .unwrap_or(&obj.key);
                if filter.is_empty() || display.to_ascii_lowercase().contains(&filter) {
                    self.selected_keys.insert(obj.key.clone());
                }
            }
        }
        if let Some(Ok(result)) = &self.find_results {
            for obj in &result.objects {
                self.selected_keys.insert(obj.key.clone());
            }
        }
        Task::none()
    }

    pub(crate) fn handle_clear_object_selection(&mut self) -> Task<Message> {
        self.selected_keys.clear();
        Task::none()
    }

    pub(crate) fn handle_new_bucket_name_changed(&mut self, val: String) -> Task<Message> {
        self.new_bucket_name = val;
        self.create_bucket_modal_error = None;
        Task::none()
    }

    pub(crate) fn handle_open_create_bucket_modal(&mut self) -> Task<Message> {
        self.new_bucket_name.clear();
        self.create_bucket_modal_error = None;
        self.create_bucket_modal_open = true;
        Task::none()
    }

    pub(crate) fn handle_close_create_bucket_modal(&mut self) -> Task<Message> {
        self.create_bucket_modal_open = false;
        self.create_bucket_modal_error = None;
        Task::none()
    }

    pub(crate) fn handle_create_bucket(&mut self) -> Task<Message> {
        let name = self.new_bucket_name.trim().to_string();
        if name.is_empty() {
            self.create_bucket_modal_error = Some("Bucket name is required.".to_string());
            return Task::none();
        }
        let client = self.client.clone();
        self.create_bucket_modal_error = None;
        Task::perform(
            async move {
                let result = client.create_bucket(&name).await;
                (name, result)
            },
            |(bucket, result)| Message::CreateBucketDone { bucket, result },
        )
    }

    // -- command helpers --

    pub(crate) fn cmd_fetch_buckets(&self) -> Task<Message> {
        let client = self.client.clone();
        Task::perform(
            async move { client.list_buckets().await },
            Message::BucketsLoaded,
        )
    }

    pub(crate) fn cmd_fetch_objects(&self) -> Task<Message> {
        let client = self.client.clone();
        let bucket = match &self.selected_bucket {
            Some(b) => b.clone(),
            None => return Task::none(),
        };
        let prefix = self.current_prefix.clone();
        Task::perform(
            async move { client.list_objects(&bucket, &prefix, "/").await },
            Message::ObjectsLoaded,
        )
    }

    pub(crate) fn cmd_fetch_detail(&self, bucket: &str, key: &str) -> Task<Message> {
        let client = self.client.clone();
        let bucket = bucket.to_string();
        let key = key.to_string();
        Task::perform(
            async move { client.head_object(&bucket, &key).await },
            Message::DetailLoaded,
        )
    }
}
