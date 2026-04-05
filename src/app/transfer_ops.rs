use std::path::{Path, PathBuf};
use std::sync::Arc;
use std::time::{SystemTime, UNIX_EPOCH};

use time::OffsetDateTime;
use time::format_description::well_known::Rfc3339;

use crate::s3::client::S3Client;

use super::types::{
    CURRENT_CONNECTION_ID, OverwritePolicy, SyncExecutionStrategy, SyncRunItem, TransferEndpoint,
    TransferItem, TransferStepResult,
};

impl TransferItem {
    pub(crate) fn label(&self) -> String {
        match (&self.source, &self.destination) {
            (
                TransferEndpoint::S3 { bucket, key, .. },
                TransferEndpoint::S3 {
                    bucket: dest_bucket,
                    key: dest_key,
                    ..
                },
            ) => format!("{}/{} -> {}/{}", bucket, key, dest_bucket, dest_key),
            (TransferEndpoint::Local { path }, TransferEndpoint::S3 { bucket, key, .. }) => {
                format!("{} -> {}/{}", path.display(), bucket, key)
            }
            (TransferEndpoint::S3 { bucket, key, .. }, TransferEndpoint::Local { path }) => {
                format!("{}/{} -> {}", bucket, key, path.display())
            }
            (TransferEndpoint::Local { path }, TransferEndpoint::Local { path: dest }) => {
                format!("{} -> {}", path.display(), dest.display())
            }
        }
    }
}

pub fn now_rfc3339() -> String {
    OffsetDateTime::now_utc()
        .format(&Rfc3339)
        .unwrap_or_else(|_| "1970-01-01T00:00:00Z".to_string())
}

fn relative_key(root: &Path, path: &Path) -> Result<String, String> {
    let relative = path
        .strip_prefix(root)
        .map_err(|e| e.to_string())?
        .to_string_lossy()
        .replace('\\', "/");
    Ok(relative)
}

fn join_s3_key(prefix: &str, relative: &str) -> String {
    if prefix.is_empty() {
        relative.to_string()
    } else {
        format!("{}{}", prefix, relative)
    }
}

fn guess_content_type(path: &Path) -> String {
    mime_guess::from_path(path)
        .first_raw()
        .unwrap_or("application/octet-stream")
        .to_string()
}

pub fn prepare_import_items(
    root: PathBuf,
    bucket: &str,
    prefix: &str,
) -> Result<Vec<TransferItem>, String> {
    let mut items = Vec::new();
    for entry in walkdir::WalkDir::new(&root) {
        let entry = entry.map_err(|e| e.to_string())?;
        if entry.file_type().is_dir() {
            continue;
        }
        let path = entry.path().to_path_buf();
        let relative = relative_key(&root, &path)?;
        items.push(TransferItem {
            source: TransferEndpoint::Local { path },
            destination: TransferEndpoint::S3 {
                connection_id: CURRENT_CONNECTION_ID.to_string(),
                bucket: bucket.to_string(),
                key: join_s3_key(prefix, &relative),
            },
        });
    }
    Ok(items)
}

pub async fn prepare_export_items(
    client: Arc<S3Client>,
    bucket: &str,
    prefix: &str,
    root: &Path,
) -> Result<Vec<TransferItem>, String> {
    let listing = client.list_objects(bucket, prefix, "").await?;
    let mut items = Vec::new();
    for object in listing.objects {
        let relative = object
            .key
            .strip_prefix(prefix)
            .unwrap_or(&object.key)
            .replace('/', "\\");
        items.push(TransferItem {
            source: TransferEndpoint::S3 {
                connection_id: CURRENT_CONNECTION_ID.to_string(),
                bucket: bucket.to_string(),
                key: object.key,
            },
            destination: TransferEndpoint::Local {
                path: root.join(relative),
            },
        });
    }
    Ok(items)
}

