use std::path::PathBuf;

use iced::Task;

use crate::app::sync_ops::{
    build_sync_plan, enumerate_local_for_sync, enumerate_s3_for_sync, prepare_copy_run_plan,
};
use crate::app::transfer_ops::{execute_sync_run_item, now_rfc3339};
use crate::app::{
    App, Message, SyncCompareMode, SyncDeletePhase, SyncDestinationNewerPolicy, SyncEndpointKind,
    SyncExecutionState, SyncListMode, SyncMode, SyncObject, SyncPlan, SyncPreset, SyncRunItem,
    SyncState, TransferEndpoint,
};
use crate::s3::client::BucketInfo;

impl App {
    pub(crate) fn handle_open_sync(&mut self) -> Task<Message> {
        let current_connection_id = self.current_connection_id();
        let mut sync = SyncState::new(current_connection_id.clone());
        sync.source_buckets = self.buckets.clone();
        sync.destination_buckets = self.buckets.clone();
        self.sync = Some(sync);
        self.section = crate::app::Section::Sync;
        Task::none()
    }

    pub(crate) fn handle_close_sync(&mut self) -> Task<Message> {
        self.sync = None;
        Task::none()
    }

    pub(crate) fn handle_sync_mode_changed(&mut self, mode: SyncMode) -> Task<Message> {
        if let Some(sync) = self.sync.as_mut() {
            sync.mode = mode;
            clear_sync_plan(sync);
        }
        Task::none()
    }

    pub(crate) fn handle_sync_preset_changed(&mut self, preset: SyncPreset) -> Task<Message> {
        if let Some(sync) = self.sync.as_mut() {
            sync.preset = preset;
            if preset != SyncPreset::Custom {
                sync.policy = preset.policy();
            }
            clear_sync_plan(sync);
        }
        Task::none()
    }

    pub(crate) fn handle_sync_source_kind_changed(
        &mut self,
        kind: SyncEndpointKind,
    ) -> Task<Message> {
        if let Some(sync) = self.sync.as_mut() {
            sync.source_kind = kind;
            clear_sync_plan(sync);
        }
        Task::none()
    }

    pub(crate) fn handle_sync_destination_kind_changed(
        &mut self,
        kind: SyncEndpointKind,
    ) -> Task<Message> {
        if let Some(sync) = self.sync.as_mut() {
            sync.destination_kind = kind;
            clear_sync_plan(sync);
        }
        Task::none()
    }

    pub(crate) fn handle_sync_source_connection_changed(
        &mut self,
        connection_id: String,
    ) -> Task<Message> {
        if let Some(sync) = self.sync.as_mut() {
            sync.source_connection_id = connection_id.clone();
            sync.source_bucket.clear();
            sync.source_buckets = None;
            sync.loading_source_buckets = true;
            clear_sync_plan(sync);
        }
        self.cmd_fetch_sync_source_buckets(&connection_id)
    }

    pub(crate) fn handle_sync_destination_connection_changed(
        &mut self,
        connection_id: String,
    ) -> Task<Message> {
        if let Some(sync) = self.sync.as_mut() {
            sync.destination_connection_id = connection_id.clone();
            sync.destination_bucket.clear();
            sync.destination_buckets = None;
            sync.loading_destination_buckets = true;
            clear_sync_plan(sync);
        }
        self.cmd_fetch_sync_destination_buckets(&connection_id)
    }

    pub(crate) fn handle_sync_source_bucket_changed(&mut self, bucket: String) -> Task<Message> {
        if let Some(sync) = self.sync.as_mut() {
            sync.source_bucket = bucket;
            clear_sync_plan(sync);
        }
        Task::none()
    }

    pub(crate) fn handle_sync_destination_bucket_changed(
        &mut self,
        bucket: String,
    ) -> Task<Message> {
        if let Some(sync) = self.sync.as_mut() {
            sync.destination_bucket = bucket;
            clear_sync_plan(sync);
        }
        Task::none()
    }

    pub(crate) fn handle_sync_source_prefix_changed(&mut self, prefix: String) -> Task<Message> {
        if let Some(sync) = self.sync.as_mut() {
            sync.source_prefix = prefix;
            clear_sync_plan(sync);
        }
        Task::none()
    }

    pub(crate) fn handle_sync_destination_prefix_changed(
        &mut self,
        prefix: String,
    ) -> Task<Message> {
        if let Some(sync) = self.sync.as_mut() {
            sync.destination_prefix = prefix;
            clear_sync_plan(sync);
        }
        Task::none()
    }

