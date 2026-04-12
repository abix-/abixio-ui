//! L7: Full end-to-end (integration, NOT isolated)
//!
//! This is NOT an isolated layer test. It runs the complete stack
//! including a real server process, real S3 client, TLS, and auth.
//!
//! How it works:
//! - Spawns a real abixio server as a child process (release build)
//! - Also spawns RustFS and MinIO for competitive comparison
//! - Uses aws-sdk-s3 (in-process), aws-cli, and rclone as clients
//! - HTTPS + SigV4 + UNSIGNED-PAYLOAD for fair comparison
//! - PUT payload read from disk, GET output written to disk
//! - 20 PUT + 3 GET warmup before timing
//!
//! What this number means: what a real user actually sees. This is
//! the number that matters for product claims and competitive
//! comparison. The gap between L6 and L7 is the cost of running as
//! a separate process with TLS and a real SDK client.

use std::process::{Command, Stdio};
use std::sync::Arc;
use std::time::Instant;

use crate::s3::client::S3Client;

use super::clients::{AwsCliHarness, measure_cli_overhead, rclone_args, rclone_mkdir, run_status};
use super::servers::{self, AbixioServer, ExternalServer};
use super::stats::{human_size, iters_for_size, BenchResult};
use super::tls::TlsMaterial;
use super::BenchArgs;

struct ServerConfig {
    name: String,
    client: Arc<S3Client>,
    endpoint: String,
    ca_cert_pem: Vec<u8>,
    access_key: String,
    secret_key: String,
}

pub async fn run(sizes: &[usize], args: &BenchArgs) -> Vec<BenchResult> {
    let mut results = Vec::new();

    let aws_path = servers::find_binary("AWS", r"C:\Program Files\Amazon\AWSCLIV2\aws.exe");
    let rclone_bin = servers::find_binary("RCLONE", r"C:\tools\rclone.exe");
    let rustfs_bin = servers::find_binary("RUSTFS_BIN", r"C:\tools\rustfs.exe");
    let minio_bin = servers::find_binary("MINIO_BIN", r"C:\tools\minio.exe");

    let has = |list: &[String], val: &str| list.iter().any(|s| s.eq_ignore_ascii_case(val));

    let tls_modes: Vec<bool> = match args.tls.to_lowercase().as_str() {
        "off" => vec![false],
        "both" => vec![true, false],
        _ => vec![true], // "on" or default
    };

    for use_tls in &tls_modes {
        let tls = if *use_tls { Some(TlsMaterial::generate()) } else { None };
        let tls_label = if *use_tls { "" } else { " (HTTP)" };

        // -- AbixIO configs --
        if has(&args.servers, "abixio") {
            let cache_states: Vec<bool> = match args.write_cache.to_lowercase().as_str() {
                "on" => vec![true],
                "off" => vec![false],
                _ => vec![false, true],
            };

            for wp in &args.write_paths {
                for &wc in &cache_states {
                    let label = if wc {
                        format!("AbixIO-{}+wc{}", wp, tls_label)
                    } else {
                        format!("AbixIO-{}{}", wp, tls_label)
                    };
                    eprintln!("--- L7: {} ---", label);

                    let mut builder = AbixioServer::builder()
                        .volume_count(1)
                        .no_auth(false)
                        .write_tier(wp)
                        .write_cache(if wc { 256 } else { 0 });
                    if let Some(t) = &tls {
                        builder = builder.tls(t);
                    }
                    let abixio = builder.start();

                    let cfg = ServerConfig {
                        name: label,
                        client: abixio.s3_client(),
                        endpoint: abixio.endpoint(),
                        ca_cert_pem: tls.as_ref().map(|t| t.ca_cert_pem.clone()).unwrap_or_default(),
                        access_key: "test".into(),
                        secret_key: "testsecret".into(),
                    };

                    results.extend(
                        run_server(&cfg, sizes, args, &aws_path, &rclone_bin).await,
                    );
                }
            }
        }

        // -- RustFS --
        if has(&args.servers, "rustfs") {
            if let Some(bin) = &rustfs_bin {
                eprintln!("--- L7: RustFS{} ---", tls_label);
                let rustfs = if let Some(t) = &tls {
                    ExternalServer::start_rustfs_tls(bin, 11701, t)
                } else {
                    ExternalServer::start_rustfs(bin, 11701)
                };
                if let Some(rustfs) = rustfs {
                    let cfg = ServerConfig {
                        name: format!("RustFS{}", tls_label),
                        client: rustfs.s3_client(("benchuser", "benchpass")),
                        endpoint: rustfs.endpoint(),
                        ca_cert_pem: tls.as_ref().map(|t| t.ca_cert_pem.clone()).unwrap_or_default(),
                        access_key: "benchuser".into(),
                        secret_key: "benchpass".into(),
                    };
                    results.extend(
                        run_server(&cfg, sizes, args, &aws_path, &rclone_bin).await,
                    );
                } else {
                    eprintln!("  failed to start RustFS, skipping");
                }
            } else {
                eprintln!("--- L7: RustFS (skipped, binary not found) ---");
            }
        }

        // -- MinIO --
        if has(&args.servers, "minio") {
            if let Some(bin) = &minio_bin {
                eprintln!("--- L7: MinIO{} ---", tls_label);
                let minio = if let Some(t) = &tls {
                    ExternalServer::start_minio_tls(bin, 11703, t)
                } else {
                    ExternalServer::start_minio(bin, 11703)
                };
                if let Some(minio) = minio {
                    let cfg = ServerConfig {
                        name: format!("MinIO{}", tls_label),
                        client: minio.s3_client(("benchuser", "benchpass")),
                        endpoint: minio.endpoint(),
                        ca_cert_pem: tls.as_ref().map(|t| t.ca_cert_pem.clone()).unwrap_or_default(),
                        access_key: "benchuser".into(),
                        secret_key: "benchpass".into(),
                    };
                    results.extend(
                        run_server(&cfg, sizes, args, &aws_path, &rclone_bin).await,
                    );
                } else {
                    eprintln!("  failed to start MinIO, skipping");
                }
            } else {
                eprintln!("--- L7: MinIO (skipped, binary not found) ---");
            }
        }
    }

    results
}

