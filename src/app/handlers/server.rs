use std::path::PathBuf;
use std::sync::Arc;

use iced::Task;

use crate::abixio::client::AdminClient;
use crate::server;

use super::super::{App, Message, Section, Selection};

impl App {
    pub(crate) fn handle_server_binary_path_changed(&mut self, v: String) -> Task<Message> {
        self.server_config.binary_path = v;
        self.server_binary_found = server::find_binary(&self.server_config.binary_path);
        Task::none()
    }

    pub(crate) fn handle_server_listen_changed(&mut self, v: String) -> Task<Message> {
        self.server_config.listen = v;
        Task::none()
    }

    pub(crate) fn handle_server_no_auth_toggled(&mut self, v: bool) -> Task<Message> {
        self.server_config.no_auth = v;
        Task::none()
    }

    pub(crate) fn handle_server_scan_interval_changed(&mut self, v: String) -> Task<Message> {
        self.server_config.scan_interval = v;
        Task::none()
    }

    pub(crate) fn handle_server_heal_interval_changed(&mut self, v: String) -> Task<Message> {
        self.server_config.heal_interval = v;
        Task::none()
    }

    pub(crate) fn handle_server_mrf_workers_changed(&mut self, v: String) -> Task<Message> {
        if let Ok(n) = v.parse::<usize>() {
            self.server_config.mrf_workers = n;
        }
        Task::none()
    }

    pub(crate) fn handle_server_auto_connect_toggled(&mut self, v: bool) -> Task<Message> {
        self.server_config.auto_connect = v;
        Task::none()
    }

    pub(crate) fn handle_server_add_volume(&mut self) -> Task<Message> {
        self.server_config.volumes.push(String::new());
        Task::none()
    }

    pub(crate) fn handle_server_remove_volume(&mut self, index: usize) -> Task<Message> {
        if index < self.server_config.volumes.len() {
            self.server_config.volumes.remove(index);
        }
        Task::none()
    }

    pub(crate) fn handle_server_volume_changed(
        &mut self,
        index: usize,
        value: String,
    ) -> Task<Message> {
        if index < self.server_config.volumes.len() {
            self.server_config.volumes[index] = value;
        }
        Task::none()
    }

    pub(crate) fn handle_server_pick_volume(&mut self, index: usize) -> Task<Message> {
        let idx = index;
        Task::perform(
            async move {
                let handle = rfd::AsyncFileDialog::new()
                    .set_title("Select volume directory")
                    .pick_folder()
                    .await;
                let path = handle.map(|h| h.path().to_path_buf());
                (idx, path)
            },
            |(idx, path)| Message::ServerVolumePathPicked(idx, path),
        )
    }

    pub(crate) fn handle_server_volume_path_picked(
        &mut self,
        index: usize,
        path: Option<PathBuf>,
    ) -> Task<Message> {
        if let Some(p) = path
            && index < self.server_config.volumes.len()
        {
            self.server_config.volumes[index] = p.to_string_lossy().to_string();
        }
        Task::none()
    }

