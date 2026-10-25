use super::LoadBalancer;
use crate::backend::Backend;
use std::net::SocketAddr;
use std::sync::Arc;

pub struct LeastConnections;

impl LeastConnections {
    pub fn new() -> Self {
        Self
    }
}

impl Default for LeastConnections {
    fn default() -> Self {
        Self::new()
    }
}

impl LoadBalancer for LeastConnections {
    fn select_with_key(
        &self,
        backends: &[Arc<Backend>],
        excluded: &[SocketAddr],
        _key: Option<&str>,
    ) -> Option<Arc<Backend>> {
        backends
            .iter()
            .filter(|b| !excluded.contains(&b.addr) && b.is_available())
            .min_by_key(|b| b.connections())
            .map(Arc::clone)
    }
}
