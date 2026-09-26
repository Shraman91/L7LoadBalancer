use super::LoadBalancer;
use crate::backend::Backend;
use std::sync::atomic::{AtomicUsize, Ordering};
use std::sync::Arc;

pub struct RoundRobin {
    cursor: AtomicUsize,
}

impl RoundRobin {
    pub fn new() -> Self {
        Self {
            cursor: AtomicUsize::new(0),
        }
    }
}

impl LoadBalancer for RoundRobin {
    fn select(&self, backends: &[Arc<Backend>]) -> Option<Arc<Backend>> {
        let len = backends.len();
        if len == 0 {
            return None;
        }

        for _ in 0..len {
            let idx = self.cursor.fetch_add(1, Ordering::Relaxed) % len;
            let candidate = &backends[idx];
            if candidate.is_healthy() {
                return Some(Arc::clone(candidate));
            }
        }
        None
    }
}
