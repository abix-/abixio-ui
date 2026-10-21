use std::error::Error;
use std::time::Duration;

#[tokio::test]
#[ignore = "requires public certificate test endpoints"]
async fn rejects_compromised_certificate_chains() {
    let client = reqwest::Client::builder()
        .timeout(Duration::from_secs(15))
        .redirect(reqwest::redirect::Policy::none())
        .build()
        .unwrap();
    client.get("https://example.com/").send().await.unwrap();
    for host in [
        "dv-g3b-emudhra.we-have-a-problem.com",
        "dv-g3b-aaa-emudhra.we-have-a-problem.com",
    ] {
        let error = client
            .get(format!("https://{host}/"))
            .send()
            .await
            .unwrap_err();
        let mut causes = Vec::new();
        let mut cause: Option<&dyn Error> = Some(&error);
        while let Some(error) = cause {
            causes.push(error.to_string());
            cause = error.source();
        }
        let detail = causes.join(": ").to_ascii_lowercase();
        assert!(detail.contains("certificate"), "{host}: {detail}");
    }
}

#[tokio::test]
#[ignore = "requires public certificate test endpoints"]
async fn sdk_rejects_compromised_certificate_chains() {
    use aws_sdk_s3::config::{BehaviorVersion, Credentials, Region, retry::RetryConfig};

    for host in [
        "dv-g3b-emudhra.we-have-a-problem.com",
        "dv-g3b-aaa-emudhra.we-have-a-problem.com",
    ] {
        let config = aws_sdk_s3::config::Builder::new()
            .behavior_version(BehaviorVersion::latest())
            .endpoint_url(format!("https://{host}"))
            .region(Region::new("us-east-1"))
            .credentials_provider(Credentials::new("test", "test", None, None, "test"))
            .retry_config(RetryConfig::disabled())
            .build();
        let client = aws_sdk_s3::Client::from_conf(config);
        let error = tokio::time::timeout(Duration::from_secs(15), client.list_buckets().send())
            .await
            .expect("TLS rejection must not time out")
            .unwrap_err();
        let detail = format!("{error:?}").to_ascii_lowercase();
        assert!(detail.contains("certificate"), "{host}: {detail}");
    }
}
