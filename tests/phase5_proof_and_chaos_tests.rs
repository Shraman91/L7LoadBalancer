use bytes::Bytes;
use http_body_util::{BodyExt, Full, StreamBody};
use hyper::body::Frame;
use hyper::header::{CONNECTION, UPGRADE};
use hyper::service::service_fn;
use hyper::{Request, Response, StatusCode};
use hyper_util::rt::{TokioExecutor, TokioIo};
use hyper_util::server::conn::auto::Builder as ConnBuilder;
use l7_load_balancer::backend::Backend;
use l7_load_balancer::balancer::{LoadBalancer, PowerOfTwoChoices, WeightedRoundRobin};
use l7_load_balancer::circuit_breaker::{CircuitBreakerConfig, STATE_OPEN};
use l7_load_balancer::config::Config;
use l7_load_balancer::server;
use l7_load_balancer::state::{AppState, AppStateHolder};
use std::collections::HashMap;
use std::convert::Infallible;
use std::sync::atomic::{AtomicUsize, Ordering};
use std::sync::Arc;
use std::time::Duration;
use tokio::io::{AsyncReadExt, AsyncWriteExt};
use tokio::net::TcpListener;
use tokio_util::sync::CancellationToken;

#[tokio::test]
async fn test_websocket_upgrade_and_bidirectional_streaming() {
    // 1. Mock backend supporting WebSocket upgrade
    let backend_listener = TcpListener::bind("127.0.0.1:0").await.unwrap();
    let backend_addr = backend_listener.local_addr().unwrap();

    tokio::spawn(async move {
        loop {
            let (stream, _) = match backend_listener.accept().await {
                Ok(s) => s,
                Err(_) => break,
            };
            let io = TokioIo::new(stream);
            tokio::spawn(async move {
                let service = service_fn(|mut req: Request<hyper::body::Incoming>| async move {
                    let is_upgrade = req
                        .headers()
                        .get(UPGRADE)
                        .map(|v| v == "websocket")
                        .unwrap_or(false);

                    if is_upgrade {
                        let resp = Response::builder()
                            .status(StatusCode::SWITCHING_PROTOCOLS)
                            .header(UPGRADE, "websocket")
                            .header(CONNECTION, "Upgrade")
                            .body(Full::new(Bytes::new()).boxed())
                            .unwrap();

                        let on_upgrade = hyper::upgrade::on(&mut req);
                        tokio::spawn(async move {
                            if let Ok(upgraded) = on_upgrade.await {
                                let mut stream = TokioIo::new(upgraded);
                                let mut buf = [0u8; 1024];
                                if let Ok(n) = stream.read(&mut buf).await {
                                    let mut response = b"echo: ".to_vec();
                                    response.extend_from_slice(&buf[..n]);
                                    let _ = stream.write_all(&response).await;
                                }
                            }
                        });

                        Ok::<_, Infallible>(resp)
                    } else {
                        Ok::<_, Infallible>(
                            Response::builder()
                                .status(StatusCode::OK)
                                .body(Full::new(Bytes::from("plain ok")).boxed())
                                .unwrap(),
                        )
                    }
                });
                let _ = ConnBuilder::new(TokioExecutor::new())
                    .serve_connection_with_upgrades(io, service)
                    .await;
            });
        }
    });

    // 2. Setup proxy
    let cfg = Config::load("config.toml").unwrap();
    let backend = Arc::new(Backend::new(
        backend_addr,
        1,
        CircuitBreakerConfig::default(),
    ));
    let state = Arc::new(AppState::new(
        vec![backend],
        Box::new(l7_load_balancer::balancer::RoundRobin::new()),
        cfg,
    ));
    let state_holder = AppStateHolder::from_state(state);

    let cancel_token = CancellationToken::new();
    let proxy_listener = TcpListener::bind("127.0.0.1:0").await.unwrap();
    let proxy_addr = proxy_listener.local_addr().unwrap();
    drop(proxy_listener);

    let sh = state_holder.clone();
    let c = cancel_token.clone();
    tokio::spawn(async move {
        let _ = server::run(proxy_addr, sh, c).await;
    });

    tokio::time::sleep(Duration::from_millis(50)).await;

    // 3. Client sends upgrade request
    let tcp_stream = tokio::net::TcpStream::connect(proxy_addr).await.unwrap();
    let io = TokioIo::new(tcp_stream);
    let (mut sender, conn) = hyper::client::conn::http1::handshake(io).await.unwrap();
    tokio::spawn(async move {
        let _ = conn.with_upgrades().await;
    });

    let req = Request::builder()
        .method("GET")
        .uri(format!("http://{}/ws", proxy_addr))
        .header("Host", "localhost")
        .header(UPGRADE, "websocket")
        .header(CONNECTION, "Upgrade")
        .body(Full::new(Bytes::new()).boxed())
        .unwrap();

    let mut resp = sender.send_request(req).await.unwrap();
    assert_eq!(resp.status(), StatusCode::SWITCHING_PROTOCOLS);

    let upgraded = hyper::upgrade::on(&mut resp).await.unwrap();
    let mut upgraded_io = TokioIo::new(upgraded);

    // Send payload through tunnel
    upgraded_io
        .write_all(b"hello websocket tunnel")
        .await
        .unwrap();

    let mut reply = [0u8; 1024];
    let n = upgraded_io.read(&mut reply).await.unwrap();
    assert_eq!(&reply[..n], b"echo: hello websocket tunnel");

    cancel_token.cancel();
}

