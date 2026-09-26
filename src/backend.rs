use std::net::SocketAddr;
use std::sync::atomic::{AtomicBool, AtomicU32, AtomicUsize, Ordering};


#[derive(Debug)]
pub struct Backend {
    pub addr: SocketAddr,
    pub weight: u32,

    pub healthy: AtomicBool,
    pub active_connections: AtomicUsize,

    pub consecutive_failures: AtomicU32,
    pub consecutive_successes: AtomicU32,
}

impl Backend {
    pub fn new(addr: SocketAddr, weight: u32) -> Self {
        Self {
            addr,
            weight,

            healthy: AtomicBool::new(true),
            active_connections: AtomicUsize::new(0),
            consecutive_failures: AtomicU32::new(0),
            consecutive_successes: AtomicU32::new(0),
        }
    }

    pub fn is_healthy(&self) -> bool {
        self.healthy.load(Ordering::Relaxed)
    }

    pub fn inc_connections(&self) {
        self.active_connections.fetch_add(1, Ordering::Relaxed);
    }

    pub fn dec_connections(&self) {
        self.active_connections.fetch_sub(1, Ordering::Relaxed);
    }

    pub fn connections(&self) -> usize {
        self.active_connections.load(Ordering::Relaxed)
    }

    pub fn record_check(&self, ok: bool, unhealthy_threshold: u32, healthy_threshold: u32) {
        if ok {
            self.consecutive_failures.store(0, Ordering::Relaxed);
            let successes = self.consecutive_successes.fetch_add(1, Ordering::Relaxed) + 1;
            if !self.is_healthy() && successes >= healthy_threshold {
                self.healthy.store(true, Ordering::Relaxed);
                tracing::info!(backend = %self.addr, "backend marked healthy");
            }
        } else {
            self.consecutive_successes.store(0, Ordering::Relaxed);
            let failures = self.consecutive_failures.fetch_add(1, Ordering::Relaxed) + 1;
            if self.is_healthy() && failures >= unhealthy_threshold {
                self.healthy.store(false, Ordering::Relaxed);
                tracing::warn!(backend = %self.addr, "backend marked unhealthy");
            }
        }
    }
}
