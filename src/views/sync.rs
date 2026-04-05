use iced::widget::{
    button, checkbox, column, container, pick_list, row, scrollable, text, text_input,
};
use iced::{Element, Length};

use crate::app::{
    App, CURRENT_CONNECTION_ID, Message, SyncCompareMode, SyncDeletePhase,
    SyncDestinationNewerPolicy, SyncEndpointKind, SyncExecutionStrategy, SyncListMode, SyncMode,
    SyncPreset,
};

impl App {
    pub fn sync_view(&self) -> Element<'_, Message> {
        let Some(sync) = &self.sync else {
            return container(
                column![
                    text("Sync").size(16),
                    text("Open the sync workflow to build preview plans for Diff, Copy, or Sync.")
                        .size(11),
                    button(text("Open Sync").size(11)).on_press(Message::OpenSync),
                ]
                .spacing(8)
                .padding(12),
            )
            .width(Length::Fill)
            .height(Length::Fill)
            .into();
        };

        let endpoint_options = vec![SyncEndpointKind::S3, SyncEndpointKind::Local];
        let mode_options = vec![SyncMode::Diff, SyncMode::Copy, SyncMode::Sync];
        let preset_options = vec![
            SyncPreset::Converge,
            SyncPreset::UpdateOnly,
            SyncPreset::Exact,
            SyncPreset::Custom,
        ];
        let compare_mode_options = vec![
            SyncCompareMode::SizeOnly,
            SyncCompareMode::SizeAndModTime,
            SyncCompareMode::UpdateIfSourceNewer,
            SyncCompareMode::ChecksumIfAvailable,
            SyncCompareMode::AlwaysOverwrite,
        ];
        let list_mode_options = vec![
            SyncListMode::Streaming,
            SyncListMode::FastList,
            SyncListMode::TopUp,
        ];
        let destination_newer_options = vec![
            SyncDestinationNewerPolicy::SourceWins,
            SyncDestinationNewerPolicy::Skip,
            SyncDestinationNewerPolicy::Conflict,
        ];
        let delete_phase_options = vec![
            SyncDeletePhase::Before,
            SyncDeletePhase::During,
            SyncDeletePhase::After,
        ];

        let mut content = column![text("Sync").size(16)].spacing(8).padding(12);

        content = content.push(text("Source").size(12));
        content = content.push(
            row![
                text("Kind").size(11).width(80),
                pick_list(
                    endpoint_options.clone(),
                    Some(sync.source_kind),
                    Message::SyncSourceKindChanged
                ),
            ]
            .spacing(6),
        );
        if sync.source_kind == SyncEndpointKind::S3 {
            content = content.push(sync_s3_endpoint_block(
                true,
                selected_connection_label(self, &sync.source_connection_id),
                &sync.source_bucket,
                &sync.source_prefix,
                sync.source_buckets
                    .as_ref()
                    .and_then(|result| result.as_ref().ok())
                    .cloned()
                    .unwrap_or_default()
                    .into_iter()
                    .map(|bucket| bucket.name)
                    .collect(),
                sync.loading_source_buckets,
                self.available_connection_options(),
            ));
        } else {
            content = content.push(sync_local_endpoint_block(
                sync.source_local_path
                    .as_ref()
                    .map(|path| path.display().to_string())
                    .unwrap_or_default(),
                Message::PickSyncSourceLocalPath,
            ));
        }

        content = content.push(text("Destination").size(12));
        content = content.push(
            row![
                text("Kind").size(11).width(80),
                pick_list(
                    endpoint_options,
                    Some(sync.destination_kind),
                    Message::SyncDestinationKindChanged
                ),
            ]
            .spacing(6),
        );
        if sync.destination_kind == SyncEndpointKind::S3 {
            content = content.push(sync_s3_endpoint_block(
                false,
                selected_connection_label(self, &sync.destination_connection_id),
                &sync.destination_bucket,
                &sync.destination_prefix,
                sync.destination_buckets
                    .as_ref()
                    .and_then(|result| result.as_ref().ok())
                    .cloned()
                    .unwrap_or_default()
                    .into_iter()
                    .map(|bucket| bucket.name)
                    .collect(),
                sync.loading_destination_buckets,
                self.available_connection_options(),
            ));
        } else {
            content = content.push(sync_local_endpoint_block(
                sync.destination_local_path
                    .as_ref()
                    .map(|path| path.display().to_string())
                    .unwrap_or_default(),
                Message::PickSyncDestinationLocalPath,
            ));
        }

