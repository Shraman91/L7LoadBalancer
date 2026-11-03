use bytes::Bytes;
use http_body_util::{BodyExt, Full};
use hyper::service::service_fn;
use hyper::{Request, Response, StatusCode};
use hyper_util::rt::{TokioExecutor, TokioIo};
use hyper_util::server::conn::auto::Builder as ConnBuilder;
use l7_load_balancer::admin;
use l7_load_balancer::backend::Backend;
use l7_load_balancer::balancer::RoundRobin;
use l7_load_balancer::circuit_breaker::CircuitBreakerConfig;
use l7_load_balancer::config::{BackendConfig, Config, PoolConfig};
use l7_load_balancer::server;
use l7_load_balancer::state::{AppState, AppStateHolder};
use std::collections::HashMap;
use std::convert::Infallible;
use std::sync::Arc;
use std::time::Duration;
use tokio::net::TcpListener;
use tokio_util::sync::CancellationToken;

#[tokio::test]
async fn test_prometheus_metrics_endpoint_and_request_metrics() {
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
                            .body(Full::new(Bytes::from("metrics test ok")))
                            .unwrap(),
                    )
                });
                let _ = ConnBuilder::new(TokioExecutor::new())
                    .serve_connection(io, service)
                    .await;
            });
        }
    });

    // 2. Setup Config
    let mut cfg = Config::load("config.toml").unwrap();
    let mut pools = HashMap::new();
    pools.insert(
        "web".to_string(),
        PoolConfig {
            algorithm: l7_load_balancer::config::Algorithm::RoundRobin,
            backends: vec![BackendConfig::new(backend_addr.to_string(), 1)],
            health_check: None,
            hash_key: None,
        },
    );
    cfg.pools = pools;
    cfg.default_pool = "web".to_string();

    let state = Arc::new(AppState::try_from_config(cfg, &[]).unwrap());
    let state_holder = AppStateHolder::from_state(state);

    let cancel_token = CancellationToken::new();

    // Start proxy
    let proxy_listener = TcpListener::bind("127.0.0.1:0").await.unwrap();
    let proxy_addr = proxy_listener.local_addr().unwrap();
    drop(proxy_listener);

    let sh1 = state_holder.clone();
    let c1 = cancel_token.clone();
    tokio::spawn(async move {
        let _ = server::run(proxy_addr, sh1, c1).await;
    });

    // Start admin server
    let admin_listener = TcpListener::bind("127.0.0.1:0").await.unwrap();
    let admin_addr = admin_listener.local_addr().unwrap();
    drop(admin_listener);

    let sh2 = state_holder.clone();
    let c2 = cancel_token.clone();
    tokio::spawn(async move {
        let _ = admin::run_admin_server(admin_addr, sh2, c2).await;
    });

    tokio::time::sleep(Duration::from_millis(60)).await;

    // 3. Send requests through proxy to generate traffic
    for _ in 0..3 {
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
    }

    // 4. Scrape Prometheus /metrics from admin port
    let admin_stream = tokio::net::TcpStream::connect(admin_addr).await.unwrap();
    let admin_io = TokioIo::new(admin_stream);
    let (mut admin_sender, admin_conn) = hyper::client::conn::http1::handshake(admin_io)
        .await
        .unwrap();
    tokio::spawn(async move {
        let _ = admin_conn.await;
    });

    let metrics_req = Request::builder()
        .method("GET")
        .uri(format!("http://{}/metrics", admin_addr))
        .header("Host", "localhost")
        .body(Full::new(Bytes::new()))
        .unwrap();

    let metrics_resp = admin_sender.send_request(metrics_req).await.unwrap();
    assert_eq!(metrics_resp.status(), StatusCode::OK);

    let content_type = metrics_resp
        .headers()
        .get(hyper::header::CONTENT_TYPE)
        .unwrap()
        .to_str()
        .unwrap();
    assert!(content_type.contains("text/plain"));

    let body_bytes = metrics_resp.into_body().collect().await.unwrap().to_bytes();
    let body_str = String::from_utf8_lossy(&body_bytes).to_string();

    // Verify expected Prometheus metrics exist in scrape output
    assert!(body_str.contains("http_requests_total{pool=\"web\",status=\"200\"} 3"));
    assert!(body_str.contains("http_request_duration_seconds_bucket"));
    assert!(body_str.contains("active_connections"));
    assert!(body_str.contains("backend_health"));
    assert!(body_str.contains("backend_circuit_state"));
    assert!(body_str.contains("rate_limited_requests_total"));
    assert!(body_str.contains("load_shed_requests_total"));

    cancel_token.cancel();
}

