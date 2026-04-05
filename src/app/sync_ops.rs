use std::collections::BTreeMap;
use std::path::{Path, PathBuf};
use std::sync::Arc;

use crate::app::{
    SyncCompareMode, SyncDestinationNewerPolicy, SyncFilterSet, SyncMode, SyncObject, SyncPlan,
    SyncPlanAction, SyncPlanItem, SyncPlanReason, SyncPlanSummary, SyncPolicy, wildcard_match,
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
}
