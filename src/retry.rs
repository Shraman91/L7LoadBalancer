use hyper::Method;
use rand::Rng;
use std::sync::atomic::{AtomicI64, AtomicU64, Ordering};
use std::time::Duration;

#[derive(Debug, Clone)]
pub struct RetryConfig {
    pub enabled: bool,
    pub max_attempts: usize,
    pub initial_backoff: Duration,
    pub max_backoff: Duration,
    pub budget_ratio: f64,
    pub min_retry_tokens: i64,
}

impl Default for RetryConfig {
    fn default() -> Self {
        Self {
            enabled: true,
            max_attempts: 3,
            initial_backoff: Duration::from_millis(100),
            max_backoff: Duration::from_millis(2000),
            budget_ratio: 0.20,
            min_retry_tokens: 10,
        }
    }
}

pub fn is_idempotent_method(method: &Method) -> bool {
    matches!(
        *method,
        Method::GET | Method::HEAD | Method::OPTIONS | Method::PUT | Method::DELETE | Method::TRACE
    )
}

#[derive(Debug)]
pub struct RetryBudget {
    // Atomic tokens scaled by 1000 to allow fractional accumulation without floating point in atomics
    tokens: AtomicI64,
    total_requests: AtomicU64,
    total_retries: AtomicU64,
    config: RetryConfig,
}

impl RetryBudget {
    pub fn new(config: RetryConfig) -> Self {
        let initial_tokens = (config.min_retry_tokens * 1000).max(1000);
        Self {
            tokens: AtomicI64::new(initial_tokens),
            total_requests: AtomicU64::new(0),
            total_retries: AtomicU64::new(0),
            config,
        }
    }

    pub fn record_request(&self) {
        if !self.config.enabled {
            return;
        }
        self.total_requests.fetch_add(1, Ordering::Relaxed);
        let token_delta = (self.config.budget_ratio * 1000.0) as i64;
        let max_tokens = (self.config.min_retry_tokens * 1000).max(100_000);

        let mut curr = self.tokens.load(Ordering::Relaxed);
        loop {
            let next = (curr + token_delta).min(max_tokens);
            match self.tokens.compare_exchange_weak(
                curr,
                next,
                Ordering::Relaxed,
                Ordering::Relaxed,
            ) {
                Ok(_) => break,
                Err(actual) => curr = actual,
            }
        }
    }

    pub fn try_acquire_retry(&self) -> bool {
        if !self.config.enabled {
            return false;
        }

        let retry_cost = 1000i64;
        let mut curr = self.tokens.load(Ordering::Relaxed);
        loop {
            if curr < retry_cost {
                return false;
            }
            let next = curr - retry_cost;
            match self.tokens.compare_exchange_weak(
                curr,
                next,
                Ordering::Relaxed,
                Ordering::Relaxed,
            ) {
                Ok(_) => {
                    self.total_retries.fetch_add(1, Ordering::Relaxed);
                    return true;
                }
                Err(actual) => curr = actual,
            }
        }
    }

    pub fn compute_backoff(&self, attempt: usize) -> Duration {
        if attempt == 0 {
            return Duration::ZERO;
        }
        let mult = 1u64 << (attempt.saturating_sub(1).min(6));
        let max_millis = self.config.max_backoff.as_millis() as u64;
        let initial_millis = self.config.initial_backoff.as_millis() as u64;
        let base_delay = (initial_millis.saturating_mul(mult)).min(max_millis);

        // Full jitter: uniformly random between 0 and base_delay
        let jitter = if base_delay > 0 {
            rand::thread_rng().gen_range(0..=base_delay)
        } else {
            0
        };
        Duration::from_millis(jitter)
    }

    pub fn total_retries(&self) -> u64 {
        self.total_retries.load(Ordering::Relaxed)
    }
}