    pub(crate) fn handle_pick_sync_source_local_path(&mut self) -> Task<Message> {
        let path = rfd::FileDialog::new().pick_folder();
        Task::perform(async move { path }, Message::SyncSourceLocalPathPicked)
    }

    pub(crate) fn handle_pick_sync_destination_local_path(&mut self) -> Task<Message> {
        let path = rfd::FileDialog::new().pick_folder();
        Task::perform(async move { path }, Message::SyncDestinationLocalPathPicked)
    }

    pub(crate) fn handle_sync_source_local_path_picked(
        &mut self,
        path: Option<PathBuf>,
    ) -> Task<Message> {
        if let Some(sync) = self.sync.as_mut() {
            sync.source_local_path = path;
            clear_sync_plan(sync);
        }
        Task::none()
    }

    pub(crate) fn handle_sync_destination_local_path_picked(
        &mut self,
        path: Option<PathBuf>,
    ) -> Task<Message> {
        if let Some(sync) = self.sync.as_mut() {
            sync.destination_local_path = path;
            clear_sync_plan(sync);
        }
        Task::none()
    }

    pub(crate) fn handle_sync_source_buckets_loaded(
        &mut self,
        result: Result<Vec<BucketInfo>, String>,
    ) -> Task<Message> {
        if let Some(sync) = self.sync.as_mut() {
            sync.loading_source_buckets = false;
            sync.source_buckets = Some(result);
        }
        Task::none()
    }

    pub(crate) fn handle_sync_destination_buckets_loaded(
        &mut self,
        result: Result<Vec<BucketInfo>, String>,
    ) -> Task<Message> {
        if let Some(sync) = self.sync.as_mut() {
            sync.loading_destination_buckets = false;
            sync.destination_buckets = Some(result);
        }
        Task::none()
    }

    pub(crate) fn handle_sync_compare_mode_changed(
        &mut self,
        mode: SyncCompareMode,
    ) -> Task<Message> {
        if let Some(sync) = self.sync.as_mut() {
            sync.tuning.compare_mode = mode;
            clear_sync_plan(sync);
        }
        Task::none()
    }

    pub(crate) fn handle_sync_list_mode_changed(&mut self, mode: SyncListMode) -> Task<Message> {
        if let Some(sync) = self.sync.as_mut() {
            sync.tuning.list_mode = mode;
            clear_sync_plan(sync);
        }
        Task::none()
    }

    pub(crate) fn handle_sync_list_workers_changed(&mut self, value: String) -> Task<Message> {
        if let Some(sync) = self.sync.as_mut() {
            sync.tuning.list_workers_text = value;
            clear_sync_plan(sync);
        }
        Task::none()
    }

    pub(crate) fn handle_sync_compare_workers_changed(&mut self, value: String) -> Task<Message> {
        if let Some(sync) = self.sync.as_mut() {
            sync.tuning.compare_workers_text = value;
            clear_sync_plan(sync);
        }
        Task::none()
    }

    pub(crate) fn handle_sync_fast_list_toggled(&mut self, enabled: bool) -> Task<Message> {
        if let Some(sync) = self.sync.as_mut() {
            sync.tuning.fast_list_enabled = enabled;
            clear_sync_plan(sync);
        }
        Task::none()
    }

    pub(crate) fn handle_sync_prefer_server_modtime_toggled(
        &mut self,
        enabled: bool,
    ) -> Task<Message> {
        if let Some(sync) = self.sync.as_mut() {
            sync.tuning.prefer_server_modtime = enabled;
            clear_sync_plan(sync);
        }
        Task::none()
    }

    pub(crate) fn handle_sync_max_planner_items_changed(&mut self, value: String) -> Task<Message> {
        if let Some(sync) = self.sync.as_mut() {
            sync.tuning.max_planner_items_text = value;
            clear_sync_plan(sync);
        }
        Task::none()
    }

    pub(crate) fn handle_sync_overwrite_changed(&mut self, enabled: bool) -> Task<Message> {
        if let Some(sync) = self.sync.as_mut() {
            sync.policy.overwrite_changed = enabled;
            sync.preset = SyncPreset::Custom;
            clear_sync_plan(sync);
        }
        Task::none()
    }

