use super::LoadBalancer;
use crate::backend::Backend;
use std::collections::hash_map::DefaultHasher;
use std::hash::{Hash, Hasher};
use std::net::SocketAddr;
use std::sync::Arc;

pub const DEFAULT_VNODES_PER_BACKEND: usize = 128;

pub struct ConsistentHash {
    // Sorted array of (hash_point, backend_addr)
    ring: Vec<(u64, SocketAddr)>,
}

fn hash_str(val: &str) -> u64 {
    let mut hasher = DefaultHasher::new();
    val.hash(&mut hasher);
    hasher.finish()
}

impl ConsistentHash {
    pub fn new(backends: &[Arc<Backend>]) -> Self {
        Self::with_vnodes(backends, DEFAULT_VNODES_PER_BACKEND)
    }

    pub fn with_vnodes(backends: &[Arc<Backend>], vnodes: usize) -> Self {
        let mut ring = Vec::with_capacity(backends.len() * vnodes);
        for backend in backends {
            for v in 0..vnodes {
                let token = format!("{}-vnode-{}", backend.addr, v);
                let point = hash_str(&token);
                ring.push((point, backend.addr));
            }
        }
        ring.sort_by_key(|(point, _)| *point);
        Self { ring }
    }
}

impl LoadBalancer for ConsistentHash {
    fn select_with_key(
        &self,
        backends: &[Arc<Backend>],
        excluded: &[SocketAddr],
        key: Option<&str>,
    ) -> Option<Arc<Backend>> {
        if backends.is_empty() || self.ring.is_empty() {
            return None;
        }

        let hash = match key {
            Some(k) => hash_str(k),
            None => {
                // If no key provided, fallback to first available backend
                return backends
                    .iter()
                    .find(|b| !excluded.contains(&b.addr) && b.is_available())
                    .map(Arc::clone);
            }
        };

        // Binary search for the first virtual node with point >= hash
        let ring_len = self.ring.len();
        let start_idx = self.ring.partition_point(|(point, _)| *point < hash) % ring_len;

        // Traverse ring from start_idx to find the first healthy, non-excluded backend
        let mut visited = Vec::new();
        for offset in 0..ring_len {
            let addr = self.ring[(start_idx + offset) % ring_len].1;
            if visited.contains(&addr) {
                continue;
            }
            visited.push(addr);

            if excluded.contains(&addr) {
                continue;
            }

            if let Some(backend) = backends.iter().find(|b| b.addr == addr) {
                if backend.is_available() {
                    return Some(Arc::clone(backend));
                }
            }
        }

        None
    }
}
