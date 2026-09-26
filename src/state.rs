use crate::backend::Backend;
use crate::balancer::LoadBalancer;
use hyper::body::Incoming;
use hyper_util::client::legacy::connect::HttpConnector;
use hyper_util::client::legacy::Client;
use hyper_util::rt::TokioExecutor;
use std::sync::Arc;

pub type ForwardingClient = Client<HttpConnector, Incoming>;

pub struct AppState {
    pub backends: Vec<Arc<Backend>>,
    pub balancer: Box<dyn LoadBalancer>,
    pub client: ForwardingClient,
}

impl AppState {
    pub fn new(backends: Vec<Arc<Backend>>, balancer: Box<dyn LoadBalancer>) -> Self {
        let client: ForwardingClient =
            Client::builder(TokioExecutor::new()).build(HttpConnector::new());
        Self {
            backends,
            balancer,
            client,
        }
    }
}