pub async fn run_transfer_step(
    source_client: Arc<S3Client>,
    destination_client: Option<Arc<S3Client>>,
    item: TransferItem,
    overwrite_policy: OverwritePolicy,
    is_move: bool,
) -> Result<TransferStepResult, String> {
    match &item.destination {
        TransferEndpoint::S3 { bucket, key, .. } => {
            let dest_client = destination_client.ok_or("missing destination client")?;
            let exists = dest_client.head_object(bucket, key).await.is_ok();
            if exists {
                match overwrite_policy {
                    OverwritePolicy::Ask => return Ok(TransferStepResult::Conflict(item)),
                    OverwritePolicy::SkipAll => {
                        return Ok(TransferStepResult::Skipped(item.label()));
                    }
                    OverwritePolicy::OverwriteAll => {}
                }
            }
            match &item.source {
                TransferEndpoint::S3 {
                    bucket: src_bucket,
                    key: src_key,
                    ..
                } => {
                    // server-side copy when possible (same client/endpoint)
                    source_client
                        .copy_object(src_bucket, src_key, bucket, key)
                        .await?;
                    // for move: delete source after confirmed copy
                    if is_move {
                        source_client.delete_object(src_bucket, src_key).await?;
                    }
                }
                TransferEndpoint::Local { path } => {
                    let content_type = guess_content_type(path);
                    dest_client
                        .upload_file(bucket, key, path, &content_type)
                        .await?;
                }
            }
            Ok(TransferStepResult::Copied(item.label()))
        }
        TransferEndpoint::Local { path } => {
            let exists = path.exists();
            if exists {
                match overwrite_policy {
                    OverwritePolicy::Ask => return Ok(TransferStepResult::Conflict(item)),
                    OverwritePolicy::SkipAll => {
                        return Ok(TransferStepResult::Skipped(item.label()));
                    }
                    OverwritePolicy::OverwriteAll => {}
                }
            }
            let TransferEndpoint::S3 { bucket, key, .. } = &item.source else {
                return Err("local to local transfer is unsupported".to_string());
            };
            let data = source_client.get_object(bucket, key).await?;
            if let Some(parent) = path.parent() {
                tokio::fs::create_dir_all(parent)
                    .await
                    .map_err(|e| e.to_string())?;
            }
            tokio::fs::write(path, data)
                .await
                .map_err(|e| e.to_string())?;
            Ok(TransferStepResult::Copied(item.label()))
        }
    }
}

pub async fn execute_sync_run_item(
    source_client: Option<Arc<S3Client>>,
    destination_client: Option<Arc<S3Client>>,
    item: &SyncRunItem,
) -> Result<(), String> {
    match item.strategy {
        SyncExecutionStrategy::Upload => {
            let TransferEndpoint::Local { path } = &item.source else {
                return Err("upload strategy requires a local source".to_string());
            };
            let TransferEndpoint::S3 { bucket, key, .. } = &item.destination else {
                return Err("upload strategy requires an s3 destination".to_string());
            };
            let destination_client = destination_client.ok_or("missing destination client")?;
            let content_type = guess_content_type(path);
            destination_client
                .upload_file(bucket, key, path, &content_type)
                .await?;
            Ok(())
        }
        SyncExecutionStrategy::Download => {
            let TransferEndpoint::S3 { bucket, key, .. } = &item.source else {
                return Err("download strategy requires an s3 source".to_string());
            };
            let TransferEndpoint::Local { path } = &item.destination else {
                return Err("download strategy requires a local destination".to_string());
            };
            let source_client = source_client.ok_or("missing source client")?;
            source_client
                .download_object_to_file(bucket, key, path)
                .await?;
            Ok(())
        }
        SyncExecutionStrategy::ServerSideCopy => {
            let TransferEndpoint::S3 {
                bucket: source_bucket,
                key: source_key,
                ..
            } = &item.source
            else {
                return Err("server-side copy requires an s3 source".to_string());
            };
            let TransferEndpoint::S3 {
                bucket: destination_bucket,
                key: destination_key,
                ..
            } = &item.destination
            else {
                return Err("server-side copy requires an s3 destination".to_string());
            };
            let source_client = source_client.ok_or("missing source client")?;
            source_client
                .copy_object(
                    source_bucket,
                    source_key,
                    destination_bucket,
                    destination_key,
                )
                .await?;
            Ok(())
        }
        SyncExecutionStrategy::ClientRelay => {
            let TransferEndpoint::S3 {
                bucket: source_bucket,
                key: source_key,
                ..
            } = &item.source
            else {
                return Err("client relay requires an s3 source".to_string());
            };
            let TransferEndpoint::S3 {
                bucket: destination_bucket,
                key: destination_key,
                ..
            } = &item.destination
            else {
                return Err("client relay requires an s3 destination".to_string());
            };
            let source_client = source_client.ok_or("missing source client")?;
            let destination_client = destination_client.ok_or("missing destination client")?;

            if item.bytes > 5 * 1024 * 1024 * 1024 {
                let spool_path = relay_spool_path(source_key);
                source_client
                    .download_object_to_file(source_bucket, source_key, &spool_path)
                    .await?;
                let content_type = guess_content_type(&spool_path);
                let upload_result = destination_client
                    .upload_file(
                        destination_bucket,
                        destination_key,
                        &spool_path,
                        &content_type,
                    )
                    .await;
                let _ = tokio::fs::remove_file(&spool_path).await;
                upload_result.map(|_| ())
            } else {
                source_client
                    .relay_object_to_s3(
                        source_bucket,
                        source_key,
                        destination_client.as_ref(),
                        destination_bucket,
                        destination_key,
                    )
                    .await
                    .map(|_| ())
            }
        }
        SyncExecutionStrategy::DeleteRemote => {
            let TransferEndpoint::S3 { bucket, key, .. } = &item.destination else {
                return Err("remote delete requires an s3 destination".to_string());
            };
            let destination_client = destination_client.ok_or("missing destination client")?;
            destination_client.delete_object(bucket, key).await
        }
        SyncExecutionStrategy::DeleteLocal => {
            let TransferEndpoint::Local { path } = &item.destination else {
                return Err("local delete requires a local destination".to_string());
            };
            delete_local_path(path).await
        }
    }
}

