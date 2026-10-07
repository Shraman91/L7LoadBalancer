use crate::circuit_breaker::CircuitBreakerConfig as CbRuntimeConfig;
use crate::retry::RetryConfig as RetryRuntimeConfig;
use serde::Deserialize;
use std::fs;
use std::net::SocketAddr;
use std::path::Path;
use std::time::Duration;

#[derive(Debug, Deserialize, Clone)]
pub struct BackendConfig {
    pub addr: String,
    #[serde(default = "default_weight")]
    pub weight: u32,
}

fn default_weight() -> u32 {
    1
}

#[derive(Debug, Deserialize, Clone)]
pub struct HealthCheckConfig {
    #[serde(default = "default_health_path")]
    pub path: String,
    #[serde(default = "default_interval")]
    pub interval_secs: u64,
    #[serde(default = "default_timeout")]
    pub timeout_secs: u64,
    #[serde(default = "default_unhealthy_threshold")]
    pub unhealthy_threshold: u32,
    #[serde(default = "default_healthy_threshold")]
    pub healthy_threshold: u32,
}

impl Default for HealthCheckConfig {
    fn default() -> Self {
        Self {
            path: default_health_path(),
            interval_secs: default_interval(),
            timeout_secs: default_timeout(),
            unhealthy_threshold: default_unhealthy_threshold(),
            healthy_threshold: default_healthy_threshold(),
        }
    }
}

fn default_health_path() -> String {
    "/health".to_string()
}
fn default_interval() -> u64 {
    5
}
fn default_timeout() -> u64 {
    2
}
fn default_unhealthy_threshold() -> u32 {
    3
}
fn default_healthy_threshold() -> u32 {
    2
}

#[derive(Debug, Deserialize, Clone)]
pub struct TimeoutsConfig {
    #[serde(default = "default_connect_timeout")]
    pub connect_secs: u64,
    #[serde(default = "default_upstream_header_timeout")]
    pub upstream_response_header_secs: u64,
    #[serde(default = "default_client_header_read_timeout")]
    pub client_header_read_secs: u64,
    #[serde(default = "default_idle_keepalive_timeout")]
    pub idle_keepalive_secs: u64,
    #[serde(default = "default_request_timeout")]
    pub request_secs: u64,
}

impl Default for TimeoutsConfig {
    fn default() -> Self {
        Self {
            connect_secs: default_connect_timeout(),
            upstream_response_header_secs: default_upstream_header_timeout(),
            client_header_read_secs: default_client_header_read_timeout(),
            idle_keepalive_secs: default_idle_keepalive_timeout(),
            request_secs: default_request_timeout(),
        }
    }
}

fn default_connect_timeout() -> u64 {
    2
}
fn default_upstream_header_timeout() -> u64 {
    10
}
fn default_client_header_read_timeout() -> u64 {
    10
}
fn default_idle_keepalive_timeout() -> u64 {
    60
}
fn default_request_timeout() -> u64 {
    30
}

#[derive(Debug, Deserialize, Clone)]
pub struct LimitsConfig {
    #[serde(default = "default_max_request_header_size")]
    pub max_request_header_size_bytes: usize,
    #[serde(default = "default_max_body_size")]
    pub max_body_size_bytes: usize, // 0 = unlimited
}

impl Default for LimitsConfig {
    fn default() -> Self {
        Self {
            max_request_header_size_bytes: default_max_request_header_size(),
            max_body_size_bytes: default_max_body_size(),
        }
    }
}

fn default_max_request_header_size() -> usize {
    65536 // 64KB
}
fn default_max_body_size() -> usize {
    0 // unlimited by default
}

#[derive(Debug, Deserialize, Clone)]
pub struct CircuitBreakerTomlConfig {
    #[serde(default = "default_true")]
    pub enabled: bool,
    #[serde(default = "default_cb_consecutive_failures")]
    pub consecutive_failures: u32,
    #[serde(default = "default_cb_cooldown")]
    pub cooldown_secs: u64,
    #[serde(default = "default_cb_half_open_trials")]
    pub half_open_max_trials: u32,
    #[serde(default = "default_cb_half_open_successes")]
    pub half_open_successes_to_close: u32,
}

impl Default for CircuitBreakerTomlConfig {
    fn default() -> Self {
        Self {
            enabled: true,
            consecutive_failures: default_cb_consecutive_failures(),
            cooldown_secs: default_cb_cooldown(),
            half_open_max_trials: default_cb_half_open_trials(),
            half_open_successes_to_close: default_cb_half_open_successes(),
        }
    }
}

fn default_true() -> bool {
    true
}
fn default_cb_consecutive_failures() -> u32 {
    5
}
fn default_cb_cooldown() -> u64 {
    10
}
fn default_cb_half_open_trials() -> u32 {
    2
}
fn default_cb_half_open_successes() -> u32 {
    2
}

