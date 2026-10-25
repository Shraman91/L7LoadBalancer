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
use tokio::net::TcpListener;
use tokio_util::sync::CancellationToken;

#[derive(Serialize)]
pub struct BackendStatus {
    pub addr: String,
    pub healthy: bool,
    pub draining: bool,
    pub active_connections: usize,
    pub circuit_state: String,
    pub weight: u32,
    pub ewma_latency_ms: f64,
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
                });
            }
            json_response(StatusCode::OK, &list)
        }
        (&Method::GET, "/admin/config") => json_response(StatusCode::OK, &state.config),
        (&Method::POST, p) if p.starts_with("/admin/backends/") && p.ends_with("/drain") => {
            let prefix = "/admin/backends/";
            let suffix = "/drain";
            let addr_str = &p[prefix.len()..p.len() - suffix.len()];
            match addr_str.parse::<SocketAddr>() {
                Ok(addr) => {
                    if let Some(backend) = state.find_backend(addr) {
                        backend.set_draining(true);
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
            let prefix = "/admin/backends/";
            let suffix = "/enable";
            let addr_str = &p[prefix.len()..p.len() - suffix.len()];
            match addr_str.parse::<SocketAddr>() {
                Ok(addr) => {
                    if let Some(backend) = state.find_backend(addr) {
                        backend.set_draining(false);
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