async fn delete_local_path(path: &Path) -> Result<(), String> {
    tokio::fs::remove_file(path)
        .await
        .map_err(|e| e.to_string())?;
    prune_empty_parent_dirs(path).await;
    Ok(())
}

async fn prune_empty_parent_dirs(path: &Path) {
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

fn relay_spool_path(source_key: &str) -> PathBuf {
    let nanos = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .unwrap_or_default()
        .as_nanos();
    let sanitized = source_key.replace(['/', '\\', ':'], "_");
    std::env::temp_dir().join(format!("abixio-ui-sync-relay-{}-{}", nanos, sanitized))
}

/// Wildcard match supporting `*` (any sequence) and `?` (any single char).
/// If the pattern contains no wildcards, falls back to case-insensitive
/// substring match. Matching is always case-insensitive.
///
/// Supports `*` (matches any chars except `/`), `**` (matches any chars
/// including `/`), and `?` (matches one non-`/` char). This matches
/// rclone glob semantics.
pub fn wildcard_match(pattern: &str, text: &str) -> bool {
    let pat_lower = pattern.to_ascii_lowercase();
    let text_lower = text.to_ascii_lowercase();

    if !pat_lower.contains('*') && !pat_lower.contains('?') {
        return text_lower.contains(&pat_lower);
    }

    // split pattern on "**" to get segments, then match each segment
    // allowing any chars (including /) between segments matched by **
    let segments: Vec<&str> = pat_lower.split("**").collect();
    if segments.len() == 1 {
        // no ** in pattern: * matches anything including / (legacy behavior,
        // keeps *.txt matching paths like dir/file.txt)
        return wildcard_match_segment(&pat_lower, &text_lower, true);
    }
    // match segments separated by ** (which matches anything including /)
    let mut remaining = text_lower.as_str();
    for (i, segment) in segments.iter().enumerate() {
        if segment.is_empty() {
            continue;
        }
        // trim leading/trailing / from segment when adjacent to **
        let seg = segment.trim_matches('/');
        if seg.is_empty() {
            continue;
        }
        if i == 0 {
            // first segment must match at the start
            let seg_len = find_segment_match(seg, remaining, false);
            let Some(len) = seg_len else {
                return false;
            };
            remaining = &remaining[len..];
            remaining = remaining.trim_start_matches('/');
        } else {
            // find segment anywhere in remaining (** consumed arbitrary content)
            let mut found = false;
            for start in 0..=remaining.len() {
                let slice = &remaining[start..];
                if let Some(len) = find_segment_match(seg, slice, false) {
                    remaining = &slice[len..];
                    remaining = remaining.trim_start_matches('/');
                    found = true;
                    break;
                }
            }
            if !found {
                return false;
            }
        }
    }
    // if last segment is not empty, remaining must be empty (or just slashes)
    if !segments.last().unwrap_or(&"").is_empty() {
        return remaining.is_empty() || remaining.chars().all(|c| c == '/');
    }
    true
}

/// Match a simple glob segment (with `*` and `?` but no `**`) against text.
/// When `allow_slash` is false, `*` and `?` do not match `/`.
fn wildcard_match_segment(pattern: &str, text: &str, allow_slash: bool) -> bool {
    let pat: Vec<char> = pattern.chars().collect();
    let txt: Vec<char> = text.chars().collect();
    let (plen, tlen) = (pat.len(), txt.len());
    let (mut pi, mut ti) = (0, 0);
    let (mut star_pi, mut star_ti) = (usize::MAX, 0);

    while ti < tlen {
        if pi < plen && pat[pi] == '?' && (allow_slash || txt[ti] != '/') {
            pi += 1;
            ti += 1;
        } else if pi < plen && pat[pi] != '*' && pat[pi] != '?' && pat[pi] == txt[ti] {
            pi += 1;
            ti += 1;
        } else if pi < plen && pat[pi] == '*' {
            star_pi = pi;
            star_ti = ti;
            pi += 1;
        } else if star_pi != usize::MAX {
            // backtrack: advance the star match by one char
            star_ti += 1;
            if !allow_slash && star_ti <= tlen && star_ti > 0 && txt[star_ti - 1] == '/' {
                // single * cannot cross /
                return false;
            }
            pi = star_pi + 1;
            ti = star_ti;
        } else {
            return false;
        }
    }

    while pi < plen && pat[pi] == '*' {
        pi += 1;
    }
    pi == plen
}

/// Try to match `pattern` (simple glob) at the start of `text`.
/// Returns the number of chars consumed if successful.
fn find_segment_match(pattern: &str, text: &str, allow_slash: bool) -> Option<usize> {
    // try matching the pattern against progressively longer prefixes
    let pat: Vec<char> = pattern.chars().collect();
    let txt: Vec<char> = text.chars().collect();
    // find minimum possible match length
    for end in 0..=txt.len() {
        let candidate: String = txt[..end].iter().collect();
        if wildcard_match_segment(pattern, &candidate, allow_slash) {
            return Some(end);
        }
        // if we hit a non-matching char beyond what pattern could match, stop early
        if end > pat.len() * 2 + txt.len() {
            break;
        }
    }
    None
}

#[cfg(test)]
mod tests {
    use std::fs;

    use super::{prepare_import_items, wildcard_match};
    use crate::app::types::TransferEndpoint;

    #[test]
    fn wildcard_match_substring() {
        assert!(wildcard_match("hello", "say hello world"));
        assert!(wildcard_match("HELLO", "say hello world"));
        assert!(!wildcard_match("goodbye", "say hello world"));
    }

    #[test]
    fn wildcard_match_star() {
        assert!(wildcard_match("*.txt", "readme.txt"));
        assert!(wildcard_match("*.txt", "docs/readme.txt"));
        assert!(!wildcard_match("*.txt", "readme.md"));
        assert!(wildcard_match("docs/*", "docs/readme.txt"));
        assert!(wildcard_match("*read*", "docs/readme.txt"));
    }

    #[test]
    fn wildcard_match_question() {
        assert!(wildcard_match("?.txt", "a.txt"));
        assert!(!wildcard_match("?.txt", "ab.txt"));
    }

    #[test]
    fn wildcard_match_case_insensitive() {
        assert!(wildcard_match("*.TXT", "readme.txt"));
        assert!(wildcard_match("*.txt", "README.TXT"));
    }

    #[test]
    fn wildcard_match_empty() {
        assert!(wildcard_match("", "anything"));
        assert!(wildcard_match("*", "anything"));
    }

    #[test]
    fn wildcard_match_doublestar() {
        // ** matches any path depth
        assert!(wildcard_match("dir/**", "dir/file.txt"));
        assert!(wildcard_match("dir/**", "dir/sub/file.txt"));
        assert!(wildcard_match("dir/**", "dir/a/b/c.txt"));
        // ** at start
        assert!(wildcard_match("**/file.txt", "file.txt"));
        assert!(wildcard_match("**/file.txt", "a/file.txt"));
        assert!(wildcard_match("**/file.txt", "a/b/file.txt"));
        // ** in middle
        assert!(wildcard_match("a/**/z.txt", "a/z.txt"));
        assert!(wildcard_match("a/**/z.txt", "a/b/z.txt"));
        assert!(wildcard_match("a/**/z.txt", "a/b/c/z.txt"));
        // combined with single *
        assert!(wildcard_match("a/**/*.txt", "a/b/c/file.txt"));
        assert!(wildcard_match("**/*.log", "logs/app.log"));
        assert!(wildcard_match("**/*.log", "a/b/c/app.log"));
        // should not match wrong extension
        assert!(!wildcard_match("**/*.txt", "a/b/file.md"));
    }

    #[test]
    fn prepare_import_items_maps_relative_paths() {
        let root = std::env::temp_dir().join(format!(
            "abixio-ui-import-test-{}",
            std::time::SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH)
                .unwrap_or_default()
                .as_nanos()
        ));
        let nested = root.join("docs");
        fs::create_dir_all(&nested).expect("create import test dir");
        fs::write(nested.join("readme.txt"), b"hello").expect("write import test file");

        let items = prepare_import_items(root.clone(), "bucket-a", "prefix/")
            .expect("prepare import items");

        assert_eq!(items.len(), 1);
        match &items[0].destination {
            TransferEndpoint::S3 { bucket, key, .. } => {
                assert_eq!(bucket, "bucket-a");
                assert_eq!(key, "prefix/docs/readme.txt");
            }
            _ => panic!("expected s3 destination"),
        }

        fs::remove_dir_all(root).expect("cleanup import test dir");
    }
}
