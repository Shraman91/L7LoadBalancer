use crate::proxy::full_body;
use crate::state::AppStateHolder;
use bytes::Bytes;
use http_body_util::combinators::BoxBody;
use hyper::body::Incoming;
use hyper::service::service_fn;
use hyper::{Method, Request, Response, StatusCode};
use hyper_util::rt::{TokioExecutor, TokioIo};
use hyper_util::server::conn::auto::Builder as ConnBuilder;
use serde::Serialize;
use std::net::SocketAddr;
use std::sync::Arc;
use std::time::{SystemTime, UNIX_EPOCH};
use tokio::net::TcpListener;
use tokio_util::sync::CancellationToken;

fn now_millis() -> u64 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .unwrap_or_default()
        .as_millis() as u64
}

#[derive(Clone, Serialize, Debug)]
pub struct AdminEvent {
    pub id: String,
    pub timestamp_ms: u64,
    pub severity: String,
    pub pool: Option<String>,
    pub backend: Option<String>,
    pub event_type: String,
    pub message: String,
}

#[derive(Serialize)]
pub struct BackendStats {
    pub addr: String,
    pub pool: String,
    pub requests_total: u64,
    pub errors_total: u64,
    pub active_connections: usize,
    pub ewma_latency_ms: f64,
    pub circuit_state: String,
    pub weight: u32,
    pub healthy: bool,
    pub draining: bool,
}

#[derive(Serialize)]
pub struct BackendStatus {
    pub addr: String,
    pub healthy: bool,
    pub draining: bool,
    pub active_connections: usize,
    pub circuit_state: String,
    pub weight: u32,
    pub ewma_latency_ms: f64,
    pub tls: bool,
}

fn check_admin_auth<B>(req: &Request<B>) -> Option<Response<BoxBody<Bytes, hyper::Error>>> {
    if let Ok(expected_token) = std::env::var("L7LB_ADMIN_TOKEN") {
        let expected_token = expected_token.trim();
        if !expected_token.is_empty() {
            let auth_header = req
                .headers()
                .get(hyper::header::AUTHORIZATION)
                .and_then(|v| v.to_str().ok());

            let expected_header = format!("Bearer {}", expected_token);
            if auth_header != Some(&expected_header) {
                return Some(error_json(
                    StatusCode::UNAUTHORIZED,
                    "unauthorized: missing or invalid bearer token",
                ));
            }
        }
    } else {
        tracing::warn!("L7LB_ADMIN_TOKEN not set: admin mutation endpoints are unauthenticated");
    }
    None
}

fn json_response<T: Serialize>(
    status: StatusCode,
    data: &T,
) -> Response<BoxBody<Bytes, hyper::Error>> {
    let body = serde_json::to_string(data).unwrap_or_else(|_| "{}".to_string());
    Response::builder()
        .status(status)
        .header(hyper::header::CONTENT_TYPE, "application/json")
        .body(full_body(body))
        .unwrap()
}

fn error_json(status: StatusCode, msg: &'static str) -> Response<BoxBody<Bytes, hyper::Error>> {
    let err_obj = serde_json::json!({
        "status": "error",
        "error": msg,
    });
    json_response(status, &err_obj)
}

pub async fn run_admin_server(
    admin_addr: SocketAddr,
    state_holder: Arc<AppStateHolder>,
    cancel_token: CancellationToken,
) -> anyhow::Result<()> {
    let listener = TcpListener::bind(admin_addr).await?;
    tracing::info!(%admin_addr, "admin server listening");

    loop {
        tokio::select! {
            _ = cancel_token.cancelled() => {
                tracing::info!("admin server shutting down");
                break;
            }
            accept_res = listener.accept() => {
                let (stream, client_addr) = match accept_res {
                    Ok(pair) => pair,
                    Err(e) => {
                        tracing::warn!(error = %e, "admin listener accept error");
                        continue;
                    }
                };

                let io = TokioIo::new(stream);
                let state_holder = Arc::clone(&state_holder);

                tokio::spawn(async move {
                    let service = service_fn(move |req: Request<Incoming>| {
                        let state_holder = Arc::clone(&state_holder);
                        async move {
                            Ok::<_, hyper::Error>(handle_admin_request(state_holder, req).await)
                        }
                    });

                    if let Err(e) = ConnBuilder::new(TokioExecutor::new())
                        .serve_connection(io, service)
                        .await
                    {
                        tracing::debug!(error = %e, client = %client_addr, "admin connection closed");
                    }
                });
            }
        }
    }

    Ok(())
}

