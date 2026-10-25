use bytes::Bytes;
use http_body_util::Full;
use hyper::header::HeaderValue;
use hyper::service::service_fn;
use hyper::{Method, Request, Response, StatusCode};
use hyper_util::rt::{TokioExecutor, TokioIo};
use hyper_util::server::conn::auto::Builder as ConnBuilder;
use l7_load_balancer::admin::handle_admin_request;
use l7_load_balancer::backend::Backend;
use l7_load_balancer::balancer::{
    ConsistentHash, LeastResponseTime, LoadBalancer, PowerOfTwoChoices,
};
use l7_load_balancer::circuit_breaker::CircuitBreakerConfig;
use l7_load_balancer::config::{Algorithm, BackendConfig, Config, PoolConfig, RouteRuleConfig};
use l7_load_balancer::proxy;
use l7_load_balancer::router::Router;
use l7_load_balancer::state::{AppState, AppStateHolder};
use std::collections::HashMap;
use std::convert::Infallible;
use std::net::SocketAddr;
use std::sync::atomic::Ordering;
use std::sync::Arc;
use std::time::Duration;
use tokio::net::TcpListener;

fn make_backend(addr_str: &str, weight: u32) -> Arc<Backend> {
    let addr: SocketAddr = addr_str.parse().unwrap();
    let cb_cfg = CircuitBreakerConfig {
        enabled: true,
        consecutive_failures: 3,
        cooldown_duration: Duration::from_millis(50),
        half_open_max_trials: 1,
        half_open_successes_to_close: 1,
    };
    Arc::new(Backend::new(addr, weight, cb_cfg))
}

#[test]
fn test_power_of_two_choices_selection() {
    let b1 = make_backend("127.0.0.1:8001", 1);
    let b2 = make_backend("127.0.0.1:8002", 1);
    let b3 = make_backend("127.0.0.1:8003", 1);

    b1.active_connections.store(10, Ordering::Relaxed);
    b2.active_connections.store(2, Ordering::Relaxed);
    b3.active_connections.store(5, Ordering::Relaxed);

    let backends = vec![Arc::clone(&b1), Arc::clone(&b2), Arc::clone(&b3)];
    let lb = PowerOfTwoChoices::new();

    // With 2 choices sampled from {b1(10), b2(2), b3(5)}:
    // Possible pairs: (b1, b2)->b2, (b1, b3)->b3, (b2, b3)->b2
    // Notice b1(10) is strictly greater than both b2 and b3, so b1 should NEVER be selected!
    for _ in 0..100 {
        let picked = lb.select(&backends).expect("should select backend");
        assert_ne!(
            picked.addr, b1.addr,
            "b1 has 10 active connections vs b2(2) and b3(5), it should never win"
        );
    }

    // If b2 and b3 are excluded, b1 is the only one remaining
    let picked = lb
        .select_excluding(&backends, &[b2.addr, b3.addr])
        .expect("only b1 left");
    assert_eq!(picked.addr, b1.addr);

    // If all are excluded
    let picked = lb.select_excluding(&backends, &[b1.addr, b2.addr, b3.addr]);
    assert!(picked.is_none());
}

#[test]
fn test_consistent_hash_distribution_and_failover() {
    let b1 = make_backend("127.0.0.1:8001", 1);
    let b2 = make_backend("127.0.0.1:8002", 1);
    let b3 = make_backend("127.0.0.1:8003", 1);
    let backends = vec![Arc::clone(&b1), Arc::clone(&b2), Arc::clone(&b3)];

    let lb = ConsistentHash::new(&backends);

    // Deterministic stickiness for identical key
    let key = "user_session_abc";
    let first = lb.select_with_key(&backends, &[], Some(key)).unwrap();
    for _ in 0..50 {
        let selected = lb.select_with_key(&backends, &[], Some(key)).unwrap();
        assert_eq!(
            selected.addr, first.addr,
            "sticky key must map consistently"
        );
    }

    // Failover: if the primary backend becomes unhealthy or is excluded
    let primary_addr = first.addr;
    let failover = lb
        .select_with_key(&backends, &[primary_addr], Some(key))
        .expect("must failover to next healthy ring node");
    assert_ne!(
        failover.addr, primary_addr,
        "must pick another node when primary is excluded"
    );

    // Verify key distribution over 100 different keys hits multiple backends
    let mut hit_counts: HashMap<SocketAddr, usize> = HashMap::new();
    for i in 0..100 {
        let k = format!("client-token-{}", i);
        let picked = lb.select_with_key(&backends, &[], Some(&k)).unwrap();
        *hit_counts.entry(picked.addr).or_insert(0) += 1;
    }
    // At least 2 of the 3 backends should receive traffic over 100 random keys
    assert!(
        hit_counts.len() >= 2,
        "consistent hashing should distribute across multiple backends, got: {:?}",
        hit_counts
    );
}

