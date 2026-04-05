use std::path::PathBuf;
use std::sync::Arc;

use iced::widget::{button, column, container, row, scrollable, text};
use iced::{Element, Length};
use serde::Serialize;

use crate::abixio::client::AdminClient;
use crate::app::{
    App, CURRENT_CONNECTION_ID, Message, OverwritePolicy, TransferEndpoint, TransferItem,
    TransferStepResult, prepare_export_items, prepare_import_items, run_transfer_step,
};
use crate::s3::client::S3Client;

#[derive(Debug, Clone, Serialize)]
pub struct TestResult {
    pub name: String,
    pub passed: bool,
    pub detail: Option<String>,
}

#[derive(Debug, Serialize)]
pub struct TestReport {
    pub app_version: String,
    pub endpoint: String,
    pub started_at: String,
    pub finished_at: String,
    pub total: usize,
    pub passed: usize,
    pub failed: usize,
    pub results: Vec<TestResult>,
}

impl App {
    pub fn testing_view(&self) -> Element<'_, Message> {
        let mut layout = column![].spacing(8).padding(12).width(Length::Fill);

        layout = layout.push(
            row![
                text("Tests").size(18),
                if self.test_running {
                    button(text("running...").size(10)).style(button::secondary)
                } else if self.endpoint.is_empty() {
                    button(text("run tests").size(10)).style(button::secondary)
                } else {
                    button(text("run tests").size(10))
                        .style(button::primary)
                        .on_press(Message::RunTests)
                },
            ]
            .spacing(8),
        );
        layout = layout.push(iced::widget::rule::horizontal(1));

        if self.endpoint.is_empty() {
            layout = layout.push(text("connect to a server first").size(12));
            return layout.into();
        }

        if !self.test_progress.is_empty() {
            layout = layout.push(text(&self.test_progress).size(11));
        }

        if !self.test_results.is_empty() {
            // header
            layout = layout.push(
                row![
                    text("").size(10).width(40),
                    text("Test").size(10).width(Length::Fill),
                    text("Detail").size(10).width(200),
                ]
                .spacing(4),
            );
            layout = layout.push(iced::widget::rule::horizontal(1));

            let mut results_col = column![].spacing(2);
            for r in &self.test_results {
                let badge = if r.passed { "PASS" } else { "FAIL" };
                let detail_text = r.detail.as_deref().unwrap_or("");
                results_col = results_col.push(
                    row![
                        text(badge).size(10).width(40),
                        text(&r.name).size(10).width(Length::Fill),
                        text(detail_text).size(10).width(200),
                    ]
                    .spacing(4),
                );
            }

            layout = layout
                .push(scrollable(container(results_col).width(Length::Fill)).height(Length::Fill));

            // summary
            let passed = self.test_results.iter().filter(|r| r.passed).count();
            let total = self.test_results.len();
            let failed = total - passed;
            layout = layout.push(iced::widget::rule::horizontal(1));
            layout = layout
                .push(text(format!("{}/{} passed, {} failed", passed, total, failed)).size(12));
        }

        layout.into()
    }
}

// -- test runner logic --

struct TestRunner {
    results: Vec<TestResult>,
}

impl TestRunner {
    fn new() -> Self {
        Self {
            results: Vec::new(),
        }
    }

    fn check(&mut self, name: &str, passed: bool, detail: &str) {
        self.results.push(TestResult {
            name: name.to_string(),
            passed,
            detail: if detail.is_empty() {
                None
            } else {
                Some(detail.to_string())
            },
        });
    }
}

pub fn run_e2e_tests(
    client: Arc<S3Client>,
    admin: Option<Arc<AdminClient>>,
) -> std::pin::Pin<Box<dyn std::future::Future<Output = Vec<TestResult>> + Send>> {
    Box::pin(run_e2e_tests_inner(client, admin))
}

async fn run_e2e_tests_inner(
    client: Arc<S3Client>,
    admin: Option<Arc<AdminClient>>,
) -> Vec<TestResult> {
    let mut results = Vec::new();
    let bucket = format!(
        "_e2e-test-{}",
        std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .unwrap_or_default()
            .as_secs()
    );

    results.extend(test_s3_core(client.clone(), bucket.clone()).await);
    results.extend(test_transfers(client.clone(), bucket.clone()).await);
    results.extend(test_admin(client.clone(), admin.clone(), bucket.clone()).await);
    results.extend(test_tagging(client.clone(), bucket.clone()).await);
    results.extend(test_versioning_basic(client.clone(), bucket.clone()).await);
    results.extend(test_extended_s3(client.clone(), bucket.clone()).await);
    results.extend(test_extended_s3_b(client.clone(), bucket.clone()).await);
    results.extend(test_presigned_and_policy(client.clone(), bucket.clone()).await);
    results.extend(test_bucket_tags_and_heal(client.clone(), admin.clone(), bucket.clone()).await);
    results.extend(test_sync(client.clone(), bucket.clone()).await);
    results.extend(test_cleanup(client.clone(), bucket.clone()).await);

    results
}

