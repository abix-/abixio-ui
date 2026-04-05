use std::sync::Arc;

use iced::Task;

use crate::abixio::client::AdminClient;
use crate::config::{self, Connection};
use crate::s3::client::S3Client;

use super::super::{App, Message, Section, Selection};

impl App {
    pub(crate) fn handle_connect_to(&mut self, name: String) -> Task<Message> {
        let conn = match self.settings.connections.iter().find(|c| c.name == name) {
            Some(c) => c.clone(),
            None => {
                self.error = Some(format!("connection '{}' not found", name));
                return Task::none();
            }
        };

        let creds = match conn.resolve_keys() {
            Ok(keys) => keys,
            Err(e) => {
                self.error = Some(format!("keychain error: {}", e));
                return Task::none();
            }
        };

        match S3Client::new(
            &conn.endpoint,
            creds.as_ref().map(|(a, s)| (a.as_str(), s.as_str())),
            &conn.region,
        ) {
            Ok(client) => {
                self.perf.set_s3_stats(client.stats().clone());
                self.client = Arc::new(client);
                self.endpoint = conn.endpoint.clone();
                self.active_connection = Some(name);
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
                self.clear_object_admin_state();
                self.loading_buckets = true;
                self.is_abixio = false;
                self.server_status = None;
                self.disks_data = None;
                self.heal_data = None;

                // create admin client and probe for AbixIO
                let admin = Arc::new(AdminClient::new(
                    &conn.endpoint,
                    creds.as_ref().map(|(a, s)| (a.as_str(), s.as_str())),
                    &conn.region,
                ));
                self.admin_client = Some(admin.clone());

                Task::batch(vec![
                    self.cmd_fetch_buckets(),
                    Task::perform(async move { admin.probe().await }, Message::AbixioDetected),
                ])
            }
            Err(e) => {
                self.error = Some(format!("connect failed: {}", e));
                Task::none()
            }
        }
    }

    pub(crate) fn handle_add_connection(&mut self) -> Task<Message> {
        let name = self.new_conn_name.trim().to_string();
        let endpoint = self.new_conn_endpoint.trim().to_string();
        let region = self.new_conn_region.trim().to_string();
        let access_key = self.new_conn_access_key.trim().to_string();
        let secret_key = self.new_conn_secret_key.clone();

        if name.is_empty() || endpoint.is_empty() {
            self.error = Some("name and endpoint are required".to_string());
            return Task::none();
        }
        if !config::is_valid_name(&name) {
            self.error = Some(
                "name must start with a letter, only alphanumeric/dash/underscore".to_string(),
            );
            return Task::none();
        }
        if !config::is_valid_endpoint(&endpoint) {
            self.error = Some("endpoint must start with http:// or https://".to_string());
            return Task::none();
        }
        // if one key is provided, both must be
        if access_key.is_empty() != secret_key.is_empty() {
            self.error = Some("provide both access key and secret key, or neither".to_string());
            return Task::none();
        }
        if !config::is_valid_access_key(&access_key) {
            self.error = Some("access key must be at least 3 characters".to_string());
            return Task::none();
        }
        if !config::is_valid_secret_key(&secret_key) {
            self.error = Some("secret key must be at least 8 characters".to_string());
            return Task::none();
        }

        let conn = Connection {
            name,
            endpoint,
            region: if region.is_empty() {
                "us-east-1".to_string()
            } else {
                region
            },
        };

        if let Err(e) = config::add_connection(&mut self.settings, conn, &access_key, &secret_key) {
            self.error = Some(format!("save failed: {}", e));
        } else {
            self.new_conn_name.clear();
            self.new_conn_endpoint.clear();
            self.new_conn_region = "us-east-1".to_string();
            self.new_conn_access_key.clear();
            self.new_conn_secret_key.clear();
            self.editing_connection = None;
        }
        Task::none()
    }

    pub(crate) fn handle_edit_connection(&mut self, name: String) -> Task<Message> {
        if let Some(conn) = self.settings.connections.iter().find(|c| c.name == name) {
            self.new_conn_name = conn.name.clone();
            self.new_conn_endpoint = conn.endpoint.clone();
            self.new_conn_region = conn.region.clone();
            self.new_conn_access_key.clear();
            self.new_conn_secret_key.clear();
            self.editing_connection = Some(name);
        }
        Task::none()
    }

    pub(crate) fn handle_test_connection(&mut self, name: String) -> Task<Message> {
        let conn = match self.settings.connections.iter().find(|c| c.name == name) {
            Some(c) => c.clone(),
            None => return Task::none(),
        };
        let creds = match conn.resolve_keys() {
            Ok(keys) => keys,
            Err(e) => {
                self.error = Some(format!("keychain error: {}", e));
                return Task::none();
            }
        };
        let client = match S3Client::new(
            &conn.endpoint,
            creds.as_ref().map(|(a, s)| (a.as_str(), s.as_str())),
            &conn.region,
        ) {
            Ok(c) => Arc::new(c),
            Err(e) => {
                self.error = Some(format!("test failed: {}", e));
                return Task::none();
            }
        };
        let conn_name = name.clone();
        Task::perform(
            async move { client.list_buckets().await.map(|_| ()) },
            move |result| Message::TestConnectionResult(conn_name.clone(), result),
        )
    }

    pub(crate) fn handle_test_connection_result(
        &mut self,
        name: String,
        result: Result<(), String>,
    ) -> Task<Message> {
        match result {
            Ok(()) => self.error = Some(format!("'{}': connection ok", name)),
            Err(e) => self.error = Some(format!("'{}': {}", name, e)),
        }
        Task::none()
    }

    pub(crate) fn handle_remove_connection(&mut self, name: String) -> Task<Message> {
        if let Err(e) = config::remove_connection(&mut self.settings, &name) {
            self.error = Some(format!("remove failed: {}", e));
        }
        if self.active_connection.as_deref() == Some(&name) {
            self.active_connection = None;
        }
        Task::none()
    }

    pub(crate) fn handle_new_conn_name_changed(&mut self, v: String) -> Task<Message> {
        self.new_conn_name = v;
        Task::none()
    }

    pub(crate) fn handle_new_conn_endpoint_changed(&mut self, v: String) -> Task<Message> {
        self.new_conn_endpoint = v;
        Task::none()
    }

    pub(crate) fn handle_new_conn_region_changed(&mut self, v: String) -> Task<Message> {
        self.new_conn_region = v;
        Task::none()
    }

    pub(crate) fn handle_new_conn_access_key_changed(&mut self, v: String) -> Task<Message> {
        self.new_conn_access_key = v;
        Task::none()
    }

    pub(crate) fn handle_new_conn_secret_key_changed(&mut self, v: String) -> Task<Message> {
        self.new_conn_secret_key = v;
        Task::none()
    }
}
