use bytes::Bytes;
use http_body_util::{BodyExt, Full};
use hyper::service::service_fn;
use hyper::{Request, Response, StatusCode};
use hyper_util::rt::{TokioExecutor, TokioIo};
use hyper_util::server::conn::auto::Builder as ConnBuilder;
use l7_load_balancer::backend::Backend;
use l7_load_balancer::balancer::RoundRobin;
use l7_load_balancer::circuit_breaker::CircuitBreakerConfig;
use l7_load_balancer::config::{BackendConfig, Config, PoolConfig, RateLimitConfig, TlsConfig};
use l7_load_balancer::rate_limit::RateLimiter;
use l7_load_balancer::server;
use l7_load_balancer::state::{AppState, AppStateHolder};
use l7_load_balancer::tls;
use std::collections::HashMap;
use std::convert::Infallible;
use std::net::SocketAddr;
use std::sync::Arc;
use std::time::Duration;
use tokio::net::TcpListener;
use tokio_rustls::TlsConnector;
use tokio_util::sync::CancellationToken;

#[tokio::test]
async fn test_rate_limiter_token_bucket_logic() {
    let cfg = RateLimitConfig {
        enabled: true,
        requests_per_second: 10.0,
        burst: 3,
        max_entries: 100,
        cleanup_interval_secs: 60,
    };
    let limiter = RateLimiter::from_config(&cfg);
    let ip1: std::net::IpAddr = "192.168.1.100".parse().unwrap();
    let ip2: std::net::IpAddr = "192.168.1.200".parse().unwrap();

    // First 3 requests should pass (burst = 3)
    assert!(limiter.check(ip1).is_ok());
    assert!(limiter.check(ip1).is_ok());
    assert!(limiter.check(ip1).is_ok());

    // 4th request exceeds burst limit
    let err = limiter.check(ip1).err();
    assert!(err.is_some());
    let retry_after = err.unwrap();
    assert!(retry_after >= 1);

    // Different IP has its own token bucket and passes
    assert!(limiter.check(ip2).is_ok());

    // Wait 250ms (refilling tokens at 10 rps)
    tokio::time::sleep(Duration::from_millis(250)).await;
    assert!(limiter.check(ip1).is_ok());
}

#[tokio::test]
async fn test_rate_limiter_bounded_memory_eviction() {
    let cfg = RateLimitConfig {
        enabled: true,
        requests_per_second: 10.0,
        burst: 5,
        max_entries: 3,
        cleanup_interval_secs: 60,
    };
    let limiter = RateLimiter::from_config(&cfg);

    // Insert 5 different IPs
    for i in 1..=5 {
        let ip: std::net::IpAddr = format!("10.0.0.{}", i).parse().unwrap();
        let _ = limiter.check(ip);
    }

    // Sleep 15ms and evict entries idle for > 10ms
    tokio::time::sleep(Duration::from_millis(15)).await;
    let evicted = limiter.prune_idle_older_than(Duration::from_millis(10));
    assert!(evicted > 0);

    // After eviction, checked IP gets a fresh token bucket
    let ip: std::net::IpAddr = "10.0.0.1".parse().unwrap();
    assert!(limiter.check(ip).is_ok());
}