#[test]
fn test_least_response_time_selection() {
    let b1 = make_backend("127.0.0.1:8001", 1);
    let b2 = make_backend("127.0.0.1:8002", 1);
    let b3 = make_backend("127.0.0.1:8003", 1);

    b1.record_latency(Duration::from_millis(150));
    b2.record_latency(Duration::from_millis(25));
    b3.record_latency(Duration::from_millis(80));

    let backends = vec![Arc::clone(&b1), Arc::clone(&b2), Arc::clone(&b3)];
    let lb = LeastResponseTime::new();

    // b2 has lowest latency (25ms)
    let picked = lb.select(&backends).expect("should select backend");
    assert_eq!(picked.addr, b2.addr);

    // If b2 is excluded, b3 has the next lowest (80ms vs 150ms)
    let picked = lb
        .select_excluding(&backends, &[b2.addr])
        .expect("should select backend");
    assert_eq!(picked.addr, b3.addr);

    // Tie-break with active connections
    b1.ewma_latency_micros
        .store(25_000, std::sync::atomic::Ordering::Relaxed);
    b2.ewma_latency_micros
        .store(25_000, std::sync::atomic::Ordering::Relaxed);
    b1.active_connections.store(5, Ordering::Relaxed);
    b2.active_connections.store(1, Ordering::Relaxed);

    let picked = lb.select(&backends).expect("should select backend");
    assert_eq!(
        picked.addr, b2.addr,
        "tie-breaker should select lower active connections"
    );
}

#[test]
fn test_router_longest_prefix_host_matching_and_rewrite() {
    let routes = vec![
        RouteRuleConfig {
            host: Some("api.example.com".to_string()),
            path_prefix: "/api/".to_string(),
            pool: "api_pool".to_string(),
            strip_prefix: Some(true),
            rewrite_prefix: None,
        },
        RouteRuleConfig {
            host: None,
            path_prefix: "/api/v2/".to_string(),
            pool: "v2_pool".to_string(),
            strip_prefix: None,
            rewrite_prefix: Some("/internal/v2".to_string()),
        },
        RouteRuleConfig {
            host: None,
            path_prefix: "/static/".to_string(),
            pool: "static_pool".to_string(),
            strip_prefix: Some(false),
            rewrite_prefix: None,
        },
    ];

    let router = Router::new(&routes, "default_pool".to_string());

    // Host match with strip_prefix
    let res = router.route(Some("api.example.com:443"), "/api/users?page=1");
    assert_eq!(res.pool, "api_pool");
    assert_eq!(res.path_and_query, "/users?page=1");

    // Longest prefix match over general prefix
    let res = router.route(Some("other.com"), "/api/v2/products/123");
    assert_eq!(res.pool, "v2_pool");
    assert_eq!(res.path_and_query, "/internal/v2/products/123");

    // Static route without rewrite or strip
    let res = router.route(None, "/static/css/main.css");
    assert_eq!(res.pool, "static_pool");
    assert_eq!(res.path_and_query, "/static/css/main.css");

    // Fallback to default pool
    let res = router.route(None, "/index.html");
    assert_eq!(res.pool, "default_pool");
    assert_eq!(res.path_and_query, "/index.html");
}

#[tokio::test]
async fn test_admin_api_endpoints() {
    let mut config = Config::load("config.toml").unwrap();
    config.admin.enabled = true;
    config.admin.addr = "127.0.0.1:9999".to_string();

    let temp_dir = std::env::temp_dir();
    let cfg_file = temp_dir.join("test_admin_config.toml");

    let (state_holder, _cancel) = AppStateHolder::new(config, cfg_file);

    // 1. GET /admin/backends
    let req = Request::builder()
        .method(Method::GET)
        .uri("/admin/backends")
        .body(Full::new(Bytes::new()))
        .unwrap();

    let resp = handle_admin_request(Arc::clone(&state_holder), req).await;
    assert_eq!(resp.status(), StatusCode::OK);

    // 2. GET /admin/config
    let req = Request::builder()
        .method(Method::GET)
        .uri("/admin/config")
        .body(Full::new(Bytes::new()))
        .unwrap();

    let resp = handle_admin_request(Arc::clone(&state_holder), req).await;
    assert_eq!(resp.status(), StatusCode::OK);

    // Get an existing backend from state
    let state = state_holder.load();
    let backend_addr = state.backends[0].addr;

    // 3. POST /admin/backends/{addr}/drain
    let drain_uri = format!("/admin/backends/{}/drain", backend_addr);
    let req = Request::builder()
        .method(Method::POST)
        .uri(drain_uri)
        .body(Full::new(Bytes::new()))
        .unwrap();

    let resp = handle_admin_request(Arc::clone(&state_holder), req).await;
    assert_eq!(resp.status(), StatusCode::OK);
    assert!(state.backends[0].is_draining());

    // 4. POST /admin/backends/{addr}/enable
    let enable_uri = format!("/admin/backends/{}/enable", backend_addr);
    let req = Request::builder()
        .method(Method::POST)
        .uri(enable_uri)
        .body(Full::new(Bytes::new()))
        .unwrap();

    let resp = handle_admin_request(Arc::clone(&state_holder), req).await;
    assert_eq!(resp.status(), StatusCode::OK);
    assert!(!state.backends[0].is_draining());

    // 5. POST to non-existent backend returns 404
    let req = Request::builder()
        .method(Method::POST)
        .uri("/admin/backends/127.0.0.1:59999/drain")
        .body(Full::new(Bytes::new()))
        .unwrap();

    let resp = handle_admin_request(Arc::clone(&state_holder), req).await;
    assert_eq!(resp.status(), StatusCode::NOT_FOUND);
}