#[tokio::test]
async fn test_large_body_streaming_flat_memory() {
    let received_bytes = Arc::new(AtomicUsize::new(0));
    let rec_clone = received_bytes.clone();

    // 1. Mock backend reading streamed request body chunk by chunk
    let backend_listener = TcpListener::bind("127.0.0.1:0").await.unwrap();
    let backend_addr = backend_listener.local_addr().unwrap();

    tokio::spawn(async move {
        loop {
            let (stream, _) = match backend_listener.accept().await {
                Ok(s) => s,
                Err(_) => break,
            };
            let io = TokioIo::new(stream);
            let rec = rec_clone.clone();
            tokio::spawn(async move {
                let service = service_fn(move |mut req: Request<hyper::body::Incoming>| {
                    let rec = rec.clone();
                    async move {
                        while let Some(chunk) = req.body_mut().frame().await {
                            if let Ok(frame) = chunk {
                                if let Some(data) = frame.data_ref() {
                                    rec.fetch_add(data.len(), Ordering::Relaxed);
                                }
                            }
                        }
                        Ok::<_, Infallible>(
                            Response::builder()
                                .status(StatusCode::OK)
                                .body(Full::new(Bytes::from("upload complete")).boxed())
                                .unwrap(),
                        )
                    }
                });
                let _ = ConnBuilder::new(TokioExecutor::new())
                    .serve_connection_with_upgrades(io, service)
                    .await;
            });
        }
    });

    // 2. Setup proxy
    let cfg = Config::load("config.toml").unwrap();
    let backend = Arc::new(Backend::new(
        backend_addr,
        1,
        CircuitBreakerConfig::default(),
    ));
    let state = Arc::new(AppState::new(
        vec![backend],
        Box::new(l7_load_balancer::balancer::RoundRobin::new()),
        cfg,
    ));
    let state_holder = AppStateHolder::from_state(state);

    let cancel_token = CancellationToken::new();
    let proxy_listener = TcpListener::bind("127.0.0.1:0").await.unwrap();
    let proxy_addr = proxy_listener.local_addr().unwrap();
    drop(proxy_listener);

    let sh = state_holder.clone();
    let c = cancel_token.clone();
    tokio::spawn(async move {
        let _ = server::run(proxy_addr, sh, c).await;
    });

    tokio::time::sleep(Duration::from_millis(50)).await;

    // 3. Client streams 10 chunks of 64KB (640 KB total)
    let chunk_size = 64 * 1024;
    let num_chunks = 10;
    let total_expected = chunk_size * num_chunks;

    let chunks: Vec<Result<Frame<Bytes>, Infallible>> = (0..num_chunks)
        .map(|_| Ok(Frame::data(Bytes::from(vec![0x42u8; chunk_size]))))
        .collect();
    let stream_body = StreamBody::new(futures_util::stream::iter(chunks));

    let tcp_stream = tokio::net::TcpStream::connect(proxy_addr).await.unwrap();
    let io = TokioIo::new(tcp_stream);
    let (mut sender, conn) = hyper::client::conn::http1::handshake(io).await.unwrap();
    tokio::spawn(async move {
        let _ = conn.with_upgrades().await;
    });

    let req = Request::builder()
        .method("POST")
        .uri(format!("http://{}/stream", proxy_addr))
        .header("Host", "localhost")
        .header("Transfer-Encoding", "chunked")
        .body(stream_body.boxed())
        .unwrap();

    let resp = sender.send_request(req).await.unwrap();
    assert_eq!(resp.status(), StatusCode::OK);
    assert_eq!(received_bytes.load(Ordering::Relaxed), total_expected);

    cancel_token.cancel();
}

