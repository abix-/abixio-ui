use iced::Task;

use crate::s3::client::VersionInfo;

use super::super::{App, Message, Selection};

impl App {
    // -- presigned sharing --

    pub(crate) fn handle_open_share_modal(&mut self) -> Task<Message> {
        self.share_modal_open = true;
        self.share_url = None;
        Task::none()
    }

    pub(crate) fn handle_close_share_modal(&mut self) -> Task<Message> {
        self.share_modal_open = false;
        self.share_url = None;
        Task::none()
    }

    pub(crate) fn handle_share_expiry_changed(&mut self, s: String) -> Task<Message> {
        self.share_expiry_secs = s.parse().unwrap_or(3600);
        Task::none()
    }

    pub(crate) fn handle_generate_share_url(&mut self) -> Task<Message> {
        if let Selection::Object { bucket, key } = &self.selection {
            let client = self.client.clone();
            let bucket = bucket.clone();
            let key = key.clone();
            let secs = self.share_expiry_secs;
            Task::perform(
                async move { client.presign_get_object(&bucket, &key, secs).await },
                Message::ShareUrlGenerated,
            )
        } else {
            Task::none()
        }
    }

    pub(crate) fn handle_share_url_generated(
        &mut self,
        r: Result<String, String>,
    ) -> Task<Message> {
        match r {
            Ok(url) => self.share_url = Some(url),
            Err(e) => self.error = Some(format!("presign failed: {}", e)),
        }
        Task::none()
    }

    // -- bucket policy/lifecycle/tags --

    pub(crate) fn handle_bucket_policy_loaded(
        &mut self,
        r: Result<String, String>,
    ) -> Task<Message> {
        self.bucket_policy = Some(r);
        Task::none()
    }

    pub(crate) fn handle_bucket_lifecycle_loaded(
        &mut self,
        r: Result<String, String>,
    ) -> Task<Message> {
        self.bucket_lifecycle = Some(r);
        Task::none()
    }

    pub(crate) fn handle_bucket_tags_loaded(
        &mut self,
        r: Result<std::collections::HashMap<String, String>, String>,
    ) -> Task<Message> {
        self.bucket_tags = Some(r);
        Task::none()
    }

    pub(crate) fn handle_delete_bucket_policy(&mut self) -> Task<Message> {
        if let Some(bucket) = &self.selected_bucket {
            let client = self.client.clone();
            let bucket = bucket.clone();
            Task::perform(
                async move { client.delete_bucket_policy(&bucket).await },
                Message::BucketPolicyDeleted,
            )
        } else {
            Task::none()
        }
    }

    pub(crate) fn handle_bucket_policy_deleted(
        &mut self,
        r: Result<(), String>,
    ) -> Task<Message> {
        if let Err(e) = r {
            self.error = Some(format!("delete policy failed: {}", e));
        }
        self.bucket_policy = Some(Ok(String::new()));
        Task::none()
    }

    pub(crate) fn handle_delete_bucket_lifecycle(&mut self) -> Task<Message> {
        if let Some(bucket) = &self.selected_bucket {
            let client = self.client.clone();
            let bucket = bucket.clone();
            Task::perform(
                async move { client.delete_bucket_lifecycle(&bucket).await },
                Message::BucketLifecycleDeleted,
            )
        } else {
            Task::none()
        }
    }

    pub(crate) fn handle_bucket_lifecycle_deleted(
        &mut self,
        r: Result<(), String>,
    ) -> Task<Message> {
        if let Err(e) = r {
            self.error = Some(format!("delete lifecycle failed: {}", e));
        }
        self.bucket_lifecycle = Some(Ok(String::new()));
        Task::none()
    }

    pub(crate) fn handle_bucket_tag_key_changed(&mut self, s: String) -> Task<Message> {
        self.bucket_tag_key = s;
        Task::none()
    }

    pub(crate) fn handle_bucket_tag_value_changed(&mut self, s: String) -> Task<Message> {
        self.bucket_tag_value = s;
        Task::none()
    }

