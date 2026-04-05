use std::collections::BTreeMap;
use std::path::{Path, PathBuf};
use std::sync::Arc;

use crate::app::{
    SyncCompareMode, SyncDestinationNewerPolicy, SyncEndpointKind, SyncExecutionStrategy,
    SyncFilterSet, SyncMode, SyncObject, SyncPlan, SyncPlanAction, SyncPlanItem, SyncPlanReason,
    SyncPlanSummary, SyncPolicy, SyncRunItem, SyncState, TransferEndpoint, wildcard_match,
};
use crate::s3::client::S3Client;

pub fn normalize_relative_path(path: &str) -> String {
    path.replace('\\', "/").trim_start_matches('/').to_string()
}

pub fn parse_patterns(text: &str) -> Vec<String> {
    text.lines()
        .map(str::trim)
        .filter(|line| !line.is_empty())
        .map(ToOwned::to_owned)
        .collect()
}

pub fn parse_size_filter(text: &str) -> Option<u64> {
    text.trim().parse().ok()
}

pub fn apply_sync_filters(object: &SyncObject, filters: &SyncFilterSet) -> bool {
    let min_size = parse_size_filter(&filters.min_size_text);
    let max_size = parse_size_filter(&filters.max_size_text);
    if let Some(min_size) = min_size
        && object.size < min_size
    {
        return false;
    }
    if let Some(max_size) = max_size
        && object.size > max_size
    {
        return false;
    }
    let include_patterns = parse_patterns(&filters.include_patterns_text);
    if !include_patterns.is_empty()
        && !include_patterns
            .iter()
            .any(|pattern| wildcard_match(pattern, &object.relative_path))
    {
        return false;
    }
    let exclude_patterns = parse_patterns(&filters.exclude_patterns_text);
    if exclude_patterns
        .iter()
        .any(|pattern| wildcard_match(pattern, &object.relative_path))
    {
        return false;
    }
    true
}

pub fn compare_sync_objects(
    source: &SyncObject,
    destination: &SyncObject,
    mode: SyncCompareMode,
    policy: SyncPolicy,
) -> (SyncPlanAction, SyncPlanReason) {
    match mode {
        SyncCompareMode::AlwaysOverwrite => {
            update_or_conflict(policy.overwrite_changed, SyncPlanReason::SourceNewer)
        }
        SyncCompareMode::SizeOnly => {
            if source.size == destination.size {
                (SyncPlanAction::Skip, SyncPlanReason::Identical)
            } else {
                update_or_conflict(policy.overwrite_changed, SyncPlanReason::SizeMismatch)
            }
        }
        SyncCompareMode::SizeAndModTime | SyncCompareMode::UpdateIfSourceNewer => {
            if source.size != destination.size {
                return update_or_conflict(policy.overwrite_changed, SyncPlanReason::SizeMismatch);
            }
            if source.modified == destination.modified {
                return (SyncPlanAction::Skip, SyncPlanReason::Identical);
            }
            if matches!(mode, SyncCompareMode::UpdateIfSourceNewer) {
                match (&source.modified, &destination.modified) {
                    (Some(source_modified), Some(destination_modified))
                        if source_modified <= destination_modified =>
                    {
                        return match policy.destination_newer_policy {
                            SyncDestinationNewerPolicy::SourceWins => update_or_conflict(
                                policy.overwrite_changed,
                                SyncPlanReason::DestinationNewer,
                            ),
                            SyncDestinationNewerPolicy::Skip => {
                                (SyncPlanAction::Skip, SyncPlanReason::DestinationNewer)
                            }
                            SyncDestinationNewerPolicy::Conflict => {
                                (SyncPlanAction::Conflict, SyncPlanReason::DestinationNewer)
                            }
                        };
                    }
                    _ => {}
                }
            }
            update_or_conflict(policy.overwrite_changed, SyncPlanReason::SourceNewer)
        }
        SyncCompareMode::ChecksumIfAvailable => {
            if source.etag.is_some()
                && source.etag == destination.etag
                && source.size == destination.size
            {
                (SyncPlanAction::Skip, SyncPlanReason::Identical)
            } else if source.size != destination.size {
                update_or_conflict(policy.overwrite_changed, SyncPlanReason::SizeMismatch)
            } else {
                update_or_conflict(policy.overwrite_changed, SyncPlanReason::ChecksumMismatch)
            }
        }
    }
}

