use crate::proxy;
use crate::state::{AppState, AppStateHolder};
use hyper::service::service_fn;
use hyper_util::rt::{TokioExecutor, TokioIo};
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
    tracing::info!(%listen_addr, "l7lb listening");

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

                let io = TokioIo::new(stream);
                let state_holder = Arc::clone(&state_holder);
                let current_state = state_holder.load();
                let header_read_timeout = Duration::from_secs(
                    current_state.config.timeouts.client_header_read_secs.max(1),
                );

                tokio::spawn(async move {
                    let service = service_fn(move |req| {
                        let state = state_holder.load();
                        async move { proxy::handle(state, client_addr, req).await }
                    });

                    let mut builder = ConnBuilder::new(TokioExecutor::new());
                    builder.http1().header_read_timeout(header_read_timeout);

                    if let Err(e) = builder.serve_connection(io, service).await {
                        tracing::debug!(error = %e, client = %client_addr, "connection closed with error");
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
