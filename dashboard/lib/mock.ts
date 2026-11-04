import { BackendStatus, LogEvent, ParsedMetrics, ProxyConfig } from "./types";

export const MOCK_CONFIG: ProxyConfig = {
  listen_addr: "0.0.0.0:8080",
  default_pool: "web",
  admin: {
    enabled: true,
    addr: "0.0.0.0:8081",
  },
  timeouts: {
    connect_secs: 3,
    upstream_response_header_secs: 5,
    client_header_read_secs: 5,
    idle_keepalive_secs: 60,
  },
  limits: {
    max_request_header_size_bytes: 16384,
    max_body_size_bytes: 10485760,
    max_concurrent_connections: 1000,
  },
  tls: {
    enabled: false,
    cert_path: "certs/cert.pem",
    key_path: "certs/key.pem",
    alpn: ["h2", "http/1.1"],
  },
  rate_limit: {
    enabled: true,
    requests_per_second: 100.0,
    burst: 150,
    max_entries: 10000,
    cleanup_interval_secs: 30,
  },
  circuit_breaker: {
    failure_threshold: 3,
    cooldown_secs: 10,
    half_open_max_trials: 2,
    half_open_successes_to_close: 2,
  },
  retry: {
    enabled: true,
    max_attempts: 3,
    initial_backoff_ms: 100,
    max_backoff_ms: 1000,
    budget_ratio: 0.2,
    min_retry_tokens: 10,
  },
  passive_health_check: {
    enabled: true,
    unhealthy_threshold: 3,
  },
  slow_start: {
    enabled: true,
    window_secs: 15,
  },
  shutdown: {
    drain_timeout_secs: 10,
  },
  access_log: {
    enabled: true,
    format: "json",
  },
  pools: {
    web: {
      algorithm: "round_robin",
      health_check: {
        path: "/health",
        interval_secs: 3,
        timeout_secs: 2,
        unhealthy_threshold: 2,
        healthy_threshold: 2,
      },
      backends: [
        { addr: "127.0.0.1:9001", weight: 3, tls: false, tls_sni: null, tls_insecure: false },
        { addr: "127.0.0.1:9002", weight: 2, tls: false, tls_sni: null, tls_insecure: false },
        { addr: "127.0.0.1:9003", weight: 1, tls: false, tls_sni: null, tls_insecure: false },
      ],
    },
    api: {
      algorithm: "power_of_two_choices",
      health_check: {
        path: "/health",
        interval_secs: 2,
        timeout_secs: 1,
        unhealthy_threshold: 3,
        healthy_threshold: 2,
      },
      backends: [
        { addr: "127.0.0.1:9011", weight: 1, tls: false, tls_sni: null, tls_insecure: false },
        { addr: "127.0.0.1:9012", weight: 1, tls: false, tls_sni: null, tls_insecure: false },
        { addr: "127.0.0.1:9013", weight: 1, tls: false, tls_sni: null, tls_insecure: false },
      ],
    },
    stateful: {
      algorithm: "consistent_hash",
      hash_key: "cookie:SESSIONID",
      health_check: {
        path: "/health",
        interval_secs: 5,
        timeout_secs: 2,
        unhealthy_threshold: 2,
        healthy_threshold: 2,
      },
      backends: [
        { addr: "127.0.0.1:9021", weight: 1, tls: false, tls_sni: null, tls_insecure: false },
      ],
    },
  },
  routes: [
    { host: null, path_prefix: "/api/", pool: "api", strip_prefix: null, rewrite_prefix: null },
    { host: null, path_prefix: "/user/", pool: "stateful", strip_prefix: null, rewrite_prefix: null },
    { host: null, path_prefix: "/", pool: "web", strip_prefix: null, rewrite_prefix: null },
  ],
};

let mockBackendsState: BackendStatus[] = [
  {
    addr: "127.0.0.1:9001",
    healthy: true,
    draining: false,
    active_connections: 16,
    circuit_state: "closed",
    weight: 3,
    ewma_latency_ms: 1.25,
    tls: false,
    pool: "web",
  },
  {
    addr: "127.0.0.1:9002",
    healthy: true,
    draining: false,
    active_connections: 12,
    circuit_state: "closed",
    weight: 2,
    ewma_latency_ms: 1.84,
    tls: false,
    pool: "web",
  },
  {
    addr: "127.0.0.1:9003",
    healthy: true,
    draining: true,
    active_connections: 1,
    circuit_state: "closed",
    weight: 1,
    ewma_latency_ms: 2.15,
    tls: false,
    pool: "web",
  },
  {
    addr: "127.0.0.1:9011",
    healthy: true,
    draining: false,
    active_connections: 28,
    circuit_state: "closed",
    weight: 1,
    ewma_latency_ms: 0.92,
    tls: false,
    pool: "api",
  },
  {
    addr: "127.0.0.1:9012",
    healthy: true,
    draining: false,
    active_connections: 4,
    circuit_state: "half_open",
    weight: 1,
    ewma_latency_ms: 38.6,
    tls: false,
    pool: "api",
    slow_start_progress: 45,
  },
  {
    addr: "127.0.0.1:9013",
    healthy: false,
    draining: false,
    active_connections: 0,
    circuit_state: "open",
    weight: 1,
    ewma_latency_ms: 0.0,
    tls: false,
    pool: "api",
  },
  {
    addr: "127.0.0.1:9021",
    healthy: true,
    draining: false,
    active_connections: 32,
    circuit_state: "closed",
    weight: 1,
    ewma_latency_ms: 2.12,
    tls: false,
    pool: "stateful",
  },
];