#[tokio::test]
async fn test_proxy_rate_limiting_returns_429() {
    // 1. Mock backend
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
                    Ok::<_, Infallible>(
                        Response::builder()
                            .status(StatusCode::OK)
                            .body(Full::new(Bytes::from("hello from backend")))
                            .unwrap(),
                    )
                });
                let _ = ConnBuilder::new(TokioExecutor::new())
                    .serve_connection(io, service)
                    .await;
            });
        }
    });

    // 2. Setup Config with rate limit burst = 2, rps = 1.0
    let mut cfg = Config::load("config.toml").unwrap();
    cfg.rate_limit.enabled = true;
    cfg.rate_limit.burst = 2;
    cfg.rate_limit.requests_per_second = 1.0;

    let backend = Arc::new(Backend::new(
        backend_addr,
        1,
        CircuitBreakerConfig::default(),
    ));
    let state = Arc::new(AppState::new(
        vec![backend],
        Box::new(RoundRobin::new()),
        cfg,
    ));
    let state_holder = AppStateHolder::from_state(state);

    let cancel_token = CancellationToken::new();
    let cancel_clone = cancel_token.clone();

    let proxy_listener = TcpListener::bind("127.0.0.1:0").await.unwrap();
    let proxy_addr = proxy_listener.local_addr().unwrap();
    drop(proxy_listener);

    let sh_clone = state_holder.clone();
    tokio::spawn(async move {
        let _ = server::run(proxy_addr, sh_clone, cancel_clone).await;
    });

    tokio::time::sleep(Duration::from_millis(50)).await;

    // Helper client request
    async fn make_req(addr: SocketAddr) -> (StatusCode, Option<String>, String) {
        let stream = tokio::net::TcpStream::connect(addr).await.unwrap();
        let io = TokioIo::new(stream);
        let (mut sender, conn) = hyper::client::conn::http1::handshake(io).await.unwrap();
        tokio::spawn(async move {
            let _ = conn.await;
        });

        let req = Request::builder()
            .method("GET")
            .uri(format!("http://{}/", addr))
            .header("Host", "localhost")
            .body(Full::new(Bytes::new()))
            .unwrap();

        let resp = sender.send_request(req).await.unwrap();
        let status = resp.status();
        let retry_after = resp
            .headers()
            .get(hyper::header::RETRY_AFTER)
            .map(|v| v.to_str().unwrap().to_string());
        let body_bytes = resp.into_body().collect().await.unwrap().to_bytes();
        let body = String::from_utf8_lossy(&body_bytes).to_string();
        (status, retry_after, body)
    }

    // First 2 requests succeed (burst = 2)
    let (s1, _, _) = make_req(proxy_addr).await;
    assert_eq!(s1, StatusCode::OK);

    let (s2, _, _) = make_req(proxy_addr).await;
    assert_eq!(s2, StatusCode::OK);

    // 3rd request rejected with 429 and Retry-After header
    let (s3, retry_after, body) = make_req(proxy_addr).await;
    assert_eq!(s3, StatusCode::TOO_MANY_REQUESTS);
    assert!(retry_after.is_some());
    assert!(body.contains("429 Too Many Requests"));

    cancel_token.cancel();
}

#[tokio::test]
async fn test_global_concurrency_load_shedding_503() {
    // 1. Slow mock backend that waits 300ms
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
                    tokio::time::sleep(Duration::from_millis(300)).await;
                    Ok::<_, Infallible>(
                        Response::builder()
                            .status(StatusCode::OK)
                            .body(Full::new(Bytes::from("slow response")))
                            .unwrap(),
                    )
                });
                let _ = ConnBuilder::new(TokioExecutor::new())
                    .serve_connection(io, service)
                    .await;
            });
        }
    });

    // 2. Proxy configured with max_concurrent_connections = 2
    let mut cfg = Config::load("config.toml").unwrap();
    cfg.limits.max_concurrent_connections = 2;

    let backend = Arc::new(Backend::new(
        backend_addr,
        1,
        CircuitBreakerConfig::default(),
    ));
    let state = Arc::new(AppState::new(
        vec![backend],
        Box::new(RoundRobin::new()),
        cfg,
    ));
    let state_holder = AppStateHolder::from_state(state);

    let cancel_token = CancellationToken::new();
    let cancel_clone = cancel_token.clone();

    let proxy_listener = TcpListener::bind("127.0.0.1:0").await.unwrap();
    let proxy_addr = proxy_listener.local_addr().unwrap();
    drop(proxy_listener);

    let sh_clone = state_holder.clone();
    tokio::spawn(async move {
        let _ = server::run(proxy_addr, sh_clone, cancel_clone).await;
    });

    tokio::time::sleep(Duration::from_millis(50)).await;

    async fn send_req(addr: SocketAddr) -> (StatusCode, String) {
        let stream = tokio::net::TcpStream::connect(addr).await.unwrap();
        let io = TokioIo::new(stream);
        let (mut sender, conn) = hyper::client::conn::http1::handshake(io).await.unwrap();
        tokio::spawn(async move {
            let _ = conn.await;
        });

        let req = Request::builder()
            .method("GET")
            .uri(format!("http://{}/", addr))
            .header("Host", "localhost")
            .body(Full::new(Bytes::new()))
            .unwrap();

        let resp = sender.send_request(req).await.unwrap();
        let status = resp.status();
        let body_bytes = resp.into_body().collect().await.unwrap().to_bytes();
        let body = String::from_utf8_lossy(&body_bytes).to_string();
        (status, body)
    }

    // Launch 2 slow concurrent requests
    let h1 = tokio::spawn(send_req(proxy_addr));
    let h2 = tokio::spawn(send_req(proxy_addr));

    // Give them a moment to establish in-flight count (2/2)
    tokio::time::sleep(Duration::from_millis(60)).await;

    // 3rd concurrent request hits the limit and must be load-shed with 503
    let (status3, body3) = send_req(proxy_addr).await;
    assert_eq!(status3, StatusCode::SERVICE_UNAVAILABLE);
    assert!(body3.contains("Server Overloaded"));

    // The two in-flight requests finish with 200 OK
    let (status1, _) = h1.await.unwrap();
    let (status2, _) = h2.await.unwrap();
    assert_eq!(status1, StatusCode::OK);
    assert_eq!(status2, StatusCode::OK);

    // After in-flight requests finish, new requests succeed again
    let (status4, _) = send_req(proxy_addr).await;
    assert_eq!(status4, StatusCode::OK);

    cancel_token.cancel();
}