        content = content.push(iced::widget::rule::horizontal(1));
        content = content.push(text("Workflow").size(12));
        content = content.push(
            row![
                text("Mode").size(11).width(80),
                pick_list(mode_options, Some(sync.mode), Message::SyncModeChanged),
            ]
            .spacing(6),
        );
        content = content.push(text(mode_summary(sync.mode)).size(11));

        if sync.mode != SyncMode::Copy {
            content = content.push(
                row![
                    text("Preset").size(11).width(80),
                    pick_list(
                        preset_options,
                        Some(sync.preset),
                        Message::SyncPresetChanged
                    ),
                ]
                .spacing(6),
            );
            content = content.push(text(sync.preset.description()).size(11));
        } else {
            content = content.push(
                text("Copy stays non-destructive: create/update only, never delete extras.")
                    .size(11),
            );
        }

        content = content.push(
            row![
                text("Compare").size(11).width(80),
                pick_list(
                    compare_mode_options,
                    Some(sync.tuning.compare_mode),
                    Message::SyncCompareModeChanged
                ),
            ]
            .spacing(6),
        );
        content = content.push(
            row![
                text("List Mode").size(11).width(80),
                pick_list(
                    list_mode_options,
                    Some(sync.tuning.list_mode),
                    Message::SyncListModeChanged
                ),
            ]
            .spacing(6),
        );
        content = content.push(
            button(
                text(if sync.show_advanced {
                    "Hide Advanced"
                } else {
                    "Show Advanced"
                })
                .size(11),
            )
            .on_press(Message::ToggleSyncAdvanced),
        );