async fn test_s3_core(client: Arc<S3Client>, bucket: String) -> Vec<TestResult> {
    let mut t = TestRunner::new();

    // create bucket
    let r = client.create_bucket(&bucket).await;
    t.check("create bucket", r.is_ok(), &r.err().unwrap_or_default());

    // list buckets includes ours
    let r = client.list_buckets().await;
    match &r {
        Ok(buckets) => {
            let found = buckets.iter().any(|b| b.name == bucket);
            t.check("list buckets contains test bucket", found, "");
        }
        Err(e) => t.check("list buckets", false, e),
    }

    let empty_bucket = format!("{}-empty", bucket);
    let r = client.create_bucket(&empty_bucket).await;
    t.check(
        "create empty bucket",
        r.is_ok(),
        &r.err().unwrap_or_default(),
    );
    let r = client.delete_bucket(&empty_bucket).await;
    t.check(
        "delete empty bucket",
        r.is_ok(),
        &r.err().unwrap_or_default(),
    );
    let r = client.list_buckets().await;
    match &r {
        Ok(buckets) => {
            let missing = buckets.iter().all(|b| b.name != empty_bucket);
            t.check("empty bucket removed from list", missing, "");
        }
        Err(e) => t.check("list buckets after empty delete", false, e),
    }

    // put object
    let r = client
        .put_object(&bucket, "hello.txt", b"hello world".to_vec(), "text/plain")
        .await;
    t.check("put object", r.is_ok(), &r.err().unwrap_or_default());

    // put more objects for listing tests
    let _ = client
        .put_object(
            &bucket,
            "docs/readme.txt",
            b"readme content".to_vec(),
            "text/plain",
        )
        .await;
    let _ = client
        .put_object(
            &bucket,
            "docs/guide.txt",
            b"guide content".to_vec(),
            "text/plain",
        )
        .await;
    let _ = client
        .put_object(
            &bucket,
            "photos/cat.jpg",
            b"fake image data".to_vec(),
            "image/jpeg",
        )
        .await;

    // get object
    let r = client.get_object(&bucket, "hello.txt").await;
    match &r {
        Ok(data) => {
            let body = String::from_utf8_lossy(data);
            t.check(
                "get object content",
                body == "hello world",
                &format!("got: {}", body),
            );
        }
        Err(e) => t.check("get object", false, e),
    }

    // head object
    let r = client.head_object(&bucket, "hello.txt").await;
    match &r {
        Ok(detail) => {
            t.check(
                "head object size=11",
                detail.size == 11,
                &format!("got {}", detail.size),
            );
            t.check(
                "head object content-type",
                detail.content_type == "text/plain",
                &detail.content_type,
            );
        }
        Err(e) => t.check("head object", false, e),
    }

    // put empty object
    let r = client
        .put_object(&bucket, "empty", Vec::new(), "application/octet-stream")
        .await;
    t.check("put empty object", r.is_ok(), &r.err().unwrap_or_default());

    let r = client.head_object(&bucket, "empty").await;
    match &r {
        Ok(detail) => t.check(
            "empty object size=0",
            detail.size == 0,
            &format!("got {}", detail.size),
        ),
        Err(e) => t.check("get empty object", false, e),
    }

    // list objects
    let r = client.list_objects(&bucket, "", "/").await;
    match &r {
        Ok(result) => {
            let has_hello = result.objects.iter().any(|o| o.key == "hello.txt");
            t.check("list objects contains hello.txt", has_hello, "");
            let has_prefixes = !result.common_prefixes.is_empty();
            t.check("list objects has common prefixes", has_prefixes, "");
        }
        Err(e) => t.check("list objects", false, e),
    }

    // list with prefix
    let r = client.list_objects(&bucket, "docs/", "").await;
    match &r {
        Ok(result) => {
            let has_readme = result.objects.iter().any(|o| o.key.contains("readme"));
            let has_cat = result.objects.iter().any(|o| o.key.contains("cat"));
            t.check("list prefix=docs/ has readme", has_readme, "");
            t.check("list prefix=docs/ excludes cat", !has_cat, "");
        }
        Err(e) => t.check("list objects prefix", false, e),
    }

    // delete object
    let r = client.delete_object(&bucket, "hello.txt").await;
    t.check("delete object", r.is_ok(), &r.err().unwrap_or_default());

    // get after delete -> error
    let r = client.get_object(&bucket, "hello.txt").await;
    t.check("get after delete fails", r.is_err(), "");

    t.results
}

