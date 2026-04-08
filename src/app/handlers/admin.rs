use iced::Task;

use crate::abixio::types::{
    ClusterNodesResponse, DisksResponse, EcConfig, HealResponse, HealStatusResponse,
    ObjectInspectResponse, StatusResponse,
};

use super::super::{App, Message};

impl App {
    pub(crate) fn handle_abixio_detected(
        &mut self,
        status: Option<StatusResponse>,
    ) -> Task<Message> {
        if let Some(s) = status {
            self.is_abixio = true;
            let cluster_enabled = s.cluster.enabled;
            self.server_status = Some(s);
            // auto-fetch disks + heal status
            let admin = self.admin_client.clone();
            let mut tasks = vec![
                Task::perform(
                    async move {
                        if let Some(a) = admin.as_ref() {
                            a.disks().await
                        } else {
                            Err("no admin client".to_string())
                        }
                    },
                    Message::DisksLoaded,
                ),
                {
                    let admin = self.admin_client.clone();
                    Task::perform(
                        async move {
                            if let Some(a) = admin.as_ref() {
                                a.heal_status().await
                            } else {
                                Err("no admin client".to_string())
                            }
                        },
                        Message::HealStatusLoaded,
                    )
                },
            ];
            if cluster_enabled {
                let admin = self.admin_client.clone();
                tasks.push(Task::perform(
                    async move {
                        if let Some(a) = admin.as_ref() {
                            a.cluster_nodes().await
                        } else {
                            Err("no admin client".to_string())
                        }
                    },
                    Message::ClusterNodesLoaded,
                ));
            }
            if self.auto_run_tests && !self.auto_test_started {
                tasks.push(Task::perform(async {}, |_| Message::AutoStartTests));
            }
            return Task::batch(tasks);
        } else {
            self.is_abixio = false;
            self.server_status = None;
            if self.auto_run_tests && !self.auto_test_started {
                return Task::perform(async {}, |_| Message::AutoStartTests);
            }
        }
        Task::none()
    }

    pub(crate) fn handle_disks_loaded(
        &mut self,
        result: Result<DisksResponse, String>,
    ) -> Task<Message> {
        self.disks_data = Some(result);
        Task::none()
    }

    pub(crate) fn handle_heal_status_loaded(
        &mut self,
        result: Result<HealStatusResponse, String>,
    ) -> Task<Message> {
        self.heal_data = Some(result);
        Task::none()
    }

    pub(crate) fn handle_object_inspect_loaded(
        &mut self,
        bucket: String,
        key: String,
        result: Result<ObjectInspectResponse, String>,
    ) -> Task<Message> {
        if !self.selected_object_matches(&bucket, &key) {
            return Task::none();
        }
        self.loading_object_inspect = false;
        self.object_inspect_target = None;
        self.object_inspect = Some(result);
        Task::none()
    }

    pub(crate) fn handle_refresh_disks(&mut self) -> Task<Message> {
        let admin = self.admin_client.clone();
        Task::perform(
            async move {
                if let Some(a) = admin.as_ref() {
                    a.disks().await
                } else {
                    Err("no admin client".to_string())
                }
            },
            Message::DisksLoaded,
        )
    }

    pub(crate) fn handle_refresh_heal_status(&mut self) -> Task<Message> {
        let admin = self.admin_client.clone();
        Task::perform(
            async move {
                if let Some(a) = admin.as_ref() {
                    a.heal_status().await
                } else {
                    Err("no admin client".to_string())
                }
            },
            Message::HealStatusLoaded,
        )
    }

    pub(crate) fn handle_refresh_object_inspect(&mut self) -> Task<Message> {
        let Some((bucket, key)) = self.current_selected_object() else {
            return Task::none();
        };
        if !self.is_abixio || self.admin_client.is_none() {
            return Task::none();
        }
        self.loading_object_inspect = true;
        self.object_inspect_target = Some((bucket.clone(), key.clone()));
        self.cmd_fetch_object_inspect(&bucket, &key)
    }

    pub(crate) fn handle_open_heal_confirm(&mut self) -> Task<Message> {
        let Some((bucket, key)) = self.current_selected_object() else {
            return Task::none();
        };
        if !self.is_abixio || self.admin_client.is_none() || self.healing_object {
            return Task::none();
        }
        self.heal_confirm_target = Some((bucket, key));
        Task::none()
    }

    pub(crate) fn handle_cancel_heal_confirm(&mut self) -> Task<Message> {
        self.heal_confirm_target = None;
        Task::none()
    }

    pub(crate) fn handle_confirm_heal_object(&mut self) -> Task<Message> {
        let Some((bucket, key)) = self.heal_confirm_target.take() else {
            return Task::none();
        };
        self.healing_object = true;
        self.healing_target = Some((bucket.clone(), key.clone()));
        self.heal_result = Some("Healing object...".to_string());
        self.cmd_heal_object(&bucket, &key)
    }

