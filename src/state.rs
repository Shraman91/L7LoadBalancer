use crate::backend::Backend;
use crate::balancer::{
    ConsistentHash, LeastConnections, LeastResponseTime, LoadBalancer, PowerOfTwoChoices,
    RoundRobin, WeightedRoundRobin,
};
use crate::config::{Algorithm, Config, HealthCheckConfig};
use crate::proxy::BoxedBody;
use crate::retry::RetryBudget;
use crate::router::Router;
use arc_swap::ArcSwap;
use hyper_util::client::legacy::connect::HttpConnector;
use hyper_util::client::legacy::Client;
use hyper_util::rt::TokioExecutor;
use std::collections::HashMap;
use std::net::SocketAddr;
use std::path::PathBuf;
use std::sync::atomic::AtomicUsize;
use std::sync::{Arc, Mutex};
use std::time::Duration;
use tokio_util::sync::CancellationToken;

pub type ForwardingClient = Client<HttpConnector, BoxedBody>;

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum HashKeyTarget {
    Ip,
    Header(String),
    Cookie(String),
}

impl HashKeyTarget {
    pub fn parse(s: &str) -> Self {
        let trimmed = s.trim();
        if trimmed.eq_ignore_ascii_case("ip") || trimmed.eq_ignore_ascii_case("client_ip") {
            Self::Ip
        } else if let Some(stripped) = trimmed.strip_prefix("header:") {
            Self::Header(stripped.trim().to_ascii_lowercase())
        } else if let Some(stripped) = trimmed.strip_prefix("cookie:") {
            Self::Cookie(stripped.trim().to_string())
        } else {
            Self::Ip
        }
    }
}

pub struct BackendPool {
    pub name: String,
    pub algorithm: Algorithm,
    pub backends: Vec<Arc<Backend>>,
    pub balancer: Box<dyn LoadBalancer>,
    pub health_check: HealthCheckConfig,
    pub hash_target: Option<HashKeyTarget>,
}

pub struct AppState {
    pub pools: HashMap<String, Arc<BackendPool>>,
    pub default_pool_name: String,
    pub router: Router,
    pub backends: Vec<Arc<Backend>>,
    pub balancer: Box<dyn LoadBalancer>,
    pub client: ForwardingClient,
    pub config: Config,
    pub retry_budget: Arc<RetryBudget>,
    pub in_flight_requests: Arc<AtomicUsize>,
}

fn create_balancer(
    algorithm: Algorithm,
    backends: &[Arc<Backend>],
    slow_start_duration: Duration,
) -> Box<dyn LoadBalancer> {
    match algorithm {
        Algorithm::RoundRobin => Box::new(RoundRobin::new()),
        Algorithm::LeastConnections => Box::new(LeastConnections::new()),
        Algorithm::WeightedRoundRobin => {
            Box::new(WeightedRoundRobin::new(backends, slow_start_duration))
        }
        Algorithm::PowerOfTwoChoices => Box::new(PowerOfTwoChoices::new()),
        Algorithm::ConsistentHash => Box::new(ConsistentHash::new(backends)),
        Algorithm::LeastResponseTime => Box::new(LeastResponseTime::new()),
    }
}

impl AppState {
    // For backwards compatibility with single-pool configs & existing tests
    pub fn new(
        backends: Vec<Arc<Backend>>,
        balancer: Box<dyn LoadBalancer>,
        config: Config,
    ) -> Self {
        let mut connector = HttpConnector::new();
        connector.set_connect_timeout(Some(Duration::from_secs(
            config.timeouts.connect_secs.max(1),
        )));
        connector.set_keepalive(Some(Duration::from_secs(
            config.timeouts.idle_keepalive_secs.max(1),
        )));

        let client: ForwardingClient = Client::builder(TokioExecutor::new()).build(connector);
        let retry_budget = Arc::new(RetryBudget::new(config.retry.to_runtime()));
        let in_flight_requests = Arc::new(AtomicUsize::new(0));

        let default_pool_name = config.default_pool.clone();
        let slow_start_duration = Duration::from_secs(if config.slow_start.enabled {
            config.slow_start.window_secs
        } else {
            0
        });

        let mut pools = HashMap::new();
        let effective = config.effective_pools();

        for (name, pool_cfg) in &effective {
            let pool_backends = if name == &default_pool_name && !backends.is_empty() {
                backends.clone()
            } else {
                let cb_runtime = config.circuit_breaker.to_runtime();
                pool_cfg
                    .backends
                    .iter()
                    .filter_map(|b| {
                        b.addr
                            .parse()
                            .ok()
                            .map(|addr| Arc::new(Backend::new(addr, b.weight, cb_runtime.clone())))
                    })
                    .collect()
            };

            let pool_balancer: Box<dyn LoadBalancer> =
                create_balancer(pool_cfg.algorithm, &pool_backends, slow_start_duration);

            let hc = pool_cfg
                .health_check
                .clone()
                .unwrap_or_else(|| config.health_check.clone());

            let hash_target = pool_cfg.hash_key.as_deref().map(HashKeyTarget::parse);

            pools.insert(
                name.clone(),
                Arc::new(BackendPool {
                    name: name.clone(),
                    algorithm: pool_cfg.algorithm,
                    backends: pool_backends,
                    balancer: pool_balancer,
                    health_check: hc,
                    hash_target,
                }),
            );
        }

        let router = Router::new(&config.routes, default_pool_name.clone());

        Self {
            pools,
            default_pool_name,
            router,
            backends,
            balancer,
            client,
            config,
            retry_budget,
            in_flight_requests,
        }
    }

