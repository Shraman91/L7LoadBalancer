use crate::circuit_breaker::{STATE_CLOSED, STATE_HALF_OPEN, STATE_OPEN};
use crate::state::AppState;
use prometheus::{
    Counter, CounterVec, Encoder, GaugeVec, HistogramOpts, HistogramVec, Opts, Registry,
    TextEncoder,
};
use std::sync::Arc;

#[derive(Clone)]
pub struct Metrics {
    pub registry: Arc<Registry>,
    pub http_requests_total: CounterVec,
    pub http_request_duration_seconds: HistogramVec,
    pub active_connections: GaugeVec,
    pub backend_health: GaugeVec,
    pub backend_circuit_state: GaugeVec,
    pub http_retries_total: CounterVec,
    pub rate_limited_requests_total: Counter,
    pub load_shed_requests_total: Counter,
}

impl Metrics {
    pub fn new() -> Result<Self, prometheus::Error> {
        let registry = Registry::new();

        let http_requests_total = CounterVec::new(
            Opts::new(
                "http_requests_total",
                "Total number of HTTP requests processed",
            ),
            &["status", "pool"],
        )?;
        registry.register(Box::new(http_requests_total.clone()))?;

        let duration_opts = HistogramOpts::new(
            "http_request_duration_seconds",
            "HTTP request latency in seconds",
        )
        .buckets(vec![
            0.001, 0.005, 0.01, 0.025, 0.05, 0.1, 0.25, 0.5, 1.0, 2.5, 5.0, 10.0,
        ]);
        let http_request_duration_seconds = HistogramVec::new(duration_opts, &["status", "pool"])?;
        registry.register(Box::new(http_request_duration_seconds.clone()))?;

        let active_connections = GaugeVec::new(
            Opts::new(
                "active_connections",
                "Number of currently active connections per backend",
            ),
            &["pool", "backend"],
        )?;
        registry.register(Box::new(active_connections.clone()))?;

        let backend_health = GaugeVec::new(
            Opts::new(
                "backend_health",
                "Backend health status (1 = healthy, 0 = unhealthy)",
            ),
            &["pool", "backend"],
        )?;
        registry.register(Box::new(backend_health.clone()))?;

        let backend_circuit_state = GaugeVec::new(
            Opts::new(
                "backend_circuit_state",
                "Backend circuit breaker state (0 = closed, 1 = half_open, 2 = open)",
            ),
            &["pool", "backend"],
        )?;
        registry.register(Box::new(backend_circuit_state.clone()))?;

        let http_retries_total = CounterVec::new(
            Opts::new("http_retries_total", "Total number of request retries"),
            &["pool"],
        )?;
        registry.register(Box::new(http_retries_total.clone()))?;

        let rate_limited_requests_total = Counter::new(
            "rate_limited_requests_total",
            "Total number of requests rejected by rate limiting (HTTP 429)",
        )?;
        registry.register(Box::new(rate_limited_requests_total.clone()))?;

        let load_shed_requests_total = Counter::new(
            "load_shed_requests_total",
            "Total number of requests shed due to concurrency limits (HTTP 503)",
        )?;
        registry.register(Box::new(load_shed_requests_total.clone()))?;

        Ok(Self {
            registry: Arc::new(registry),
            http_requests_total,
            http_request_duration_seconds,
            active_connections,
            backend_health,
            backend_circuit_state,
            http_retries_total,
            rate_limited_requests_total,
            load_shed_requests_total,
        })
    }

    /// Collect live state metrics from AppState and encode everything into Prometheus text format.
    pub fn gather(&self, state: &AppState) -> String {
        // Collect live backend gauges
        for (pool_name, pool) in &state.pools {
            for backend in &pool.backends {
                let addr = backend.addr.to_string();
                self.active_connections
                    .with_label_values(&[pool_name, &addr])
                    .set(backend.connections() as f64);

                let health = if backend.is_healthy() { 1.0 } else { 0.0 };
                self.backend_health
                    .with_label_values(&[pool_name, &addr])
                    .set(health);

                let circuit_val = match backend.circuit_breaker.state() {
                    STATE_CLOSED => 0.0,
                    STATE_HALF_OPEN => 1.0,
                    STATE_OPEN => 2.0,
                    _ => 0.0,
                };
                self.backend_circuit_state
                    .with_label_values(&[pool_name, &addr])
                    .set(circuit_val);
            }
        }

        let encoder = TextEncoder::new();
        let metric_families = self.registry.gather();
        let mut buffer = Vec::new();
        if let Err(e) = encoder.encode(&metric_families, &mut buffer) {
            tracing::error!(error = %e, "failed to encode prometheus metrics");
        }
        String::from_utf8(buffer).unwrap_or_default()
    }
}