    pub(crate) fn handle_sync_delete_extras_changed(&mut self, enabled: bool) -> Task<Message> {
        if let Some(sync) = self.sync.as_mut() {
            sync.policy.delete_extras = enabled;
            sync.preset = SyncPreset::Custom;
            clear_sync_plan(sync);
        }
        Task::none()
    }

    pub(crate) fn handle_sync_destination_newer_policy_changed(
        &mut self,
        policy: SyncDestinationNewerPolicy,
    ) -> Task<Message> {
        if let Some(sync) = self.sync.as_mut() {
            sync.policy.destination_newer_policy = policy;
            sync.preset = SyncPreset::Custom;
            clear_sync_plan(sync);
        }
        Task::none()
    }

    pub(crate) fn handle_sync_delete_phase_changed(
        &mut self,
        phase: SyncDeletePhase,
    ) -> Task<Message> {
        if let Some(sync) = self.sync.as_mut() {
            sync.policy.delete_phase = phase;
            sync.preset = SyncPreset::Custom;
            clear_sync_plan(sync);
        }
        Task::none()
    }

    pub(crate) fn handle_sync_preview_before_run_changed(
        &mut self,
        enabled: bool,
    ) -> Task<Message> {
        if let Some(sync) = self.sync.as_mut() {
            sync.preview_before_run = enabled;
            clear_sync_plan(sync);
        }
        Task::none()
    }

    pub(crate) fn handle_sync_allow_direct_run_changed(&mut self, enabled: bool) -> Task<Message> {
        if let Some(sync) = self.sync.as_mut() {
            sync.allow_direct_run = enabled;
            clear_sync_plan(sync);
        }
        Task::none()
    }

    pub(crate) fn handle_sync_include_patterns_changed(&mut self, value: String) -> Task<Message> {
        if let Some(sync) = self.sync.as_mut() {
            sync.filters.include_patterns_text = value;
            clear_sync_plan(sync);
        }
        Task::none()
    }

    pub(crate) fn handle_sync_exclude_patterns_changed(&mut self, value: String) -> Task<Message> {
        if let Some(sync) = self.sync.as_mut() {
            sync.filters.exclude_patterns_text = value;
            clear_sync_plan(sync);
        }
        Task::none()
    }

    pub(crate) fn handle_sync_newer_than_changed(&mut self, value: String) -> Task<Message> {
        if let Some(sync) = self.sync.as_mut() {
            sync.filters.newer_than_text = value;
            clear_sync_plan(sync);
        }
        Task::none()
    }

    pub(crate) fn handle_sync_older_than_changed(&mut self, value: String) -> Task<Message> {
        if let Some(sync) = self.sync.as_mut() {
            sync.filters.older_than_text = value;
            clear_sync_plan(sync);
        }
        Task::none()
    }

    pub(crate) fn handle_sync_min_size_changed(&mut self, value: String) -> Task<Message> {
        if let Some(sync) = self.sync.as_mut() {
            sync.filters.min_size_text = value;
            clear_sync_plan(sync);
        }
        Task::none()
    }

    pub(crate) fn handle_sync_max_size_changed(&mut self, value: String) -> Task<Message> {
        if let Some(sync) = self.sync.as_mut() {
            sync.filters.max_size_text = value;
            clear_sync_plan(sync);
        }
        Task::none()
    }

    pub(crate) fn handle_toggle_sync_advanced(&mut self) -> Task<Message> {
        if let Some(sync) = self.sync.as_mut() {
            sync.show_advanced = !sync.show_advanced;
        }
        Task::none()
    }

    pub(crate) fn handle_start_sync_plan(&mut self) -> Task<Message> {
        let source_params = {
            let Some(sync) = self.sync.as_mut() else {
                return Task::none();
            };

            sync.error = None;
            sync.plan = None;
            sync.source_snapshot = None;
            sync.destination_snapshot = None;
            sync.running = true;
            sync.telemetry.stage = "Enumerating source".to_string();
            sync.telemetry.source_scanned = 0;
            sync.telemetry.destination_scanned = 0;
            sync.telemetry.compared = 0;
            sync.telemetry.filtered = 0;
            sync.telemetry.started_at = Some(now_rfc3339());
            sync.telemetry.last_update_at = sync.telemetry.started_at.clone();

            if let Err(error) = validate_sync_config(sync) {
                sync.running = false;
                sync.error = Some(error);
                sync.telemetry.stage = "Idle".to_string();
                return Task::none();
            }

            sync_endpoint_params(sync, true)
        };

        match enumerate_task(self, source_params, true) {
            Ok(task) => task,
            Err(error) => {
                if let Some(sync) = self.sync.as_mut() {
                    sync.running = false;
                    sync.error = Some(error);
                    sync.telemetry.stage = "Idle".to_string();
                }
                Task::none()
            }
        }
    }