impl CircuitBreakerTomlConfig {
    pub fn to_runtime(&self) -> CbRuntimeConfig {
        CbRuntimeConfig {
            enabled: self.enabled,
            consecutive_failures: self.consecutive_failures,
            cooldown_duration: Duration::from_secs(self.cooldown_secs),
            half_open_max_trials: self.half_open_max_trials,
            half_open_successes_to_close: self.half_open_successes_to_close,
        }
    }
}

#[derive(Debug, Deserialize, Clone)]
pub struct RetryTomlConfig {
    #[serde(default = "default_true")]
    pub enabled: bool,
    #[serde(default = "default_retry_max_attempts")]
    pub max_attempts: usize,
    #[serde(default = "default_retry_initial_backoff_ms")]
    pub initial_backoff_ms: u64,
    #[serde(default = "default_retry_max_backoff_ms")]
    pub max_backoff_ms: u64,
    #[serde(default = "default_retry_budget_ratio")]
    pub budget_ratio: f64,
    #[serde(default = "default_retry_min_tokens")]
    pub min_retry_tokens: i64,
}

impl Default for RetryTomlConfig {
    fn default() -> Self {
        Self {
            enabled: true,
            max_attempts: default_retry_max_attempts(),
            initial_backoff_ms: default_retry_initial_backoff_ms(),
            max_backoff_ms: default_retry_max_backoff_ms(),
            budget_ratio: default_retry_budget_ratio(),
            min_retry_tokens: default_retry_min_tokens(),
        }
    }
}

fn default_retry_max_attempts() -> usize {
    3
}
fn default_retry_initial_backoff_ms() -> u64 {
    100
}
fn default_retry_max_backoff_ms() -> u64 {
    2000
}
fn default_retry_budget_ratio() -> f64 {
    0.20
}
fn default_retry_min_tokens() -> i64 {
    10
}

impl RetryTomlConfig {
    pub fn to_runtime(&self) -> RetryRuntimeConfig {
        RetryRuntimeConfig {
            enabled: self.enabled,
            max_attempts: self.max_attempts,
            initial_backoff: Duration::from_millis(self.initial_backoff_ms),
            max_backoff: Duration::from_millis(self.max_backoff_ms),
            budget_ratio: self.budget_ratio,
            min_retry_tokens: self.min_retry_tokens,
        }
    }
}

#[derive(Debug, Deserialize, Clone)]
pub struct PassiveHealthCheckConfig {
    #[serde(default = "default_true")]
    pub enabled: bool,
    #[serde(default = "default_passive_unhealthy_threshold")]
    pub unhealthy_threshold: u32,
}

impl Default for PassiveHealthCheckConfig {
    fn default() -> Self {
        Self {
            enabled: true,
            unhealthy_threshold: default_passive_unhealthy_threshold(),
        }
    }
}

fn default_passive_unhealthy_threshold() -> u32 {
    3
}

#[derive(Debug, Deserialize, Clone)]
pub struct SlowStartConfig {
    #[serde(default = "default_true")]
    pub enabled: bool,
    #[serde(default = "default_slow_start_window")]
    pub window_secs: u64,
}

impl Default for SlowStartConfig {
    fn default() -> Self {
        Self {
            enabled: true,
            window_secs: default_slow_start_window(),
        }
    }
}

fn default_slow_start_window() -> u64 {
    30
}

#[derive(Debug, Deserialize, Clone)]
pub struct ShutdownConfig {
    #[serde(default = "default_drain_timeout")]
    pub drain_timeout_secs: u64,
}

impl Default for ShutdownConfig {
    fn default() -> Self {
        Self {
            drain_timeout_secs: default_drain_timeout(),
        }
    }
}

fn default_drain_timeout() -> u64 {
    30
}

#[derive(Debug, Deserialize, Clone, Copy, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum Algorithm {
    RoundRobin,
    WeightedRoundRobin,
    LeastConnections,
}

#[derive(Debug, Deserialize, Clone)]
pub struct Config {
    pub listen_addr: String,
    pub algorithm: Algorithm,
    #[serde(default)]
    pub health_check: HealthCheckConfig,
    pub backends: Vec<BackendConfig>,

    #[serde(default)]
    pub timeouts: TimeoutsConfig,
    #[serde(default)]
    pub limits: LimitsConfig,
    #[serde(default)]
    pub circuit_breaker: CircuitBreakerTomlConfig,
    #[serde(default)]
    pub retry: RetryTomlConfig,
    #[serde(default)]
    pub passive_health_check: PassiveHealthCheckConfig,
    #[serde(default)]
    pub slow_start: SlowStartConfig,
    #[serde(default)]
    pub shutdown: ShutdownConfig,
}

impl Config {
    pub fn load(path: impl AsRef<Path>) -> anyhow::Result<Self> {
        let raw = fs::read_to_string(path)?;
        let cfg: Config = toml::from_str(&raw)?;
        if cfg.backends.is_empty() {
            anyhow::bail!("config must define at least one backend");
        }
        Ok(cfg)
    }

    pub fn listen_socket_addr(&self) -> anyhow::Result<SocketAddr> {
        Ok(self.listen_addr.parse()?)
    }
}
