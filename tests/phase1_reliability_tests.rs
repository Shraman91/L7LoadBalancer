use http_body_util::Full;
use hyper::body::Bytes;
use hyper::header::{HeaderValue, CONNECTION};
use hyper::service::service_fn;
use hyper::{Request, Response, StatusCode};
use hyper_util::rt::{TokioExecutor, TokioIo};
use hyper_util::server::conn::auto::Builder as ConnBuilder;
use l7_load_balancer::backend::Backend;
use l7_load_balancer::balancer::{LoadBalancer, RoundRobin};
use l7_load_balancer::circuit_breaker::{
    CircuitBreaker, CircuitBreakerConfig, STATE_CLOSED, STATE_HALF_OPEN, STATE_OPEN,
};
use l7_load_balancer::config::Config;
use l7_load_balancer::proxy;
use l7_load_balancer::state::AppState;
use std::convert::Infallible;
use std::net::SocketAddr;
use std::sync::atomic::{AtomicUsize, Ordering};
use std::sync::Arc;
use std::time::Duration;
use tokio::net::TcpListener;

#[test]
fn test_circuit_breaker_transitions() {
    let cb_cfg = CircuitBreakerConfig {
        enabled: true,
        consecutive_failures: 2,
        cooldown_duration: Duration::from_millis(50),
        half_open_max_trials: 1,
        half_open_successes_to_close: 1,
    };
    let cb = CircuitBreaker::new(cb_cfg);

    assert_eq!(cb.state(), STATE_CLOSED);
    assert!(cb.can_attempt());

    // 1st failure
    cb.record_failure();
    assert_eq!(cb.state(), STATE_CLOSED);
    assert!(cb.can_attempt());

    // 2nd failure -> Open
    cb.record_failure();
    assert_eq!(cb.state(), STATE_OPEN);
    assert!(!cb.can_attempt());

    // Wait cooldown -> HalfOpen
    std::thread::sleep(Duration::from_millis(60));
    assert!(cb.can_attempt());
    assert_eq!(cb.state(), STATE_HALF_OPEN);

    // In half-open, exceeded max trials
    assert!(!cb.can_attempt());

    // Success in half-open -> Closed
    cb.record_success();
    assert_eq!(cb.state(), STATE_CLOSED);
    assert!(cb.can_attempt());
}

#[test]
fn test_slow_start_weight_ramp() {
    let cb_cfg = CircuitBreakerConfig::default();
    let addr: SocketAddr = "127.0.0.1:9999".parse().unwrap();
    let backend = Backend::new(addr, 100, cb_cfg);

    // Initial weight within zero duration is full weight
    assert_eq!(backend.effective_weight(Duration::ZERO), 100);

    // Duration of 10s: at t=0, effective weight is >= 1
    let ramp_dur = Duration::from_secs(10);
    let initial = backend.effective_weight(ramp_dur);
    assert!(initial <= 10);
}

