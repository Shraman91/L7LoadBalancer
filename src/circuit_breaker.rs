use std::sync::atomic::{AtomicU32, AtomicU64, AtomicU8, Ordering};
use std::time::{Duration, SystemTime, UNIX_EPOCH};

pub const STATE_CLOSED: u8 = 0;
pub const STATE_OPEN: u8 = 1;
pub const STATE_HALF_OPEN: u8 = 2;

#[derive(Debug, Clone)]
pub struct CircuitBreakerConfig {
    pub enabled: bool,
    pub consecutive_failures: u32,
    pub cooldown_duration: Duration,
    pub half_open_max_trials: u32,
    pub half_open_successes_to_close: u32,
}

impl Default for CircuitBreakerConfig {
    fn default() -> Self {
        Self {
            enabled: true,
            consecutive_failures: 5,
            cooldown_duration: Duration::from_secs(10),
            half_open_max_trials: 2,
            half_open_successes_to_close: 2,
        }
    }
}

#[derive(Debug)]
pub struct CircuitBreaker {
    state: AtomicU8,
    consecutive_failures: AtomicU32,
    opened_at_millis: AtomicU64,
    half_open_trials: AtomicU32,
    half_open_successes: AtomicU32,
    config: CircuitBreakerConfig,
}

fn now_millis() -> u64 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .unwrap_or_default()
        .as_millis() as u64
}

impl CircuitBreaker {
    pub fn new(config: CircuitBreakerConfig) -> Self {
        Self {
            state: AtomicU8::new(STATE_CLOSED),
            consecutive_failures: AtomicU32::new(0),
            opened_at_millis: AtomicU64::new(0),
            half_open_trials: AtomicU32::new(0),
            half_open_successes: AtomicU32::new(0),
            config,
        }
    }

    pub fn state(&self) -> u8 {
        self.state.load(Ordering::Acquire)
    }

    pub fn state_str(&self) -> &'static str {
        match self.state() {
            STATE_CLOSED => "closed",
            STATE_OPEN => "open",
            STATE_HALF_OPEN => "half_open",
            _ => "unknown",
        }
    }

    pub fn can_attempt(&self) -> bool {
        if !self.config.enabled {
            return true;
        }

        let current_state = self.state.load(Ordering::Acquire);
        match current_state {
            STATE_CLOSED => true,
            STATE_OPEN => {
                let opened_at = self.opened_at_millis.load(Ordering::Acquire);
                let now = now_millis();
                if now.saturating_sub(opened_at) >= self.config.cooldown_duration.as_millis() as u64
                {
                    // Attempt transition to HalfOpen
                    if self
                        .state
                        .compare_exchange(
                            STATE_OPEN,
                            STATE_HALF_OPEN,
                            Ordering::AcqRel,
                            Ordering::Acquire,
                        )
                        .is_ok()
                    {
                        self.half_open_trials.store(1, Ordering::Release);
                        self.half_open_successes.store(0, Ordering::Release);
                        return true;
                    }
                }
                false
            }
            STATE_HALF_OPEN => {
                let prev = self.half_open_trials.fetch_add(1, Ordering::AcqRel);
                prev < self.config.half_open_max_trials
            }
            _ => true,
        }
    }

    pub fn record_success(&self) {
        if !self.config.enabled {
            return;
        }

        let current_state = self.state.load(Ordering::Acquire);
        match current_state {
            STATE_CLOSED => {
                self.consecutive_failures.store(0, Ordering::Release);
            }
            STATE_HALF_OPEN => {
                let successes = self.half_open_successes.fetch_add(1, Ordering::AcqRel) + 1;
                if successes >= self.config.half_open_successes_to_close
                    && self
                        .state
                        .compare_exchange(
                            STATE_HALF_OPEN,
                            STATE_CLOSED,
                            Ordering::AcqRel,
                            Ordering::Acquire,
                        )
                        .is_ok()
                {
                    self.consecutive_failures.store(0, Ordering::Release);
                    self.half_open_trials.store(0, Ordering::Release);
                    self.half_open_successes.store(0, Ordering::Release);
                }
            }
            _ => {}
        }
    }

    pub fn record_failure(&self) {
        if !self.config.enabled {
            return;
        }

        let current_state = self.state.load(Ordering::Acquire);
        match current_state {
            STATE_CLOSED => {
                let fails = self.consecutive_failures.fetch_add(1, Ordering::AcqRel) + 1;
                if fails >= self.config.consecutive_failures {
                    self.opened_at_millis.store(now_millis(), Ordering::Release);
                    let _ = self.state.compare_exchange(
                        STATE_CLOSED,
                        STATE_OPEN,
                        Ordering::AcqRel,
                        Ordering::Acquire,
                    );
                }
            }
            STATE_HALF_OPEN => {
                self.opened_at_millis.store(now_millis(), Ordering::Release);
                let _ = self.state.compare_exchange(
                    STATE_HALF_OPEN,
                    STATE_OPEN,
                    Ordering::AcqRel,
                    Ordering::Acquire,
                );
            }
            _ => {}
        }
    }
}
