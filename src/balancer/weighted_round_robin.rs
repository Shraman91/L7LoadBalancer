use super::LoadBalancer;
use crate::backend::Backend;
use std::net::SocketAddr;
use std::sync::atomic::{AtomicI64, Ordering};
use std::sync::Arc;
use std::time::Duration;

pub struct WeightedRoundRobin {
    current_weights: Vec<AtomicI64>,
    slow_start_duration: Duration,
}

impl WeightedRoundRobin {
    pub fn new(backends: &[Arc<Backend>], slow_start_duration: Duration) -> Self {
        Self {
            current_weights: backends.iter().map(|_| AtomicI64::new(0)).collect(),
            slow_start_duration,
        }
    }
}

impl LoadBalancer for WeightedRoundRobin {
    fn select_excluding(
        &self,
        backends: &[Arc<Backend>],
        excluded: &[SocketAddr],
    ) -> Option<Arc<Backend>> {
        if backends.is_empty() || backends.len() != self.current_weights.len() {
            return None;
        }

        let total_weight: i64 = backends
            .iter()
            .filter(|b| !excluded.contains(&b.addr) && b.is_available())
            .map(|b| b.effective_weight(self.slow_start_duration) as i64)
            .sum();

        if total_weight == 0 {
            return None;
        }

        let mut best_idx: Option<usize> = None;
        let mut best_weight: i64 = i64::MIN;

        for (i, backend) in backends.iter().enumerate() {
            if excluded.contains(&backend.addr) || !backend.is_available() {
                continue;
            }
            let weight = backend.effective_weight(self.slow_start_duration) as i64;
            let new_weight = self.current_weights[i].fetch_add(weight, Ordering::Relaxed) + weight;

            if new_weight > best_weight {
                best_weight = new_weight;
                best_idx = Some(i);
            }
        }

        let idx = best_idx?;
        self.current_weights[idx].fetch_sub(total_weight, Ordering::Relaxed);

        Some(Arc::clone(&backends[idx]))
    }
}
