use super::LoadBalancer;
use crate::backend::Backend;
use std::net::SocketAddr;
use std::sync::atomic::Ordering;
use std::sync::Arc;

pub struct LeastResponseTime;

impl LeastResponseTime {
    pub fn new() -> Self {
        Self
    }
}

impl Default for LeastResponseTime {
    fn default() -> Self {
        Self::new()
    }
}

impl LoadBalancer for LeastResponseTime {
    fn select_with_key(
        &self,
        backends: &[Arc<Backend>],
        excluded: &[SocketAddr],
        _key: Option<&str>,
    ) -> Option<Arc<Backend>> {
        backends
            .iter()
            .filter(|b| !excluded.contains(&b.addr) && b.is_available())
            .min_by_key(|b| {
                (
                    b.ewma_latency_micros.load(Ordering::Relaxed),
                    b.connections(),
                )
            })
            .map(Arc::clone)
    }
}