    pub(crate) fn handle_add_bucket_tag(&mut self) -> Task<Message> {
        let key = self.bucket_tag_key.trim().to_string();
        let value = self.bucket_tag_value.trim().to_string();
        if key.is_empty() {
            return Task::none();
        }
        let mut tags = match &self.bucket_tags {
            Some(Ok(t)) => t.clone(),
            _ => std::collections::HashMap::new(),
        };
        tags.insert(key, value);
        self.bucket_tag_key.clear();
        self.bucket_tag_value.clear();
        if let Some(bucket) = &self.selected_bucket {
            let client = self.client.clone();
            let bucket = bucket.clone();
            Task::perform(
                async move { client.put_bucket_tags(&bucket, tags).await },
                Message::BucketTagsSaved,
            )
        } else {
            Task::none()
        }
    }

    pub(crate) fn handle_remove_bucket_tag(&mut self, tag_key: String) -> Task<Message> {
        let mut tags = match &self.bucket_tags {
            Some(Ok(t)) => t.clone(),
            _ => std::collections::HashMap::new(),
        };
        tags.remove(&tag_key);
        if let Some(bucket) = &self.selected_bucket {
            let client = self.client.clone();
            let bucket = bucket.clone();
            if tags.is_empty() {
                Task::perform(
                    async move { client.delete_bucket_tags(&bucket).await },
                    Message::BucketTagsSaved,
                )
            } else {
                Task::perform(
                    async move { client.put_bucket_tags(&bucket, tags).await },
                    Message::BucketTagsSaved,
                )
            }
        } else {
            Task::none()
        }
    }

    pub(crate) fn handle_bucket_tags_saved(&mut self, r: Result<(), String>) -> Task<Message> {
        match r {
            Ok(()) => {
                if let Some(bucket) = &self.selected_bucket {
                    self.cmd_fetch_bucket_tags(bucket)
                } else {
                    Task::none()
                }
            }
            Err(e) => {
                self.error = Some(format!("bucket tag save failed: {}", e));
                Task::none()
            }
        }
    }

    // -- object preview --

    pub(crate) fn handle_preview_loaded(
        &mut self,
        r: Result<String, String>,
    ) -> Task<Message> {
        self.object_preview = Some(r);
        Task::none()
    }

    // -- versioning --

    pub(crate) fn handle_versioning_status_loaded(
        &mut self,
        r: Result<String, String>,
    ) -> Task<Message> {
        self.bucket_versioning = Some(r);
        Task::none()
    }

    pub(crate) fn handle_versions_loaded(
        &mut self,
        r: Result<Vec<VersionInfo>, String>,
    ) -> Task<Message> {
        self.loading_versions = false;
        self.object_versions = Some(r);
        Task::none()
    }

    pub(crate) fn handle_enable_versioning(&mut self) -> Task<Message> {
        if let Some(bucket) = &self.selected_bucket {
            let client = self.client.clone();
            let bucket = bucket.clone();
            Task::perform(
                async move { client.put_bucket_versioning(&bucket, "Enabled").await },
                Message::VersioningToggled,
            )
        } else {
            Task::none()
        }
    }

    pub(crate) fn handle_suspend_versioning(&mut self) -> Task<Message> {
        if let Some(bucket) = &self.selected_bucket {
            let client = self.client.clone();
            let bucket = bucket.clone();
            Task::perform(
                async move { client.put_bucket_versioning(&bucket, "Suspended").await },
                Message::VersioningToggled,
            )
        } else {
            Task::none()
        }
    }

    pub(crate) fn handle_versioning_toggled(&mut self, r: Result<(), String>) -> Task<Message> {
        match r {
            Ok(()) => {
                if let Some(bucket) = &self.selected_bucket {
                    let client = self.client.clone();
                    let bucket = bucket.clone();
                    Task::perform(
                        async move { client.get_bucket_versioning(&bucket).await },
                        Message::VersioningStatusLoaded,
                    )
                } else {
                    Task::none()
                }
            }
            Err(e) => {
                self.error = Some(format!("versioning toggle failed: {}", e));
                Task::none()
            }
        }
    }