#[tokio::test]
async fn test_tls_termination_and_forwarded_proto() {
    tls::init_crypto_provider();

    // 1. Mock backend
    let backend_listener = TcpListener::bind("127.0.0.1:0").await.unwrap();
    let backend_addr = backend_listener.local_addr().unwrap();

    let proto_received = Arc::new(parking_lot::Mutex::new(None::<String>));
    let proto_received_clone = proto_received.clone();

    tokio::spawn(async move {
        loop {
            let (stream, _) = match backend_listener.accept().await {
                Ok(s) => s,
                Err(_) => break,
            };
            let io = TokioIo::new(stream);
            let pr = proto_received_clone.clone();
            tokio::spawn(async move {
                let service = service_fn(move |req: Request<hyper::body::Incoming>| {
                    let pr = pr.clone();
                    async move {
                        if let Some(proto) = req.headers().get("x-forwarded-proto") {
                            *pr.lock() = Some(proto.to_str().unwrap().to_string());
                        }
                        Ok::<_, Infallible>(
                            Response::builder()
                                .status(StatusCode::OK)
                                .body(Full::new(Bytes::from("tls backend ok")))
                                .unwrap(),
                        )
                    }
                });
                let _ = ConnBuilder::new(TokioExecutor::new())
                    .serve_connection(io, service)
                    .await;
            });
        }
    });

    // 2. Setup Proxy Config with TLS enabled
    let mut cfg = Config::load("config.toml").unwrap();
    cfg.backends = vec![BackendConfig::new(backend_addr.to_string(), 1)];
    cfg.tls = TlsConfig {
        enabled: true,
        cert_path: "tests/fixtures/cert.pem".to_string(),
        key_path: "tests/fixtures/key.pem".to_string(),
        alpn: vec!["http/1.1".to_string()],
    };

    let state = Arc::new(AppState::try_from_config(cfg.clone(), &[]).unwrap());
    let state_holder = AppStateHolder::from_state(state);

    let cancel_token = CancellationToken::new();
    let cancel_clone = cancel_token.clone();

    let proxy_listener = TcpListener::bind("127.0.0.1:0").await.unwrap();
    let proxy_addr = proxy_listener.local_addr().unwrap();
    drop(proxy_listener);

    let sh_clone = state_holder.clone();
    tokio::spawn(async move {
        let _ = server::run(proxy_addr, sh_clone, cancel_clone).await;
    });

    tokio::time::sleep(Duration::from_millis(80)).await;

    // 3. Connect via TLS client (using DangerAcceptAnyServerCert to trust self-signed test cert)
    let client_tls_config = tls::create_client_tls_config(true);
    let connector = TlsConnector::from(client_tls_config);
    let tcp_stream = tokio::net::TcpStream::connect(proxy_addr).await.unwrap();
    let server_name = rustls::pki_types::ServerName::try_from("localhost").unwrap();
    let tls_stream = connector.connect(server_name, tcp_stream).await.unwrap();
    let io = TokioIo::new(tls_stream);

    let (mut sender, conn) = hyper::client::conn::http1::handshake(io).await.unwrap();
    tokio::spawn(async move {
        let _ = conn.await;
    });

    let req = Request::builder()
        .method("GET")
        .uri(format!("https://localhost:{}/", proxy_addr.port()))
        .header("Host", "localhost")
        .body(Full::new(Bytes::new()))
        .unwrap();

    let resp = sender.send_request(req).await.unwrap();
    assert_eq!(resp.status(), StatusCode::OK);

    let body_bytes = resp.into_body().collect().await.unwrap().to_bytes();
    assert_eq!(body_bytes, "tls backend ok");

    // Verify backend received X-Forwarded-Proto: https
    let received_proto = proto_received.lock().clone();
    assert_eq!(received_proto, Some("https".to_string()));

    cancel_token.cancel();
}