    pub(crate) fn handle_sync_source_enumerated(
        &mut self,
        result: Result<Vec<SyncObject>, String>,
    ) -> Task<Message> {
        match result {
            Ok(objects) => {
                let destination_params = {
                    let Some(sync) = self.sync.as_mut() else {
                        return Task::none();
                    };
                    sync.telemetry.source_scanned = objects.len();
                    sync.telemetry.stage = "Enumerating destination".to_string();
                    sync.telemetry.last_update_at = Some(now_rfc3339());
                    sync.source_snapshot = Some(objects);
                    sync_endpoint_params(sync, false)
                };
                match enumerate_task(self, destination_params, false) {
                    Ok(task) => task,
                    Err(error) => {
                        if let Some(sync) = self.sync.as_mut() {
                            sync.running = false;
                            sync.error = Some(error);
                            sync.telemetry.stage = "Idle".to_string();
                        }
                        Task::none()
                    }
                }
            }
            Err(error) => {
                if let Some(sync) = self.sync.as_mut() {
                    sync.running = false;
                    sync.error = Some(error);
                    sync.telemetry.stage = "Idle".to_string();
                }
                Task::none()
            }
        }
    }

    pub(crate) fn handle_sync_destination_enumerated(
        &mut self,
        result: Result<Vec<SyncObject>, String>,
    ) -> Task<Message> {
        let Some(sync) = self.sync.as_mut() else {
            return Task::none();
        };

        match result {
            Ok(objects) => {
                sync.telemetry.destination_scanned = objects.len();
                sync.destination_snapshot = Some(objects);
                sync.telemetry.stage = "Comparing".to_string();
                sync.telemetry.last_update_at = Some(now_rfc3339());

                let source = sync.source_snapshot.clone().unwrap_or_default();
                let destination = sync.destination_snapshot.clone().unwrap_or_default();
                let planner_limit = match parse_planner_limit(&sync.tuning.max_planner_items_text) {
                    Ok(value) => value,
                    Err(error) => {
                        sync.running = false;
                        sync.telemetry.stage = "Idle".to_string();
                        sync.error = Some(error);
                        return Task::none();
                    }
                };
                if source.len().saturating_add(destination.len()) > planner_limit {
                    sync.running = false;
                    sync.telemetry.stage = "Idle".to_string();
                    sync.error = Some(format!(
                        "Planner limit exceeded: {} source + destination entries is above {} items.",
                        source.len().saturating_add(destination.len()),
                        planner_limit
                    ));
                    return Task::none();
                }

                let mode = sync.mode;
                let policy = sync.policy;
                let compare_mode = sync.tuning.compare_mode;
                Task::perform(
                    async move {
                        Ok(build_sync_plan(
                            source,
                            destination,
                            mode,
                            policy,
                            compare_mode,
                        ))
                    },
                    Message::SyncPlanBuilt,
                )
            }
            Err(error) => {
                sync.running = false;
                sync.error = Some(error);
                sync.telemetry.stage = "Idle".to_string();
                Task::none()
            }
        }
    }

    pub(crate) fn handle_sync_plan_built(
        &mut self,
        result: Result<SyncPlan, String>,
    ) -> Task<Message> {
        let Some(sync) = self.sync.as_mut() else {
            return Task::none();
        };

        sync.running = false;
        sync.telemetry.stage = "Idle".to_string();
        sync.telemetry.last_update_at = Some(now_rfc3339());

        match result {
            Ok(plan) => {
                sync.telemetry.compared = plan.items.len();
                sync.run_plan = if sync.mode == SyncMode::Copy {
                    match prepare_copy_run_plan(sync, &plan) {
                        Ok(items) => Some(items),
                        Err(error) => {
                            sync.error = Some(error);
                            None
                        }
                    }
                } else {
                    None
                };
                sync.plan = Some(plan);
            }
            Err(error) => sync.error = Some(error),
        }
        Task::none()
    }

