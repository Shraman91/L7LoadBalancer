export type CircuitState = "closed" | "open" | "half_open";

export interface BackendStatus {
  addr: string;
  healthy: boolean;
  draining: boolean;
  active_connections: number;
  circuit_state: CircuitState;
  weight: number;
  ewma_latency_ms: number;
  tls: boolean;
  pool?: string;
  slow_start_progress?: number; // 0 to 100% if in slow-start
}

export interface BackendConfig {
  addr: string;
  weight: number;
  tls: boolean;
  tls_sni?: string | null;
  tls_insecure: boolean;
}

export interface HealthCheckConfig {
  path: string;
  interval_secs: number;
  timeout_secs: number;
  unhealthy_threshold: number;
  healthy_threshold: number;
}

export interface PoolConfig {
  algorithm: string;
  health_check?: HealthCheckConfig | null;
  backends: BackendConfig[];
  hash_key?: string | null;
}

export interface RouteRuleConfig {
  host?: string | null;
  path_prefix: string;
  pool: string;
  strip_prefix?: boolean | null;
  rewrite_prefix?: string | null;
}

export interface TlsConfig {
  enabled: boolean;
  cert_path: string;
  key_path: string;
  alpn: string[];
}

export interface RateLimitConfig {
  enabled: boolean;
  requests_per_second: number;
  burst: number;
  max_entries: number;
  cleanup_interval_secs: number;
}

export interface CircuitBreakerConfig {
  failure_threshold: number;
  cooldown_secs: number;
  half_open_max_trials: number;
  half_open_successes_to_close: number;
}

export interface RetryConfig {
  enabled: boolean;
  max_attempts: number;
  initial_backoff_ms: number;
  max_backoff_ms: number;
  budget_ratio: number;
  min_retry_tokens: number;
}

export interface TimeoutsConfig {
  connect_secs: number;
  upstream_response_header_secs: number;
  client_header_read_secs: number;
  idle_keepalive_secs: number;
}

export interface LimitsConfig {
  max_request_header_size_bytes: number;
  max_body_size_bytes: number;
  max_concurrent_connections: number;
}

export interface ProxyConfig {
  listen_addr: string;
  default_pool: string;
  admin: {
    enabled: boolean;
    addr: string;
  };
  timeouts: TimeoutsConfig;
  limits: LimitsConfig;
  tls: TlsConfig;
  rate_limit: RateLimitConfig;
  circuit_breaker: CircuitBreakerConfig;
  retry: RetryConfig;
  passive_health_check: {
    enabled: boolean;
    unhealthy_threshold: number;
  };
  slow_start: {
    enabled: boolean;
    window_secs: number;
  };
  shutdown: {
    drain_timeout_secs: number;
  };
  access_log: {
    enabled: boolean;
    format: string;
  };
  pools: Record<string, PoolConfig>;
  routes: RouteRuleConfig[];
}

export interface ParsedMetrics {
  totalRequests: number;
  requestsPerSec: number;
  rate2xx: number;
  rate4xx: number;
  rate5xx: number;
  errorRate5xxPct: number;
  p50LatencyMs: number;
  p90LatencyMs: number;
  p99LatencyMs: number;
  totalActiveConnections: number;
  rateLimitedTotal: number;
  loadShedTotal: number;
  retriesTotal: number;
  trafficTimeline: Array<{
    timestamp: string;
    rate2xx: number;
    rate4xx: number;
    rate5xx: number;
  }>;
  latencyTimeline: Array<{
    timestamp: string;
    p50: number;
    p90: number;
    p99: number;
  }>;
}

export type EventSeverity = "info" | "warning" | "critical";

export interface LogEvent {
  id: string;
  timestamp: string;
  timestampMs: number;
  severity: EventSeverity;
  pool?: string;
  backend?: string;
  type:
    | "circuit_open"
    | "circuit_half_open"
    | "circuit_close"
    | "backend_unhealthy"
    | "backend_healthy"
    | "backend_drained"
    | "backend_enabled"
    | "rate_limit_spike"
    | "load_shedding";
  message: string;
}

export interface TrafficLabConfig {
  path: string;
  method: "GET" | "POST" | "PUT" | "DELETE";
  requestsPerSecond: number;
  durationSecs: number;
}

export interface TrafficLabResult {
  running: boolean;
  totalSent: number;
  success2xx: number;
  failed5xx: number;
  rateLimited429: number;
  retried: number;
  p50Ms: number;
  p99Ms: number;
  startedAt?: number;
  finishedAt?: number;
}