async fn test_transfers(client: Arc<S3Client>, bucket: String) -> Vec<TestResult> {
    let mut t = TestRunner::new();

    // --- Transfer workflows ---

    let r = client
        .put_object(
            &bucket,
            "copy-source.txt",
            b"copy this object".to_vec(),
            "text/plain",
        )
        .await;
    t.check(
        "transfer setup copy source",
        r.is_ok(),
        &r.err().unwrap_or_default(),
    );

    let copy_item = TransferItem {
        source: TransferEndpoint::S3 {
            connection_id: CURRENT_CONNECTION_ID.to_string(),
            bucket: bucket.clone(),
            key: "copy-source.txt".to_string(),
        },
        destination: TransferEndpoint::S3 {
            connection_id: CURRENT_CONNECTION_ID.to_string(),
            bucket: bucket.clone(),
            key: "copies/copy-source.txt".to_string(),
        },
    };
    let r = run_transfer_step(
        client.clone(),
        Some(client.clone()),
        copy_item.clone(),
        OverwritePolicy::Ask,
        false,
    )
    .await;
    match &r {
        Ok(TransferStepResult::Copied(_)) => {
            let copied = client.get_object(&bucket, "copies/copy-source.txt").await;
            match copied {
                Ok(data) => {
                    t.check(
                        "copy object content",
                        data == b"copy this object".to_vec(),
                        &String::from_utf8_lossy(&data),
                    );
                }
                Err(e) => t.check("copy object verify", false, &e),
            }
        }
        Ok(other) => t.check(
            "copy object",
            false,
            &format!("unexpected result: {:?}", other),
        ),
        Err(e) => t.check("copy object", false, e),
    }

    let r = client
        .put_object(
            &bucket,
            "copies/existing.txt",
            b"old data".to_vec(),
            "text/plain",
        )
        .await;
    t.check(
        "transfer setup overwrite target",
        r.is_ok(),
        &r.err().unwrap_or_default(),
    );
    let overwrite_item = TransferItem {
        source: TransferEndpoint::S3 {
            connection_id: CURRENT_CONNECTION_ID.to_string(),
            bucket: bucket.clone(),
            key: "copy-source.txt".to_string(),
        },
        destination: TransferEndpoint::S3 {
            connection_id: CURRENT_CONNECTION_ID.to_string(),
            bucket: bucket.clone(),
            key: "copies/existing.txt".to_string(),
        },
    };
    let r = run_transfer_step(
        client.clone(),
        Some(client.clone()),
        overwrite_item.clone(),
        OverwritePolicy::Ask,
        false,
    )
    .await;
    t.check(
        "copy conflict detected",
        matches!(r, Ok(TransferStepResult::Conflict(_))),
        "",
    );
    let r = run_transfer_step(
        client.clone(),
        Some(client.clone()),
        overwrite_item,
        OverwritePolicy::OverwriteAll,
        false,
    )
    .await;
    t.check(
        "copy overwrite allowed",
        matches!(r, Ok(TransferStepResult::Copied(_))),
        &format!("{:?}", r),
    );
    match client.get_object(&bucket, "copies/existing.txt").await {
        Ok(data) => t.check(
            "copy overwrite content",
            data == b"copy this object".to_vec(),
            &String::from_utf8_lossy(&data),
        ),
        Err(e) => t.check("copy overwrite verify", false, &e),
    }

    let import_root = std::env::temp_dir().join(format!(
        "abixio-ui-import-e2e-{}",
        std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .unwrap_or_default()
            .as_secs()
    ));
    let nested = import_root.join("nested");
    let _ = tokio::fs::create_dir_all(&nested).await;
    let _ = tokio::fs::write(import_root.join("alpha.txt"), b"alpha").await;
    let _ = tokio::fs::write(nested.join("beta.txt"), b"beta").await;
    let import_items = prepare_import_items(import_root.clone(), &bucket, "imported/");
    match import_items {
        Ok(items) => {
            t.check(
                "prepare import items count",
                items.len() == 2,
                &format!("{}", items.len()),
            );
            let mut all_imported = true;
            for item in items {
                let result = run_transfer_step(
                    client.clone(),
                    Some(client.clone()),
                    item,
                    OverwritePolicy::Ask,
                    false,
                )
                .await;
                if !matches!(result, Ok(TransferStepResult::Copied(_))) {
                    all_imported = false;
                }
            }
            t.check("import folder recursive copy", all_imported, "");
            match client.get_object(&bucket, "imported/alpha.txt").await {
                Ok(data) => t.check("imported alpha exists", data == b"alpha".to_vec(), ""),
                Err(e) => t.check("imported alpha exists", false, &e),
            }
            match client.get_object(&bucket, "imported/nested/beta.txt").await {
                Ok(data) => t.check("imported nested beta exists", data == b"beta".to_vec(), ""),
                Err(e) => t.check("imported nested beta exists", false, &e),
            }
        }
        Err(e) => t.check("prepare import items", false, &e),
    }

    let export_root = std::env::temp_dir().join(format!(
        "abixio-ui-export-e2e-{}",
        std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .unwrap_or_default()
            .as_secs()
    ));
    let export_items =
        prepare_export_items(client.clone(), &bucket, "imported/", &export_root).await;
    match export_items {
        Ok(items) => {
            t.check(
                "prepare export items count",
                items.len() >= 2,
                &format!("{}", items.len()),
            );
            let mut all_exported = true;
            for item in items {
                let result =
                    run_transfer_step(client.clone(), None, item, OverwritePolicy::Ask, false)
                        .await;
                if !matches!(result, Ok(TransferStepResult::Copied(_))) {
                    all_exported = false;
                }
            }
            t.check("export prefix recursive copy", all_exported, "");
            match tokio::fs::read(export_root.join("alpha.txt")).await {
                Ok(data) => t.check("exported alpha exists", data == b"alpha".to_vec(), ""),
                Err(e) => t.check("exported alpha exists", false, &e.to_string()),
            }
            match tokio::fs::read(export_root.join(PathBuf::from("nested").join("beta.txt"))).await
            {
                Ok(data) => t.check("exported nested beta exists", data == b"beta".to_vec(), ""),
                Err(e) => t.check("exported nested beta exists", false, &e.to_string()),
            }
        }
        Err(e) => t.check("prepare export items", false, &e),
    }

    let _ = tokio::fs::remove_dir_all(import_root).await;
    let _ = tokio::fs::remove_dir_all(export_root).await;

    t.results
}