fn update_or_conflict(
    overwrite_changed: bool,
    reason: SyncPlanReason,
) -> (SyncPlanAction, SyncPlanReason) {
    if overwrite_changed {
        (SyncPlanAction::Update, reason)
    } else {
        (SyncPlanAction::Conflict, reason)
    }
}

pub fn build_sync_plan(
    source: Vec<SyncObject>,
    destination: Vec<SyncObject>,
    mode: SyncMode,
    policy: SyncPolicy,
    compare_mode: SyncCompareMode,
) -> SyncPlan {
    let mut source_map = BTreeMap::new();
    let mut destination_map = BTreeMap::new();
    for item in source {
        source_map.insert(item.relative_path.clone(), item);
    }
    for item in destination {
        destination_map.insert(item.relative_path.clone(), item);
    }

    let mut summary = SyncPlanSummary {
        source_scanned: source_map.len(),
        destination_scanned: destination_map.len(),
        ..SyncPlanSummary::default()
    };
    let mut items = Vec::new();
    let mut all_paths: Vec<String> = source_map
        .keys()
        .chain(destination_map.keys())
        .cloned()
        .collect();
    all_paths.sort();
    all_paths.dedup();

    for path in all_paths {
        let source = source_map.get(&path).cloned();
        let destination = destination_map.get(&path).cloned();

        let (action, reason) = match (&source, &destination) {
            (Some(source), Some(destination)) if mode == SyncMode::Copy => {
                compare_copy_objects(source, destination, compare_mode)
            }
            (Some(source), Some(destination)) => {
                compare_sync_objects(source, destination, compare_mode, policy)
            }
            (Some(_), None) => (SyncPlanAction::Create, SyncPlanReason::MissingOnDestination),
            (None, Some(_)) => missing_on_source_action(mode, policy),
            (None, None) => continue,
        };

        match action {
            SyncPlanAction::Create => {
                summary.creates += 1;
                summary.bytes_to_create += source.as_ref().map(|item| item.size).unwrap_or(0);
            }
            SyncPlanAction::Update => {
                summary.updates += 1;
                summary.bytes_to_update += source.as_ref().map(|item| item.size).unwrap_or(0);
            }
            SyncPlanAction::Delete => {
                summary.deletes += 1;
                summary.bytes_to_delete += destination.as_ref().map(|item| item.size).unwrap_or(0);
            }
            SyncPlanAction::Skip => summary.skips += 1,
            SyncPlanAction::Conflict => summary.conflicts += 1,
        }

        items.push(SyncPlanItem {
            action,
            reason,
            relative_path: path,
            source,
            destination,
        });
    }

    SyncPlan { items, summary }
}

