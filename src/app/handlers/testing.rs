use std::path::PathBuf;

use iced::Task;

use crate::views::testing::TestResult;

use super::super::transfer_ops::now_rfc3339;
use super::super::{App, Message};

impl App {
    pub(crate) fn handle_run_tests(&mut self) -> Task<Message> {
        self.begin_tests()
    }

    pub(crate) fn handle_tests_complete(&mut self, results: Vec<TestResult>) -> Task<Message> {
        self.test_running = false;
        let passed = results.iter().filter(|r| r.passed).count();
        let total = results.len();
        self.test_progress = format!("done: {}/{} passed", passed, total);
        self.test_results = results;
        if let Some(path) = self.test_report_path.clone() {
            let report = crate::views::testing::TestReport {
                app_version: env!("CARGO_PKG_VERSION").to_string(),
                endpoint: self.endpoint.clone(),
                started_at: self.test_started_at.clone().unwrap_or_else(now_rfc3339),
                finished_at: now_rfc3339(),
                total,
                passed,
                failed: total - passed,
                results: self.test_results.clone(),
            };
            Task::perform(
                async move { crate::views::testing::write_test_report(path, report).await },
                Message::TestReportWritten,
            )
        } else {
            Task::none()
        }
    }

    pub(crate) fn handle_auto_start_tests(&mut self) -> Task<Message> {
        if self.auto_run_tests && !self.auto_test_started {
            self.auto_test_started = true;
            self.begin_tests()
        } else {
            Task::none()
        }
    }

    pub(crate) fn handle_test_report_written(
        &mut self,
        result: Result<PathBuf, String>,
    ) -> Task<Message> {
        match result {
            Ok(path) => {
                println!("{}", path.display());
                Task::none()
            }
            Err(error) => {
                self.error = Some(format!("Failed to write test report: {}", error));
                Task::none()
            }
        }
    }

    pub(crate) fn begin_tests(&mut self) -> Task<Message> {
        if self.test_running || self.endpoint.is_empty() {
            return Task::none();
        }
        self.test_running = true;
        self.test_results.clear();
        self.test_progress = "running tests...".to_string();
        self.test_started_at = Some(now_rfc3339());
        let client = self.client.clone();
        let admin = if self.is_abixio {
            self.admin_client.clone()
        } else {
            None
        };
        Task::perform(
            async move { crate::views::testing::run_e2e_tests(client, admin).await },
            Message::TestsComplete,
        )
    }
}
