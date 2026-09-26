use serde::Deserialize;
use std::fs;
use std::net::SocketAddr;
use std::path::Path;

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
    pub health_check: HealthCheckConfig,
    pub backends: Vec<BackendConfig>,
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
