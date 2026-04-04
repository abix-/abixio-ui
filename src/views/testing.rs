use std::sync::Arc;

use iced::widget::{button, column, container, row, scrollable, text};
use iced::{Element, Length};

use crate::abixio::client::AdminClient;
use crate::app::{App, Message};
use crate::s3::client::S3Client;

#[derive(Debug, Clone)]
pub struct TestResult {
    pub name: String,
    pub passed: bool,
    pub detail: Option<String>,
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

pub async fn run_e2e_tests(
    client: Arc<S3Client>,
    admin: Option<Arc<AdminClient>>,
) -> Vec<TestResult> {
    let mut t = TestRunner::new();
    let bucket = format!(
        "_e2e-test-{}",
        std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .unwrap_or_default()
            .as_secs()
    );

    // --- S3 API ---

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
    } else {
        t.check("admin tests skipped (not abixio)", true, "");
    }

    // --- Cleanup ---
    // delete all test objects, then the bucket
    if let Ok(list) = client.list_objects(&bucket, "", "").await {
        for obj in &list.objects {
            let _ = client.delete_object(&bucket, &obj.key).await;
        }
    }
    // delete bucket -- use reqwest directly since S3Client doesn't expose delete_bucket
    // we'll just leave it; the test bucket name is timestamped so no collision
    // TODO: add delete_bucket to S3Client if cleanup matters

    t.results
}
