pub mod backend;
pub mod balancer;
pub mod circuit_breaker;
pub mod config;
pub mod error;
pub mod health;
pub mod proxy;
pub mod retry;
pub mod server;
pub mod state;

use crate::backend::Backend;
use crate::balancer::{LeastConnections, LoadBalancer, RoundRobin, WeightedRoundRobin};
use crate::config::{Algorithm, Config};
use crate::state::AppState;
use clap::Parser;
use std::sync::Arc;
use std::time::Duration;
use tokio_util::sync::CancellationToken;
use tracing_subscriber::EnvFilter;

#[derive(Parser, Debug)]
#[command(name = "l7lb", about = "A production-grade L7 (HTTP) load balancer")]
struct Args {
    /// Path to the TOML config file
    #[arg(short, long, default_value = "config.toml")]
    config: String,
}

#[tokio::main]
async fn main() -> anyhow::Result<()> {
    tracing_subscriber::fmt()
        .with_env_filter(EnvFilter::try_from_default_env().unwrap_or_else(|_| "info".into()))
        .init();

    let args = Args::parse();
    let cfg = Config::load(&args.config)?;
    let listen_addr = cfg.listen_socket_addr()?;

    let cb_runtime_cfg = cfg.circuit_breaker.to_runtime();
    let backends: Vec<Arc<Backend>> = cfg
        .backends
        .iter()
        .map(|b| -> anyhow::Result<Arc<Backend>> {
            let addr = b.addr.parse()?;
            Ok(Arc::new(Backend::new(
                addr,
                b.weight,
                cb_runtime_cfg.clone(),
            )))
        })
        .collect::<anyhow::Result<_>>()?;

    tracing::info!(count = backends.len(), algorithm = ?cfg.algorithm, "loaded backends");

    let slow_start_duration = Duration::from_secs(if cfg.slow_start.enabled {
        cfg.slow_start.window_secs
    } else {
        0
    });

    let balancer: Box<dyn LoadBalancer> = match cfg.algorithm {
        Algorithm::RoundRobin => Box::new(RoundRobin::new()),
        Algorithm::LeastConnections => Box::new(LeastConnections::new()),
        Algorithm::WeightedRoundRobin => {
            Box::new(WeightedRoundRobin::new(&backends, slow_start_duration))
        }
    };

    let cancel_token = CancellationToken::new();

    // Spawn signal listener for SIGINT / SIGTERM
    let cancel_for_signal = cancel_token.clone();
    tokio::spawn(async move {
        #[cfg(unix)]
        {
            use tokio::signal::unix::{signal, SignalKind};
            let mut sigterm = match signal(SignalKind::terminate()) {
                Ok(s) => s,
                Err(e) => {
                    tracing::error!(error = %e, "failed to register SIGTERM handler");
                    return;
                }
            };
            tokio::select! {
                _ = tokio::signal::ctrl_c() => {
                    tracing::info!("received SIGINT (Ctrl+C)");
                }
                _ = sigterm.recv() => {
                    tracing::info!("received SIGTERM");
                }
            }
        }

        #[cfg(not(unix))]
        {
            if let Err(e) = tokio::signal::ctrl_c().await {
                tracing::error!(error = %e, "failed to listen for ctrl_c signal");
                return;
            }
            tracing::info!("received Ctrl+C signal");
        }

        cancel_for_signal.cancel();
    });

    health::spawn_health_checks(
        backends.clone(),
        cfg.health_check.clone(),
        cancel_token.clone(),
    );

    let state = Arc::new(AppState::new(backends, balancer, cfg));

    server::run(listen_addr, state, cancel_token).await
}