fn compare_copy_objects(
    source: &SyncObject,
    destination: &SyncObject,
    mode: SyncCompareMode,
) -> (SyncPlanAction, SyncPlanReason) {
    match mode {
        SyncCompareMode::AlwaysOverwrite => (SyncPlanAction::Update, SyncPlanReason::SourceNewer),
        SyncCompareMode::SizeOnly => {
            if source.size == destination.size {
                (SyncPlanAction::Skip, SyncPlanReason::Identical)
            } else {
                (SyncPlanAction::Update, SyncPlanReason::SizeMismatch)
            }
        }
        SyncCompareMode::SizeAndModTime => {
            if source.size != destination.size {
                (SyncPlanAction::Update, SyncPlanReason::SizeMismatch)
            } else if source.modified == destination.modified {
                (SyncPlanAction::Skip, SyncPlanReason::Identical)
            } else {
                (SyncPlanAction::Update, SyncPlanReason::SourceNewer)
            }
        }
        SyncCompareMode::UpdateIfSourceNewer => {
            if source.size != destination.size {
                (SyncPlanAction::Update, SyncPlanReason::SizeMismatch)
            } else if source.modified == destination.modified {
                (SyncPlanAction::Skip, SyncPlanReason::Identical)
            } else {
                match (&source.modified, &destination.modified) {
                    (Some(source_modified), Some(destination_modified))
                        if source_modified <= destination_modified =>
                    {
                        (SyncPlanAction::Skip, SyncPlanReason::DestinationNewer)
                    }
                    _ => (SyncPlanAction::Update, SyncPlanReason::SourceNewer),
                }
            }
        }
        SyncCompareMode::ChecksumIfAvailable => {
            if source.etag.is_some()
                && source.etag == destination.etag
                && source.size == destination.size
            {
                (SyncPlanAction::Skip, SyncPlanReason::Identical)
            } else if source.size != destination.size {
                (SyncPlanAction::Update, SyncPlanReason::SizeMismatch)
            } else {
                (SyncPlanAction::Update, SyncPlanReason::ChecksumMismatch)
            }
        }
    }
}

fn missing_on_source_action(
    mode: SyncMode,
    policy: SyncPolicy,
) -> (SyncPlanAction, SyncPlanReason) {
    match mode {
        SyncMode::Copy => (SyncPlanAction::Skip, SyncPlanReason::MissingOnSource),
        SyncMode::Sync | SyncMode::Diff if policy.delete_extras => {
            (SyncPlanAction::Delete, SyncPlanReason::MissingOnSource)
        }
        SyncMode::Sync | SyncMode::Diff => (SyncPlanAction::Skip, SyncPlanReason::MissingOnSource),
    }
}

pub fn prepare_copy_run_plan(
    sync: &SyncState,
    plan: &SyncPlan,
) -> Result<Vec<SyncRunItem>, String> {
    let mut items = Vec::new();
    for plan_item in &plan.items {
        if !matches!(
            plan_item.action,
            SyncPlanAction::Create | SyncPlanAction::Update
        ) {
            continue;
        }
        let source = build_transfer_endpoint(sync, true, &plan_item.relative_path)?;
        let destination = build_transfer_endpoint(sync, false, &plan_item.relative_path)?;
        let strategy = determine_execution_strategy(&source, &destination)?;
        let bytes = plan_item
            .source
            .as_ref()
            .map(|object| object.size)
            .unwrap_or(0);
        items.push(SyncRunItem {
            relative_path: plan_item.relative_path.clone(),
            action: plan_item.action,
            source,
            destination,
            strategy,
            bytes,
        });
    }
    Ok(items)
}

fn build_transfer_endpoint(
    sync: &SyncState,
    source: bool,
    relative_path: &str,
) -> Result<TransferEndpoint, String> {
    let path_fragment = relative_path.replace('/', std::path::MAIN_SEPARATOR_STR);
    match if source {
        sync.source_kind
    } else {
        sync.destination_kind
    } {
        SyncEndpointKind::S3 => Ok(TransferEndpoint::S3 {
            connection_id: if source {
                sync.source_connection_id.clone()
            } else {
                sync.destination_connection_id.clone()
            },
            bucket: if source {
                sync.source_bucket.clone()
            } else {
                sync.destination_bucket.clone()
            },
            key: join_s3_key(
                if source {
                    &sync.source_prefix
                } else {
                    &sync.destination_prefix
                },
                relative_path,
            ),
        }),
        SyncEndpointKind::Local => {
            let root = if source {
                sync.source_local_path.clone()
            } else {
                sync.destination_local_path.clone()
            }
            .ok_or_else(|| "Local path is required.".to_string())?;
            Ok(TransferEndpoint::Local {
                path: root.join(path_fragment),
            })
        }
    }
}