        if sync.show_advanced {
            let mut advanced = column![
                row![
                    text("List Workers").size(11).width(130),
                    text_input("8", &sync.tuning.list_workers_text)
                        .on_input(Message::SyncListWorkersChanged),
                ]
                .spacing(6),
                row![
                    text("Compare Workers").size(11).width(130),
                    text_input("8", &sync.tuning.compare_workers_text)
                        .on_input(Message::SyncCompareWorkersChanged),
                ]
                .spacing(6),
                row![
                    text("Transfer Workers").size(11).width(130),
                    text_input("4", &sync.tuning.transfer_workers_text)
                        .on_input(Message::SyncTransferWorkersChanged),
                ]
                .spacing(6),
                row![
                    text("Planner Limit").size(11).width(130),
                    text_input("250000", &sync.tuning.max_planner_items_text)
                        .on_input(Message::SyncMaxPlannerItemsChanged),
                ]
                .spacing(6),
                row![
                    text("Bandwidth Limit").size(11).width(130),
                    text_input("e.g. 10M/s", &sync.tuning.bwlimit_text)
                        .on_input(Message::SyncBwlimitChanged),
                ]
                .spacing(6),
                row![
                    text("Multipart Cutoff").size(11).width(130),
                    text_input("8M", &sync.tuning.multipart_cutoff_text)
                        .on_input(Message::SyncMultipartCutoffChanged),
                ]
                .spacing(6),
                row![
                    text("Multipart Chunk").size(11).width(130),
                    text_input("8M", &sync.tuning.multipart_chunk_size_text)
                        .on_input(Message::SyncMultipartChunkSizeChanged),
                ]
                .spacing(6),
                checkbox(sync.tuning.fast_list_enabled)
                    .label("Fast list enabled")
                    .on_toggle(Message::SyncFastListToggled),
                checkbox(sync.tuning.prefer_server_modtime)
                    .label("Prefer server modtime")
                    .on_toggle(Message::SyncPreferServerModtimeToggled),
            ]
            .spacing(6);

            if sync.mode != SyncMode::Copy {
                advanced = advanced.push(text("Sync Policy").size(11));
                advanced = advanced.push(
                    checkbox(sync.policy.overwrite_changed)
                        .label("Overwrite changed destination objects")
                        .on_toggle(Message::SyncOverwriteChanged),
                );
                advanced = advanced.push(
                    checkbox(sync.policy.delete_extras)
                        .label("Delete destination-only objects")
                        .on_toggle(Message::SyncDeleteExtrasChanged),
                );
                advanced = advanced.push(
                    row![
                        text("Dest Newer").size(11).width(130),
                        pick_list(
                            destination_newer_options,
                            Some(sync.policy.destination_newer_policy),
                            Message::SyncDestinationNewerPolicyChanged
                        ),
                    ]
                    .spacing(6),
                );
                advanced = advanced.push(
                    row![
                        text("Delete Phase").size(11).width(130),
                        pick_list(
                            delete_phase_options,
                            Some(sync.policy.delete_phase),
                            Message::SyncDeletePhaseChanged
                        ),
                    ]
                    .spacing(6),
                );
                advanced = advanced.push(
                    checkbox(sync.delete_guardrails.ignore_errors)
                        .label("Ignore transfer/delete errors")
                        .on_toggle(Message::SyncIgnoreErrorsChanged),
                );
                advanced = advanced.push(text("Delete Guardrails").size(11));
                advanced = advanced.push(
                    row![
                        text("Delete Workers").size(11).width(130),
                        text_input("4", &sync.delete_guardrails.delete_workers_text)
                            .on_input(Message::SyncDeleteWorkersChanged),
                    ]
                    .spacing(6),
                );
                advanced = advanced.push(
                    row![
                        text("Max Deletes").size(11).width(130),
                        text_input("off", &sync.delete_guardrails.max_delete_count_text)
                            .on_input(Message::SyncMaxDeleteCountChanged),
                    ]
                    .spacing(6),
                );
                advanced = advanced.push(
                    row![
                        text("Max Delete Bytes").size(11).width(130),
                        text_input("off", &sync.delete_guardrails.max_delete_bytes_text)
                            .on_input(Message::SyncMaxDeleteBytesChanged),
                    ]
                    .spacing(6),
                );
            }

            advanced = advanced.push(text("Filters").size(11));
            advanced = advanced.push(
                text_input(
                    "include patterns, one per line",
                    &sync.filters.include_patterns_text,
                )
                .on_input(Message::SyncIncludePatternsChanged),
            );
            advanced = advanced.push(
                text_input(
                    "exclude patterns, one per line",
                    &sync.filters.exclude_patterns_text,
                )
                .on_input(Message::SyncExcludePatternsChanged),
            );
            advanced = advanced.push(
                text_input("newer than (e.g. 7d, 2w)", &sync.filters.newer_than_text)
                    .on_input(Message::SyncNewerThanChanged),
            );
            advanced = advanced.push(
                text_input("older than (e.g. 30d, 1y)", &sync.filters.older_than_text)
                    .on_input(Message::SyncOlderThanChanged),
            );
            advanced = advanced.push(
                text_input("min size (e.g. 10M)", &sync.filters.min_size_text)
                    .on_input(Message::SyncMinSizeChanged),
            );
            advanced = advanced.push(
                text_input("max size (e.g. 1G)", &sync.filters.max_size_text)
                    .on_input(Message::SyncMaxSizeChanged),
            );

            content = content.push(advanced);
        }

        content = content.push(iced::widget::rule::horizontal(1));
        let copy_ready = sync.mode == SyncMode::Copy
            && sync
                .run_plan
                .as_ref()
                .is_some_and(|plan| !plan.transfers.is_empty())
            && !sync
                .execution
                .as_ref()
                .is_some_and(|execution| execution.running);
        let sync_ready = sync.mode == SyncMode::Sync
            && sync.plan.is_some()
            && !sync
                .execution
                .as_ref()
                .is_some_and(|execution| execution.running);
        content = content.push(
            row![
                if sync.running {
                    button(text("Planning...").size(11))
                } else {
                    button(text(build_plan_label(sync.mode)).size(11))
                        .on_press(Message::StartSyncPlan)
                },
                if copy_ready {
                    button(text("Run Copy").size(11)).on_press(Message::StartSyncCopy)
                } else if sync.mode == SyncMode::Copy
                    && sync
                        .execution
                        .as_ref()
                        .is_some_and(|execution| execution.running)
                {
                    button(text("Copying...").size(11))
                } else {
                    button(text("Run Copy").size(11))
                },
                if sync_ready {
                    button(text("Run Sync").size(11)).on_press(Message::StartSync)
                } else if sync.mode == SyncMode::Sync
                    && sync
                        .execution
                        .as_ref()
                        .is_some_and(|execution| execution.running)
                {
                    button(text("Syncing...").size(11))
                } else {
                    button(text("Run Sync").size(11))
                },
                button(text("Reset").size(11)).on_press(Message::OpenSync),
            ]
            .spacing(8),
        );

