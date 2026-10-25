mod consistent_hash;
mod least_connections;
mod least_response_time;
mod power_of_two;
mod round_robin;
mod weighted_round_robin;

pub use consistent_hash::ConsistentHash;
pub use least_connections::LeastConnections;
pub use least_response_time::LeastResponseTime;
pub use power_of_two::PowerOfTwoChoices;
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
    ) -> Option<Arc<Backend>> {
        self.select_with_key(backends, excluded, None)
    }

    fn select_with_key(
        &self,
        backends: &[Arc<Backend>],
        excluded: &[SocketAddr],
        key: Option<&str>,
    ) -> Option<Arc<Backend>>;
}