    pub(crate) fn handle_delete_version(&mut self, vid: String) -> Task<Message> {
        if let Selection::Object { bucket, key } = &self.selection {
            let client = self.client.clone();
            let bucket = bucket.clone();
            let key = key.clone();
            self.loading_versions = true;
            Task::perform(
                async move { client.delete_object_version(&bucket, &key, &vid).await },
                Message::VersionDeleted,
            )
        } else {
            Task::none()
        }
    }

    pub(crate) fn handle_version_deleted(&mut self, r: Result<(), String>) -> Task<Message> {
        match r {
            Ok(()) => {
                if let Selection::Object { bucket, key } = &self.selection {
                    self.cmd_fetch_versions(bucket, key)
                } else {
                    self.loading_versions = false;
                    Task::none()
                }
            }
            Err(e) => {
                self.loading_versions = false;
                self.error = Some(format!("version delete failed: {}", e));
                Task::none()
            }
        }
    }

    pub(crate) fn handle_restore_version(&mut self, vid: String) -> Task<Message> {
        if let Selection::Object { bucket, key } = &self.selection {
            let client = self.client.clone();
            let bucket = bucket.clone();
            let key = key.clone();
            self.loading_versions = true;
            Task::perform(
                async move {
                    let data = client.get_object_version(&bucket, &key, &vid).await?;
                    client
                        .put_object(&bucket, &key, data, "application/octet-stream")
                        .await
                },
                Message::VersionRestored,
            )
        } else {
            Task::none()
        }
    }

    pub(crate) fn handle_version_restored(
        &mut self,
        r: Result<String, String>,
    ) -> Task<Message> {
        match r {
            Ok(_) => {
                if let Selection::Object { bucket, key } = &self.selection {
                    Task::batch(vec![
                        self.cmd_fetch_versions(bucket, key),
                        self.cmd_fetch_detail(bucket, key),
                    ])
                } else {
                    self.loading_versions = false;
                    Task::none()
                }
            }
            Err(e) => {
                self.loading_versions = false;
                self.error = Some(format!("version restore failed: {}", e));
                Task::none()
            }
        }
    }

    // -- object tags --

    pub(crate) fn handle_tags_loaded(
        &mut self,
        r: Result<std::collections::HashMap<String, String>, String>,
    ) -> Task<Message> {
        self.loading_tags = false;
        self.object_tags = Some(r);
        Task::none()
    }

    pub(crate) fn handle_tag_key_changed(&mut self, s: String) -> Task<Message> {
        self.editing_tag_key = s;
        Task::none()
    }

    pub(crate) fn handle_tag_value_changed(&mut self, s: String) -> Task<Message> {
        self.editing_tag_value = s;
        Task::none()
    }

    pub(crate) fn handle_add_tag(&mut self) -> Task<Message> {
        let key = self.editing_tag_key.trim().to_string();
        let value = self.editing_tag_value.trim().to_string();
        if key.is_empty() {
            return Task::none();
        }
        let mut tags = match &self.object_tags {
            Some(Ok(t)) => t.clone(),
            _ => std::collections::HashMap::new(),
        };
        if tags.len() >= 10 && !tags.contains_key(&key) {
            self.error = Some("max 10 tags per object".to_string());
            return Task::none();
        }
        tags.insert(key, value);
        self.editing_tag_key.clear();
        self.editing_tag_value.clear();
        self.loading_tags = true;
        if let Selection::Object { bucket, key } = &self.selection {
            let client = self.client.clone();
            let bucket = bucket.clone();
            let key = key.clone();
            Task::perform(
                async move {
                    client.put_object_tags(&bucket, &key, tags).await
                },
                Message::TagsSaved,
            )
        } else {
            Task::none()
        }
    }

