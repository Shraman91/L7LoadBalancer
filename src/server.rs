use crate::proxy;
use crate::state::AppState;
use hyper::service::service_fn;
use hyper_util::rt::{TokioExecutor, TokioIo};
use hyper_util::server::conn::auto::Builder as ConnBuilder;
use std::net::SocketAddr;
use std::sync::Arc;
use tokio::net::TcpListener;

pub async fn run(listen_addr: SocketAddr, state: Arc<AppState>) -> anyhow::Result<()> {
    let listener = TcpListener::bind(listen_addr).await?;
    tracing::info!(%listen_addr, "l7lb listening");

    loop {
        let (stream, client_addr) = match listener.accept().await {
            Ok(pair) => pair,
            Err(e) => {
                tracing::warn!(error = %e, "failed to accept connection");
                continue;
            }
        };

        let io = TokioIo::new(stream);
        let state = Arc::clone(&state);

        tokio::spawn(async move {
            let service = service_fn(move |req| {
                let state = Arc::clone(&state);
                async move { proxy::handle(state, client_addr, req).await }
            });

            if let Err(e) = ConnBuilder::new(TokioExecutor::new())
                .serve_connection(io, service)
                .await
            {
                tracing::debug!(error = %e, client = %client_addr, "connection closed with error");
            }
        });
    }
}
