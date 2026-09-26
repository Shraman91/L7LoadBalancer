mod backend;
mod balancer;
mod config;
mod health;
mod proxy;
mod server;
mod state;

use crate::backend::Backend;
use crate::balancer::{LeastConnections, LoadBalancer, RoundRobin, WeightedRoundRobin};
use crate::config::{Algorithm, Config};
use crate::state::AppState;
use clap::Parser;
use std::sync::Arc;
use tracing_subscriber::EnvFilter;

#[derive(Parser, Debug)]
#[command(name = "l7lb", about = "A small L7 (HTTP) load balancer")]
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

    let backends: Vec<Arc<Backend>> = cfg
        .backends
        .iter()
        .map(|b| -> anyhow::Result<Arc<Backend>> {
            let addr = b.addr.parse()?;
            Ok(Arc::new(Backend::new(addr, b.weight)))
        })
        .collect::<anyhow::Result<_>>()?;

    tracing::info!(count = backends.len(), algorithm = ?cfg.algorithm, "loaded backends");

    let balancer: Box<dyn LoadBalancer> = match cfg.algorithm {
        Algorithm::RoundRobin => Box::new(RoundRobin::new()),
        Algorithm::LeastConnections => Box::new(LeastConnections::new()),
        Algorithm::WeightedRoundRobin => Box::new(WeightedRoundRobin::new(&backends)),
    };

    health::spawn_health_checks(backends.clone(), cfg.health_check.clone());

    let state = Arc::new(AppState::new(backends, balancer));

    server::run(listen_addr, state).await
}