        content = content.push(text(format!(
            "Stage: {} | Source scanned: {} | Destination scanned: {} | Compared: {}",
            sync.telemetry.stage,
            sync.telemetry.source_scanned,
            sync.telemetry.destination_scanned,
            sync.telemetry.compared
        )));

        if let Some(error) = &sync.error {
            content = content.push(text(format!("Error: {}", error)).size(11));
        }

        if let Some(plan) = &sync.plan {
            content = content.push(text("Plan Summary").size(12));
            content = content.push(text(format!(
                "Create: {}  Update: {}  Delete: {}  Skip: {}  Conflict: {}",
                plan.summary.creates,
                plan.summary.updates,
                plan.summary.deletes,
                plan.summary.skips,
                plan.summary.conflicts
            )));
            content = content.push(text(format!(
                "Bytes create: {}  update: {}  delete: {}",
                format_bytes(plan.summary.bytes_to_create),
                format_bytes(plan.summary.bytes_to_update),
                format_bytes(plan.summary.bytes_to_delete)
            )));
            if let Some(run_plan) = &sync.run_plan {
                let relay_count = run_plan
                    .transfers
                    .iter()
                    .filter(|item| item.strategy == SyncExecutionStrategy::ClientRelay)
                    .count();
                if relay_count > 0 {
                    content = content.push(
                        text(format!(
                            "Warning: {} copy item(s) will use client relay instead of server-side copy.",
                            relay_count
                        ))
                        .size(11),
                    );
                }
                if sync.mode == SyncMode::Sync {
                    content = content.push(text(format!(
                        "Delete phase: {}  Ignore errors: {}  Delete workers: {}",
                        sync.policy.delete_phase,
                        if sync.delete_guardrails.ignore_errors {
                            "yes"
                        } else {
                            "no"
                        },
                        sync.delete_guardrails.delete_workers_text
                    )));
                }
            }
            if plan.items.is_empty() {
                content = content.push(text("No plan items yet.").size(11));
            } else {
                for item in &plan.items {
                    let strategy = sync.run_plan.as_ref().and_then(|items| {
                        items
                            .transfers
                            .iter()
                            .chain(items.deletes.iter())
                            .find(|run_item| run_item.relative_path == item.relative_path)
                            .map(|run_item| run_item.strategy)
                    });
                    content = content.push(text(format!(
                        "{:?}  {}  {:?}{}",
                        item.action,
                        item.relative_path,
                        item.reason,
                        strategy
                            .map(|strategy| format!("  [{}]", strategy))
                            .unwrap_or_default()
                    )));
                }
            }
        } else {
            content = content.push(text("No plan built yet.").size(11));
        }

        if let Some(execution) = &sync.execution {
            content = content.push(iced::widget::rule::horizontal(1));
            content = content.push(text("Execution").size(12));
            content = content.push(text(format!(
                "Copied: {}  Deleted: {}  Failures: {} / {}  Bytes: {} / {}",
                execution.completed_transfers,
                execution.completed_deletes,
                execution.failed_transfers,
                execution.failed_deletes,
                format_bytes(execution.bytes_done),
                format_bytes(execution.total_bytes)
            )));
            {
                let throughput = sync
                    .telemetry
                    .bytes_per_sec
                    .map(|bps| format!("{}/s", format_bytes(bps as u64)))
                    .unwrap_or_else(|| "-".to_string());
                let max_workers = sync
                    .tuning
                    .transfer_workers_text
                    .trim()
                    .parse::<usize>()
                    .unwrap_or(4);
                content = content.push(text(format!(
                    "Throughput: {}  Active: {} / {}",
                    throughput, sync.telemetry.active_transfers, max_workers
                )));
            }
            if execution.has_client_relay {
                content = content.push(
                    text("Execution includes client-relayed S3 copies. Data will traverse this client.")
                        .size(11),
                );
            }
            if let Some(item) = &execution.current_item {
                content = content.push(text(format!("Current: {}", item)).size(11));
            }
            if let Some(strategy) = execution.current_strategy {
                content = content.push(text(format!("Strategy: {}", strategy)).size(11));
            }
            if let Some(summary) = &execution.summary {
                content = content.push(text(summary).size(11));
            }
            if execution.delete_phase_skipped {
                content = content.push(
                    text("Delete phase skipped because earlier transfers failed and ignore-errors is off.")
                        .size(11),
                );
            }
        }

