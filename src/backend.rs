use crate::circuit_breaker::{CircuitBreaker, CircuitBreakerConfig};
use std::net::SocketAddr;
use std::sync::atomic::{AtomicBool, AtomicU32, AtomicU64, AtomicUsize, Ordering};
use std::time::{Duration, SystemTime, UNIX_EPOCH};

fn now_millis() -> u64 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .unwrap_or_default()
        .as_millis() as u64
}

#[derive(Debug)]
pub struct Backend {
    pub addr: SocketAddr,
    pub weight: u32,

    pub healthy: AtomicBool,
    pub draining: AtomicBool,
    pub active_connections: AtomicUsize,

    pub consecutive_failures: AtomicU32,
    pub consecutive_successes: AtomicU32,
    pub consecutive_passive_failures: AtomicU32,

    pub healthy_since_millis: AtomicU64,
    pub circuit_breaker: CircuitBreaker,
}

impl Backend {
    pub fn new(addr: SocketAddr, weight: u32, cb_config: CircuitBreakerConfig) -> Self {
        Self {
            addr,
            weight,

            healthy: AtomicBool::new(true),
            draining: AtomicBool::new(false),
            active_connections: AtomicUsize::new(0),
            consecutive_failures: AtomicU32::new(0),
            consecutive_successes: AtomicU32::new(0),
            consecutive_passive_failures: AtomicU32::new(0),
            healthy_since_millis: AtomicU64::new(now_millis()),
            circuit_breaker: CircuitBreaker::new(cb_config),
        }
    }

    pub fn is_healthy(&self) -> bool {
        self.healthy.load(Ordering::Acquire) && !self.draining.load(Ordering::Acquire)
    }

    pub fn is_available(&self) -> bool {
        self.is_healthy() && self.circuit_breaker.can_attempt()
    }

    pub fn set_draining(&self, draining: bool) {
        self.draining.store(draining, Ordering::Release);
    }

    pub fn is_draining(&self) -> bool {
        self.draining.load(Ordering::Acquire)
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

    pub fn effective_weight(&self, slow_start_duration: Duration) -> u32 {
        if self.weight == 0 {
            return 0;
        }
        if slow_start_duration.is_zero() {
            return self.weight;
        }

        let healthy_since = self.healthy_since_millis.load(Ordering::Acquire);
        if healthy_since == 0 {
            return self.weight;
        }

        let elapsed = now_millis().saturating_sub(healthy_since);
        let total_ms = slow_start_duration.as_millis() as u64;
        if elapsed >= total_ms || total_ms == 0 {
            self.weight
        } else {
            let progress = elapsed as f64 / total_ms as f64;
            let ramped = (self.weight as f64 * progress).round() as u32;
            ramped.max(1).min(self.weight)
        }
    }

    pub fn record_active_check(&self, ok: bool, unhealthy_threshold: u32, healthy_threshold: u32) {
        if ok {
            self.consecutive_failures.store(0, Ordering::Release);
            self.consecutive_passive_failures
                .store(0, Ordering::Release);
            let successes = self.consecutive_successes.fetch_add(1, Ordering::AcqRel) + 1;
            if !self.healthy.load(Ordering::Acquire) && successes >= healthy_threshold {
                self.healthy.store(true, Ordering::Release);
                self.healthy_since_millis
                    .store(now_millis(), Ordering::Release);
                tracing::info!(backend = %self.addr, "backend marked healthy by active health check");
            }
        } else {
            self.consecutive_successes.store(0, Ordering::Release);
            let failures = self.consecutive_failures.fetch_add(1, Ordering::AcqRel) + 1;
            if self.healthy.load(Ordering::Acquire) && failures >= unhealthy_threshold {
                self.healthy.store(false, Ordering::Release);
                tracing::warn!(backend = %self.addr, "backend marked unhealthy by active health check");
            }
        }
    }

    pub fn record_passive_check(&self, ok: bool, unhealthy_threshold: u32) {
        if ok {
            self.consecutive_passive_failures
                .store(0, Ordering::Release);
            self.circuit_breaker.record_success();
        } else {
            self.circuit_breaker.record_failure();
            let failures = self
                .consecutive_passive_failures
                .fetch_add(1, Ordering::AcqRel)
                + 1;
            if self.healthy.load(Ordering::Acquire) && failures >= unhealthy_threshold {
                self.healthy.store(false, Ordering::Release);
                tracing::warn!(backend = %self.addr, failures, "backend marked unhealthy by passive health check");
            }
        }
    }
}