#[tokio::test]
async fn test_drain_on_shutdown() {
    // Mock backend that delays response by 200ms
    let backend_listener = TcpListener::bind("127.0.0.1:0").await.unwrap();
    let backend_addr = backend_listener.local_addr().unwrap();

    tokio::spawn(async move {
        loop {
            let (stream, _) = match backend_listener.accept().await {
                Ok(s) => s,
                Err(_) => break,
            };
            let io = TokioIo::new(stream);
            tokio::spawn(async move {
                let service = service_fn(|_req: Request<hyper::body::Incoming>| async move {
                    tokio::time::sleep(Duration::from_millis(200)).await;
                    Ok::<_, Infallible>(
                        Response::builder()
                            .status(StatusCode::OK)
                            .body(Full::new(Bytes::from("drained ok")).boxed())
                            .unwrap(),
                    )
                });
                let _ = ConnBuilder::new(TokioExecutor::new())
                    .serve_connection_with_upgrades(io, service)
                    .await;
            });
        }
    });

    let mut cfg = Config::load("config.toml").unwrap();
    cfg.shutdown.drain_timeout_secs = 2;

    let backend = Arc::new(Backend::new(
        backend_addr,
        1,
        CircuitBreakerConfig::default(),
    ));
    let state = Arc::new(AppState::new(
        vec![backend],
        Box::new(l7_load_balancer::balancer::RoundRobin::new()),
        cfg,
    ));
    let state_holder = AppStateHolder::from_state(state);

    let cancel_token = CancellationToken::new();
    let proxy_listener = TcpListener::bind("127.0.0.1:0").await.unwrap();
    let proxy_addr = proxy_listener.local_addr().unwrap();
    drop(proxy_listener);

    let sh = state_holder.clone();
    let c = cancel_token.clone();
    let server_handle = tokio::spawn(async move {
        let _ = server::run(proxy_addr, sh, c).await;
    });

    tokio::time::sleep(Duration::from_millis(50)).await;

    // Send request
    let stream = tokio::net::TcpStream::connect(proxy_addr).await.unwrap();
    let io = TokioIo::new(stream);
    let (mut sender, conn) = hyper::client::conn::http1::handshake(io).await.unwrap();
    tokio::spawn(async move {
        let _ = conn.with_upgrades().await;
    });

    let req = Request::builder()
        .method("GET")
        .uri(format!("http://{}/", proxy_addr))
        .header("Host", "localhost")
        .body(Full::new(Bytes::new()).boxed())
        .unwrap();

    let req_fut = sender.send_request(req);

    // Cancel while request is in-flight
    tokio::time::sleep(Duration::from_millis(40)).await;
    cancel_token.cancel();

    // The in-flight request finishes successfully within the 2-second drain window
    let resp = req_fut.await.unwrap();
    assert_eq!(resp.status(), StatusCode::OK);

    let body = resp.into_body().collect().await.unwrap().to_bytes();
    assert_eq!(body, "drained ok");

    // Server should exit gracefully
    let _ = server_handle.await;
}

#[tokio::test]
async fn test_distribution_per_algorithm() {
    let cb_cfg = CircuitBreakerConfig::default();
    let b1 = Arc::new(Backend::new(
        "10.0.0.1:8080".parse().unwrap(),
        1,
        cb_cfg.clone(),
    ));
    let b2 = Arc::new(Backend::new(
        "10.0.0.2:8080".parse().unwrap(),
        2,
        cb_cfg.clone(),
    ));
    let b3 = Arc::new(Backend::new("10.0.0.3:8080".parse().unwrap(), 3, cb_cfg));

    let backends = vec![b1.clone(), b2.clone(), b3.clone()];

    // Test Weighted Round Robin: ratio should reflect weights 1 : 2 : 3
    let wrr = WeightedRoundRobin::new(&backends, Duration::from_secs(0));
    let mut counts = HashMap::new();
    for _ in 0..60 {
        if let Some(b) = wrr.select(&backends) {
            *counts.entry(b.addr).or_insert(0) += 1;
        }
    }

    assert_eq!(*counts.get(&b1.addr).unwrap(), 10);
    assert_eq!(*counts.get(&b2.addr).unwrap(), 20);
    assert_eq!(*counts.get(&b3.addr).unwrap(), 30);

    // Test Power of Two Choices: chooses backend with lower active connections
    let p2c = PowerOfTwoChoices::new();
    b1.inc_connections();
    b1.inc_connections(); // b1 has 2 conns
    b2.inc_connections(); // b2 has 1 conn
                          // b3 has 0 conns

    let mut b3_selected_count = 0;
    for _ in 0..50 {
        if let Some(b) = p2c.select(&backends) {
            if b.addr == b3.addr {
                b3_selected_count += 1;
            }
        }
    }
    // b3 (0 active conns) should win the majority of random pairs
    assert!(b3_selected_count > 25);
}