        scrollable(content)
            .width(Length::Fill)
            .height(Length::Fill)
            .into()
    }

    pub fn sync_delete_confirm_modal(&self) -> Element<'_, Message> {
        let Some(sync) = &self.sync else {
            return container(text("")).width(Length::Shrink).into();
        };
        let Some(confirm) = &sync.delete_confirm else {
            return container(text("")).width(Length::Shrink).into();
        };

        let mut body = column![
            text("Confirm Sync Delete").size(16),
            text("This sync run will delete destination-only objects.").size(11),
            text(format!("Planned deletes: {}", confirm.planned_deletes)).size(11),
            text(format!(
                "Planned delete bytes: {}",
                format_bytes(confirm.planned_delete_bytes)
            ))
            .size(11),
        ]
        .spacing(8)
        .padding(12);

        if let Some(reason) = &confirm.threshold_reason {
            body = body.push(text(format!("Extra confirmation required: {}", reason)).size(11));
        }
        if confirm.typed_required {
            body = body.push(text(format!(
                "Type 'delete {}' to enable sync.",
                confirm.planned_deletes
            )));
            body = body.push(
                text_input("delete count", &confirm.confirm_text)
                    .on_input(Message::SyncDeleteConfirmTextChanged),
            );
        }

        body = body.push(
            row![
                button(text("Cancel").size(11)).on_press(Message::CancelSyncDeleteConfirm),
                button(text("Run Sync").size(11)).on_press(Message::ConfirmSyncDeleteRun),
            ]
            .spacing(8),
        );

        modal_card(body, 460)
    }
}

fn selected_connection_label(app: &App, connection_id: &str) -> String {
    if connection_id == CURRENT_CONNECTION_ID {
        app.current_connection_label()
    } else {
        connection_id.to_string()
    }
}

fn build_plan_label(mode: SyncMode) -> &'static str {
    match mode {
        SyncMode::Diff => "Build Diff Plan",
        SyncMode::Copy => "Build Copy Plan",
        SyncMode::Sync => "Build Sync Plan",
    }
}

fn mode_summary(mode: SyncMode) -> &'static str {
    match mode {
        SyncMode::Diff => "Preview only. No writes. Use current sync policy to classify changes.",
        SyncMode::Copy => "High-throughput copy semantics. Missing and changed objects only.",
        SyncMode::Sync => {
            "Flexible reconcile workflow with overwrite, delete, and conflict policy controls."
        }
    }
}