async fn run_server(
    cfg: &ServerConfig,
    sizes: &[usize],
    args: &BenchArgs,
    aws_path: &Option<String>,
    rclone_bin: &Option<String>,
) -> Vec<BenchResult> {
    let mut results = Vec::new();
    let has = |list: &[String], val: &str| list.iter().any(|s| s.eq_ignore_ascii_case(val));

    let ca_path = tempfile::NamedTempFile::new().unwrap();
    std::fs::write(ca_path.path(), &cfg.ca_cert_pem).unwrap();

    // -- sdk client --
    if has(&args.clients, "sdk") {
        let bucket = "bench-sdk";
        let _ = cfg.client.create_bucket(bucket).await;

        for &size in sizes {
            let iters = args.iters.unwrap_or_else(|| iters_for_size(size));
            let label = human_size(size);

            // write payload to disk (fairness: all clients read from disk)
            let tmpdir = tempfile::TempDir::new().unwrap();
            let srcpath = tmpdir.path().join("payload.dat");
            let sinkpath = tmpdir.path().join("out.dat");
            std::fs::write(&srcpath, vec![0x42u8; size]).unwrap();

            // warmup: 20 PUT + 3 GET (read from disk each time)
            for i in 0..20 {
                let data = tokio::fs::read(&srcpath).await.unwrap();
                let _ = cfg.client.put_object_unsigned(
                    bucket, &format!("w_{}_{}", label, i),
                    data, "application/octet-stream",
                ).await;
            }
            for i in 0..3 {
                let _ = cfg.client.download_object_to_file(
                    bucket, &format!("w_{}_{}", label, i), &sinkpath,
                ).await;
            }

            // roundtrip verification: PUT then GET, compare sizes
            let verify_key = format!("verify_{}", label);
            let verify_data = tokio::fs::read(&srcpath).await.unwrap();
            let orig_size = verify_data.len();
            cfg.client.put_object_unsigned(
                bucket, &verify_key, verify_data, "application/octet-stream",
            ).await.unwrap_or_else(|e| panic!("roundtrip PUT failed for {}: {}", label, e));
            let verify_sink = tmpdir.path().join("verify.dat");
            cfg.client.download_object_to_file(
                bucket, &verify_key, &verify_sink,
            ).await.unwrap_or_else(|e| panic!("roundtrip GET failed for {}: {}", label, e));
            let got_size = std::fs::metadata(&verify_sink).map(|m| m.len() as usize).unwrap_or(0);
            assert_eq!(
                orig_size, got_size,
                "roundtrip size mismatch for {} on {}: put {} bytes, got {} bytes",
                label, cfg.name, orig_size, got_size,
            );

            // PUT (unsigned payload, canonical benchmark mode)
            if has(&args.ops, "PUT") {
                let mut timings = Vec::with_capacity(iters);
                for i in 0..iters {
                    let data = tokio::fs::read(&srcpath).await.unwrap();
                    let t = Instant::now();
                    let _ = cfg.client.put_object_unsigned(
                        bucket, &format!("sdk_{}_{}", label, i),
                        data, "application/octet-stream",
                    ).await;
                    timings.push(t.elapsed());
                }
                results.push(BenchResult {
                    layer: "L7".into(), op: "put".into(), size, iters,
                    write_path: None, write_cache: None,
                    server: Some(cfg.name.clone()), client: Some("sdk".into()),
                    timings,
                });

                // PUT (signed payload, for comparison)
                let mut timings = Vec::with_capacity(iters);
                for i in 0..iters {
                    let data = tokio::fs::read(&srcpath).await.unwrap();
                    let t = Instant::now();
                    let _ = cfg.client.put_object(
                        bucket, &format!("sdk_signed_{}_{}", label, i),
                        data, "application/octet-stream",
                    ).await;
                    timings.push(t.elapsed());
                }
                results.push(BenchResult {
                    layer: "L7".into(), op: "put_signed".into(), size, iters,
                    write_path: None, write_cache: None,
                    server: Some(cfg.name.clone()), client: Some("sdk".into()),
                    timings,
                });
            }

            // GET (download to disk file each iter, fairness)
            if has(&args.ops, "GET") {
                let mut timings = Vec::with_capacity(iters);
                for i in 0..iters {
                    let t = Instant::now();
                    let _ = cfg.client.download_object_to_file(
                        bucket, &format!("sdk_{}_{}", label, i), &sinkpath,
                    ).await;
                    timings.push(t.elapsed());
                }
                results.push(BenchResult {
                    layer: "L7".into(), op: "get".into(), size, iters,
                    write_path: None, write_cache: None,
                    server: Some(cfg.name.clone()), client: Some("sdk".into()),
                    timings,
                });
            }

            eprintln!("  sdk {} done ({} iters)", label, iters);
        }

        // seed 100 objects for HEAD/LIST/DELETE
        let meta_bucket = "bench-meta";
        let _ = cfg.client.create_bucket(meta_bucket).await;
        let meta_payload = vec![0x42u8; 4096];
        for i in 0..100 {
            let _ = cfg.client.put_object_unsigned(
                meta_bucket, &format!("meta/{}", i),
                meta_payload.clone(), "application/octet-stream",
            ).await;
        }

        // warmup for meta ops
        for i in 0..3 {
            let _ = cfg.client.head_object(meta_bucket, &format!("meta/{}", i)).await;
            let _ = cfg.client.list_objects(meta_bucket, "meta/", "").await;
        }

        // HEAD
        if has(&args.ops, "HEAD") {
            let iters = args.iters.unwrap_or(100);
            let mut timings = Vec::with_capacity(iters);
            for i in 0..iters {
                let t = Instant::now();
                let _ = cfg.client.head_object(meta_bucket, &format!("meta/{}", i)).await;
                timings.push(t.elapsed());
            }
            results.push(BenchResult {
                layer: "L7".into(), op: "head".into(), size: 0, iters,
                write_path: None, write_cache: None,
                server: Some(cfg.name.clone()), client: Some("sdk".into()),
                timings,
            });
            eprintln!("  sdk HEAD done ({} iters)", iters);
        }

        // LIST
        if has(&args.ops, "LIST") {
            let iters = args.iters.unwrap_or(50);
            let mut timings = Vec::with_capacity(iters);
            for _ in 0..iters {
                let t = Instant::now();
                let _ = cfg.client.list_objects(meta_bucket, "meta/", "").await;
                timings.push(t.elapsed());
            }
            results.push(BenchResult {
                layer: "L7".into(), op: "list".into(), size: 0, iters,
                write_path: None, write_cache: None,
                server: Some(cfg.name.clone()), client: Some("sdk".into()),
                timings,
            });
            eprintln!("  sdk LIST done ({} iters)", iters);
        }

        // DELETE
        if has(&args.ops, "DELETE") {
            let iters = args.iters.unwrap_or(100);
            let mut timings = Vec::with_capacity(iters);
            for i in 0..iters {
                let t = Instant::now();
                let _ = cfg.client.delete_object(meta_bucket, &format!("meta/{}", i)).await;
                timings.push(t.elapsed());
            }
            results.push(BenchResult {
                layer: "L7".into(), op: "delete".into(), size: 0, iters,
                write_path: None, write_cache: None,
                server: Some(cfg.name.clone()), client: Some("sdk".into()),
                timings,
            });
            eprintln!("  sdk DELETE done ({} iters)", iters);
        }
    }

    // -- aws-cli --
    if has(&args.clients, "aws-cli") {
        if let Some(aws) = aws_path {
            let harness = AwsCliHarness::new(
                aws.clone(), ca_path.path(), &cfg.access_key, &cfg.secret_key,
            );
            harness.create_bucket(&cfg.endpoint, "bench-aws");

            // warmup + measure per-process spawn overhead
            let overhead = harness.measure_overhead(&cfg.endpoint, 10);
            eprintln!("  aws-cli overhead: {:.1}ms", overhead.as_secs_f64() * 1000.0);

            for &size in sizes {
                if !has(&args.ops, "PUT") && !has(&args.ops, "GET") { continue; }
                let iters = args.iters.unwrap_or_else(|| iters_for_size(size));
                let label = human_size(size);
                let tmpdir = tempfile::TempDir::new().unwrap();
                let srcpath = tmpdir.path().join("payload.dat");
                std::fs::write(&srcpath, vec![0x42u8; size]).unwrap();

                if has(&args.ops, "PUT") {
                    let mut timings = Vec::with_capacity(iters);
                    for i in 0..iters {
                        let t = Instant::now();
                        harness.put_object(
                            &cfg.endpoint, "bench-aws",
                            &format!("aws_{}_{}", label, i), &srcpath,
                        );
                        timings.push(t.elapsed());
                    }
                    results.push(BenchResult {
                        layer: "L7".into(), op: "put".into(), size, iters,
                        write_path: None, write_cache: None,
                        server: Some(cfg.name.clone()), client: Some("aws-cli".into()),
                        timings,
                    });
                }

                if has(&args.ops, "GET") {
                    let sinkdir = tempfile::TempDir::new().unwrap();
                    let mut timings = Vec::with_capacity(iters);
                    for i in 0..iters {
                        let sinkpath = sinkdir.path().join(format!("{}.dat", i));
                        let t = Instant::now();
                        harness.get_object(
                            &cfg.endpoint, "bench-aws",
                            &format!("aws_{}_{}", label, i), &sinkpath,
                        );
                        timings.push(t.elapsed());
                    }
                    results.push(BenchResult {
                        layer: "L7".into(), op: "get".into(), size, iters,
                        write_path: None, write_cache: None,
                        server: Some(cfg.name.clone()), client: Some("aws-cli".into()),
                        timings,
                    });
                }

                eprintln!("  aws-cli {} done ({} iters)", label, iters);
            }
        } else {
            eprintln!("  aws-cli (skipped, binary not found)");
        }
    }

    // -- rclone --
    if has(&args.clients, "rclone") {
        if let Some(rclone) = rclone_bin {
            rclone_mkdir(
                rclone, &cfg.endpoint, ca_path.path(),
                &cfg.access_key, &cfg.secret_key, "bench-rclone",
            );

            // warmup + measure per-process spawn overhead
            let rclone_overhead = measure_cli_overhead(
                rclone,
                &[
                    "lsd", ":s3:",
                    "--s3-provider", "Other",
                    "--s3-endpoint", &cfg.endpoint,
                    "--s3-access-key-id", &cfg.access_key,
                    "--s3-secret-access-key", &cfg.secret_key,
                    "--s3-force-path-style",
                    "--s3-use-unsigned-payload", "true",
                    "--ca-cert", ca_path.path().to_str().unwrap(),
                ],
                10,
            );
            eprintln!("  rclone overhead: {:.1}ms", rclone_overhead.as_secs_f64() * 1000.0);

            for &size in sizes {
                if !has(&args.ops, "PUT") && !has(&args.ops, "GET") { continue; }
                let iters = args.iters.unwrap_or_else(|| iters_for_size(size));
                let label = human_size(size);
                let tmpdir = tempfile::TempDir::new().unwrap();
                let srcpath = tmpdir.path().join("payload.dat");
                std::fs::write(&srcpath, vec![0x42u8; size]).unwrap();

                if has(&args.ops, "PUT") {
                    let mut timings = Vec::with_capacity(iters);
                    for i in 0..iters {
                        let t = Instant::now();
                        let mut cmd = Command::new(rclone);
                        cmd.arg("copyto")
                            .arg(&srcpath)
                            .arg(format!(":s3:bench-rclone/rc_{}_{}", label, i))
                            .args(rclone_args(
                                &cfg.endpoint, ca_path.path(),
                                &cfg.access_key, &cfg.secret_key,
                            ))
                            .stdout(Stdio::null())
                            .stderr(Stdio::null());
                        run_status(cmd, "rclone put");
                        timings.push(t.elapsed());
                    }
                    results.push(BenchResult {
                        layer: "L7".into(), op: "put".into(), size, iters,
                        write_path: None, write_cache: None,
                        server: Some(cfg.name.clone()), client: Some("rclone".into()),
                        timings,
                    });
                }

                if has(&args.ops, "GET") {
                    let sinkdir = tempfile::TempDir::new().unwrap();
                    let mut timings = Vec::with_capacity(iters);
                    for i in 0..iters {
                        let sinkpath = sinkdir.path().join(format!("{}.dat", i));
                        let t = Instant::now();
                        let mut cmd = Command::new(rclone);
                        cmd.arg("copyto")
                            .arg(format!(":s3:bench-rclone/rc_{}_{}", label, i))
                            .arg(&sinkpath)
                            .args(rclone_args(
                                &cfg.endpoint, ca_path.path(),
                                &cfg.access_key, &cfg.secret_key,
                            ))
                            .stdout(Stdio::null())
                            .stderr(Stdio::null());
                        run_status(cmd, "rclone get");
                        timings.push(t.elapsed());
                    }
                    results.push(BenchResult {
                        layer: "L7".into(), op: "get".into(), size, iters,
                        write_path: None, write_cache: None,
                        server: Some(cfg.name.clone()), client: Some("rclone".into()),
                        timings,
                    });
                }

                eprintln!("  rclone {} done ({} iters)", label, iters);
            }
        } else {
            eprintln!("  rclone (skipped, binary not found)");
        }
    }

    results
}