#[tokio::test]
async fn test_chaos_backend_killed_mid_request() {
    // Backend accepts connection and immediately aborts / drops it
    let backend_listener = TcpListener::bind("127.0.0.1:0").await.unwrap();
    let backend_addr = backend_listener.local_addr().unwrap();

    tokio::spawn(async move {
        while let Ok((stream, _)) = backend_listener.accept().await {
            // Drop stream immediately without reading or writing
            drop(stream);
        }
    });

    let cfg = Config::load("config.toml").unwrap();
    let backend = Arc::new(Backend::new(
        backend_addr,
        1,
        CircuitBreakerConfig::default(),
    ));
    let state = Arc::new(AppState::new(
        vec![backend],
        Box::new(l7_load_balancer::balancer::RoundRobin::new()),
        cfg,
    ));
    let state_holder = AppStateHolder::from_state(state);

    let cancel_token = CancellationToken::new();
    let proxy_listener = TcpListener::bind("127.0.0.1:0").await.unwrap();
    let proxy_addr = proxy_listener.local_addr().unwrap();
    drop(proxy_listener);

    let sh = state_holder.clone();
    let c = cancel_token.clone();
    tokio::spawn(async move {
        let _ = server::run(proxy_addr, sh, c).await;
    });

    tokio::time::sleep(Duration::from_millis(50)).await;

    // Send request: proxy catches abrupt connection reset and returns 502 cleanly
    let stream = tokio::net::TcpStream::connect(proxy_addr).await.unwrap();
    let io = TokioIo::new(stream);
    let (mut sender, conn) = hyper::client::conn::http1::handshake(io).await.unwrap();
    tokio::spawn(async move {
        let _ = conn.with_upgrades().await;
    });

    let req = Request::builder()
        .method("GET")
        .uri(format!("http://{}/", proxy_addr))
        .header("Host", "localhost")
        .body(Full::new(Bytes::new()).boxed())
        .unwrap();

    let resp = sender.send_request(req).await.unwrap();
    assert_eq!(resp.status(), StatusCode::BAD_GATEWAY);

    cancel_token.cancel();
}

#[tokio::test]
async fn test_chaos_very_slow_backend_timeout() {
    // Backend that takes 800ms
    let backend_listener = TcpListener::bind("127.0.0.1:0").await.unwrap();
    let backend_addr = backend_listener.local_addr().unwrap();

    tokio::spawn(async move {
        while let Ok((stream, _)) = backend_listener.accept().await {
            let io = TokioIo::new(stream);
            tokio::spawn(async move {
                let service = service_fn(|_req: Request<hyper::body::Incoming>| async move {
                    tokio::time::sleep(Duration::from_millis(1500)).await;
                    Ok::<_, Infallible>(
                        Response::builder()
                            .status(StatusCode::OK)
                            .body(Full::new(Bytes::from("slow response")).boxed())
                            .unwrap(),
                    )
                });
                let _ = ConnBuilder::new(TokioExecutor::new())
                    .serve_connection_with_upgrades(io, service)
                    .await;
            });
        }
    });

    // Configure upstream response header timeout to 1 second
    let mut cfg = Config::load("config.toml").unwrap();
    cfg.timeouts.upstream_response_header_secs = 1;
    cfg.retry.enabled = false;

    let backend = Arc::new(Backend::new(
        backend_addr,
        1,
        CircuitBreakerConfig::default(),
    ));
    let state = Arc::new(AppState::new(
        vec![backend.clone()],
        Box::new(l7_load_balancer::balancer::RoundRobin::new()),
        cfg,
    ));
    let state_holder = AppStateHolder::from_state(state);

    let cancel_token = CancellationToken::new();
    let proxy_listener = TcpListener::bind("127.0.0.1:0").await.unwrap();
    let proxy_addr = proxy_listener.local_addr().unwrap();
    drop(proxy_listener);

    let sh = state_holder.clone();
    let c = cancel_token.clone();
    tokio::spawn(async move {
        let _ = server::run(proxy_addr, sh, c).await;
    });

    tokio::time::sleep(Duration::from_millis(50)).await;

    // Send request that triggers timeout
    let stream = tokio::net::TcpStream::connect(proxy_addr).await.unwrap();
    let io = TokioIo::new(stream);
    let (mut sender, conn) = hyper::client::conn::http1::handshake(io).await.unwrap();
    tokio::spawn(async move {
        let _ = conn.with_upgrades().await;
    });

    let req = Request::builder()
        .method("GET")
        .uri(format!("http://{}/", proxy_addr))
        .header("Host", "localhost")
        .body(Full::new(Bytes::new()).boxed())
        .unwrap();

    let resp = sender.send_request(req).await.unwrap();
    // Upstream header timeout triggers 504 Gateway Timeout
    assert_eq!(resp.status(), StatusCode::GATEWAY_TIMEOUT);

    cancel_token.cancel();
}