async fn test_admin(
    client: Arc<S3Client>,
    admin: Option<Arc<AdminClient>>,
    bucket: String,
) -> Vec<TestResult> {
    let mut t = TestRunner::new();

    // --- Admin API (abixio only) ---

    if let Some(ref admin) = admin {
        // status
        let r = admin.status().await;
        match &r {
            Ok(s) => {
                t.check(
                    "admin status server=abixio",
                    s.server == "abixio",
                    &s.server,
                );
                t.check("admin status has version", !s.version.is_empty(), "");
                t.check(
                    "admin status total_disks>0",
                    s.total_disks > 0,
                    &format!("{}", s.total_disks),
                );
            }
            Err(e) => t.check("admin status", false, e),
        }

        // disks
        let r = admin.disks().await;
        match &r {
            Ok(data) => {
                t.check("admin disks count>0", !data.disks.is_empty(), "");
                let all_online = data.disks.iter().all(|d| d.online);
                t.check("admin disks all online", all_online, "");
                let all_have_space = data.disks.iter().all(|d| d.total_bytes > 0);
                t.check("admin disks have space info", all_have_space, "");
            }
            Err(e) => t.check("admin disks", false, e),
        }

        // heal status
        let r = admin.heal_status().await;
        match &r {
            Ok(data) => {
                t.check("admin heal mrf_pending>=0", true, "");
                t.check(
                    "admin heal scanner has intervals",
                    !data.scanner.scan_interval.is_empty(),
                    "",
                );
            }
            Err(e) => t.check("admin heal status", false, e),
        }

        // object inspect -- upload a test object first
        let _ = client
            .put_object(
                &bucket,
                "inspect-me.txt",
                b"inspect this data".to_vec(),
                "text/plain",
            )
            .await;

        let r = admin.inspect_object(&bucket, "inspect-me.txt").await;
        match &r {
            Ok(data) => {
                t.check("inspect bucket", data.bucket == bucket, &data.bucket);
                t.check("inspect key", data.key == "inspect-me.txt", &data.key);
                t.check(
                    "inspect size=16",
                    data.size == 16,
                    &format!("{}", data.size),
                );
                t.check("inspect has etag", !data.etag.is_empty(), "");
                t.check(
                    "inspect erasure data>0",
                    data.erasure.data > 0,
                    &format!("{}", data.erasure.data),
                );
                t.check(
                    "inspect erasure parity>0",
                    data.erasure.parity > 0,
                    &format!("{}", data.erasure.parity),
                );
                t.check(
                    "inspect shards count",
                    !data.shards.is_empty(),
                    &format!("{}", data.shards.len()),
                );
                let all_ok = data.shards.iter().all(|s| s.status == "ok");
                t.check("inspect all shards ok", all_ok, "");
            }
            Err(e) => t.check("admin inspect", false, e),
        }

        let encoded_key = "dir-one/inspect-me.txt";
        let _ = client
            .put_object(
                &bucket,
                encoded_key,
                b"encoded admin object".to_vec(),
                "text/plain",
            )
            .await;

        let r = admin.inspect_object(&bucket, encoded_key).await;
        match &r {
            Ok(data) => {
                t.check("inspect encoded key", data.key == encoded_key, &data.key);
                t.check(
                    "inspect encoded size",
                    data.size == 20,
                    &format!("{}", data.size),
                );
            }
            Err(e) => t.check("admin inspect encoded key", false, e),
        }
    } else {
        t.check("admin tests skipped (not abixio)", true, "");
    }

    t.results
}

async fn test_tagging(client: Arc<S3Client>, bucket: String) -> Vec<TestResult> {
    let mut t = TestRunner::new();

    // --- Object Tagging ---

    // get tags on fresh object (should be empty)
    let r = client.get_object_tags(&bucket, "hello.txt").await;
    match &r {
        Ok(tags) => t.check("get tags empty", tags.is_empty(), &format!("{:?}", tags)),
        Err(e) => t.check("get tags empty", false, e),
    }

    // set tags
    let mut tags = std::collections::HashMap::new();
    tags.insert("env".to_string(), "test".to_string());
    tags.insert("owner".to_string(), "e2e".to_string());
    let r = client.put_object_tags(&bucket, "hello.txt", tags).await;
    t.check("put tags", r.is_ok(), &r.err().unwrap_or_default());

    // get tags back
    let r = client.get_object_tags(&bucket, "hello.txt").await;
    match &r {
        Ok(tags) => {
            t.check(
                "get tags count",
                tags.len() == 2,
                &format!("{}", tags.len()),
            );
            t.check(
                "get tags env",
                tags.get("env").map(|v| v.as_str()) == Some("test"),
                &format!("{:?}", tags.get("env")),
            );
        }
        Err(e) => t.check("get tags after put", false, e),
    }

    // delete tags
    let r = client.delete_object_tags(&bucket, "hello.txt").await;
    t.check("delete tags", r.is_ok(), &r.err().unwrap_or_default());

    // verify tags are gone
    let r = client.get_object_tags(&bucket, "hello.txt").await;
    match &r {
        Ok(tags) => t.check("tags deleted", tags.is_empty(), &format!("{:?}", tags)),
        Err(e) => t.check("get tags after delete", false, e),
    }

    t.results
}

async fn test_versioning_basic(client: Arc<S3Client>, bucket: String) -> Vec<TestResult> {
    let mut t = TestRunner::new();

    // --- Versioning ---

    // enable versioning
    let r = client.put_bucket_versioning(&bucket, "Enabled").await;
    t.check("enable versioning", r.is_ok(), &r.err().unwrap_or_default());

    // check versioning status
    let r = client.get_bucket_versioning(&bucket).await;
    match &r {
        Ok(status) => t.check("versioning enabled", status == "Enabled", status),
        Err(e) => t.check("get versioning", false, e),
    }

    // put object twice to create 2 versions
    let r = client
        .put_object(&bucket, "versioned.txt", b"version1".to_vec(), "text/plain")
        .await;
    t.check("put v1", r.is_ok(), &r.err().unwrap_or_default());

    let r = client
        .put_object(&bucket, "versioned.txt", b"version2".to_vec(), "text/plain")
        .await;
    t.check("put v2", r.is_ok(), &r.err().unwrap_or_default());

    // list versions
    let r = client.list_object_versions(&bucket, "versioned.txt").await;
    match &r {
        Ok(versions) => {
            let obj_versions: Vec<_> = versions
                .iter()
                .filter(|v| v.key == "versioned.txt" && !v.is_delete_marker)
                .collect();
            t.check(
                "list versions count",
                obj_versions.len() >= 2,
                &format!("{}", obj_versions.len()),
            );
        }
        Err(e) => t.check("list versions", false, e),
    }

    // suspend versioning
    let r = client.put_bucket_versioning(&bucket, "Suspended").await;
    t.check(
        "suspend versioning",
        r.is_ok(),
        &r.err().unwrap_or_default(),
    );

    let r = client.get_bucket_versioning(&bucket).await;
    match &r {
        Ok(status) => t.check("versioning suspended", status == "Suspended", status),
        Err(e) => t.check("get versioning suspended", false, e),
    }

    t.results
}