#[tokio::test]
async fn test_hot_reload_preserves_backend_health_and_updates_routes() {
    let temp_dir = std::env::temp_dir();
    let cfg_path = temp_dir.join(format!(
        "l7lb_hot_reload_test_{}.toml",
        std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .unwrap()
            .as_nanos()
    ));

    let initial_toml = r#"
listen_addr = "127.0.0.1:8080"

[timeouts]
connect_secs = 2
idle_keepalive_secs = 30
request_timeout_secs = 5

[health_check]
interval_secs = 10
timeout_secs = 1
path = "/health"
consecutive_successes = 2
consecutive_failures = 3

default_pool = "main"

[pools.main]
algorithm = "round_robin"

[[pools.main.backends]]
addr = "127.0.0.1:9001"
weight = 1

[[pools.main.backends]]
addr = "127.0.0.1:9002"
weight = 1
"#;

    tokio::fs::write(&cfg_path, initial_toml).await.unwrap();

    let initial_cfg = Config::load(&cfg_path).unwrap();
    let (state_holder, _cancel) = AppStateHolder::new(initial_cfg, cfg_path.clone());

    // Mark 127.0.0.1:9001 as unhealthy and record EWMA latency
    let state = state_holder.load();
    let b1 = state
        .find_backend("127.0.0.1:9001".parse().unwrap())
        .expect("b1 exists");
    b1.set_healthy(false);
    b1.record_latency(Duration::from_millis(42));
    assert!(!b1.is_healthy());

    // Write updated config: adds a route and changes weight of 9001 to 5
    let updated_toml = r#"
listen_addr = "127.0.0.1:8080"

[timeouts]
connect_secs = 2
idle_keepalive_secs = 30
request_timeout_secs = 5

[health_check]
interval_secs = 10
timeout_secs = 1
path = "/health"
consecutive_successes = 2
consecutive_failures = 3

default_pool = "main"

[pools.main]
algorithm = "least_response_time"

[[pools.main.backends]]
addr = "127.0.0.1:9001"
weight = 5

[[pools.main.backends]]
addr = "127.0.0.1:9002"
weight = 1

[[routes]]
path_prefix = "/special"
pool = "main"
"#;

    tokio::fs::write(&cfg_path, updated_toml).await.unwrap();

    // Perform reload
    state_holder.reload().expect("reload should succeed");

    let new_state = state_holder.load();
    assert_eq!(
        new_state.pools["main"].algorithm,
        Algorithm::LeastResponseTime
    );
    assert_eq!(new_state.router.routes.len(), 1);
    assert_eq!(new_state.router.routes[0].path_prefix, "/special");

    // Verify existing backend health and EWMA latency were preserved, and weight was updated
    let reloaded_b1 = new_state
        .find_backend("127.0.0.1:9001".parse().unwrap())
        .expect("b1 exists in reloaded state");
    assert!(
        !reloaded_b1.is_healthy(),
        "unhealthy state MUST be preserved across hot reload"
    );
    assert_eq!(
        reloaded_b1.weight(),
        5,
        "weight must be updated to new config value"
    );
    assert_eq!(
        reloaded_b1.ewma_latency(),
        Duration::from_millis(42),
        "EWMA latency must be preserved"
    );

    // Clean up temp file
    let _ = tokio::fs::remove_file(&cfg_path).await;
}