    pub(crate) fn handle_start_sync_copy(&mut self) -> Task<Message> {
        let Some(sync) = self.sync.as_mut() else {
            return Task::none();
        };
        if sync.mode != SyncMode::Copy {
            return Task::none();
        }
        let Some(items) = sync.run_plan.clone() else {
            sync.error = Some("Build a copy plan before running copy.".to_string());
            return Task::none();
        };
        if items.is_empty() {
            sync.error = Some("The current copy plan has no actionable items.".to_string());
            return Task::none();
        }

        let total_bytes = items.iter().map(|item| item.bytes).sum();
        let has_client_relay = items.iter().any(|item| {
            matches!(
                item.strategy,
                crate::app::SyncExecutionStrategy::ClientRelay
            )
        });
        sync.execution = Some(SyncExecutionState {
            items: items.clone(),
            next_index: 0,
            completed: 0,
            skipped: 0,
            failed: 0,
            bytes_done: 0,
            total_bytes,
            current_item: None,
            current_strategy: None,
            running: true,
            summary: None,
            has_client_relay,
        });
        sync.telemetry.stage = "Copying".to_string();
        sync.telemetry.last_update_at = Some(now_rfc3339());
        self.cmd_process_next_sync_copy_step()
    }

    pub(crate) fn handle_sync_copy_step_finished(
        &mut self,
        result: Result<SyncRunItem, String>,
    ) -> Task<Message> {
        let Some(sync) = self.sync.as_mut() else {
            return Task::none();
        };
        let Some(execution) = sync.execution.as_mut() else {
            return Task::none();
        };

        match result {
            Ok(item) => {
                execution.completed += 1;
                execution.bytes_done += item.bytes;
                execution.next_index += 1;
                execution.current_item = Some(item.relative_path);
                execution.current_strategy = Some(item.strategy);
                sync.telemetry.last_update_at = Some(now_rfc3339());
                self.cmd_process_next_sync_copy_step()
            }
            Err(error) => {
                execution.failed += 1;
                execution.next_index += 1;
                execution.summary = Some(format!(
                    "Copy failed on item {} of {}.",
                    execution.next_index,
                    execution.items.len()
                ));
                self.error = Some(format!("Copy failed: {}", error));
                self.cmd_process_next_sync_copy_step()
            }
        }
    }

    pub(crate) fn cmd_fetch_sync_source_buckets(&self, connection_id: &str) -> Task<Message> {
        let connection_id = connection_id.to_string();
        match self.make_client_for_connection(&connection_id) {
            Ok(client) => Task::perform(
                async move { client.list_buckets().await },
                Message::SyncSourceBucketsLoaded,
            ),
            Err(error) => {
                Task::perform(async move { Err(error) }, Message::SyncSourceBucketsLoaded)
            }
        }
    }

    pub(crate) fn cmd_fetch_sync_destination_buckets(&self, connection_id: &str) -> Task<Message> {
        let connection_id = connection_id.to_string();
        match self.make_client_for_connection(&connection_id) {
            Ok(client) => Task::perform(
                async move { client.list_buckets().await },
                Message::SyncDestinationBucketsLoaded,
            ),
            Err(error) => Task::perform(
                async move { Err(error) },
                Message::SyncDestinationBucketsLoaded,
            ),
        }
    }

    pub(crate) fn cmd_process_next_sync_copy_step(&mut self) -> Task<Message> {
        let Some(sync) = self.sync.as_mut() else {
            return Task::none();
        };
        let Some(execution) = sync.execution.as_mut() else {
            return Task::none();
        };

        if execution.next_index >= execution.items.len() {
            execution.running = false;
            execution.current_strategy = None;
            execution.current_item = None;
            execution.summary = Some(format!(
                "Done. Copied: {}. Failed: {}. Bytes: {} / {}.",
                execution.completed, execution.failed, execution.bytes_done, execution.total_bytes
            ));
            sync.telemetry.stage = "Idle".to_string();
            sync.telemetry.last_update_at = Some(now_rfc3339());
            return Task::none();
        }

        let item = execution.items[execution.next_index].clone();
        execution.current_item = Some(item.relative_path.clone());
        execution.current_strategy = Some(item.strategy);
        execution.running = true;

        let source_client = match &item.source {
            TransferEndpoint::S3 { connection_id, .. } => {
                match self.make_client_for_connection(connection_id) {
                    Ok(client) => Some(client),
                    Err(error) => {
                        return Task::perform(
                            async move { Err(error) },
                            Message::SyncCopyStepFinished,
                        );
                    }
                }
            }
            TransferEndpoint::Local { .. } => None,
        };
        let destination_client = match &item.destination {
            TransferEndpoint::S3 { connection_id, .. } => {
                match self.make_client_for_connection(connection_id) {
                    Ok(client) => Some(client),
                    Err(error) => {
                        return Task::perform(
                            async move { Err(error) },
                            Message::SyncCopyStepFinished,
                        );
                    }
                }
            }
            TransferEndpoint::Local { .. } => None,
        };
        Task::perform(
            async move {
                execute_sync_run_item(source_client, destination_client, &item)
                    .await
                    .map(|_| item)
            },
            Message::SyncCopyStepFinished,
        )
    }
}