    pub(crate) fn handle_start_server(&mut self) -> Task<Message> {
        if self.server_running {
            return Task::none();
        }

        // validate: need at least one volume
        let real_vols: Vec<_> = self
            .server_config
            .volumes
            .iter()
            .filter(|v| !v.trim().is_empty())
            .collect();
        if real_vols.is_empty() {
            self.error = Some("add at least one volume path before starting".to_string());
            return Task::none();
        }

        self.server_log.clear();

        match server::spawn(&self.server_config) {
            Ok((child, mut rx)) => {
                self.server_child = Some(child);
                self.server_running = true;
                self.server_log
                    .push("[ui] server starting...".to_string());

                // save config on successful start
                self.settings.server = self.server_config.clone();
                let _ = crate::config::save(&self.settings);

                // stream log lines into messages
                let log_task = Task::run(
                    iced::stream::channel(128, move |mut sender: iced::futures::channel::mpsc::Sender<Message>| async move {
                        while let Some(event) = rx.recv().await {
                            let msg = match event {
                                server::ServerEvent::Line(line) => Message::ServerLogLine(line),
                                server::ServerEvent::Exited(code) => Message::ServerExited(code),
                            };
                            if sender.try_send(msg).is_err() {
                                break;
                            }
                        }
                        // streams ended = process exited
                        let _ = sender.try_send(Message::ServerExited(None));
                    }),
                    std::convert::identity,
                );

                // auto-connect after a short delay
                let mut tasks = vec![log_task];
                if self.server_config.auto_connect {
                    let endpoint =
                        server::listen_to_endpoint(&self.server_config.listen);
                    let no_auth = self.server_config.no_auth;
                    tasks.push(Task::perform(
                        async move {
                            // give the server a moment to bind
                            tokio::time::sleep(std::time::Duration::from_millis(1500)).await;
                            (endpoint, no_auth)
                        },
                        move |(endpoint, no_auth)| {
                            Message::ServerAutoConnect(endpoint, no_auth)
                        },
                    ));
                }

                Task::batch(tasks)
            }
            Err(e) => {
                self.error = Some(e);
                Task::none()
            }
        }
    }

    pub(crate) fn handle_stop_server(&mut self) -> Task<Message> {
        if let Some(mut child) = self.server_child.take() {
            // kill_on_drop will handle it, but let's be explicit
            let _ = child.start_kill();
            self.server_log.push("[ui] server stopped.".to_string());
        }
        self.server_running = false;
        Task::none()
    }

    pub(crate) fn handle_server_log_line(&mut self, line: String) -> Task<Message> {
        self.server_log.push(line);
        // cap at 500 lines
        if self.server_log.len() > 500 {
            self.server_log.drain(..self.server_log.len() - 500);
        }
        Task::none()
    }

    pub(crate) fn handle_server_exited(&mut self, code: Option<i32>) -> Task<Message> {
        self.server_running = false;
        self.server_child = None;
        let msg = match code {
            Some(c) => format!("[ui] server exited with code {}", c),
            None => "[ui] server exited.".to_string(),
        };
        self.server_log.push(msg);
        Task::none()
    }

    pub(crate) fn handle_server_save_config(&mut self) -> Task<Message> {
        self.settings.server = self.server_config.clone();
        match crate::config::save(&self.settings) {
            Ok(()) => self.error = Some("server config saved.".to_string()),
            Err(e) => self.error = Some(format!("save failed: {}", e)),
        }
        Task::none()
    }

    pub(crate) fn handle_server_auto_connect(
        &mut self,
        endpoint: String,
        no_auth: bool,
    ) -> Task<Message> {
        if !self.server_running {
            return Task::none();
        }

        let creds: Option<(&str, &str)> = if no_auth { None } else { None };

        match crate::s3::client::S3Client::new(&endpoint, creds, "us-east-1") {
            Ok(client) => {
                self.perf.set_s3_stats(client.stats().clone());
                self.client = Arc::new(client);
                self.endpoint = endpoint.clone();
                self.active_connection = Some("(server)".to_string());
                self.section = Section::Browse;
                self.selection = Selection::None;
                self.buckets = None;
                self.objects = None;
                self.detail = None;
                self.selected_bucket = None;
                self.current_prefix.clear();
                self.object_filter.clear();
                self.selected_keys.clear();
                self.find_results = None;
                self.reset_bucket_document_states();
                self.bucket_tags = None;
                self.clear_object_admin_state();
                self.loading_buckets = true;
                self.is_abixio = false;
                self.server_status = None;
                self.disks_data = None;
                self.heal_data = None;

                let admin = Arc::new(AdminClient::new(&endpoint, creds, "us-east-1"));
                self.admin_client = Some(admin.clone());

                Task::batch(vec![
                    self.cmd_fetch_buckets(),
                    Task::perform(async move { admin.probe().await }, Message::AbixioDetected),
                ])
            }
            Err(e) => {
                self.server_log
                    .push(format!("[ui] auto-connect failed: {}", e));
                Task::none()
            }
        }
    }
}