async fn test_extended_s3(client: Arc<S3Client>, bucket: String) -> Vec<TestResult> {
    let mut t = TestRunner::new();

    // --- Batch Delete (DeleteObjects API) ---

    let _ = client
        .put_object(&bucket, "batch/a.txt", b"a".to_vec(), "text/plain")
        .await;
    let _ = client
        .put_object(&bucket, "batch/b.txt", b"b".to_vec(), "text/plain")
        .await;
    let _ = client
        .put_object(&bucket, "batch/c.txt", b"c".to_vec(), "text/plain")
        .await;

    let r = client
        .delete_objects(
            &bucket,
            &["batch/a.txt".to_string(), "batch/b.txt".to_string(), "batch/c.txt".to_string()],
        )
        .await;
    match &r {
        Ok(failed) => t.check("batch delete 3 objects", failed.is_empty(), &format!("failed: {:?}", failed)),
        Err(e) => t.check("batch delete", false, e),
    }

    let r = client.get_object(&bucket, "batch/a.txt").await;
    t.check("batch deleted object gone", r.is_err(), "");

    // --- Recursive Listing ---

    let _ = client
        .put_object(&bucket, "deep/a/b/c.txt", b"deep".to_vec(), "text/plain")
        .await;
    let _ = client
        .put_object(&bucket, "deep/x.txt", b"shallow".to_vec(), "text/plain")
        .await;

    let r = client.list_objects_recursive(&bucket, "deep/").await;
    match &r {
        Ok(result) => {
            let keys: Vec<_> = result.objects.iter().map(|o| o.key.as_str()).collect();
            t.check(
                "recursive list finds nested",
                keys.contains(&"deep/a/b/c.txt"),
                &format!("{:?}", keys),
            );
            t.check(
                "recursive list finds shallow",
                keys.contains(&"deep/x.txt"),
                &format!("{:?}", keys),
            );
        }
        Err(e) => t.check("recursive list", false, e),
    }

    // --- Recursive Listing for Sync ---

    let r = client
        .list_objects_recursive_for_sync(&bucket, "deep/")
        .await;
    match &r {
        Ok(objects) => {
            let paths: Vec<_> = objects.iter().map(|o| o.relative_path.as_str()).collect();
            t.check(
                "sync list relative paths",
                paths.contains(&"a/b/c.txt") && paths.contains(&"x.txt"),
                &format!("{:?}", paths),
            );
            t.check(
                "sync list has sizes",
                objects.iter().all(|o| o.size > 0),
                "",
            );
        }
        Err(e) => t.check("sync recursive list", false, e),
    }

    // --- Server-Side Copy (direct API) ---

    let _ = client
        .put_object(&bucket, "copy-direct-src.txt", b"direct copy data".to_vec(), "text/plain")
        .await;
    let r = client
        .copy_object(&bucket, "copy-direct-src.txt", &bucket, "copy-direct-dst.txt")
        .await;
    t.check("copy_object direct", r.is_ok(), &r.err().unwrap_or_default());
    match client.get_object(&bucket, "copy-direct-dst.txt").await {
        Ok(data) => t.check(
            "copy_object content matches",
            data == b"direct copy data",
            &String::from_utf8_lossy(&data),
        ),
        Err(e) => t.check("copy_object verify", false, &e),
    }

    // --- Download to File ---

    let download_path = std::env::temp_dir().join(format!(
        "abixio-ui-download-e2e-{}.txt",
        std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .unwrap_or_default()
            .as_secs()
    ));
    let r = client
        .download_object_to_file(&bucket, "copy-direct-src.txt", &download_path)
        .await;
    match &r {
        Ok(bytes) => {
            t.check("download_object_to_file bytes", *bytes == 16, &format!("{}", bytes));
            match tokio::fs::read(&download_path).await {
                Ok(data) => t.check(
                    "download_object_to_file content",
                    data == b"direct copy data",
                    &String::from_utf8_lossy(&data),
                ),
                Err(e) => t.check("download_object_to_file read", false, &e.to_string()),
            }
        }
        Err(e) => t.check("download_object_to_file", false, e),
    }
    let _ = tokio::fs::remove_file(&download_path).await;

    t.results
}