fn determine_execution_strategy(
    source: &TransferEndpoint,
    destination: &TransferEndpoint,
) -> Result<SyncExecutionStrategy, String> {
    match (source, destination) {
        (TransferEndpoint::Local { .. }, TransferEndpoint::S3 { .. }) => {
            Ok(SyncExecutionStrategy::Upload)
        }
        (TransferEndpoint::S3 { .. }, TransferEndpoint::Local { .. }) => {
            Ok(SyncExecutionStrategy::Download)
        }
        (
            TransferEndpoint::S3 {
                connection_id: source_connection,
                ..
            },
            TransferEndpoint::S3 {
                connection_id: destination_connection,
                ..
            },
        ) => {
            if source_connection == destination_connection {
                Ok(SyncExecutionStrategy::ServerSideCopy)
            } else {
                Ok(SyncExecutionStrategy::ClientRelay)
            }
        }
        (TransferEndpoint::Local { .. }, TransferEndpoint::Local { .. }) => {
            Err("Local to local copy is not supported in sync execution.".to_string())
        }
    }
}

fn join_s3_key(prefix: &str, relative: &str) -> String {
    if prefix.is_empty() {
        relative.to_string()
    } else if prefix.ends_with('/') {
        format!("{}{}", prefix, relative)
    } else {
        format!("{}/{}", prefix, relative)
    }
}

pub fn enumerate_local_for_sync(
    root: PathBuf,
    filters: &SyncFilterSet,
) -> Result<Vec<SyncObject>, String> {
    let mut objects = Vec::new();
    for entry in walkdir::WalkDir::new(&root) {
        let entry = entry.map_err(|error| error.to_string())?;
        if entry.file_type().is_dir() {
            continue;
        }
        let path = entry.path().to_path_buf();
        let relative = relative_path(&root, &path)?;
        let metadata = entry.metadata().map_err(|error| error.to_string())?;
        let object = SyncObject {
            relative_path: relative,
            size: metadata.len(),
            modified: metadata.modified().ok().and_then(|value| {
                time::OffsetDateTime::from(value)
                    .format(&time::format_description::well_known::Rfc3339)
                    .ok()
            }),
            etag: None,
            is_dir_marker: false,
        };
        if apply_sync_filters(&object, filters) {
            objects.push(object);
        }
    }
    Ok(objects)
}

pub async fn enumerate_s3_for_sync(
    client: Arc<S3Client>,
    bucket: &str,
    prefix: &str,
    filters: &SyncFilterSet,
) -> Result<Vec<SyncObject>, String> {
    let objects = client
        .list_objects_recursive_for_sync(bucket, prefix)
        .await?;
    Ok(objects
        .into_iter()
        .filter(|object| apply_sync_filters(object, filters))
        .collect())
}