#[tokio::test]
async fn test_upstream_backend_tls() {
    tls::init_crypto_provider();

    // 1. Mock HTTPS backend
    let acceptor = tls::create_tls_acceptor(
        "tests/fixtures/cert.pem",
        "tests/fixtures/key.pem",
        &["http/1.1".to_string()],
    )
    .unwrap();

    let backend_listener = TcpListener::bind("127.0.0.1:0").await.unwrap();
    let backend_addr = backend_listener.local_addr().unwrap();

    tokio::spawn(async move {
        loop {
            let (stream, _) = match backend_listener.accept().await {
                Ok(s) => s,
                Err(_) => break,
            };
            let acceptor = acceptor.clone();
            tokio::spawn(async move {
                let tls_stream = match acceptor.accept(stream).await {
                    Ok(s) => s,
                    Err(_) => return,
                };
                let io = TokioIo::new(tls_stream);
                let service = service_fn(|_req: Request<hyper::body::Incoming>| async move {
                    Ok::<_, Infallible>(
                        Response::builder()
                            .status(StatusCode::OK)
                            .body(Full::new(Bytes::from("hello from secure backend")))
                            .unwrap(),
                    )
                });
                let _ = ConnBuilder::new(TokioExecutor::new())
                    .serve_connection(io, service)
                    .await;
            });
        }
    });

    // 2. Setup proxy with upstream backend TLS enabled and tls_insecure = true
    let mut cfg = Config::load("config.toml").unwrap();
    let mut backend_cfg = BackendConfig::new(backend_addr.to_string(), 1);
    backend_cfg.tls = true;
    backend_cfg.tls_insecure = true;
    backend_cfg.tls_sni = Some("localhost".to_string());

    let mut pools = HashMap::new();
    pools.insert(
        "main".to_string(),
        PoolConfig {
            algorithm: l7_load_balancer::config::Algorithm::RoundRobin,
            backends: vec![backend_cfg],
            health_check: None,
            hash_key: None,
        },
    );
    cfg.pools = pools;

    let state = Arc::new(AppState::try_from_config(cfg, &[]).unwrap());
    let state_holder = AppStateHolder::from_state(state);

    let cancel_token = CancellationToken::new();
    let cancel_clone = cancel_token.clone();

    let proxy_listener = TcpListener::bind("127.0.0.1:0").await.unwrap();
    let proxy_addr = proxy_listener.local_addr().unwrap();
    drop(proxy_listener);

    let sh_clone = state_holder.clone();
    tokio::spawn(async move {
        let _ = server::run(proxy_addr, sh_clone, cancel_clone).await;
    });

    tokio::time::sleep(Duration::from_millis(80)).await;

    // 3. Client sends plain HTTP to proxy
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
        .body(Full::new(Bytes::new()))
        .unwrap();

    let resp = sender.send_request(req).await.unwrap();
    assert_eq!(resp.status(), StatusCode::OK);

    let body_bytes = resp.into_body().collect().await.unwrap().to_bytes();
    assert_eq!(body_bytes, "hello from secure backend");

    cancel_token.cancel();
}

#[tokio::test]
async fn test_tls_cert_hot_reload() {
    tls::init_crypto_provider();

    let temp_dir = std::env::temp_dir();
    let temp_config_path = temp_dir.join(format!("l7lb_test_tls_{}.toml", std::process::id()));

    let valid_toml = r#"
listen_addr = "127.0.0.1:9099"
algorithm = "round_robin"

[[backends]]
addr = "127.0.0.1:8081"
weight = 1

[tls]
enabled = true
cert_path = "tests/fixtures/cert.pem"
key_path = "tests/fixtures/key.pem"
alpn = ["http/1.1"]
"#;

    tokio::fs::write(&temp_config_path, valid_toml)
        .await
        .unwrap();

    let config = Config::load(&temp_config_path).unwrap();
    let (holder, cancel) = AppStateHolder::new(config, temp_config_path.clone());

    assert!(holder.load().tls_acceptor.is_some());

    // Valid reload succeeds
    assert!(holder.reload().is_ok());
    assert!(holder.load().tls_acceptor.is_some());

    // Write invalid cert path into config file
    let invalid_toml = r#"
listen_addr = "127.0.0.1:9099"
algorithm = "round_robin"

[[backends]]
addr = "127.0.0.1:8081"
weight = 1

[tls]
enabled = true
cert_path = "tests/fixtures/non_existent_cert.pem"
key_path = "tests/fixtures/key.pem"
alpn = ["http/1.1"]
"#;
    tokio::fs::write(&temp_config_path, invalid_toml)
        .await
        .unwrap();

    // Reload must fail gracefully and preserve old valid state
    let reload_res = holder.reload();
    assert!(reload_res.is_err());
    assert!(holder.load().tls_acceptor.is_some());

    cancel.cancel();
    let _ = tokio::fs::remove_file(&temp_config_path).await;
}
