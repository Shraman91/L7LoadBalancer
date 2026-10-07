use crate::backend::Backend;
use crate::balancer::LoadBalancer;
use crate::config::Config;
use crate::proxy::BoxedBody;
use crate::retry::RetryBudget;
use hyper_util::client::legacy::connect::HttpConnector;
use hyper_util::client::legacy::Client;
use hyper_util::rt::TokioExecutor;
use std::sync::atomic::AtomicUsize;
use std::sync::Arc;
use std::time::Duration;

pub type ForwardingClient = Client<HttpConnector, BoxedBody>;

pub struct AppState {
    pub backends: Vec<Arc<Backend>>,
    pub balancer: Box<dyn LoadBalancer>,
    pub client: ForwardingClient,
    pub config: Config,
    pub retry_budget: Arc<RetryBudget>,
    pub in_flight_requests: Arc<AtomicUsize>,
}

impl AppState {
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

        Self {
            backends,
            balancer,
            client,
            config,
            retry_budget,
            in_flight_requests,
        }
    }
}
