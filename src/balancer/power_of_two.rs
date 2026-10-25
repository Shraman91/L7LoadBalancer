use super::LoadBalancer;
use crate::backend::Backend;
use rand::Rng;
use std::net::SocketAddr;
use std::sync::Arc;

pub struct PowerOfTwoChoices;

impl PowerOfTwoChoices {
    pub fn new() -> Self {
        Self
    }
}

impl Default for PowerOfTwoChoices {
    fn default() -> Self {
        Self::new()
    }
}

impl LoadBalancer for PowerOfTwoChoices {
    fn select_with_key(
        &self,
        backends: &[Arc<Backend>],
        excluded: &[SocketAddr],
        _key: Option<&str>,
    ) -> Option<Arc<Backend>> {
        let available: Vec<&Arc<Backend>> = backends
            .iter()
            .filter(|b| !excluded.contains(&b.addr) && b.is_available())
            .collect();

        match available.len() {
            0 => None,
            1 => Some(Arc::clone(available[0])),
            n => {
                let mut rng = rand::thread_rng();
                let idx1 = rng.gen_range(0..n);
                let mut idx2 = rng.gen_range(0..n - 1);
                if idx2 >= idx1 {
                    idx2 += 1;
                }

                let b1 = available[idx1];
                let b2 = available[idx2];

                if b1.connections() <= b2.connections() {
                    Some(Arc::clone(b1))
                } else {
                    Some(Arc::clone(b2))
                }
            }
        }
    }
}
