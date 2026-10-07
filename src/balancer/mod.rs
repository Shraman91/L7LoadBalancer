mod least_connections;
mod round_robin;
mod weighted_round_robin;

pub use least_connections::LeastConnections;
pub use round_robin::RoundRobin;
pub use weighted_round_robin::WeightedRoundRobin;

use crate::backend::Backend;
use std::net::SocketAddr;
use std::sync::Arc;

pub trait LoadBalancer: Send + Sync {
    fn select(&self, backends: &[Arc<Backend>]) -> Option<Arc<Backend>> {
        self.select_excluding(backends, &[])
    }

    fn select_excluding(
        &self,
        backends: &[Arc<Backend>],
        excluded: &[SocketAddr],
    ) -> Option<Arc<Backend>>;
}
