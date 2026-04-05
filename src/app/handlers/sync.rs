use std::path::{Path, PathBuf};

use iced::Task;

use crate::app::sync_ops::{
    build_sync_plan, enumerate_local_for_sync, enumerate_s3_for_sync, prepare_sync_run_plan,
};
use crate::app::transfer_ops::{execute_sync_run_item, now_rfc3339};
use crate::app::{
    App, Message, SyncCompareMode, SyncDeleteBatchResult, SyncDeleteConfirmState, SyncDeletePhase,
    SyncDestinationNewerPolicy, SyncEndpointKind, SyncExecutionPhase, SyncExecutionState,
    SyncListMode, SyncMode, SyncObject, SyncPlan, SyncPreset, SyncRunItem, SyncRunPlan, SyncState,
    TransferEndpoint,
};
use crate::s3::client::BucketInfo;

const DELETE_CONFIRM_COUNT_THRESHOLD: usize = 1000;
const DELETE_CONFIRM_BYTES_THRESHOLD: u64 = 10 * 1024 * 1024 * 1024;

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

    pub(crate) fn handle_sync_ignore_errors_changed(&mut self, enabled: bool) -> Task<Message> {
        if let Some(sync) = self.sync.as_mut() {
            sync.delete_guardrails.ignore_errors = enabled;
            clear_sync_plan(sync);
        }
        Task::none()
    }

    pub(crate) fn handle_sync_delete_workers_changed(&mut self, value: String) -> Task<Message> {
        if let Some(sync) = self.sync.as_mut() {
            sync.delete_guardrails.delete_workers_text = value;
            clear_sync_plan(sync);
        }
        Task::none()
    }

    pub(crate) fn handle_sync_max_delete_count_changed(&mut self, value: String) -> Task<Message> {
        if let Some(sync) = self.sync.as_mut() {
            sync.delete_guardrails.max_delete_count_text = value;
            clear_sync_plan(sync);
        }
        Task::none()
    }

    pub(crate) fn handle_sync_max_delete_bytes_changed(&mut self, value: String) -> Task<Message> {
        if let Some(sync) = self.sync.as_mut() {
            sync.delete_guardrails.max_delete_bytes_text = value;
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
            sync.run_plan = None;
            sync.delete_confirm = None;
            sync.source_snapshot = None;
            sync.destination_snapshot = None;
            sync.execution = None;
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
                sync.run_plan = match sync.mode {
                    SyncMode::Diff => None,
                    SyncMode::Copy | SyncMode::Sync => match prepare_sync_run_plan(sync, &plan) {
                        Ok(run_plan) => Some(run_plan),
                        Err(error) => {
                            sync.error = Some(error);
                            None
                        }
                    },
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
        let Some(run_plan) = sync.run_plan.clone() else {
            sync.error = Some("Build a copy plan before running copy.".to_string());
            return Task::none();
        };
        if run_plan.transfers.is_empty() {
            sync.error = Some("The current copy plan has no actionable items.".to_string());
            return Task::none();
        }

        start_copy_execution(sync);
        self.dispatch_sync_execution()
    }

    pub(crate) fn handle_start_sync(&mut self) -> Task<Message> {
        let Some(sync) = self.sync.as_mut() else {
            return Task::none();
        };
        if sync.mode != SyncMode::Sync {
            return Task::none();
        }
        let Some(plan) = sync.plan.as_ref() else {
            sync.error = Some("Build a sync plan before running sync.".to_string());
            return Task::none();
        };
        if plan.summary.conflicts > 0 {
            sync.error =
                Some("Resolve sync conflicts in the plan before running sync.".to_string());
            return Task::none();
        }
        let Some(run_plan) = sync.run_plan.as_ref() else {
            sync.error = Some("Build a sync plan before running sync.".to_string());
            return Task::none();
        };
        if run_plan.transfers.is_empty() && run_plan.deletes.is_empty() {
            sync.error = Some("The current sync plan has no actionable items.".to_string());
            return Task::none();
        }
        if let Err(error) = validate_sync_guardrails(sync, run_plan) {
            sync.error = Some(error);
            return Task::none();
        }

        sync.error = None;
        sync.delete_confirm = None;
        if run_plan.deletes.is_empty() {
            start_sync_execution(sync);
            return self.dispatch_sync_execution();
        }

        sync.delete_confirm = Some(build_delete_confirm_state(sync, run_plan));
        Task::none()
    }

    pub(crate) fn handle_cancel_sync_delete_confirm(&mut self) -> Task<Message> {
        if let Some(sync) = self.sync.as_mut() {
            sync.delete_confirm = None;
        }
        Task::none()
    }

    pub(crate) fn handle_sync_delete_confirm_text_changed(
        &mut self,
        value: String,
    ) -> Task<Message> {
        if let Some(sync) = self.sync.as_mut()
            && let Some(confirm) = sync.delete_confirm.as_mut()
        {
            confirm.confirm_text = value;
        }
        Task::none()
    }

    pub(crate) fn handle_confirm_sync_delete_run(&mut self) -> Task<Message> {
        let Some(sync) = self.sync.as_mut() else {
            return Task::none();
        };
        let Some(confirm) = sync.delete_confirm.as_ref() else {
            return Task::none();
        };
        if confirm.typed_required {
            let expected = format!("delete {}", confirm.planned_deletes);
            if confirm.confirm_text.trim() != expected {
                sync.error = Some(format!(
                    "Type '{}' to confirm this destructive sync.",
                    expected
                ));
                return Task::none();
            }
        }

        sync.delete_confirm = None;
        sync.error = None;
        start_sync_execution(sync);
        self.dispatch_sync_execution()
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

        execution.active_transfer = false;
        match result {
            Ok(item) => {
                execution.completed_transfers += 1;
                execution.bytes_done += item.bytes;
                execution.next_transfer_index += 1;
                execution.current_item = Some(item.relative_path);
                execution.current_strategy = Some(item.strategy);
                sync.telemetry.last_update_at = Some(now_rfc3339());
            }
            Err(error) => {
                execution.failed_transfers += 1;
                execution.transfer_failed = true;
                execution.next_transfer_index += 1;
                self.error = Some(format!("Sync transfer failed: {}", error));
                sync.telemetry.last_update_at = Some(now_rfc3339());
                if !sync.delete_guardrails.ignore_errors
                    && matches!(execution.phase, SyncExecutionPhase::DeletingDuring)
                {
                    execution.phase = SyncExecutionPhase::Stopped;
                }
            }
        }
        self.dispatch_sync_execution()
    }

    pub(crate) fn handle_sync_delete_batch_finished(
        &mut self,
        result: Result<SyncDeleteBatchResult, String>,
    ) -> Task<Message> {
        let Some(sync) = self.sync.as_mut() else {
            return Task::none();
        };
        let Some(execution) = sync.execution.as_mut() else {
            return Task::none();
        };

        execution.active_delete_batches = execution.active_delete_batches.saturating_sub(1);
        match result {
            Ok(batch) => {
                execution.completed_deletes += batch.completed;
                execution.failed_deletes += batch.failed;
                execution.bytes_done += batch.bytes;
                execution.current_item = Some(batch.label);
                sync.telemetry.last_update_at = Some(now_rfc3339());
            }
            Err(error) => {
                execution.failed_deletes += 1;
                self.error = Some(format!("Sync delete failed: {}", error));
                sync.telemetry.last_update_at = Some(now_rfc3339());
                if !sync.delete_guardrails.ignore_errors {
                    execution.phase = SyncExecutionPhase::Stopped;
                }
            }
        }

        self.dispatch_sync_execution()
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

    fn dispatch_sync_execution(&mut self) -> Task<Message> {
        let mut pending_specs = Vec::new();

        {
            let Some(sync) = self.sync.as_mut() else {
                return Task::none();
            };
            let Some(execution) = sync.execution.as_mut() else {
                return Task::none();
            };

            let delete_workers =
                parse_delete_workers(&sync.delete_guardrails.delete_workers_text).unwrap_or(4);

            loop {
                match execution.phase {
                    SyncExecutionPhase::DeletingBefore => {
                        if execution.active_delete_batches == 0
                            && execution.next_delete_index >= execution.run_plan.deletes.len()
                        {
                            execution.phase = SyncExecutionPhase::Copying;
                            continue;
                        }
                        break;
                    }
                    SyncExecutionPhase::Copying => {
                        if execution.active_transfer
                            || execution.next_transfer_index < execution.run_plan.transfers.len()
                        {
                            break;
                        }
                        if sync.mode == SyncMode::Copy {
                            execution.phase = SyncExecutionPhase::Done;
                            continue;
                        }
                        if execution.run_plan.deletes.is_empty() {
                            execution.phase = SyncExecutionPhase::Done;
                            continue;
                        }
                        if execution.transfer_failed && !sync.delete_guardrails.ignore_errors {
                            execution.delete_phase_skipped = true;
                            execution.phase = SyncExecutionPhase::Done;
                            continue;
                        }
                        execution.phase = match sync.policy.delete_phase {
                            SyncDeletePhase::After => SyncExecutionPhase::DeletingAfter,
                            SyncDeletePhase::During => SyncExecutionPhase::DeletingDuring,
                            SyncDeletePhase::Before => SyncExecutionPhase::Done,
                        };
                        continue;
                    }
                    SyncExecutionPhase::DeletingDuring => {
                        let transfers_done = !execution.active_transfer
                            && execution.next_transfer_index >= execution.run_plan.transfers.len();
                        let deletes_done = execution.active_delete_batches == 0
                            && execution.next_delete_index >= execution.run_plan.deletes.len();
                        if transfers_done && deletes_done {
                            execution.phase = SyncExecutionPhase::Done;
                            continue;
                        }
                        break;
                    }
                    SyncExecutionPhase::DeletingAfter => {
                        if execution.active_delete_batches == 0
                            && execution.next_delete_index >= execution.run_plan.deletes.len()
                        {
                            execution.phase = SyncExecutionPhase::Done;
                            continue;
                        }
                        break;
                    }
                    SyncExecutionPhase::Done | SyncExecutionPhase::Stopped => {
                        execution.running = false;
                        execution.current_strategy = None;
                        execution.current_item = None;
                        execution.summary = Some(final_sync_summary(execution));
                        sync.telemetry.stage = "Idle".to_string();
                        sync.telemetry.last_update_at = Some(now_rfc3339());
                        return Task::none();
                    }
                }
            }

            match execution.phase {
                SyncExecutionPhase::DeletingBefore => {
                    sync.telemetry.stage = "Deleting before copy".to_string();
                    while execution.active_delete_batches < delete_workers {
                        let Some(spec) = next_delete_batch_spec(execution) else {
                            break;
                        };
                        execution.active_delete_batches += 1;
                        pending_specs.push(spec);
                    }
                }
                SyncExecutionPhase::Copying => {
                    sync.telemetry.stage = if sync.mode == SyncMode::Copy {
                        "Copying".to_string()
                    } else {
                        "Syncing".to_string()
                    };
                    if !execution.active_transfer
                        && execution.next_transfer_index < execution.run_plan.transfers.len()
                        && let Some(spec) = next_transfer_spec(execution)
                    {
                        execution.active_transfer = true;
                        pending_specs.push(spec);
                    }
                }
                SyncExecutionPhase::DeletingDuring => {
                    sync.telemetry.stage = "Syncing with deletes".to_string();
                    if !execution.active_transfer
                        && execution.next_transfer_index < execution.run_plan.transfers.len()
                        && let Some(spec) = next_transfer_spec(execution)
                    {
                        execution.active_transfer = true;
                        pending_specs.push(spec);
                    }
                    while execution.active_delete_batches < delete_workers {
                        let Some(spec) = next_delete_batch_spec(execution) else {
                            break;
                        };
                        execution.active_delete_batches += 1;
                        pending_specs.push(spec);
                    }
                }
                SyncExecutionPhase::DeletingAfter => {
                    sync.telemetry.stage = "Deleting extras".to_string();
                    while execution.active_delete_batches < delete_workers {
                        let Some(spec) = next_delete_batch_spec(execution) else {
                            break;
                        };
                        execution.active_delete_batches += 1;
                        pending_specs.push(spec);
                    }
                }
                SyncExecutionPhase::Done | SyncExecutionPhase::Stopped => {}
            }
        }

        if pending_specs.is_empty() {
            if let Some(sync) = self.sync.as_mut()
                && let Some(execution) = sync.execution.as_mut()
            {
                execution.running = false;
                execution.summary = Some(final_sync_summary(execution));
                sync.telemetry.stage = "Idle".to_string();
                sync.telemetry.last_update_at = Some(now_rfc3339());
            }
            Task::none()
        } else {
            let tasks: Vec<_> = pending_specs
                .into_iter()
                .map(|spec| spec.into_task(self))
                .collect();
            Task::batch(tasks)
        }
    }
}

fn clear_sync_plan(sync: &mut SyncState) {
    sync.plan = None;
    sync.run_plan = None;
    sync.execution = None;
    sync.delete_confirm = None;
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
    parse_delete_workers(&sync.delete_guardrails.delete_workers_text)?;
    parse_optional_usize(
        &sync.delete_guardrails.max_delete_count_text,
        "Max delete count",
    )?;
    parse_optional_u64(
        &sync.delete_guardrails.max_delete_bytes_text,
        "Max delete bytes",
    )?;
    Ok(())
}

fn validate_sync_guardrails(sync: &SyncState, run_plan: &SyncRunPlan) -> Result<(), String> {
    if let Some(limit) = parse_optional_usize(
        &sync.delete_guardrails.max_delete_count_text,
        "Max delete count",
    )? && run_plan.deletes.len() > limit
    {
        return Err(format!(
            "Delete count guardrail exceeded: {} planned deletes is above {}.",
            run_plan.deletes.len(),
            limit
        ));
    }
    if let Some(limit) = parse_optional_u64(
        &sync.delete_guardrails.max_delete_bytes_text,
        "Max delete bytes",
    )? && run_plan.total_delete_bytes > limit
    {
        return Err(format!(
            "Delete byte guardrail exceeded: {} planned delete bytes is above {}.",
            run_plan.total_delete_bytes, limit
        ));
    }
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

fn parse_delete_workers(text: &str) -> Result<usize, String> {
    let value = text
        .trim()
        .parse::<usize>()
        .map_err(|_| "Delete workers must be a positive integer.".to_string())?;
    if value == 0 {
        return Err("Delete workers must be greater than zero.".to_string());
    }
    Ok(value)
}

fn parse_optional_usize(text: &str, label: &str) -> Result<Option<usize>, String> {
    if text.trim().is_empty() {
        return Ok(None);
    }
    let value = text
        .trim()
        .parse::<usize>()
        .map_err(|_| format!("{} must be a positive integer.", label))?;
    if value == 0 {
        return Err(format!("{} must be greater than zero.", label));
    }
    Ok(Some(value))
}

fn parse_optional_u64(text: &str, label: &str) -> Result<Option<u64>, String> {
    if text.trim().is_empty() {
        return Ok(None);
    }
    let value = text
        .trim()
        .parse::<u64>()
        .map_err(|_| format!("{} must be a positive integer.", label))?;
    if value == 0 {
        return Err(format!("{} must be greater than zero.", label));
    }
    Ok(Some(value))
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

fn start_copy_execution(sync: &mut SyncState) {
    let Some(run_plan) = sync.run_plan.clone() else {
        return;
    };
    sync.execution = Some(SyncExecutionState {
        phase: SyncExecutionPhase::Copying,
        total_bytes: run_plan.total_transfer_bytes,
        has_client_relay: run_plan.has_client_relay,
        run_plan,
        next_transfer_index: 0,
        next_delete_index: 0,
        active_transfer: false,
        active_delete_batches: 0,
        completed_transfers: 0,
        completed_deletes: 0,
        failed_transfers: 0,
        failed_deletes: 0,
        bytes_done: 0,
        current_item: None,
        current_strategy: None,
        running: true,
        summary: None,
        delete_phase_skipped: false,
        transfer_failed: false,
    });
    sync.telemetry.stage = "Copying".to_string();
    sync.telemetry.last_update_at = Some(now_rfc3339());
}

fn start_sync_execution(sync: &mut SyncState) {
    let Some(run_plan) = sync.run_plan.clone() else {
        return;
    };
    let phase = match sync.policy.delete_phase {
        SyncDeletePhase::Before if !run_plan.deletes.is_empty() => {
            SyncExecutionPhase::DeletingBefore
        }
        SyncDeletePhase::During if !run_plan.deletes.is_empty() => {
            SyncExecutionPhase::DeletingDuring
        }
        _ => SyncExecutionPhase::Copying,
    };
    sync.execution = Some(SyncExecutionState {
        phase,
        total_bytes: run_plan.total_transfer_bytes + run_plan.total_delete_bytes,
        has_client_relay: run_plan.has_client_relay,
        run_plan,
        next_transfer_index: 0,
        next_delete_index: 0,
        active_transfer: false,
        active_delete_batches: 0,
        completed_transfers: 0,
        completed_deletes: 0,
        failed_transfers: 0,
        failed_deletes: 0,
        bytes_done: 0,
        current_item: None,
        current_strategy: None,
        running: true,
        summary: None,
        delete_phase_skipped: false,
        transfer_failed: false,
    });
    sync.telemetry.stage = "Syncing".to_string();
    sync.telemetry.last_update_at = Some(now_rfc3339());
}

fn build_delete_confirm_state(sync: &SyncState, run_plan: &SyncRunPlan) -> SyncDeleteConfirmState {
    let threshold_reason = delete_confirm_reason(
        run_plan.deletes.len(),
        run_plan.total_delete_bytes,
        sync.telemetry.destination_scanned,
        sync.telemetry.source_scanned,
    );
    SyncDeleteConfirmState {
        planned_deletes: run_plan.deletes.len(),
        planned_delete_bytes: run_plan.total_delete_bytes,
        typed_required: threshold_reason.is_some(),
        threshold_reason,
        confirm_text: String::new(),
    }
}

fn delete_confirm_reason(
    planned_deletes: usize,
    planned_delete_bytes: u64,
    destination_scanned: usize,
    source_scanned: usize,
) -> Option<String> {
    if planned_deletes > DELETE_CONFIRM_COUNT_THRESHOLD {
        return Some(format!(
            "Delete count exceeds {} objects.",
            DELETE_CONFIRM_COUNT_THRESHOLD
        ));
    }
    if planned_delete_bytes > DELETE_CONFIRM_BYTES_THRESHOLD {
        return Some("Delete bytes exceed 10 GiB.".to_string());
    }
    if destination_scanned > 0 && planned_deletes.saturating_mul(4) > destination_scanned {
        return Some("Deletes are more than 25% of the scanned destination.".to_string());
    }
    if source_scanned == 0 && planned_deletes > 0 {
        return Some("The source scan is empty while deletes are planned.".to_string());
    }
    None
}

fn final_sync_summary(execution: &SyncExecutionState) -> String {
    let mut summary = format!(
        "Done. Copied: {}. Deleted: {}. Transfer failures: {}. Delete failures: {}. Bytes: {} / {}.",
        execution.completed_transfers,
        execution.completed_deletes,
        execution.failed_transfers,
        execution.failed_deletes,
        execution.bytes_done,
        execution.total_bytes
    );
    if execution.delete_phase_skipped {
        summary.push_str(" Delete phase was skipped because earlier transfers failed.");
    }
    summary
}

enum PendingSyncTask {
    Transfer(SyncRunItem),
    DeleteRemote {
        connection_id: String,
        bucket: String,
        keys: Vec<String>,
        bytes: u64,
        label: String,
    },
    DeleteLocal {
        path: PathBuf,
        bytes: u64,
        label: String,
    },
}

impl PendingSyncTask {
    fn into_task(self, app: &App) -> Task<Message> {
        match self {
            PendingSyncTask::Transfer(item) => {
                let source_client = match &item.source {
                    TransferEndpoint::S3 { connection_id, .. } => {
                        match app.make_client_for_connection(connection_id) {
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
                        match app.make_client_for_connection(connection_id) {
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
            PendingSyncTask::DeleteRemote {
                connection_id,
                bucket,
                keys,
                bytes,
                label,
            } => match app.make_client_for_connection(&connection_id) {
                Ok(client) => Task::perform(
                    async move {
                        let failed = client.delete_objects(&bucket, &keys).await?;
                        let completed = keys.len().saturating_sub(failed.len());
                        if !failed.is_empty() {
                            return Err(format!(
                                "failed to delete {} object(s) from {}",
                                failed.len(),
                                bucket
                            ));
                        }
                        Ok(SyncDeleteBatchResult {
                            completed,
                            failed: 0,
                            bytes,
                            label,
                        })
                    },
                    Message::SyncDeleteBatchFinished,
                ),
                Err(error) => {
                    Task::perform(async move { Err(error) }, Message::SyncDeleteBatchFinished)
                }
            },
            PendingSyncTask::DeleteLocal { path, bytes, label } => Task::perform(
                async move {
                    delete_local_sync_path(&path).await?;
                    Ok(SyncDeleteBatchResult {
                        completed: 1,
                        failed: 0,
                        bytes,
                        label,
                    })
                },
                Message::SyncDeleteBatchFinished,
            ),
        }
    }
}

fn next_transfer_spec(execution: &SyncExecutionState) -> Option<PendingSyncTask> {
    let item = execution
        .run_plan
        .transfers
        .get(execution.next_transfer_index)?
        .clone();
    Some(PendingSyncTask::Transfer(item))
}

fn next_delete_batch_spec(execution: &mut SyncExecutionState) -> Option<PendingSyncTask> {
    let first = execution
        .run_plan
        .deletes
        .get(execution.next_delete_index)?
        .clone();
    match &first.destination {
        TransferEndpoint::S3 {
            connection_id,
            bucket,
            ..
        } => {
            let start = execution.next_delete_index;
            let mut end = start;
            let mut keys = Vec::new();
            let mut bytes = 0_u64;
            while end < execution.run_plan.deletes.len() && keys.len() < 1000 {
                let item = &execution.run_plan.deletes[end];
                match &item.destination {
                    TransferEndpoint::S3 {
                        connection_id: item_connection,
                        bucket: item_bucket,
                        key,
                    } if item_connection == connection_id && item_bucket == bucket => {
                        keys.push(key.clone());
                        bytes += item.bytes;
                        end += 1;
                    }
                    _ => break,
                }
            }
            execution.next_delete_index = end;
            let label = format!("{}/{}", bucket, keys.last().cloned().unwrap_or_default());
            Some(PendingSyncTask::DeleteRemote {
                connection_id: connection_id.clone(),
                bucket: bucket.clone(),
                keys,
                bytes,
                label,
            })
        }
        TransferEndpoint::Local { path } => {
            execution.next_delete_index += 1;
            Some(PendingSyncTask::DeleteLocal {
                path: path.clone(),
                bytes: first.bytes,
                label: first.relative_path.clone(),
            })
        }
    }
}

async fn delete_local_sync_path(path: &Path) -> Result<(), String> {
    tokio::fs::remove_file(path)
        .await
        .map_err(|e| e.to_string())?;
    prune_empty_sync_dirs(path).await;
    Ok(())
}

async fn prune_empty_sync_dirs(path: &Path) {
    let mut current = path.parent();
    while let Some(dir) = current {
        let mut entries = match tokio::fs::read_dir(dir).await {
            Ok(entries) => entries,
            Err(_) => break,
        };
        match entries.next_entry().await {
            Ok(None) => {
                if tokio::fs::remove_dir(dir).await.is_err() {
                    break;
                }
                current = dir.parent();
            }
            Ok(Some(_)) | Err(_) => break,
        }
    }
}