    pub fn from_config(config: Config, existing_backends: &[Arc<Backend>]) -> Self {
        let mut connector = HttpConnector::new();
        connector.set_connect_timeout(Some(Duration::from_secs(
            config.timeouts.connect_secs.max(1),
        )));
        connector.set_keepalive(Some(Duration::from_secs(
            config.timeouts.idle_keepalive_secs.max(1),
        )));

        let client: ForwardingClient = Client::builder(TokioExecutor::new()).build(connector);
        let retry_budget = Arc::new(RetryBudget::new(config.retry.to_runtime()));
        let in_flight_requests = Arc::new(AtomicUsize::new(0));

        let default_pool_name = config.default_pool.clone();
        let slow_start_duration = Duration::from_secs(if config.slow_start.enabled {
            config.slow_start.window_secs
        } else {
            0
        });

        let cb_runtime = config.circuit_breaker.to_runtime();
        let effective = config.effective_pools();
        let mut pools = HashMap::new();
        let mut all_unique_backends: HashMap<SocketAddr, Arc<Backend>> = HashMap::new();

        for (name, pool_cfg) in &effective {
            let mut pool_backends = Vec::new();
            for b_cfg in &pool_cfg.backends {
                if let Ok(addr) = b_cfg.addr.parse::<SocketAddr>() {
                    // Check if existing backend can be reused to preserve health and state
                    let backend =
                        if let Some(existing) = existing_backends.iter().find(|b| b.addr == addr) {
                            existing.set_weight(b_cfg.weight);
                            Arc::clone(existing)
                        } else if let Some(already_created) = all_unique_backends.get(&addr) {
                            already_created.set_weight(b_cfg.weight);
                            Arc::clone(already_created)
                        } else {
                            Arc::new(Backend::new(addr, b_cfg.weight, cb_runtime.clone()))
                        };

                    all_unique_backends.insert(addr, Arc::clone(&backend));
                    pool_backends.push(backend);
                }
            }

            let pool_balancer =
                create_balancer(pool_cfg.algorithm, &pool_backends, slow_start_duration);
            let hc = pool_cfg
                .health_check
                .clone()
                .unwrap_or_else(|| config.health_check.clone());
            let hash_target = pool_cfg.hash_key.as_deref().map(HashKeyTarget::parse);

            pools.insert(
                name.clone(),
                Arc::new(BackendPool {
                    name: name.clone(),
                    algorithm: pool_cfg.algorithm,
                    backends: pool_backends,
                    balancer: pool_balancer,
                    health_check: hc,
                    hash_target,
                }),
            );
        }

        let router = Router::new(&config.routes, default_pool_name.clone());
        let backends: Vec<Arc<Backend>> = all_unique_backends.into_values().collect();

        let default_balancer = if let Some(p) = pools.get(&default_pool_name) {
            create_balancer(p.algorithm, &p.backends, slow_start_duration)
        } else {
            Box::new(RoundRobin::new())
        };

        Self {
            pools,
            default_pool_name,
            router,
            backends,
            balancer: default_balancer,
            client,
            config,
            retry_budget,
            in_flight_requests,
        }
    }

    pub fn default_pool(&self) -> &Arc<BackendPool> {
        self.pools
            .get(&self.default_pool_name)
            .or_else(|| self.pools.values().next())
            .expect("at least one pool must exist")
    }

    pub fn find_backend(&self, addr: SocketAddr) -> Option<Arc<Backend>> {
        self.backends
            .iter()
            .find(|b| b.addr == addr)
            .map(Arc::clone)
    }
}

pub struct AppStateHolder {
    pub inner: ArcSwap<AppState>,
    pub config_path: PathBuf,
    pub health_cancel: Mutex<Option<CancellationToken>>,
}

impl AppStateHolder {
    pub fn new(config: Config, config_path: PathBuf) -> (Arc<Self>, CancellationToken) {
        let initial_cancel = CancellationToken::new();
        let state = AppState::from_config(config, &[]);
        let holder = Arc::new(Self {
            inner: ArcSwap::new(Arc::new(state)),
            config_path,
            health_cancel: Mutex::new(Some(initial_cancel.clone())),
        });
        (holder, initial_cancel)
    }

    pub fn load(&self) -> Arc<AppState> {
        self.inner.load_full()
    }

    pub fn reload(&self) -> Result<(), String> {
        tracing::info!(path = ?self.config_path, "initiating config hot reload");
        let new_config = match Config::load(&self.config_path) {
            Ok(c) => c,
            Err(e) => {
                let err_msg = format!("hot reload failed: config validation error: {e}");
                tracing::error!(%err_msg);
                return Err(err_msg);
            }
        };

        let current_state = self.inner.load_full();
        let new_state = AppState::from_config(new_config, &current_state.backends);

        // Cancel previous health checks and setup new ones
        let new_cancel = CancellationToken::new();
        {
            let mut guard = self.health_cancel.lock().unwrap();
            if let Some(old_cancel) = guard.replace(new_cancel.clone()) {
                old_cancel.cancel();
            }
        }

        // Spawn health checks for each pool in the new state
        for pool in new_state.pools.values() {
            crate::health::spawn_health_checks(
                pool.backends.clone(),
                pool.health_check.clone(),
                new_cancel.clone(),
            );
        }

        self.inner.store(Arc::new(new_state));
        tracing::info!("config hot reload succeeded");
        Ok(())
    }
}