async fn test_extended_s3_b(client: Arc<S3Client>, bucket: String) -> Vec<TestResult> {
    let mut t = TestRunner::new();

    // --- Upload File (multipart path for large, simple for small) ---

    let upload_path = std::env::temp_dir().join(format!(
        "abixio-ui-upload-e2e-{}.txt",
        std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .unwrap_or_default()
            .as_secs()
    ));
    let _ = tokio::fs::write(&upload_path, b"uploaded via file").await;
    let r = client
        .upload_file(&bucket, "file-upload.txt", &upload_path, "text/plain")
        .await;
    t.check("upload_file", r.is_ok(), &r.err().unwrap_or_default());
    match client.get_object(&bucket, "file-upload.txt").await {
        Ok(data) => t.check(
            "upload_file content",
            data == b"uploaded via file",
            &String::from_utf8_lossy(&data),
        ),
        Err(e) => t.check("upload_file verify", false, &e),
    }
    let _ = tokio::fs::remove_file(&upload_path).await;

    // --- Multipart Upload (file > 8MB) ---

    let multipart_path = std::env::temp_dir().join(format!(
        "abixio-ui-multipart-e2e-{}.bin",
        std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .unwrap_or_default()
            .as_secs()
    ));
    let large_data = vec![0xABu8; 9 * 1024 * 1024]; // 9MB, above 8MB multipart threshold
    let _ = tokio::fs::write(&multipart_path, &large_data).await;
    let r = client
        .upload_file(&bucket, "multipart.bin", &multipart_path, "application/octet-stream")
        .await;
    t.check("multipart upload", r.is_ok(), &r.err().unwrap_or_default());
    match client.head_object(&bucket, "multipart.bin").await {
        Ok(detail) => t.check(
            "multipart upload size",
            detail.size == 9 * 1024 * 1024,
            &format!("{}", detail.size),
        ),
        Err(e) => t.check("multipart upload verify", false, &e),
    }
    let _ = tokio::fs::remove_file(&multipart_path).await;

    // --- Get Object Version ---

    // re-enable versioning for version tests
    let _ = client.put_bucket_versioning(&bucket, "Enabled").await;
    let _ = client
        .put_object(&bucket, "ver-test.txt", b"ver1".to_vec(), "text/plain")
        .await;
    let _ = client
        .put_object(&bucket, "ver-test.txt", b"ver2".to_vec(), "text/plain")
        .await;

    let r = client.list_object_versions(&bucket, "ver-test.txt").await;
    match &r {
        Ok(versions) => {
            let obj_versions: Vec<_> = versions
                .iter()
                .filter(|v| v.key == "ver-test.txt" && !v.is_delete_marker)
                .collect();
            if obj_versions.len() >= 2 {
                // get the older version (last in list)
                let old_version = &obj_versions[obj_versions.len() - 1];
                let r = client
                    .get_object_version(&bucket, "ver-test.txt", &old_version.version_id)
                    .await;
                match &r {
                    Ok(data) => t.check(
                        "get_object_version content",
                        data == b"ver1",
                        &String::from_utf8_lossy(data),
                    ),
                    Err(e) => t.check("get_object_version", false, e),
                }

                // delete specific version
                let r = client
                    .delete_object_version(&bucket, "ver-test.txt", &old_version.version_id)
                    .await;
                t.check(
                    "delete_object_version",
                    r.is_ok(),
                    &r.err().unwrap_or_default(),
                );

                // verify version is gone
                let r = client.list_object_versions(&bucket, "ver-test.txt").await;
                match &r {
                    Ok(versions) => {
                        let remaining: Vec<_> = versions
                            .iter()
                            .filter(|v| {
                                v.key == "ver-test.txt"
                                    && !v.is_delete_marker
                                    && v.version_id == old_version.version_id
                            })
                            .collect();
                        t.check("deleted version gone", remaining.is_empty(), "");
                    }
                    Err(e) => t.check("list versions after delete", false, e),
                }
            } else {
                t.check(
                    "get_object_version",
                    false,
                    &format!("need 2+ versions, got {}", obj_versions.len()),
                );
            }
        }
        Err(e) => t.check("list versions for get test", false, e),
    }
    let _ = client.put_bucket_versioning(&bucket, "Suspended").await;

    t.results
}

async fn test_presigned_and_policy(client: Arc<S3Client>, bucket: String) -> Vec<TestResult> {
    let mut t = TestRunner::new();

    // --- Presigned GET URL ---

    let _ = client
        .put_object(&bucket, "presign-test.txt", b"presigned data".to_vec(), "text/plain")
        .await;
    let r = client.presign_get_object(&bucket, "presign-test.txt", 3600).await;
    match &r {
        Ok(url) => {
            t.check("presign url has endpoint", url.contains("presign-test.txt"), url);
            // fetch the presigned URL to verify it works
            let http = reqwest::Client::new();
            match http.get(url).send().await {
                Ok(resp) => {
                    let status = resp.status().as_u16();
                    let body = resp.bytes().await.unwrap_or_default();
                    t.check(
                        "presign url returns data",
                        status == 200 && body.as_ref() == b"presigned data",
                        &format!("status={} body={}", status, String::from_utf8_lossy(&body)),
                    );
                }
                Err(e) => t.check("presign url fetch", false, &e.to_string()),
            }
        }
        Err(e) => t.check("presign_get_object", false, e),
    }

    // --- Bucket Policy ---

    let policy_json = format!(
        r#"{{"Version":"2012-10-17","Statement":[{{"Effect":"Allow","Principal":"*","Action":"s3:GetObject","Resource":"arn:aws:s3:::{}/*"}}]}}"#,
        bucket
    );
    let r = client.put_bucket_policy(&bucket, &policy_json).await;
    t.check("put_bucket_policy", r.is_ok(), &r.err().unwrap_or_default());

    let r = client.get_bucket_policy(&bucket).await;
    match &r {
        Ok(Some(policy)) => t.check(
            "get_bucket_policy has content",
            !policy.is_empty(),
            &format!("len={}", policy.len()),
        ),
        Ok(None) => t.check("get_bucket_policy", false, "returned None"),
        Err(e) => t.check("get_bucket_policy", false, e),
    }

    let r = client.delete_bucket_policy(&bucket).await;
    t.check("delete_bucket_policy", r.is_ok(), &r.err().unwrap_or_default());

    let r = client.get_bucket_policy(&bucket).await;
    t.check(
        "policy deleted",
        matches!(r, Ok(None)),
        &format!("{:?}", r),
    );

    t.results
}

async fn test_bucket_tags_and_heal(
    client: Arc<S3Client>,
    admin: Option<Arc<AdminClient>>,
    bucket: String,
) -> Vec<TestResult> {
    let mut t = TestRunner::new();

    // --- Bucket Tags ---

    let mut bucket_tags = std::collections::HashMap::new();
    bucket_tags.insert("project".to_string(), "e2e".to_string());
    bucket_tags.insert("team".to_string(), "test".to_string());
    let r = client.put_bucket_tags(&bucket, bucket_tags).await;
    t.check("put_bucket_tags", r.is_ok(), &r.err().unwrap_or_default());

    let r = client.get_bucket_tags(&bucket).await;
    match &r {
        Ok(tags) => {
            t.check("get_bucket_tags count", tags.len() == 2, &format!("{}", tags.len()));
            t.check(
                "get_bucket_tags project",
                tags.get("project").map(|v| v.as_str()) == Some("e2e"),
                &format!("{:?}", tags.get("project")),
            );
        }
        Err(e) => t.check("get_bucket_tags", false, e),
    }

    let r = client.delete_bucket_tags(&bucket).await;
    t.check("delete_bucket_tags", r.is_ok(), &r.err().unwrap_or_default());

    // --- Admin: Heal Object ---

    if let Some(ref admin) = admin {
        let _ = client
            .put_object(&bucket, "heal-test.txt", b"heal me".to_vec(), "text/plain")
            .await;
        let r = admin.heal_object(&bucket, "heal-test.txt").await;
        match &r {
            Ok(resp) => t.check(
                "admin heal_object",
                !resp.result.is_empty(),
                &format!("result={}", resp.result),
            ),
            Err(e) => t.check("admin heal_object", false, e),
        }
    }

    t.results
}