fn sync_s3_endpoint_block(
    is_source: bool,
    connection_label: String,
    bucket: &str,
    prefix: &str,
    bucket_options: Vec<String>,
    loading_buckets: bool,
    connection_options: Vec<String>,
) -> Element<'static, Message> {
    let mut col = column![
        row![
            text("Connection").size(11).width(80),
            pick_list(connection_options, Some(connection_label), move |label| {
                if label == "Current connection" {
                    if is_source {
                        Message::SyncSourceConnectionChanged(CURRENT_CONNECTION_ID.to_string())
                    } else {
                        Message::SyncDestinationConnectionChanged(CURRENT_CONNECTION_ID.to_string())
                    }
                } else if is_source {
                    Message::SyncSourceConnectionChanged(label)
                } else {
                    Message::SyncDestinationConnectionChanged(label)
                }
            }),
        ]
        .spacing(6),
    ]
    .spacing(6);

    if loading_buckets {
        col = col.push(text("Loading buckets...").size(11));
    } else {
        col = col.push(
            row![
                text("Bucket").size(11).width(80),
                pick_list(
                    bucket_options,
                    if bucket.is_empty() {
                        None
                    } else {
                        Some(bucket.to_string())
                    },
                    move |value| if is_source {
                        Message::SyncSourceBucketChanged(value)
                    } else {
                        Message::SyncDestinationBucketChanged(value)
                    }
                ),
            ]
            .spacing(6),
        );
    }

    col.push(
        row![
            text("Prefix").size(11).width(80),
            text_input("prefix/", prefix).on_input(move |value| if is_source {
                Message::SyncSourcePrefixChanged(value)
            } else {
                Message::SyncDestinationPrefixChanged(value)
            }),
        ]
        .spacing(6),
    )
    .into()
}

fn sync_local_endpoint_block(
    path_text: String,
    pick_message: Message,
) -> Element<'static, Message> {
    column![
        text(if path_text.is_empty() {
            "No local path selected.".to_string()
        } else {
            format!("Path: {}", path_text)
        })
        .size(11),
        button(text("Choose Folder").size(11)).on_press(pick_message),
    ]
    .spacing(6)
    .into()
}

fn modal_card<'a>(content: iced::widget::Column<'a, Message>, width: u16) -> Element<'a, Message> {
    use iced::widget::container;

    let card = container(content)
        .width(width as u32)
        .style(iced::widget::container::rounded_box)
        .padding(0);

    container(card)
        .width(Length::Fill)
        .height(Length::Fill)
        .center_x(Length::Fill)
        .center_y(Length::Fill)
        .into()
}

fn format_bytes(n: u64) -> String {
    const KB: u64 = 1024;
    const MB: u64 = 1024 * KB;
    const GB: u64 = 1024 * MB;

    if n >= GB {
        format!("{:.2} GB", n as f64 / GB as f64)
    } else if n >= MB {
        format!("{:.1} MB", n as f64 / MB as f64)
    } else if n >= KB {
        format!("{:.1} KB", n as f64 / KB as f64)
    } else {
        format!("{} B", n)
    }
}

impl std::fmt::Display for SyncMode {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::Diff => write!(f, "Diff"),
            Self::Copy => write!(f, "Copy"),
            Self::Sync => write!(f, "Sync"),
        }
    }
}

impl std::fmt::Display for SyncPreset {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "{}", self.title())
    }
}

impl std::fmt::Display for SyncDestinationNewerPolicy {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::SourceWins => write!(f, "Source Wins"),
            Self::Skip => write!(f, "Skip"),
            Self::Conflict => write!(f, "Conflict"),
        }
    }
}

impl std::fmt::Display for SyncDeletePhase {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::Before => write!(f, "Before"),
            Self::During => write!(f, "During"),
            Self::After => write!(f, "After"),
        }
    }
}

impl std::fmt::Display for SyncCompareMode {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::SizeOnly => write!(f, "Size Only"),
            Self::SizeAndModTime => write!(f, "Size + Modtime"),
            Self::UpdateIfSourceNewer => write!(f, "Update If Source Newer"),
            Self::ChecksumIfAvailable => write!(f, "Checksum If Available"),
            Self::AlwaysOverwrite => write!(f, "Always Overwrite"),
        }
    }
}

impl std::fmt::Display for SyncListMode {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::Streaming => write!(f, "Streaming"),
            Self::FastList => write!(f, "Fast List"),
            Self::TopUp => write!(f, "Top Up"),
        }
    }
}

impl std::fmt::Display for SyncEndpointKind {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::S3 => write!(f, "S3"),
            Self::Local => write!(f, "Local"),
        }
    }
}

impl std::fmt::Display for SyncExecutionStrategy {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::Upload => write!(f, "upload"),
            Self::Download => write!(f, "download"),
            Self::ServerSideCopy => write!(f, "server-side copy"),
            Self::ClientRelay => write!(f, "client relay"),
            Self::DeleteRemote => write!(f, "remote delete"),
            Self::DeleteLocal => write!(f, "local delete"),
        }
    }
}
