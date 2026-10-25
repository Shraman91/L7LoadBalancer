pub mod admin;
pub mod backend;
pub mod balancer;
pub mod circuit_breaker;
pub mod config;
pub mod error;
pub mod health;
pub mod proxy;
pub mod retry;
pub mod router;
pub mod server;
pub mod state;

use crate::config::Config;
use crate::state::AppStateHolder;
use clap::Parser;
use std::path::PathBuf;
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

    let (state_holder, health_cancel) =
        AppStateHolder::new(cfg.clone(), PathBuf::from(&args.config));

    let initial_state = state_holder.load();
    tracing::info!(
        pools = initial_state.pools.len(),
        backends = initial_state.backends.len(),
        routes = initial_state.router.routes.len(),
        "loaded initial configuration"
    );

    for pool in initial_state.pools.values() {
        health::spawn_health_checks(
            pool.backends.clone(),
            pool.health_check.clone(),
            health_cancel.clone(),
        );
    }

    let shutdown_cancel = CancellationToken::new();

    // Spawn signal listener for SIGINT / SIGTERM
    let cancel_for_signal = shutdown_cancel.clone();
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

    // Spawn SIGHUP listener on Unix
    #[cfg(unix)]
    {
        let holder = Arc::clone(&state_holder);
        let cancel = shutdown_cancel.clone();
        tokio::spawn(async move {
            use tokio::signal::unix::{signal, SignalKind};
            if let Ok(mut sighup) = signal(SignalKind::hangup()) {
                loop {
                    tokio::select! {
                        _ = cancel.cancelled() => break,
                        _ = sighup.recv() => {
                            tracing::info!("received SIGHUP, initiating hot reload");
                            if let Err(e) = holder.reload() {
                                tracing::error!(error = %e, "SIGHUP reload failed");
                            }
                        }
                    }
                }
            }
        });
    }

    // Spawn file watcher for config hot reload
    let watcher_holder = Arc::clone(&state_holder);
    let watcher_cancel = shutdown_cancel.clone();
    let config_path = PathBuf::from(&args.config);
    tokio::task::spawn_blocking(move || {
        use notify::{
            Config as NotifyConfig, EventKind, RecommendedWatcher, RecursiveMode, Watcher,
        };
        let (tx, rx) = std::sync::mpsc::channel();
        let mut watcher = match RecommendedWatcher::new(tx, NotifyConfig::default()) {
            Ok(w) => w,
            Err(e) => {
                tracing::warn!(error = %e, "file watcher could not be created");
                return;
            }
        };

        if let Err(e) = watcher.watch(&config_path, RecursiveMode::NonRecursive) {
            tracing::warn!(error = %e, path = ?config_path, "failed to watch config file");
            return;
        }

        while !watcher_cancel.is_cancelled() {
            match rx.recv_timeout(Duration::from_millis(500)) {
                Ok(Ok(event)) => {
                    if matches!(event.kind, EventKind::Modify(_) | EventKind::Create(_)) {
                        std::thread::sleep(Duration::from_millis(50));
                        if let Err(e) = watcher_holder.reload() {
                            tracing::error!(error = %e, "file watcher reload failed");
                        }
                    }
                }
                Ok(Err(e)) => {
                    tracing::debug!(error = %e, "file watcher notification error");
                }
                Err(std::sync::mpsc::RecvTimeoutError::Timeout) => {}
                Err(std::sync::mpsc::RecvTimeoutError::Disconnected) => break,
            }
        }
    });

    // Spawn Admin API server
    if cfg.admin.enabled {
        let admin_addr = cfg.admin_socket_addr()?;
        let admin_holder = Arc::clone(&state_holder);
        let admin_cancel = shutdown_cancel.clone();
        tokio::spawn(async move {
            if let Err(e) = admin::run_admin_server(admin_addr, admin_holder, admin_cancel).await {
                tracing::error!(error = %e, "admin server stopped with error");
            }
        });
    }

    server::run(listen_addr, state_holder, shutdown_cancel).await
}