pub async fn handle_admin_request<B>(
    state_holder: Arc<AppStateHolder>,
    req: Request<B>,
) -> Response<BoxBody<Bytes, hyper::Error>> {
    let state = state_holder.load();
    let method = req.method();
    let path = req.uri().path();

    match (method, path) {
        (&Method::GET, "/admin/backends") => {
            let mut list = Vec::new();
            for b in &state.backends {
                list.push(BackendStatus {
                    addr: b.addr.to_string(),
                    healthy: b.is_healthy(),
                    draining: b.is_draining(),
                    active_connections: b.connections(),
                    circuit_state: b.circuit_breaker.state_str().to_string(),
                    weight: b.weight(),
                    ewma_latency_ms: b.ewma_latency_ms(),
                    tls: b.is_tls(),
                });
            }
            json_response(StatusCode::OK, &list)
        }
        (&Method::GET, "/admin/config") => json_response(StatusCode::OK, &state.config),
        (&Method::GET, "/admin/stats") => {
            let mut list = Vec::new();
            for (pool_name, pool) in &state.pools {
                for b in &pool.backends {
                    list.push(BackendStats {
                        addr: b.addr.to_string(),
                        pool: pool_name.clone(),
                        requests_total: b.total_requests(),
                        errors_total: b.total_errors(),
                        active_connections: b.connections(),
                        ewma_latency_ms: b.ewma_latency_ms(),
                        circuit_state: b.circuit_breaker.state_str().to_string(),
                        weight: b.weight(),
                        healthy: b.is_healthy(),
                        draining: b.is_draining(),
                    });
                }
            }
            json_response(StatusCode::OK, &list)
        }
        (&Method::GET, "/admin/events") => {
            let rx = state.event_sender.subscribe();
            let stream = futures_util::stream::unfold(rx, |mut rx| async move {
                match rx.recv().await {
                    Ok(event) => {
                        let json =
                            serde_json::to_string(&event).unwrap_or_else(|_| "{}".to_string());
                        let frame =
                            hyper::body::Frame::data(Bytes::from(format!("data: {}\n\n", json)));
                        Some((Ok::<_, hyper::Error>(frame), rx))
                    }
                    Err(tokio::sync::broadcast::error::RecvError::Lagged(_)) => {
                        let frame = hyper::body::Frame::data(Bytes::from(": lagged\n\n"));
                        Some((Ok(frame), rx))
                    }
                    Err(tokio::sync::broadcast::error::RecvError::Closed) => None,
                }
            });
            let body = http_body_util::StreamBody::new(stream);
            let boxed: BoxBody<Bytes, hyper::Error> = http_body_util::BodyExt::boxed(body);
            Response::builder()
                .status(StatusCode::OK)
                .header(hyper::header::CONTENT_TYPE, "text/event-stream")
                .header(hyper::header::CACHE_CONTROL, "no-cache")
                .header(hyper::header::CONNECTION, "keep-alive")
                .body(boxed)
                .unwrap()
        }
        (&Method::GET, "/metrics") => {
            let body = state.metrics.gather(&state);
            Response::builder()
                .status(StatusCode::OK)
                .header(
                    hyper::header::CONTENT_TYPE,
                    "text/plain; version=0.0.4; charset=utf-8",
                )
                .body(full_body(body))
                .unwrap()
        }
        (&Method::POST, p) if p.starts_with("/admin/backends/") && p.ends_with("/drain") => {
            if let Some(unauth) = check_admin_auth(&req) {
                return unauth;
            }
            let prefix = "/admin/backends/";
            let suffix = "/drain";
            let addr_str = &p[prefix.len()..p.len() - suffix.len()];
            match addr_str.parse::<SocketAddr>() {
                Ok(addr) => {
                    if let Some(backend) = state.find_backend(addr) {
                        backend.set_draining(true);
                        state.emit_event(AdminEvent {
                            id: format!("evt-{}", rand::random::<u32>()),
                            timestamp_ms: now_millis(),
                            severity: "info".to_string(),
                            pool: None,
                            backend: Some(addr.to_string()),
                            event_type: "backend_drained".to_string(),
                            message: format!("backend {} marked draining", addr),
                        });
                        let res = serde_json::json!({
                            "status": "ok",
                            "message": "backend marked draining",
                            "addr": addr.to_string(),
                        });
                        json_response(StatusCode::OK, &res)
                    } else {
                        error_json(StatusCode::NOT_FOUND, "backend not found")
                    }
                }
                Err(_) => error_json(StatusCode::BAD_REQUEST, "invalid backend address format"),
            }
        }
        (&Method::POST, p) if p.starts_with("/admin/backends/") && p.ends_with("/enable") => {
            if let Some(unauth) = check_admin_auth(&req) {
                return unauth;
            }
            let prefix = "/admin/backends/";
            let suffix = "/enable";
            let addr_str = &p[prefix.len()..p.len() - suffix.len()];
            match addr_str.parse::<SocketAddr>() {
                Ok(addr) => {
                    if let Some(backend) = state.find_backend(addr) {
                        backend.set_draining(false);
                        state.emit_event(AdminEvent {
                            id: format!("evt-{}", rand::random::<u32>()),
                            timestamp_ms: now_millis(),
                            severity: "info".to_string(),
                            pool: None,
                            backend: Some(addr.to_string()),
                            event_type: "backend_enabled".to_string(),
                            message: format!("backend {} enabled", addr),
                        });
                        let res = serde_json::json!({
                            "status": "ok",
                            "message": "backend enabled",
                            "addr": addr.to_string(),
                        });
                        json_response(StatusCode::OK, &res)
                    } else {
                        error_json(StatusCode::NOT_FOUND, "backend not found")
                    }
                }
                Err(_) => error_json(StatusCode::BAD_REQUEST, "invalid backend address format"),
            }
        }
        _ => error_json(StatusCode::NOT_FOUND, "endpoint not found"),
    }
}