#[tokio::test]
async fn test_x_request_id_generation_and_propagation() {
    let captured_request_id = Arc::new(parking_lot::Mutex::new(None::<String>));
    let cap_clone = captured_request_id.clone();

    // 1. Mock backend capturing incoming X-Request-Id header
    let backend_listener = TcpListener::bind("127.0.0.1:0").await.unwrap();
    let backend_addr = backend_listener.local_addr().unwrap();

    tokio::spawn(async move {
        loop {
            let (stream, _) = match backend_listener.accept().await {
                Ok(s) => s,
                Err(_) => break,
            };
            let io = TokioIo::new(stream);
            let cap = cap_clone.clone();
            tokio::spawn(async move {
                let service = service_fn(move |req: Request<hyper::body::Incoming>| {
                    let cap = cap.clone();
                    async move {
                        if let Some(h) = req.headers().get("x-request-id") {
                            *cap.lock() = Some(h.to_str().unwrap().to_string());
                        }
                        Ok::<_, Infallible>(
                            Response::builder()
                                .status(StatusCode::OK)
                                .body(Full::new(Bytes::from("hello with request id")))
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

    // 2. Setup proxy
    let cfg = Config::load("config.toml").unwrap();
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
    let proxy_listener = TcpListener::bind("127.0.0.1:0").await.unwrap();
    let proxy_addr = proxy_listener.local_addr().unwrap();
    drop(proxy_listener);

    let sh = state_holder.clone();
    let c = cancel_token.clone();
    tokio::spawn(async move {
        let _ = server::run(proxy_addr, sh, c).await;
    });

    tokio::time::sleep(Duration::from_millis(50)).await;

    // Case A: Request without X-Request-Id -> Proxy generates one
    {
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

        // Header returned to client
        let client_rid = resp
            .headers()
            .get("x-request-id")
            .unwrap()
            .to_str()
            .unwrap();
        assert_eq!(client_rid.len(), 32);

        // Header forwarded to upstream backend matches
        let upstream_rid = captured_request_id.lock().clone().unwrap();
        assert_eq!(client_rid, upstream_rid);
    }

    // Case B: Request with custom X-Request-Id -> Proxy preserves and propagates it
    {
        let custom_id = "trace-client-abcdef-123456";
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
            .header("x-request-id", custom_id)
            .body(Full::new(Bytes::new()))
            .unwrap();

        let resp = sender.send_request(req).await.unwrap();
        assert_eq!(resp.status(), StatusCode::OK);

        let client_rid = resp
            .headers()
            .get("x-request-id")
            .unwrap()
            .to_str()
            .unwrap();
        assert_eq!(client_rid, custom_id);

        let upstream_rid = captured_request_id.lock().clone().unwrap();
        assert_eq!(upstream_rid, custom_id);
    }

    cancel_token.cancel();
}

#[tokio::test]
async fn test_access_log_json_structure() {
    let raw_json = serde_json::json!({
        "timestamp": 1700000000000u64,
        "request_id": "0123456789abcdef0123456789abcdef",
        "client_ip": "127.0.0.1",
        "method": "POST",
        "path": "/api/v1/resource?query=1",
        "status": 200,
        "pool": "api",
        "backend": "127.0.0.1:9011",
        "latency_ms": 4.52,
        "retries": 1
    });

    let json_str = raw_json.to_string();
    let parsed: serde_json::Value = serde_json::from_str(&json_str).unwrap();

    assert_eq!(parsed["request_id"], "0123456789abcdef0123456789abcdef");
    assert_eq!(parsed["client_ip"], "127.0.0.1");
    assert_eq!(parsed["method"], "POST");
    assert_eq!(parsed["path"], "/api/v1/resource?query=1");
    assert_eq!(parsed["status"], 200);
    assert_eq!(parsed["pool"], "api");
    assert_eq!(parsed["backend"], "127.0.0.1:9011");
    assert_eq!(parsed["latency_ms"], 4.52);
    assert_eq!(parsed["retries"], 1);
}