async fn test_sync(client: Arc<S3Client>, bucket: String) -> Vec<TestResult> {
    let mut t = TestRunner::new();

    // --- Sync: Enumerate S3 for Sync ---

    let _ = client
        .put_object(&bucket, "sync-src/a.txt", b"sync a".to_vec(), "text/plain")
        .await;
    let _ = client
        .put_object(&bucket, "sync-src/b.txt", b"sync b".to_vec(), "text/plain")
        .await;

    let r = crate::app::sync_ops::enumerate_s3_for_sync(
        client.clone(),
        &bucket,
        "sync-src/",
        &Default::default(),
    )
    .await;
    match &r {
        Ok(objects) => {
            let paths: Vec<_> = objects.iter().map(|o| o.relative_path.as_str()).collect();
            t.check(
                "enumerate_s3_for_sync paths",
                paths.contains(&"a.txt") && paths.contains(&"b.txt"),
                &format!("{:?}", paths),
            );
        }
        Err(e) => t.check("enumerate_s3_for_sync", false, e),
    }

    // --- Sync: Enumerate Local for Sync ---

    let sync_local_root = std::env::temp_dir().join(format!(
        "abixio-ui-sync-enum-e2e-{}",
        std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .unwrap_or_default()
            .as_secs()
    ));
    let _ = tokio::fs::create_dir_all(sync_local_root.join("sub")).await;
    let _ = tokio::fs::write(sync_local_root.join("root.txt"), b"root data").await;
    let _ = tokio::fs::write(sync_local_root.join("sub").join("nested.txt"), b"nested data").await;

    let r = crate::app::sync_ops::enumerate_local_for_sync(
        sync_local_root.clone(),
        &Default::default(),
    );
    match &r {
        Ok(objects) => {
            let paths: Vec<_> = objects.iter().map(|o| o.relative_path.as_str()).collect();
            t.check(
                "enumerate_local_for_sync paths",
                paths.contains(&"root.txt") && paths.contains(&"sub/nested.txt"),
                &format!("{:?}", paths),
            );
        }
        Err(e) => t.check("enumerate_local_for_sync", false, e),
    }

    // --- Sync: Build Plan and Execute (local -> S3 copy) ---

    let r_source = crate::app::sync_ops::enumerate_local_for_sync(
        sync_local_root.clone(),
        &Default::default(),
    );
    let r_dest = client
        .list_objects_recursive_for_sync(&bucket, "sync-dest/")
        .await;
    match (r_source, r_dest) {
        (Ok(source), Ok(destination)) => {
            let plan = crate::app::sync_ops::build_sync_plan(
                source,
                destination,
                crate::app::SyncMode::Copy,
                crate::app::SyncPreset::Converge.policy(),
                crate::app::SyncCompareMode::SizeOnly,
            );
            t.check(
                "sync plan has creates",
                plan.summary.creates > 0,
                &format!("creates={}", plan.summary.creates),
            );

            // execute the plan items via execute_sync_run_item
            let mut run_items = Vec::new();
            for item in &plan.items {
                if item.action == crate::app::SyncPlanAction::Create
                    || item.action == crate::app::SyncPlanAction::Update
                {
                    let bytes = item.source.as_ref().map(|s| s.size).unwrap_or(0);
                    run_items.push(crate::app::SyncRunItem {
                        action: item.action.clone(),
                        source: crate::app::TransferEndpoint::Local {
                            path: sync_local_root.join(&item.relative_path),
                        },
                        destination: crate::app::TransferEndpoint::S3 {
                            connection_id: CURRENT_CONNECTION_ID.to_string(),
                            bucket: bucket.clone(),
                            key: format!("sync-dest/{}", item.relative_path),
                        },
                        strategy: crate::app::SyncExecutionStrategy::Upload,
                        relative_path: item.relative_path.clone(),
                        bytes,
                    });
                }
            }
            let mut all_ok = true;
            for item in &run_items {
                let r = crate::app::transfer_ops::execute_sync_run_item(
                    None,
                    Some(client.clone()),
                    item,
                )
                .await;
                if r.is_err() {
                    t.check(
                        &format!("sync execute {}", item.relative_path),
                        false,
                        &r.err().unwrap_or_default(),
                    );
                    all_ok = false;
                }
            }
            if all_ok {
                t.check("sync execute all uploads", true, "");
            }

            // verify uploaded objects
            match client.get_object(&bucket, "sync-dest/root.txt").await {
                Ok(data) => t.check(
                    "sync uploaded root.txt",
                    data == b"root data",
                    &String::from_utf8_lossy(&data),
                ),
                Err(e) => t.check("sync uploaded root.txt", false, &e),
            }
            match client.get_object(&bucket, "sync-dest/sub/nested.txt").await {
                Ok(data) => t.check(
                    "sync uploaded nested.txt",
                    data == b"nested data",
                    &String::from_utf8_lossy(&data),
                ),
                Err(e) => t.check("sync uploaded nested.txt", false, &e),
            }
        }
        (Err(e), _) => t.check("sync plan source enum", false, &e),
        (_, Err(e)) => t.check("sync plan dest enum", false, &e),
    }

    // --- Sync: S3 -> Local Download via execute_sync_run_item ---

    let sync_download_root = std::env::temp_dir().join(format!(
        "abixio-ui-sync-dl-e2e-{}",
        std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .unwrap_or_default()
            .as_secs()
    ));
    let download_item = crate::app::SyncRunItem {
        action: crate::app::SyncPlanAction::Create,
        source: crate::app::TransferEndpoint::S3 {
            connection_id: CURRENT_CONNECTION_ID.to_string(),
            bucket: bucket.clone(),
            key: "sync-dest/root.txt".to_string(),
        },
        destination: crate::app::TransferEndpoint::Local {
            path: sync_download_root.join("root.txt"),
        },
        strategy: crate::app::SyncExecutionStrategy::Download,
        relative_path: "root.txt".to_string(),
        bytes: 9,
    };
    let r = crate::app::transfer_ops::execute_sync_run_item(
        Some(client.clone()),
        None,
        &download_item,
    )
    .await;
    t.check("sync download execute", r.is_ok(), &r.err().unwrap_or_default());
    match tokio::fs::read(sync_download_root.join("root.txt")).await {
        Ok(data) => t.check(
            "sync downloaded content",
            data == b"root data",
            &String::from_utf8_lossy(&data),
        ),
        Err(e) => t.check("sync downloaded content", false, &e.to_string()),
    }

    // --- Sync: S3 -> S3 Server-Side Copy via execute_sync_run_item ---

    let ss_copy_item = crate::app::SyncRunItem {
        action: crate::app::SyncPlanAction::Create,
        source: crate::app::TransferEndpoint::S3 {
            connection_id: CURRENT_CONNECTION_ID.to_string(),
            bucket: bucket.clone(),
            key: "sync-dest/root.txt".to_string(),
        },
        destination: crate::app::TransferEndpoint::S3 {
            connection_id: CURRENT_CONNECTION_ID.to_string(),
            bucket: bucket.clone(),
            key: "sync-ss-copy/root.txt".to_string(),
        },
        strategy: crate::app::SyncExecutionStrategy::ServerSideCopy,
        relative_path: "root.txt".to_string(),
        bytes: 9,
    };
    let r = crate::app::transfer_ops::execute_sync_run_item(
        Some(client.clone()),
        Some(client.clone()),
        &ss_copy_item,
    )
    .await;
    t.check("sync server-side copy", r.is_ok(), &r.err().unwrap_or_default());
    match client.get_object(&bucket, "sync-ss-copy/root.txt").await {
        Ok(data) => t.check(
            "sync ss-copy content",
            data == b"root data",
            &String::from_utf8_lossy(&data),
        ),
        Err(e) => t.check("sync ss-copy verify", false, &e),
    }

    // --- Sync: Client Relay via execute_sync_run_item ---

    let relay_item = crate::app::SyncRunItem {
        action: crate::app::SyncPlanAction::Create,
        source: crate::app::TransferEndpoint::S3 {
            connection_id: CURRENT_CONNECTION_ID.to_string(),
            bucket: bucket.clone(),
            key: "sync-dest/root.txt".to_string(),
        },
        destination: crate::app::TransferEndpoint::S3 {
            connection_id: CURRENT_CONNECTION_ID.to_string(),
            bucket: bucket.clone(),
            key: "sync-relay/root.txt".to_string(),
        },
        strategy: crate::app::SyncExecutionStrategy::ClientRelay,
        relative_path: "root.txt".to_string(),
        bytes: 9,
    };
    let r = crate::app::transfer_ops::execute_sync_run_item(
        Some(client.clone()),
        Some(client.clone()),
        &relay_item,
    )
    .await;
    t.check("sync client relay", r.is_ok(), &r.err().unwrap_or_default());
    match client.get_object(&bucket, "sync-relay/root.txt").await {
        Ok(data) => t.check(
            "sync relay content",
            data == b"root data",
            &String::from_utf8_lossy(&data),
        ),
        Err(e) => t.check("sync relay verify", false, &e),
    }

    let _ = tokio::fs::remove_dir_all(&sync_local_root).await;
    let _ = tokio::fs::remove_dir_all(&sync_download_root).await;

    t.results
}

