use std::process::{Command, Stdio};
use std::sync::Arc;
use std::time::Instant;

use crate::s3::client::S3Client;

use super::clients::{AwsCliHarness, rclone_args, rclone_mkdir, run_status};
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
    let tls = TlsMaterial::generate();

    let aws_path = servers::find_binary("AWS", r"C:\Program Files\Amazon\AWSCLIV2\aws.exe");
    let rclone_bin = servers::find_binary("RCLONE", r"C:\tools\rclone.exe");
    let rustfs_bin = servers::find_binary("RUSTFS_BIN", r"C:\tools\rustfs.exe");
    let minio_bin = servers::find_binary("MINIO_BIN", r"C:\tools\minio.exe");

    let has = |list: &[String], val: &str| list.iter().any(|s| s.eq_ignore_ascii_case(val));

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
                    format!("AbixIO-{}+wc", wp)
                } else {
                    format!("AbixIO-{}", wp)
                };
                eprintln!("--- L7: {} ---", label);

                let abixio = AbixioServer::builder()
                    .volume_count(1)
                    .no_auth(false)
                    .tls(&tls)
                    .write_tier(wp)
                    .write_cache(if wc { 256 } else { 0 })
                    .start();

                let cfg = ServerConfig {
                    name: label,
                    client: abixio.s3_client(),
                    endpoint: abixio.endpoint(),
                    ca_cert_pem: tls.ca_cert_pem.clone(),
                    access_key: "test".into(),
                    secret_key: "testsecret".into(),
                };

                results.extend(
                    run_server(&cfg, sizes, args, &aws_path, &rclone_bin).await,
                );
                // abixio dropped here, child killed, temp reclaimed
            }
        }
    }

    // -- RustFS --
    if has(&args.servers, "rustfs") {
        if let Some(bin) = &rustfs_bin {
            eprintln!("--- L7: RustFS ---");
            let rustfs = ExternalServer::start_rustfs_tls(bin, 11701, &tls)
                .unwrap_or_else(|| panic!("failed to start RustFS"));
            let cfg = ServerConfig {
                name: "RustFS".into(),
                client: rustfs.s3_client(("benchuser", "benchpass")),
                endpoint: rustfs.endpoint(),
                ca_cert_pem: tls.ca_cert_pem.clone(),
                access_key: "benchuser".into(),
                secret_key: "benchpass".into(),
            };
            results.extend(
                run_server(&cfg, sizes, args, &aws_path, &rclone_bin).await,
            );
        } else {
            eprintln!("--- L7: RustFS (skipped, binary not found) ---");
        }
    }

    // -- MinIO --
    if has(&args.servers, "minio") {
        if let Some(bin) = &minio_bin {
            eprintln!("--- L7: MinIO ---");
            let minio = ExternalServer::start_minio_tls(bin, 11703, &tls)
                .unwrap_or_else(|| panic!("failed to start MinIO"));
            let cfg = ServerConfig {
                name: "MinIO".into(),
                client: minio.s3_client(("benchuser", "benchpass")),
                endpoint: minio.endpoint(),
                ca_cert_pem: tls.ca_cert_pem.clone(),
                access_key: "benchuser".into(),
                secret_key: "benchpass".into(),
            };
            results.extend(
                run_server(&cfg, sizes, args, &aws_path, &rclone_bin).await,
            );
        } else {
            eprintln!("--- L7: MinIO (skipped, binary not found) ---");
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
            let data = vec![0x42u8; size];
            let iters = args.iters.unwrap_or_else(|| iters_for_size(size));
            let label = human_size(size);

            // warmup
            for i in 0..3 {
                let _ = cfg.client.put_object_unsigned(
                    bucket, &format!("w_{}_{}", label, i),
                    data.clone(), "application/octet-stream",
                ).await;
            }

            // PUT
            if has(&args.ops, "PUT") {
                let mut timings = Vec::with_capacity(iters);
                for i in 0..iters {
                    let t = Instant::now();
                    let _ = cfg.client.put_object_unsigned(
                        bucket, &format!("sdk_{}_{}", label, i),
                        data.clone(), "application/octet-stream",
                    ).await;
                    timings.push(t.elapsed());
                }
                results.push(BenchResult {
                    layer: "L7".into(), op: "put".into(), size, iters,
                    write_path: None, write_cache: None,
                    server: Some(cfg.name.clone()), client: Some("sdk".into()),
                    timings,
                });
            }

            // GET
            if has(&args.ops, "GET") {
                let mut timings = Vec::with_capacity(iters);
                for i in 0..iters {
                    let t = Instant::now();
                    let _ = cfg.client.get_object(bucket, &format!("sdk_{}_{}", label, i)).await;
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

        // HEAD
        if has(&args.ops, "HEAD") {
            let iters = args.iters.unwrap_or(100);
            let mut timings = Vec::with_capacity(iters);
            for i in 0..iters {
                let t = Instant::now();
                let _ = cfg.client.head_object(bucket, &format!("sdk_4KB_{}", i % 50)).await;
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
                let _ = cfg.client.list_objects(bucket, "sdk_4KB_", "").await;
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
                let _ = cfg.client.delete_object(bucket, &format!("sdk_4KB_{}", i)).await;
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