    pub(crate) fn handle_remove_tag(&mut self, tag_key: String) -> Task<Message> {
        let mut tags = match &self.object_tags {
            Some(Ok(t)) => t.clone(),
            _ => std::collections::HashMap::new(),
        };
        tags.remove(&tag_key);
        self.loading_tags = true;
        if let Selection::Object { bucket, key } = &self.selection {
            let client = self.client.clone();
            let bucket = bucket.clone();
            let key = key.clone();
            if tags.is_empty() {
                Task::perform(
                    async move {
                        client.delete_object_tags(&bucket, &key).await
                    },
                    Message::TagsSaved,
                )
            } else {
                Task::perform(
                    async move {
                        client.put_object_tags(&bucket, &key, tags).await
                    },
                    Message::TagsSaved,
                )
            }
        } else {
            Task::none()
        }
    }

    pub(crate) fn handle_tags_saved(&mut self, r: Result<(), String>) -> Task<Message> {
        match r {
            Ok(()) => {
                if let Selection::Object { bucket, key } = &self.selection {
                    self.cmd_fetch_tags(bucket, key)
                } else {
                    self.loading_tags = false;
                    Task::none()
                }
            }
            Err(e) => {
                self.loading_tags = false;
                self.error = Some(format!("tag save failed: {}", e));
                Task::none()
            }
        }
    }

    // -- command helpers --

    pub(crate) fn cmd_fetch_versions(&self, bucket: &str, key: &str) -> Task<Message> {
        let client = self.client.clone();
        let bucket = bucket.to_string();
        let key = key.to_string();
        Task::perform(
            async move { client.list_object_versions(&bucket, &key).await },
            Message::VersionsLoaded,
        )
    }

    pub(crate) fn cmd_fetch_bucket_policy(&self, bucket: &str) -> Task<Message> {
        let client = self.client.clone();
        let bucket = bucket.to_string();
        Task::perform(
            async move { client.get_bucket_policy(&bucket).await },
            Message::BucketPolicyLoaded,
        )
    }

    pub(crate) fn cmd_fetch_bucket_lifecycle(&self, bucket: &str) -> Task<Message> {
        let client = self.client.clone();
        let bucket = bucket.to_string();
        Task::perform(
            async move { client.get_bucket_lifecycle(&bucket).await },
            Message::BucketLifecycleLoaded,
        )
    }

    pub(crate) fn cmd_fetch_bucket_tags(&self, bucket: &str) -> Task<Message> {
        let client = self.client.clone();
        let bucket = bucket.to_string();
        Task::perform(
            async move { client.get_bucket_tags(&bucket).await },
            Message::BucketTagsLoaded,
        )
    }

    pub(crate) fn cmd_fetch_preview(&self, bucket: &str, key: &str) -> Task<Message> {
        let client = self.client.clone();
        let bucket = bucket.to_string();
        let key = key.to_string();
        Task::perform(
            async move {
                let data = client.get_object(&bucket, &key).await?;
                // take first 4KB, lossy UTF-8
                let preview_bytes = &data[..data.len().min(4096)];
                Ok(String::from_utf8_lossy(preview_bytes).to_string())
            },
            Message::PreviewLoaded,
        )
    }

    pub(crate) fn cmd_fetch_versioning_status(&self, bucket: &str) -> Task<Message> {
        let client = self.client.clone();
        let bucket = bucket.to_string();
        Task::perform(
            async move { client.get_bucket_versioning(&bucket).await },
            Message::VersioningStatusLoaded,
        )
    }

    pub(crate) fn cmd_fetch_tags(&self, bucket: &str, key: &str) -> Task<Message> {
        let client = self.client.clone();
        let bucket = bucket.to_string();
        let key = key.to_string();
        Task::perform(
            async move { client.get_object_tags(&bucket, &key).await },
            Message::TagsLoaded,
        )
    }
}