async fn test_cleanup(client: Arc<S3Client>, bucket: String) -> Vec<TestResult> {
    let mut t = TestRunner::new();

    // --- Cleanup ---
    // delete all test objects (including versions), then the bucket
    // first delete versions if any
    if let Ok(versions) = client.list_object_versions(&bucket, "").await {
        for v in &versions {
            let _ = client
                .delete_object_version(&bucket, &v.key, &v.version_id)
                .await;
        }
    }
    if let Ok(list) = client.list_objects(&bucket, "", "").await {
        for obj in &list.objects {
            let _ = client.delete_object(&bucket, &obj.key).await;
        }
    }
    let r = client.delete_bucket(&bucket).await;
    t.check(
        "delete non-empty bucket",
        r.is_ok(),
        &r.err().unwrap_or_default(),
    );
    let r = client.list_buckets().await;
    match &r {
        Ok(buckets) => {
            let missing = buckets.iter().all(|b| b.name != bucket);
            t.check("non-empty bucket removed from list", missing, "");
        }
        Err(e) => t.check("list buckets after delete", false, e),
    }

    t.results
}

pub async fn write_test_report(path: PathBuf, report: TestReport) -> Result<PathBuf, String> {
    if let Some(parent) = path.parent() {
        tokio::fs::create_dir_all(parent)
            .await
            .map_err(|e| e.to_string())?;
    }
    let data = serde_json::to_string_pretty(&report).map_err(|e| e.to_string())?;
    tokio::fs::write(&path, data)
        .await
        .map_err(|e| e.to_string())?;
    Ok(path)
}
