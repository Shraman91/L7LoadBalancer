use crate::proxy;
use crate::state::{AppState, AppStateHolder};
use hyper::service::service_fn;
use hyper_util::rt::{TokioExecutor, TokioIo, TokioTimer};
use hyper_util::server::conn::auto::Builder as ConnBuilder;
use std::net::SocketAddr;
use std::sync::atomic::Ordering;
use std::sync::Arc;
use std::time::{Duration, Instant};
use tokio::net::TcpListener;
use tokio_util::sync::CancellationToken;

pub async fn run(
    listen_addr: SocketAddr,
    state_holder: Arc<AppStateHolder>,
    cancel_token: CancellationToken,
) -> anyhow::Result<()> {
    let listener = TcpListener::bind(listen_addr).await?;
    let is_tls = state_holder.load().config.tls.enabled;
    tracing::info!(%listen_addr, tls = is_tls, "l7lb listening");

    loop {
        tokio::select! {
            _ = cancel_token.cancelled() => {
                tracing::info!("shutdown signal received, stopping listener accept loop");
                break;
            }
            accept_res = listener.accept() => {
                let (stream, client_addr) = match accept_res {
                    Ok(pair) => pair,
                    Err(e) => {
                        tracing::warn!(error = %e, "failed to accept connection");
                        continue;
                    }
                };

                let state_holder = Arc::clone(&state_holder);
                let current_state = state_holder.load();
                let tls_acceptor = current_state.tls_acceptor.clone();
                let header_read_timeout = Duration::from_secs(
                    current_state.config.timeouts.client_header_read_secs.max(1),
                );
                let handshake_timeout = Duration::from_secs(
                    current_state.config.timeouts.connect_secs.max(5),
                );

                tokio::spawn(async move {
                    let service = service_fn(move |req| {
                        let state = state_holder.load();
                        async move { proxy::handle(state, client_addr, req).await }
                    });

                    let mut builder = ConnBuilder::new(TokioExecutor::new());
                    builder
                        .http1()
                        .timer(TokioTimer::new())
                        .header_read_timeout(header_read_timeout);
                    builder.http2().timer(TokioTimer::new());

                    if let Some(acceptor) = tls_acceptor {
                        let tls_stream = match tokio::time::timeout(handshake_timeout, acceptor.accept(stream)).await {
                            Ok(Ok(s)) => s,
                            Ok(Err(e)) => {
                                tracing::debug!(error = %e, client = %client_addr, "TLS handshake failed");
                                return;
                            }
                            Err(_) => {
                                tracing::warn!(client = %client_addr, "TLS handshake timed out");
                                return;
                            }
                        };
                        let io = TokioIo::new(tls_stream);
                        if let Err(e) = builder.serve_connection(io, service).await {
                            tracing::debug!(error = %e, client = %client_addr, "TLS connection closed with error");
                        }
                    } else {
                        let io = TokioIo::new(stream);
                        if let Err(e) = builder.serve_connection(io, service).await {
                            tracing::debug!(error = %e, client = %client_addr, "connection closed with error");
                        }
                    }
                });
            }
        }
    }

    // Graceful drain
    let current_state = state_holder.load();
    let drain_secs = current_state.config.shutdown.drain_timeout_secs;
    tracing::info!(
        drain_timeout_secs = drain_secs,
        "draining in-flight requests"
    );

    let start_drain = Instant::now();
    let timeout = Duration::from_secs(drain_secs);

    while current_state.in_flight_requests.load(Ordering::Relaxed) > 0 {
        if start_drain.elapsed() >= timeout {
            tracing::warn!(
                remaining = current_state.in_flight_requests.load(Ordering::Relaxed),
                "drain deadline reached, forcing shutdown"
            );
            break;
        }
        tokio::time::sleep(Duration::from_millis(50)).await;
    }

    tracing::info!("graceful shutdown complete");
    Ok(())
}

pub async fn run_with_state(
    listen_addr: SocketAddr,
    state: Arc<AppState>,
    cancel_token: CancellationToken,
) -> anyhow::Result<()> {
    let holder = Arc::new(AppStateHolder {
        inner: arc_swap::ArcSwap::new(state),
        config_path: std::path::PathBuf::from("config.toml"),
        health_cancel: std::sync::Mutex::new(None),
    });
    run(listen_addr, holder, cancel_token).await
}