#[tokio::test]
async fn test_retry_idempotent_vs_non_idempotent() {
    // Spin up 2 mock backends:
    // Backend 1 fails with 502
    // Backend 2 succeeds with 200 OK
    let b1_calls = Arc::new(AtomicUsize::new(0));
    let b1_calls_clone = b1_calls.clone();
    let b1_listener = TcpListener::bind("127.0.0.1:0").await.unwrap();
    let b1_addr = b1_listener.local_addr().unwrap();

    tokio::spawn(async move {
        while let Ok((stream, _)) = b1_listener.accept().await {
            let calls = b1_calls_clone.clone();
            let io = TokioIo::new(stream);
            tokio::spawn(async move {
                let service = service_fn(move |_req: Request<hyper::body::Incoming>| {
                    calls.fetch_add(1, Ordering::Relaxed);
                    async move {
                        Ok::<_, Infallible>(
                            Response::builder()
                                .status(StatusCode::BAD_GATEWAY)
                                .body(Full::new(Bytes::from("b1 failed")))
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

    let b2_calls = Arc::new(AtomicUsize::new(0));
    let b2_calls_clone = b2_calls.clone();
    let b2_listener = TcpListener::bind("127.0.0.1:0").await.unwrap();
    let b2_addr = b2_listener.local_addr().unwrap();

    tokio::spawn(async move {
        while let Ok((stream, _)) = b2_listener.accept().await {
            let calls = b2_calls_clone.clone();
            let io = TokioIo::new(stream);
            tokio::spawn(async move {
                let service = service_fn(move |_req: Request<hyper::body::Incoming>| {
                    calls.fetch_add(1, Ordering::Relaxed);
                    async move {
                        Ok::<_, Infallible>(
                            Response::builder()
                                .status(StatusCode::OK)
                                .body(Full::new(Bytes::from("b2 success")))
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

    let cb_cfg = CircuitBreakerConfig::default();
    let backends = vec![
        Arc::new(Backend::new(b1_addr, 1, cb_cfg.clone())),
        Arc::new(Backend::new(b2_addr, 1, cb_cfg)),
    ];

    let mut cfg = Config::load("config.toml").unwrap();
    cfg.retry.enabled = true;
    cfg.retry.max_attempts = 3;
    cfg.retry.initial_backoff_ms = 1;

    let balancer: Box<dyn LoadBalancer> = Box::new(RoundRobin::new());
    let state = Arc::new(AppState::new(backends, balancer, cfg));

    let state_clone = state.clone();
    let client_addr = "127.0.0.1:55555".parse().unwrap();

    // Spawn server to test proxy handle
    let req_listener = TcpListener::bind("127.0.0.1:0").await.unwrap();
    let req_addr = req_listener.local_addr().unwrap();

    tokio::spawn(async move {
        let (stream, _) = req_listener.accept().await.unwrap();
        let io = TokioIo::new(stream);
        let service = service_fn(move |req| {
            let state = state_clone.clone();
            async move { proxy::handle(state, client_addr, req).await }
        });
        let _ = ConnBuilder::new(TokioExecutor::new())
            .serve_connection(io, service)
            .await;
    });

    let client = hyper_util::client::legacy::Client::builder(TokioExecutor::new())
        .build(hyper_util::client::legacy::connect::HttpConnector::new());

    let get_req = Request::builder()
        .uri(format!("http://{req_addr}/test"))
        .method("GET")
        .body(Full::new(Bytes::new()))
        .unwrap();

    let resp = client.request(get_req).await.unwrap();
    assert_eq!(resp.status(), StatusCode::OK);
    assert_eq!(b1_calls.load(Ordering::Relaxed), 1);
    assert_eq!(b2_calls.load(Ordering::Relaxed), 1);
}

#[tokio::test]
async fn test_hop_by_hop_and_forwarded_headers() {
    let listener = TcpListener::bind("127.0.0.1:0").await.unwrap();
    let backend_addr = listener.local_addr().unwrap();

    tokio::spawn(async move {
        while let Ok((stream, _)) = listener.accept().await {
            let io = TokioIo::new(stream);
            tokio::spawn(async move {
                let service = service_fn(move |req: Request<hyper::body::Incoming>| async move {
                    // Check headers
                    let headers = req.headers();
                    assert!(!headers.contains_key(CONNECTION));
                    assert!(!headers.contains_key("keep-alive"));
                    assert!(!headers.contains_key("proxy-authorization"));
                    assert!(!headers.contains_key("custom-hop"));
                    assert!(headers.contains_key("x-forwarded-for"));
                    assert!(headers.contains_key("x-forwarded-proto"));
                    assert!(headers.contains_key("x-forwarded-host"));

                    Ok::<_, Infallible>(
                        Response::builder()
                            .header("Connection", "close")
                            .header("X-Custom-Resp", "valid")
                            .body(Full::new(Bytes::from("header test ok")))
                            .unwrap(),
                    )
                });
                let _ = ConnBuilder::new(TokioExecutor::new())
                    .serve_connection(io, service)
                    .await;
            });
        }
    });

    let cb_cfg = CircuitBreakerConfig::default();
    let backends = vec![Arc::new(Backend::new(backend_addr, 1, cb_cfg))];
    let cfg = Config::load("config.toml").unwrap();
    let balancer: Box<dyn LoadBalancer> = Box::new(RoundRobin::new());
    let state = Arc::new(AppState::new(backends, balancer, cfg));

    let req_listener = TcpListener::bind("127.0.0.1:0").await.unwrap();
    let req_addr = req_listener.local_addr().unwrap();
    let state_clone = state.clone();
    let client_addr = "127.0.0.1:44444".parse().unwrap();

    tokio::spawn(async move {
        while let Ok((stream, _)) = req_listener.accept().await {
            let io = TokioIo::new(stream);
            let state = state_clone.clone();
            tokio::spawn(async move {
                let service = service_fn(move |req| {
                    let state = state.clone();
                    async move { proxy::handle(state, client_addr, req).await }
                });
                let _ = ConnBuilder::new(TokioExecutor::new())
                    .serve_connection(io, service)
                    .await;
            });
        }
    });

    let client = hyper_util::client::legacy::Client::builder(TokioExecutor::new())
        .build(hyper_util::client::legacy::connect::HttpConnector::new());

    let req = Request::builder()
        .uri(format!("http://{req_addr}/headers"))
        .header("Connection", "close, custom-hop")
        .header("Keep-Alive", "timeout=5")
        .header("Proxy-Authorization", "Basic 123")
        .header("Custom-Hop", "remove-me")
        .header("Host", "my-app.local")
        .body(Full::new(Bytes::new()))
        .unwrap();

    let resp = client.request(req).await.unwrap();
    assert_eq!(resp.status(), StatusCode::OK);
    assert!(!resp.headers().contains_key("custom-resp-hop"));
    assert_eq!(
        resp.headers().get("X-Custom-Resp").unwrap(),
        &HeaderValue::from_static("valid")
    );
}

#[test]
fn test_header_sanitization_units() {
    let mut req_headers = hyper::HeaderMap::new();
    req_headers.insert("connection", HeaderValue::from_static("close, x-foo"));
    req_headers.insert("keep-alive", HeaderValue::from_static("timeout=10"));
    req_headers.insert("proxy-authenticate", HeaderValue::from_static("basic"));
    req_headers.insert("proxy-custom", HeaderValue::from_static("val"));
    req_headers.insert("x-foo", HeaderValue::from_static("bar"));
    req_headers.insert("host", HeaderValue::from_static("example.com"));
    req_headers.insert("x-normal", HeaderValue::from_static("stay"));

    let client_addr = "192.168.1.50:12345".parse().unwrap();
    proxy::sanitize_request_headers(&mut req_headers, client_addr, false);

    assert!(!req_headers.contains_key("connection"));
    assert!(!req_headers.contains_key("keep-alive"));
    assert!(!req_headers.contains_key("proxy-authenticate"));
    assert!(!req_headers.contains_key("proxy-custom"));
    assert!(!req_headers.contains_key("x-foo"));
    assert_eq!(
        req_headers.get("x-normal").unwrap(),
        &HeaderValue::from_static("stay")
    );
    assert_eq!(
        req_headers.get("x-forwarded-for").unwrap(),
        &HeaderValue::from_static("192.168.1.50")
    );
    assert_eq!(
        req_headers.get("x-forwarded-proto").unwrap(),
        &HeaderValue::from_static("http")
    );
    assert_eq!(
        req_headers.get("x-forwarded-host").unwrap(),
        &HeaderValue::from_static("example.com")
    );

    let mut resp_headers = hyper::HeaderMap::new();
    resp_headers.insert("connection", HeaderValue::from_static("close, x-bar"));
    resp_headers.insert("transfer-encoding", HeaderValue::from_static("chunked"));
    resp_headers.insert("x-bar", HeaderValue::from_static("remove"));
    resp_headers.insert("x-app", HeaderValue::from_static("keep"));

    proxy::sanitize_response_headers(&mut resp_headers, false);
    assert!(!resp_headers.contains_key("connection"));
    assert!(!resp_headers.contains_key("transfer-encoding"));
    assert!(!resp_headers.contains_key("x-bar"));
    assert_eq!(
        resp_headers.get("x-app").unwrap(),
        &HeaderValue::from_static("keep")
    );
}

#[tokio::test]
async fn test_no_healthy_backends_returns_503() {
    let cb_cfg = CircuitBreakerConfig::default();
    let dead_addr: SocketAddr = "127.0.0.1:1".parse().unwrap();
    let backend = Arc::new(Backend::new(dead_addr, 1, cb_cfg));
    backend.healthy.store(false, Ordering::Relaxed);

    let backends = vec![backend];
    let cfg = Config::load("config.toml").unwrap();
    let balancer: Box<dyn LoadBalancer> = Box::new(RoundRobin::new());
    let state = Arc::new(AppState::new(backends, balancer, cfg));

    let req_listener = TcpListener::bind("127.0.0.1:0").await.unwrap();
    let req_addr = req_listener.local_addr().unwrap();
    let state_clone = state.clone();
    let client_addr = "127.0.0.1:33333".parse().unwrap();

    tokio::spawn(async move {
        if let Ok((stream, _)) = req_listener.accept().await {
            let io = TokioIo::new(stream);
            let service = service_fn(move |req| {
                let state = state_clone.clone();
                async move { proxy::handle(state, client_addr, req).await }
            });
            let _ = ConnBuilder::new(TokioExecutor::new())
                .serve_connection(io, service)
                .await;
        }
    });

    let client = hyper_util::client::legacy::Client::builder(TokioExecutor::new())
        .build(hyper_util::client::legacy::connect::HttpConnector::new());

    let req = Request::builder()
        .uri(format!("http://{req_addr}/"))
        .body(Full::new(Bytes::new()))
        .unwrap();

    let resp = client.request(req).await.unwrap();
    assert_eq!(resp.status(), StatusCode::SERVICE_UNAVAILABLE);
}

#[tokio::test]
async fn test_passive_health_check_marks_unhealthy() {
    // Backend that fails with 500
    let listener = TcpListener::bind("127.0.0.1:0").await.unwrap();
    let backend_addr = listener.local_addr().unwrap();

    tokio::spawn(async move {
        while let Ok((stream, _)) = listener.accept().await {
            let io = TokioIo::new(stream);
            tokio::spawn(async move {
                let service = service_fn(move |_req: Request<hyper::body::Incoming>| async move {
                    Ok::<_, Infallible>(
                        Response::builder()
                            .status(StatusCode::INTERNAL_SERVER_ERROR)
                            .body(Full::new(Bytes::from("server error")))
                            .unwrap(),
                    )
                });
                let _ = ConnBuilder::new(TokioExecutor::new())
                    .serve_connection(io, service)
                    .await;
            });
        }
    });

    let cb_cfg = CircuitBreakerConfig::default();
    let backend = Arc::new(Backend::new(backend_addr, 1, cb_cfg));
    let backends = vec![backend.clone()];

    let mut cfg = Config::load("config.toml").unwrap();
    cfg.passive_health_check.unhealthy_threshold = 2;
    cfg.retry.enabled = false;

    let balancer: Box<dyn LoadBalancer> = Box::new(RoundRobin::new());
    let state = Arc::new(AppState::new(backends, balancer, cfg));

    let req_listener = TcpListener::bind("127.0.0.1:0").await.unwrap();
    let req_addr = req_listener.local_addr().unwrap();
    let state_clone = state.clone();
    let client_addr = "127.0.0.1:22222".parse().unwrap();

    tokio::spawn(async move {
        while let Ok((stream, _)) = req_listener.accept().await {
            let io = TokioIo::new(stream);
            let state = state_clone.clone();
            tokio::spawn(async move {
                let service = service_fn(move |req| {
                    let state = state.clone();
                    async move { proxy::handle(state, client_addr, req).await }
                });
                let _ = ConnBuilder::new(TokioExecutor::new())
                    .serve_connection(io, service)
                    .await;
            });
        }
    });

    let client = hyper_util::client::legacy::Client::builder(TokioExecutor::new())
        .build(hyper_util::client::legacy::connect::HttpConnector::new());

    assert!(backend.is_healthy());

    // Request 1: fails
    let req1 = Request::builder()
        .uri(format!("http://{req_addr}/"))
        .body(Full::new(Bytes::new()))
        .unwrap();
    let _ = client.request(req1).await.unwrap();
    assert!(backend.is_healthy());

    // Request 2: fails -> hits threshold (2) -> marked unhealthy
    let req2 = Request::builder()
        .uri(format!("http://{req_addr}/"))
        .body(Full::new(Bytes::new()))
        .unwrap();
    let _ = client.request(req2).await.unwrap();
    assert!(!backend.is_healthy());
}