fn relative_path(root: &Path, path: &Path) -> Result<String, String> {
    let relative = path.strip_prefix(root).map_err(|error| error.to_string())?;
    Ok(normalize_relative_path(&relative.to_string_lossy()))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn build_sync_plan_creates_missing_destination_items() {
        let plan = build_sync_plan(
            vec![SyncObject {
                relative_path: "file.txt".to_string(),
                size: 5,
                modified: None,
                etag: None,
                is_dir_marker: false,
            }],
            Vec::new(),
            SyncMode::Diff,
            crate::app::SyncPreset::Converge.policy(),
            SyncCompareMode::SizeOnly,
        );
        assert_eq!(plan.summary.creates, 1);
    }

    #[test]
    fn copy_mode_skips_destination_only_objects() {
        let plan = build_sync_plan(
            Vec::new(),
            vec![SyncObject {
                relative_path: "file.txt".to_string(),
                size: 5,
                modified: None,
                etag: None,
                is_dir_marker: false,
            }],
            SyncMode::Copy,
            crate::app::SyncPreset::Converge.policy(),
            SyncCompareMode::SizeOnly,
        );
        assert_eq!(plan.summary.deletes, 0);
        assert_eq!(plan.summary.skips, 1);
    }

    #[test]
    fn sync_mode_deletes_destination_only_objects_when_policy_enables_it() {
        let plan = build_sync_plan(
            Vec::new(),
            vec![SyncObject {
                relative_path: "file.txt".to_string(),
                size: 5,
                modified: None,
                etag: None,
                is_dir_marker: false,
            }],
            SyncMode::Sync,
            crate::app::SyncPreset::Converge.policy(),
            SyncCompareMode::SizeOnly,
        );
        assert_eq!(plan.summary.deletes, 1);
    }

    #[test]
    fn copy_mode_skips_destination_newer_objects_for_update_if_source_newer() {
        let plan = build_sync_plan(
            vec![SyncObject {
                relative_path: "file.txt".to_string(),
                size: 5,
                modified: Some("2025-01-01T00:00:00Z".to_string()),
                etag: None,
                is_dir_marker: false,
            }],
            vec![SyncObject {
                relative_path: "file.txt".to_string(),
                size: 5,
                modified: Some("2026-01-01T00:00:00Z".to_string()),
                etag: None,
                is_dir_marker: false,
            }],
            SyncMode::Copy,
            crate::app::SyncPreset::Converge.policy(),
            SyncCompareMode::UpdateIfSourceNewer,
        );
        assert_eq!(plan.summary.skips, 1);
        assert_eq!(plan.summary.conflicts, 0);
        assert_eq!(plan.summary.updates, 0);
    }

    #[test]
    fn advanced_policy_without_overwrite_marks_conflict() {
        let mut policy = crate::app::SyncPreset::Converge.policy();
        policy.overwrite_changed = false;
        let (action, reason) = compare_sync_objects(
            &SyncObject {
                relative_path: "file.txt".to_string(),
                size: 10,
                modified: Some("2026-01-01T00:00:00Z".to_string()),
                etag: Some("abc".to_string()),
                is_dir_marker: false,
            },
            &SyncObject {
                relative_path: "file.txt".to_string(),
                size: 20,
                modified: Some("2025-01-01T00:00:00Z".to_string()),
                etag: Some("xyz".to_string()),
                is_dir_marker: false,
            },
            SyncCompareMode::SizeOnly,
            policy,
        );
        assert_eq!(action, SyncPlanAction::Conflict);
        assert_eq!(reason, SyncPlanReason::SizeMismatch);
    }

    #[test]
    fn include_and_exclude_patterns_are_applied() {
        let object = SyncObject {
            relative_path: "logs/app.log".to_string(),
            size: 42,
            modified: None,
            etag: None,
            is_dir_marker: false,
        };
        let filters = SyncFilterSet {
            include_patterns_text: "*.log".to_string(),
            exclude_patterns_text: "logs/archive/*".to_string(),
            newer_than_text: String::new(),
            older_than_text: String::new(),
            min_size_text: String::new(),
            max_size_text: String::new(),
        };
        assert!(apply_sync_filters(&object, &filters));

        let excluded = SyncObject {
            relative_path: "logs/archive/app.log".to_string(),
            ..object
        };
        assert!(!apply_sync_filters(&excluded, &filters));
    }

    #[test]
    fn prepare_copy_run_plan_marks_cross_endpoint_s3_as_client_relay() {
        let mut sync = crate::app::SyncState::new("source-a".to_string());
        sync.mode = SyncMode::Copy;
        sync.source_kind = SyncEndpointKind::S3;
        sync.destination_kind = SyncEndpointKind::S3;
        sync.source_connection_id = "source-a".to_string();
        sync.destination_connection_id = "dest-b".to_string();
        sync.source_bucket = "source-bucket".to_string();
        sync.destination_bucket = "dest-bucket".to_string();

        let plan = build_sync_plan(
            vec![SyncObject {
                relative_path: "file.txt".to_string(),
                size: 5,
                modified: None,
                etag: None,
                is_dir_marker: false,
            }],
            Vec::new(),
            SyncMode::Copy,
            crate::app::SyncPreset::Converge.policy(),
            SyncCompareMode::SizeOnly,
        );

        let run_plan = prepare_copy_run_plan(&sync, &plan).expect("copy run plan");
        assert_eq!(run_plan.len(), 1);
        assert_eq!(run_plan[0].strategy, SyncExecutionStrategy::ClientRelay);
    }
}
