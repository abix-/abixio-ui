use std::time::Instant;

use super::stats::{human_size, iters_for_size, BenchResult};

pub async fn run(sizes: &[usize], iters_override: Option<usize>) -> Vec<BenchResult> {
    let mut results = Vec::new();

    eprintln!("--- L1: HTTP transport ---");

    // PUT server: reads body, returns 200
    let put_listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
    let put_addr = put_listener.local_addr().unwrap();
    tokio::spawn(async move {
        loop {
            let (stream, _) = match put_listener.accept().await {
                Ok(v) => v,
                Err(_) => return,
            };
            stream.set_nodelay(true).ok();
            let io = hyper_util::rt::TokioIo::new(stream);
            tokio::spawn(async move {
                let svc = hyper::service::service_fn(
                    |req: hyper::Request<hyper::body::Incoming>| async move {
                        use http_body_util::BodyExt;
                        let _ = req.into_body().collect().await;
                        Ok::<_, hyper::Error>(hyper::Response::new(
                            http_body_util::Full::new(bytes::Bytes::from("ok")),
                        ))
                    },
                );
                let _ = hyper::server::conn::http1::Builder::new()
                    .serve_connection(io, svc)
                    .await;
            });
        }
    });

    let client = reqwest::Client::new();

    for &size in sizes {
        let data = vec![0x42u8; size];
        let iters = iters_override.unwrap_or_else(|| iters_for_size(size));
        let label = human_size(size);
        let put_url = format!("http://{}/test", put_addr);

        // warmup
        for _ in 0..3 {
            client.put(&put_url).body(data.clone()).send().await.unwrap();
        }

        // PUT
        let mut timings = Vec::with_capacity(iters);
        for _ in 0..iters {
            let t = Instant::now();
            client.put(&put_url).body(data.clone()).send().await.unwrap();
            timings.push(t.elapsed());
        }
        results.push(BenchResult {
            layer: "L1".into(),
            op: "http_put".into(),
            size,
            iters,
            write_path: None,
            write_cache: None,
            server: None,
            client: None,
            timings,
        });

        // GET server: returns sized response
        let response_bytes = bytes::Bytes::from(data.clone());
        let get_listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
        let get_addr = get_listener.local_addr().unwrap();
        let body = response_bytes.clone();
        tokio::spawn(async move {
            loop {
                let (stream, _) = match get_listener.accept().await {
                    Ok(v) => v,
                    Err(_) => return,
                };
                stream.set_nodelay(true).ok();
                let io = hyper_util::rt::TokioIo::new(stream);
                let body = body.clone();
                tokio::spawn(async move {
                    let svc = hyper::service::service_fn(
                        move |_req: hyper::Request<hyper::body::Incoming>| {
                            let body = body.clone();
                            async move {
                                Ok::<_, hyper::Error>(hyper::Response::new(
                                    http_body_util::Full::new(body),
                                ))
                            }
                        },
                    );
                    let _ = hyper::server::conn::http1::Builder::new()
                        .serve_connection(io, svc)
                        .await;
                });
            }
        });

        let get_url = format!("http://{}/test", get_addr);

        // warmup
        for _ in 0..3 {
            let r = client.get(&get_url).send().await.unwrap();
            let _ = r.bytes().await.unwrap();
        }

        // GET
        let mut timings = Vec::with_capacity(iters);
        for _ in 0..iters {
            let t = Instant::now();
            let r = client.get(&get_url).send().await.unwrap();
            let _ = r.bytes().await.unwrap();
            timings.push(t.elapsed());
        }
        results.push(BenchResult {
            layer: "L1".into(),
            op: "http_get".into(),
            size,
            iters,
            write_path: None,
            write_cache: None,
            server: None,
            client: None,
            timings,
        });

        eprintln!("  {} done ({} iters)", label, iters);
    }

    results
}
