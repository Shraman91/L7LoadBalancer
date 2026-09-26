use super::LoadBalancer;
use crate::backend::Backend;
use std::sync::atomic::{AtomicI64, Ordering};
use std::sync::Arc;

pub struct WeightedRoundRobin {
    // current effective weight per backend index, reset/accumulated each pick
    current_weights: Vec<AtomicI64>,
}

impl WeightedRoundRobin {
    pub fn new(backends: &[Arc<Backend>]) -> Self {
        Self {
            current_weights: backends.iter().map(|_| AtomicI64::new(0)).collect(),
        }
    }
}

impl LoadBalancer for WeightedRoundRobin {
    fn select(&self, backends: &[Arc<Backend>]) -> Option<Arc<Backend>> {
        if backends.is_empty() || backends.len() != self.current_weights.len() {
            return None;
        }

        let total_weight: i64 = backends
            .iter()
            .filter(|b| b.is_healthy())
            .map(|b| b.weight as i64)
            .sum();

        if total_weight == 0 {
            return None;
        }

        let mut best_idx: Option<usize> = None;
        let mut best_weight: i64 = i64::MIN;

        for (i, backend) in backends.iter().enumerate() {
            if !backend.is_healthy() {
                continue;
            }
            let new_weight =
                self.current_weights[i].fetch_add(backend.weight as i64, Ordering::Relaxed)
                    + backend.weight as i64;

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