fn clear_sync_plan(sync: &mut SyncState) {
    sync.plan = None;
    sync.run_plan = None;
    sync.execution = None;
    sync.error = None;
    sync.source_snapshot = None;
    sync.destination_snapshot = None;
}

fn validate_sync_config(sync: &SyncState) -> Result<(), String> {
    if sync.source_kind == SyncEndpointKind::S3 && sync.source_bucket.trim().is_empty() {
        return Err("Source bucket is required.".to_string());
    }
    if sync.destination_kind == SyncEndpointKind::S3 && sync.destination_bucket.trim().is_empty() {
        return Err("Destination bucket is required.".to_string());
    }
    if sync.source_kind == SyncEndpointKind::Local && sync.source_local_path.is_none() {
        return Err("Source local path is required.".to_string());
    }
    if sync.destination_kind == SyncEndpointKind::Local && sync.destination_local_path.is_none() {
        return Err("Destination local path is required.".to_string());
    }
    if same_sync_endpoint(sync) {
        return Err("Source and destination must be different.".to_string());
    }
    parse_planner_limit(&sync.tuning.max_planner_items_text)?;
    Ok(())
}

fn same_sync_endpoint(sync: &SyncState) -> bool {
    match (sync.source_kind, sync.destination_kind) {
        (SyncEndpointKind::S3, SyncEndpointKind::S3) => {
            sync.source_connection_id == sync.destination_connection_id
                && sync.source_bucket == sync.destination_bucket
                && sync.source_prefix == sync.destination_prefix
        }
        (SyncEndpointKind::Local, SyncEndpointKind::Local) => {
            sync.source_local_path == sync.destination_local_path
        }
        _ => false,
    }
}

fn parse_planner_limit(text: &str) -> Result<usize, String> {
    let value = text
        .trim()
        .parse::<usize>()
        .map_err(|_| "Planner limit must be a positive integer.".to_string())?;
    if value == 0 {
        return Err("Planner limit must be greater than zero.".to_string());
    }
    Ok(value)
}

fn sync_endpoint_params(
    sync: &SyncState,
    source: bool,
) -> (
    SyncEndpointKind,
    Option<PathBuf>,
    String,
    String,
    String,
    crate::app::SyncFilterSet,
) {
    (
        if source {
            sync.source_kind
        } else {
            sync.destination_kind
        },
        if source {
            sync.source_local_path.clone()
        } else {
            sync.destination_local_path.clone()
        },
        if source {
            sync.source_connection_id.clone()
        } else {
            sync.destination_connection_id.clone()
        },
        if source {
            sync.source_bucket.clone()
        } else {
            sync.destination_bucket.clone()
        },
        if source {
            sync.source_prefix.clone()
        } else {
            sync.destination_prefix.clone()
        },
        sync.filters.clone(),
    )
}

fn enumerate_task(
    app: &App,
    params: (
        SyncEndpointKind,
        Option<PathBuf>,
        String,
        String,
        String,
        crate::app::SyncFilterSet,
    ),
    source: bool,
) -> Result<Task<Message>, String> {
    let (kind, local_path, connection_id, bucket, prefix, filters) = params;
    match kind {
        SyncEndpointKind::Local => {
            let path = local_path.ok_or_else(|| "Local path is required.".to_string())?;
            Ok(Task::perform(
                async move { enumerate_local_for_sync(path, &filters) },
                if source {
                    Message::SyncSourceEnumerated
                } else {
                    Message::SyncDestinationEnumerated
                },
            ))
        }
        SyncEndpointKind::S3 => {
            let client = app.make_client_for_connection(&connection_id)?;
            Ok(Task::perform(
                async move { enumerate_s3_for_sync(client, &bucket, &prefix, &filters).await },
                if source {
                    Message::SyncSourceEnumerated
                } else {
                    Message::SyncDestinationEnumerated
                },
            ))
        }
    }
}