#[tokio::test]
async fn test_chaos_intermittent_5xx_retries_and_circuit_breaker() {
    // Backend 1 always returns 500
    let b1_listener = TcpListener::bind("127.0.0.1:0").await.unwrap();
    let b1_addr = b1_listener.local_addr().unwrap();

    tokio::spawn(async move {
        while let Ok((stream, _)) = b1_listener.accept().await {
            let io = TokioIo::new(stream);
            tokio::spawn(async move {
                let service = service_fn(|_req: Request<hyper::body::Incoming>| async move {
                    Ok::<_, Infallible>(
                        Response::builder()
                            .status(StatusCode::SERVICE_UNAVAILABLE)
                            .body(Full::new(Bytes::from("b1 503 unavailable")).boxed())
                            .unwrap(),
                    )
                });
                let _ = ConnBuilder::new(TokioExecutor::new())
                    .serve_connection_with_upgrades(io, service)
                    .await;
            });
        }
    });

    // Backend 2 always returns 200 OK
    let b2_listener = TcpListener::bind("127.0.0.1:0").await.unwrap();
    let b2_addr = b2_listener.local_addr().unwrap();

    tokio::spawn(async move {
        while let Ok((stream, _)) = b2_listener.accept().await {
            let io = TokioIo::new(stream);
            tokio::spawn(async move {
                let service = service_fn(|_req: Request<hyper::body::Incoming>| async move {
                    Ok::<_, Infallible>(
                        Response::builder()
                            .status(StatusCode::OK)
                            .body(Full::new(Bytes::from("b2 success")).boxed())
                            .unwrap(),
                    )
                });
                let _ = ConnBuilder::new(TokioExecutor::new())
                    .serve_connection_with_upgrades(io, service)
                    .await;
            });
        }
    });

    let mut cfg = Config::load("config.toml").unwrap();
    cfg.retry.enabled = true;
    cfg.retry.max_attempts = 2;
    cfg.retry.initial_backoff_ms = 1;

    let cb_cfg = CircuitBreakerConfig {
        enabled: true,
        consecutive_failures: 2,
        cooldown_duration: Duration::from_millis(500),
        half_open_max_trials: 1,
        half_open_successes_to_close: 1,
    };

    let b1 = Arc::new(Backend::new(b1_addr, 1, cb_cfg.clone()));
    let b2 = Arc::new(Backend::new(b2_addr, 1, cb_cfg));

    let state = Arc::new(AppState::new(
        vec![b1.clone(), b2.clone()],
        Box::new(l7_load_balancer::balancer::RoundRobin::new()),
        cfg,
    ));
    let state_holder = AppStateHolder::from_state(state);

    let cancel_token = CancellationToken::new();
    let proxy_listener = TcpListener::bind("127.0.0.1:0").await.unwrap();
    let proxy_addr = proxy_listener.local_addr().unwrap();
    drop(proxy_listener);

    let sh = state_holder.clone();
    let c = cancel_token.clone();
    tokio::spawn(async move {
        let _ = server::run(proxy_addr, sh, c).await;
    });

    tokio::time::sleep(Duration::from_millis(50)).await;

    // Send 2 requests: B1 fails, proxy retries to B2 and succeeds with 200 OK
    for _ in 0..2 {
        let stream = tokio::net::TcpStream::connect(proxy_addr).await.unwrap();
        let io = TokioIo::new(stream);
        let (mut sender, conn) = hyper::client::conn::http1::handshake(io).await.unwrap();
        tokio::spawn(async move {
            let _ = conn.await;
        });

        let req = Request::builder()
            .method("GET")
            .uri(format!("http://{}/", proxy_addr))
            .header("Host", "localhost")
            .body(Full::new(Bytes::new()).boxed())
            .unwrap();

        let resp = sender.send_request(req).await.unwrap();
        assert_eq!(resp.status(), StatusCode::OK);
        let body = resp.into_body().collect().await.unwrap().to_bytes();
        assert_eq!(body, "b2 success");
    }

    // After 2 consecutive failures, Backend 1's circuit breaker has transitioned to OPEN
    assert_eq!(b1.circuit_breaker.state(), STATE_OPEN);

    cancel_token.cancel();
}