    pub(crate) fn handle_heal_object_finished(
        &mut self,
        bucket: String,
        key: String,
        result: Result<HealResponse, String>,
    ) -> Task<Message> {
        let healing_matches = self.healing_target.as_ref() == Some(&(bucket.clone(), key.clone()));
        if healing_matches {
            self.healing_object = false;
            self.healing_target = None;
        }
        if !self.selected_object_matches(&bucket, &key) {
            return Task::none();
        }
        match result {
            Ok(heal) => {
                let suffix = heal
                    .shards_fixed
                    .map(|count| format!(" ({} shards fixed)", count))
                    .unwrap_or_default();
                self.heal_result = Some(format!("{}{}", heal.result, suffix));
                self.loading_object_inspect = true;
                self.object_inspect_target = Some((bucket.clone(), key.clone()));
                Task::batch(vec![
                    self.cmd_fetch_object_inspect(&bucket, &key),
                    self.refresh_heal_status_task(),
                ])
            }
            Err(error) => {
                self.heal_result = Some(format!("Heal failed: {}", error));
                self.error = Some(format!("Heal failed: {}", error));
                Task::none()
            }
        }
    }

    // -- command helpers --

    pub(crate) fn cmd_fetch_object_inspect(&self, bucket: &str, key: &str) -> Task<Message> {
        let admin = self.admin_client.clone();
        let bucket = bucket.to_string();
        let key = key.to_string();
        Task::perform(
            async move {
                let result = if let Some(a) = admin.as_ref() {
                    a.inspect_object(&bucket, &key).await
                } else {
                    Err("no admin client".to_string())
                };
                (bucket, key, result)
            },
            |(bucket, key, result)| Message::ObjectInspectLoaded {
                bucket,
                key,
                result,
            },
        )
    }

    pub(crate) fn cmd_heal_object(&self, bucket: &str, key: &str) -> Task<Message> {
        let admin = self.admin_client.clone();
        let bucket = bucket.to_string();
        let key = key.to_string();
        Task::perform(
            async move {
                let result = if let Some(a) = admin.as_ref() {
                    a.heal_object(&bucket, &key).await
                } else {
                    Err("no admin client".to_string())
                };
                (bucket, key, result)
            },
            |(bucket, key, result)| Message::HealObjectFinished {
                bucket,
                key,
                result,
            },
        )
    }

    pub(crate) fn refresh_heal_status_task(&self) -> Task<Message> {
        let admin = self.admin_client.clone();
        Task::perform(
            async move {
                if let Some(a) = admin.as_ref() {
                    a.heal_status().await
                } else {
                    Err("no admin client".to_string())
                }
            },
            Message::HealStatusLoaded,
        )
    }

    pub(crate) fn handle_refresh_cluster_nodes(&mut self) -> Task<Message> {
        let admin = self.admin_client.clone();
        Task::perform(
            async move {
                if let Some(a) = admin.as_ref() {
                    a.cluster_nodes().await
                } else {
                    Err("no admin client".to_string())
                }
            },
            Message::ClusterNodesLoaded,
        )
    }

    pub(crate) fn handle_cluster_nodes_loaded(
        &mut self,
        result: Result<ClusterNodesResponse, String>,
    ) -> Task<Message> {
        self.cluster_nodes = Some(result);
        Task::none()
    }

    pub(crate) fn handle_bucket_ftt_loaded(
        &mut self,
        result: Result<EcConfig, String>,
    ) -> Task<Message> {
        self.bucket_ftt = Some(result.map(|c| c.ftt));
        Task::none()
    }

    pub(crate) fn cmd_fetch_bucket_ftt(&self, bucket: &str) -> Task<Message> {
        let admin = self.admin_client.clone();
        let bucket = bucket.to_string();
        Task::perform(
            async move {
                if let Some(a) = admin.as_ref() {
                    a.get_bucket_ftt(&bucket).await
                } else {
                    Err("no admin client".to_string())
                }
            },
            Message::BucketFttLoaded,
        )
    }

    pub(crate) fn clear_object_admin_state(&mut self) {
        self.object_inspect = None;
        self.loading_object_inspect = false;
        self.object_inspect_target = None;
        self.heal_confirm_target = None;
        self.healing_object = false;
        self.healing_target = None;
        self.heal_result = None;
        self.object_tags = None;
        self.loading_tags = false;
        self.editing_tag_key.clear();
        self.editing_tag_value.clear();
        self.object_versions = None;
        self.loading_versions = false;
        self.object_preview = None;
        self.share_modal_open = false;
        self.share_url = None;
    }
}
