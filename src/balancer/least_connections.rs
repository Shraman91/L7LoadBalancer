use super::LoadBalancer;
use crate::backend::Backend;
use std::sync::Arc;

pub struct LeastConnections;

impl LeastConnections {
    pub fn new() -> Self {
        Self
    }
}

impl LoadBalancer for LeastConnections {
    fn select(&self, backends: &[Arc<Backend>]) -> Option<Arc<Backend>> {
        backends
            .iter()
            .filter(|b| b.is_healthy())
            .min_by_key(|b| b.connections())
            .map(Arc::clone)
    }
}
