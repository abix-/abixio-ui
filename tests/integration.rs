//! Integration tests that launch real abixio server instances and run the
//! full e2e test suite against them with various volume configurations.
//!
//! These tests are `#[ignore]` so they don't run in normal `cargo test`.
//! Run with: `cargo test --test integration -- --ignored`
//!
//! The abixio binary is discovered via `ABIXIO_BIN` env var, or from known
//! paths. Set `ABIXIO_BIN` to override.

#[path = "support/mod.rs"]
mod support;

use support::server::AbixioServer;

use abixio_ui::views::testing::run_e2e_tests;

async fn run_and_assert(server: &AbixioServer) {
    let client = server.s3_client();
    let admin = server.admin_client();
    let results = run_e2e_tests(client, Some(admin)).await;

    let failed: Vec<_> = results.iter().filter(|r| !r.passed).collect();
    if !failed.is_empty() {
        let mut msg = format!("{} test(s) failed:\n", failed.len());
        for f in &failed {
            msg.push_str(&format!(
                "  FAIL: {} -- {}\n",
                f.name,
                f.detail.as_deref().unwrap_or("(no detail)")
            ));
        }
        panic!("{}", msg);
    }

    let total = results.len();
    eprintln!("  {} tests passed on port {}", total, server.endpoint());
}

/// 4 volumes (default config, server picks erasure layout per bucket)
#[tokio::test(flavor = "multi_thread")]
#[ignore]
async fn full_e2e_default_config() {
    let server = AbixioServer::builder().start();
    run_and_assert(&server).await;
}

/// 2 volumes -- minimal multi-volume setup
#[tokio::test(flavor = "multi_thread")]
#[ignore]
async fn e2e_two_volumes() {
    let server = AbixioServer::builder().volume_count(2).start();
    run_and_assert(&server).await;
}

/// 8 volumes -- high volume count
#[tokio::test(flavor = "multi_thread")]
#[ignore]
async fn e2e_eight_volumes() {
    let server = AbixioServer::builder().volume_count(8).start();
    run_and_assert(&server).await;
}

/// 1 volume -- single disk, no erasure possible
#[tokio::test(flavor = "multi_thread")]
#[ignore]
async fn e2e_single_volume() {
    let server = AbixioServer::builder().volume_count(1).start();
    run_and_assert(&server).await;
}

/// 6 volumes -- odd non-power-of-two count
#[tokio::test(flavor = "multi_thread")]
#[ignore]
async fn e2e_six_volumes() {
    let server = AbixioServer::builder().volume_count(6).start();
    run_and_assert(&server).await;
}