export function getMockBackends(): BackendStatus[] {
  // Add subtle realistic fluctuations
  return mockBackendsState.map((b) => {
    if (!b.healthy || b.draining) return { ...b };
    const jitter = Math.floor(Math.random() * 5) - 2;
    const conns = Math.max(1, b.active_connections + jitter);
    const latencyJitter = (Math.random() * 0.2 - 0.1);
    const latency = Math.max(0.2, Math.round((b.ewma_latency_ms + latencyJitter) * 100) / 100);
    return {
      ...b,
      active_connections: conns,
      ewma_latency_ms: latency,
    };
  });
}

export function setMockBackendDrain(addr: string, draining: boolean): boolean {
  const target = mockBackendsState.find((b) => b.addr === addr);
  if (target) {
    target.draining = draining;
    return true;
  }
  return false;
}

export const MOCK_EVENTS: LogEvent[] = [
  {
    id: "evt-001",
    timestamp: "18:14:02",
    timestampMs: Date.now() - 420000,
    severity: "critical",
    pool: "api",
    backend: "127.0.0.1:9013",
    type: "circuit_open",
    message: "Circuit breaker tripped OPEN after 3 consecutive 503 errors",
  },
  {
    id: "evt-002",
    timestamp: "18:15:20",
    timestampMs: Date.now() - 340000,
    severity: "warning",
    pool: "api",
    backend: "127.0.0.1:9012",
    type: "circuit_half_open",
    message: "Cooldown elapsed: entering HALF-OPEN state (admitting trial probes)",
  },
  {
    id: "evt-003",
    timestamp: "18:17:45",
    timestampMs: Date.now() - 210000,
    severity: "info",
    pool: "web",
    backend: "127.0.0.1:9003",
    type: "backend_drained",
    message: "Operator drained backend: stopping new requests, draining 2 in-flight",
  },
  {
    id: "evt-004",
    timestamp: "18:19:10",
    timestampMs: Date.now() - 110000,
    severity: "warning",
    pool: "web",
    type: "rate_limit_spike",
    message: "Client 192.168.1.104 exceeded burst limit: 429 Too Many Requests emitted",
  },
  {
    id: "evt-005",
    timestamp: "18:21:05",
    timestampMs: Date.now() - 25000,
    severity: "info",
    pool: "api",
    backend: "127.0.0.1:9011",
    type: "backend_healthy",
    message: "Active health check probe 200 OK (threshold 2/2 met)",
  },
];

let baseTrafficTimeline = Array.from({ length: 20 }).map((_, i) => {
  const d = new Date(Date.now() - (20 - i) * 3000);
  const timeStr = d.toLocaleTimeString("en-US", { hour12: false, hour: "2-digit", minute: "2-digit", second: "2-digit" });
  return {
    timestamp: timeStr,
    rate2xx: Math.round(180 + Math.random() * 40),
    rate4xx: Math.round(4 + Math.random() * 3),
    rate5xx: Math.round(1 + Math.random() * 2),
  };
});

let baseLatencyTimeline = Array.from({ length: 20 }).map((_, i) => {
  const d = new Date(Date.now() - (20 - i) * 3000);
  const timeStr = d.toLocaleTimeString("en-US", { hour12: false, hour: "2-digit", minute: "2-digit", second: "2-digit" });
  return {
    timestamp: timeStr,
    p50: 0.65 + Math.random() * 0.1,
    p90: 1.25 + Math.random() * 0.3,
    p99: 2.45 + Math.random() * 0.6,
  };
});

export function getMockMetrics(): ParsedMetrics {
  const now = new Date();
  const timeStr = now.toLocaleTimeString("en-US", { hour12: false, hour: "2-digit", minute: "2-digit", second: "2-digit" });

  const current2xx = Math.round(210 + Math.random() * 35);
  const current4xx = Math.round(3 + Math.random() * 2);
  const current5xx = Math.round(Math.random() * 2);

  baseTrafficTimeline = [
    ...baseTrafficTimeline.slice(1),
    { timestamp: timeStr, rate2xx: current2xx, rate4xx: current4xx, rate5xx: current5xx },
  ];

  baseLatencyTimeline = [
    ...baseLatencyTimeline.slice(1),
    {
      timestamp: timeStr,
      p50: Math.round((0.68 + Math.random() * 0.08) * 100) / 100,
      p90: Math.round((1.18 + Math.random() * 0.15) * 100) / 100,
      p99: Math.round((1.95 + Math.random() * 0.3) * 100) / 100,
    },
  ];

  const totalReqs = current2xx + current4xx + current5xx;
  const errorPct = totalReqs > 0 ? (current5xx / totalReqs) * 100 : 0;

  return {
    totalRequests: 849201,
    requestsPerSec: totalReqs,
    rate2xx: current2xx,
    rate4xx: current4xx,
    rate5xx: current5xx,
    errorRate5xxPct: Math.round(errorPct * 10) / 10,
    p50LatencyMs: 0.72,
    p90LatencyMs: 1.24,
    p99LatencyMs: 2.10,
    totalActiveConnections: 93,
    rateLimitedTotal: 142,
    loadShedTotal: 0,
    retriesTotal: 48,
    trafficTimeline: baseTrafficTimeline,
    latencyTimeline: baseLatencyTimeline,
  };
}