#[tokio::test]
async fn test_multi_pool_proxy_routing_end_to_end() {
    // 1. Start Server A (main pool)
    let listener_a = TcpListener::bind("127.0.0.1:0").await.unwrap();
    let addr_a = listener_a.local_addr().unwrap();
    tokio::spawn(async move {
        loop {
            if let Ok((stream, _)) = listener_a.accept().await {
                let io = TokioIo::new(stream);
                tokio::spawn(async move {
                    let _ = ConnBuilder::new(TokioExecutor::new())
                        .serve_connection(
                            io,
                            service_fn(|_req: Request<hyper::body::Incoming>| async move {
                                Ok::<_, Infallible>(
                                    Response::builder()
                                        .status(StatusCode::OK)
                                        .header("X-Pool-Served", "main")
                                        .body(Full::new(Bytes::from("hello from main")))
                                        .unwrap(),
                                )
                            }),
                        )
                        .await;
                });
            }
        }
    });

    // 2. Start Server B (api pool)
    let listener_b = TcpListener::bind("127.0.0.1:0").await.unwrap();
    let addr_b = listener_b.local_addr().unwrap();
    tokio::spawn(async move {
        loop {
            if let Ok((stream, _)) = listener_b.accept().await {
                let io = TokioIo::new(stream);
                tokio::spawn(async move {
                    let _ = ConnBuilder::new(TokioExecutor::new())
                        .serve_connection(
                            io,
                            service_fn(|req: Request<hyper::body::Incoming>| async move {
                                let path = req.uri().path().to_string();
                                Ok::<_, Infallible>(
                                    Response::builder()
                                        .status(StatusCode::OK)
                                        .header("X-Pool-Served", "api")
                                        .header("X-Received-Path", path)
                                        .body(Full::new(Bytes::from("hello from api")))
                                        .unwrap(),
                                )
                            }),
                        )
                        .await;
                });
            }
        }
    });

    // 3. Configure load balancer with two pools and route rule
    let mut config = Config::load("config.toml").unwrap();
    config.default_pool = "main".to_string();

    let mut pools = HashMap::new();
    pools.insert(
        "main".to_string(),
        PoolConfig {
            algorithm: Algorithm::RoundRobin,
            backends: vec![BackendConfig {
                addr: addr_a.to_string(),
                weight: 1,
            }],
            health_check: None,
            hash_key: None,
        },
    );
    pools.insert(
        "api".to_string(),
        PoolConfig {
            algorithm: Algorithm::PowerOfTwoChoices,
            backends: vec![BackendConfig {
                addr: addr_b.to_string(),
                weight: 1,
            }],
            health_check: None,
            hash_key: None,
        },
    );
    config.pools = pools;

    config.routes = vec![RouteRuleConfig {
        host: None,
        path_prefix: "/api/".to_string(),
        pool: "api".to_string(),
        strip_prefix: Some(true),
        rewrite_prefix: None,
    }];

    let state = Arc::new(AppState::from_config(config, &[]));
    let state_clone = Arc::clone(&state);

    // 4. Spawn proxy server listener
    let proxy_listener = TcpListener::bind("127.0.0.1:0").await.unwrap();
    let proxy_addr = proxy_listener.local_addr().unwrap();

    tokio::spawn(async move {
        loop {
            if let Ok((stream, client_addr)) = proxy_listener.accept().await {
                let io = TokioIo::new(stream);
                let state = Arc::clone(&state_clone);
                tokio::spawn(async move {
                    let service = service_fn(move |req| {
                        let state = Arc::clone(&state);
                        async move { proxy::handle(state, client_addr, req).await }
                    });
                    let _ = ConnBuilder::new(TokioExecutor::new())
                        .serve_connection(io, service)
                        .await;
                });
            }
        }
    });

    let client = hyper_util::client::legacy::Client::builder(TokioExecutor::new())
        .build(hyper_util::client::legacy::connect::HttpConnector::new());

    // 5. Request to /home -> routes to main pool
    let req = Request::builder()
        .method(Method::GET)
        .uri(format!("http://{proxy_addr}/home"))
        .body(Full::new(Bytes::new()))
        .unwrap();

    let resp = client.request(req).await.unwrap();
    assert_eq!(resp.status(), StatusCode::OK);
    assert_eq!(
        resp.headers().get("X-Pool-Served").unwrap(),
        HeaderValue::from_static("main")
    );

    // 6. Request to /api/users -> routes to api pool with strip_prefix to /users
    let req = Request::builder()
        .method(Method::GET)
        .uri(format!("http://{proxy_addr}/api/users?id=1"))
        .body(Full::new(Bytes::new()))
        .unwrap();

    let resp = client.request(req).await.unwrap();
    assert_eq!(resp.status(), StatusCode::OK);
    assert_eq!(
        resp.headers().get("X-Pool-Served").unwrap(),
        HeaderValue::from_static("api")
    );
    assert_eq!(
        resp.headers().get("X-Received-Path").unwrap(),
        HeaderValue::from_static("/users")
    );
}
