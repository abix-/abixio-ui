use std::path::{Path, PathBuf};
use std::process::{Command, Stdio};
use std::time::{Duration, Instant};

pub fn run_status(mut cmd: Command, purpose: &str) {
    let status = cmd
        .status()
        .unwrap_or_else(|e| panic!("failed to run {}: {}", purpose, e));
    assert!(status.success(), "{} failed with status {}", purpose, status);
}

pub fn measure_cli_overhead(bin: &str, args: &[&str], n: usize) -> Duration {
    for _ in 0..3 {
        let _ = Command::new(bin)
            .args(args)
            .stdout(Stdio::null())
            .stderr(Stdio::null())
            .status();
    }
    let start = Instant::now();
    for _ in 0..n {
        let _ = Command::new(bin)
            .args(args)
            .stdout(Stdio::null())
            .stderr(Stdio::null())
            .status();
    }
    start.elapsed() / n as u32
}

// -- AWS CLI --

pub struct AwsCliHarness {
    aws: String,
    _temp: tempfile::TempDir,
    config_path: PathBuf,
    credentials_path: PathBuf,
    pub ca_bundle_path: PathBuf,
}

impl AwsCliHarness {
    pub fn new(aws: String, ca_bundle_path: &Path, access_key: &str, secret_key: &str) -> Self {
        let temp = tempfile::TempDir::new().expect("create aws cli tempdir");
        let config_path = temp.path().join("config");
        let credentials_path = temp.path().join("credentials");
        let ca_bundle_copy = temp.path().join("ca.pem");
        std::fs::copy(ca_bundle_path, &ca_bundle_copy).expect("copy CA bundle");

        let config = format!(
            "[profile bench]\nregion = us-east-1\nca_bundle = {}\ns3 =\n    addressing_style = path\n    payload_signing_enabled = false\n",
            ca_bundle_copy.display()
        );
        let credentials = format!(
            "[bench]\naws_access_key_id = {}\naws_secret_access_key = {}\n",
            access_key, secret_key
        );
        std::fs::write(&config_path, config).expect("write aws config");
        std::fs::write(&credentials_path, credentials).expect("write aws credentials");

        Self {
            aws,
            _temp: temp,
            config_path,
            credentials_path,
            ca_bundle_path: ca_bundle_copy,
        }
    }

    pub fn command(&self) -> Command {
        let mut cmd = Command::new(&self.aws);
        cmd.env("AWS_CONFIG_FILE", &self.config_path)
            .env("AWS_SHARED_CREDENTIALS_FILE", &self.credentials_path)
            .env("AWS_PROFILE", "bench")
            .env("AWS_EC2_METADATA_DISABLED", "true")
            .arg("--no-cli-pager");
        cmd
    }

    pub fn create_bucket(&self, endpoint: &str, bucket: &str) {
        let mut cmd = self.command();
        cmd.args(["--endpoint-url", endpoint, "--ca-bundle"])
            .arg(&self.ca_bundle_path)
            .args(["s3api", "create-bucket", "--bucket", bucket]);
        run_status(cmd, "aws create-bucket");
    }

    pub fn measure_overhead(&self, endpoint: &str, n: usize) -> Duration {
        measure_cli_overhead(
            &self.aws,
            &[
                "--no-cli-pager",
                "--endpoint-url", endpoint,
                "--ca-bundle", self.ca_bundle_path.to_str().expect("ca path utf8"),
                "s3api", "list-buckets",
            ],
            n,
        )
    }

    pub fn put_object(&self, endpoint: &str, bucket: &str, key: &str, body_path: &Path) {
        let mut cmd = self.command();
        cmd.args(["--endpoint-url", endpoint, "--ca-bundle"])
            .arg(&self.ca_bundle_path)
            .args(["s3api", "put-object", "--bucket", bucket, "--key", key, "--body"])
            .arg(body_path);
        run_status(cmd, "aws put-object");
    }

    pub fn get_object(&self, endpoint: &str, bucket: &str, key: &str, out_path: &Path) {
        let mut cmd = self.command();
        cmd.args(["--endpoint-url", endpoint, "--ca-bundle"])
            .arg(&self.ca_bundle_path)
            .args(["s3api", "get-object", "--bucket", bucket, "--key", key])
            .arg(out_path);
        run_status(cmd, "aws get-object");
    }
}

// -- rclone --

pub fn rclone_args(
    endpoint: &str,
    ca_bundle_path: &Path,
    access_key: &str,
    secret_key: &str,
) -> Vec<String> {
    vec![
        "--s3-provider".into(), "Other".into(),
        "--s3-endpoint".into(), endpoint.into(),
        "--s3-access-key-id".into(), access_key.into(),
        "--s3-secret-access-key".into(), secret_key.into(),
        "--s3-force-path-style".into(),
        "--s3-use-unsigned-payload".into(), "true".into(),
        "--ca-cert".into(), ca_bundle_path.to_string_lossy().into(),
    ]
}

pub fn rclone_mkdir(
    rclone: &str,
    endpoint: &str,
    ca_bundle_path: &Path,
    access_key: &str,
    secret_key: &str,
    bucket: &str,
) {
    let mut cmd = Command::new(rclone);
    cmd.arg("mkdir")
        .arg(format!(":s3:{}", bucket))
        .args(rclone_args(endpoint, ca_bundle_path, access_key, secret_key))
        .stdout(Stdio::null())
        .stderr(Stdio::null());
    run_status(cmd, "rclone mkdir");
}
