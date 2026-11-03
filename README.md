# l7lb: Production-Grade Layer 7 HTTP Reverse Proxy and Load Balancer

A high-performance, fault-tolerant Layer 7 (HTTP/1.1 and HTTP/2) reverse proxy and load balancer written in Rust directly on top of `tokio` and `hyper` (no web framework). Built for extreme reliability, low latency, and zero-downtime operations, `l7lb` enforces lock-free atomic operations on the hot path, end-to-end streaming bodies with zero heap buffering, multi-pool content-based routing, adaptive load balancing algorithms, per-client IP rate limiting, circuit breakers with retries and jitter, and full Prometheus observability.

---

## Architectural Tenets

- **Lock-Free Hot Path**: Routing and backend selection execute without acquiring `Mutex` or `RwLock`. Shared state is coordinated via atomic operations (`AtomicUsize`, `AtomicBool`), atomic compare-and-swap (CAS), and `arc-swap` pointer swaps.
- **End-to-End Streaming**: Request and response payloads stream continuously between client and upstream backends. Payloads are never buffered into memory, preserving a flat memory profile even under multi-gigabyte transfers.
- **Resilience Against Cascading Failures**: Independent per-backend atomic circuit breakers, passive failure tracking, moving-window retry budgets with jitter, and slow-start traffic ramping prevent thundering herds and retry storms.
- **Zero-Downtime Dynamic Configuration**: SIGHUP signals and automatic file-watcher notifications trigger atomic pointer swaps (`ArcSwap`) that update routes, pools, and thresholds without dropping in-flight connections or losing backend health history.
- **Defense in Depth**: Front-end TLS termination via `rustls` (restricted to TLS 1.2 and TLS 1.3 only), 64-shard concurrent token-bucket rate limiting, and global concurrency caps with graceful load shedding.

---

## System Architecture

```
                                  CLIENT TRAFFIC
                                        |
                                        v
                    +---------------------------------------+
                    |           l7lb Listener               |
                    |   (tokio::net::TcpListener :8080)     |
                    +---------------------------------------+
                                        |
                                        v
                    +---------------------------------------+
                    |       TLS Termination (rustls)        |
                    |    TLS 1.2/1.3, ALPN ("h2", "http/1.1")|
                    +---------------------------------------+
                                        |
                                        v
                    +---------------------------------------+
                    |    Global Concurrency Check (CAS)     |
                    |   In-Flight < max_concurrent_connections?
                    |     No  -> 503 Service Unavailable    |
                    |     Yes -> Acquire In-Flight Slot     |
                    +---------------------------------------+
                                        |
                                        v
                    +---------------------------------------+
                    |   Sharded Token-Bucket Rate Limiter   |
                    |   Per-Client IP (64 Concurrent Shards)|
                    |     Exceeded -> 429 Too Many Requests |
                    |                 + Retry-After header  |
                    +---------------------------------------+
                                        |
                                        v
                    +---------------------------------------+
                    |      Content-Based L7 Router          |
                    |   Host Matching -> Longest Prefix Path|
                    |   Optional Prefix Stripping/Rewriting |
                    +---------------------------------------+
                                        |
                                        v
                    +---------------------------------------+
                    |        Selected Backend Pool          |
                    |  Algorithm: RR / WRR / LC / P2C / CH / LRT
                    |  Filters: Healthy & Non-Draining Nodes|
                    +---------------------------------------+
                                        |
                                        v
                    +---------------------------------------+
                    |       Circuit Breaker Evaluation      |
                    |  Closed -> Normal Forward             |
                    |  Open   -> Fast-fail / Try Next Node  |
                    |  Half-Open -> Bounded Probe Trial     |
                    +---------------------------------------+
                                        |
                                        v
                    +---------------------------------------+
                    |        Upstream Dispatch & Retry      |
                    |  - Hop-by-hop headers stripped        |
                    |  - X-Forwarded-*, X-Request-Id injected
                    |  - Optional Upstream TLS / SNI        |
                    |  - Idempotent Retry on 502/503/504    |
                    +---------------------------------------+
                          |             |             |
                          v             v             v
                    +-----------+ +-----------+ +-----------+
                    | Backend 1 | | Backend 2 | | Backend 3 |
                    |  :9001    | |  :9002    | |  :9003    |
                    +-----------+ +-----------+ +-----------+
```

---

## Request Lifecycle

1. **Connection Ingestion**: An incoming TCP connection is accepted. If TLS is enabled, the handshake negotiates TLS 1.2 or 1.3 and settles the protocol via ALPN (`h2` or `http/1.1`).
2. **Global Concurrency Guard**: An atomic CAS verifies that active in-flight requests do not exceed `limits.max_concurrent_connections`. If full, a 503 Service Unavailable ("Server Overloaded") response is returned immediately.
3. **Client IP Rate Limiting**: The client IP is hashed into one of 64 independent token bucket shards. If available tokens are depleted, a 429 Too Many Requests response is returned with a calculated `Retry-After: <seconds>` header.
4. **Header Sanitation & Tracing**: Hop-by-hop headers (`Connection`, `Keep-Alive`, `Proxy-*`, `TE`, `Trailer`, `Transfer-Encoding`, `Upgrade`) are stripped. `X-Forwarded-For`, `X-Forwarded-Proto`, `X-Forwarded-Host`, and `X-Request-Id` (generated if absent) are injected.
5. **Route Resolution**: The router evaluates configured route rules, matching by exact Host header and longest path prefix. The request path is rewritten or stripped if configured, and routed to the target backend pool.
6. **Backend Selection**: The pool's configured balancing algorithm selects an optimal healthy backend node, excluding draining nodes and tripped circuit breakers.
7. **Streaming Dispatch & Retries**: The request body streams directly to the upstream server. If the upstream server fails with a connection error or a 502/503/504 status, idempotent methods (`GET`, `HEAD`, `OPTIONS`, `PUT`, `DELETE`) retry against alternate healthy backends using exponential backoff with full jitter, governed by a moving-window retry token budget.
8. **Observability & Response**: The response body streams back to the client. Prometheus counters, latency histograms, and structured JSON access logs (including request ID, status, latency, and retry count) are recorded.

---

## Load Balancing Algorithms

Every pool defines its own load balancing strategy implementing the `LoadBalancer` trait:

| Algorithm Key | Selection Strategy | Primary Use Case |
|---|---|---|
| `round_robin` | Atomic round-robin cycling through all healthy backends. | Uniform backend clusters with uniform request processing costs. |
| `weighted_round_robin` | Smooth interleaved weighted round-robin (Nginx-style) with dynamic slow-start weight ramping. | Clusters with heterogeneous hardware specifications. |
| `least_connections` | Lock-free scan selecting the healthy backend with the fewest active connections. | Workloads with variable request execution times (e.g., long-lived queries). |
| `power_of_two_choices` | Randomly samples two distinct healthy nodes and selects the one with fewer active connections. | Large-scale distributed deployments to avoid herd oscillations. |
| `consistent_hash` | Ketama-style virtual node ring (128 vnodes per node) hashing on client IP, HTTP header, or cookie. | Stateful services, cache tiers, and sticky user sessions with ring failover. |
| `least_response_time` | Exponentially Weighted Moving Average (EWMA, alpha = 0.2) tracking backend latency, tie-breaking on active connections. | Dynamic cloud workloads with fluctuating upstream network or CPU performance. |

---

## Reliability and Fault Tolerance

### Atomic Circuit Breaker
Every backend maintains an atomic circuit breaker operating across three states:
- **Closed**: Requests pass normally. Consecutive upstream failures increment an atomic counter. When the failure threshold is reached, the state transitions to `Open`.
- **Open**: Requests fast-fail immediately without attempting upstream connections, protecting backends from overload. After a configured cooldown window, the state transitions to `HalfOpen`.
- **HalfOpen**: A bounded number of trial requests are admitted. If trial requests succeed, the breaker resets to `Closed`. If any trial request fails, it transitions immediately back to `Open`.

### Idempotent Retries with Token Budget and Jitter
- **Safety Guarantee**: Only idempotent HTTP methods (`GET`, `HEAD`, `OPTIONS`, `PUT`, `DELETE`, `TRACE`) with completed bodies are eligible for retry. Non-idempotent methods (`POST`, `PATCH`) and active streaming bodies are never retried to prevent double-execution.
- **Exponential Backoff with Full Jitter**: Retries calculate backoff with decorrelated randomization between zero and $2^{\text{attempt}} \times \text{initial\_backoff}$.
- **Token Budget**: A moving-window budget caps total retries at ~20% of recent traffic, preventing retry storms during upstream outages.

### Active and Passive Health Checking
- **Active Probes**: Dedicated background tasks probe `/health` endpoints at regular intervals with success and failure thresholds to avoid flapping.
- **Passive Probes**: Live request errors (connect failures, timeouts, 502/503/504) trigger immediate health state updates on the hot path without waiting for the next active probe interval.

### Slow-Start Weight Ramping
When a backend recovers from an unhealthy state, its effective weight starts at 1 and gradually ramps up to its configured target over a configurable window (e.g. 30 seconds), preventing recovered instances from being swamped immediately.

---

## Security and Traffic Control

### Modern TLS Termination
- Powered by `rustls` 0.23 and `tokio-rustls`.
- Strictly enforces TLS 1.2 and TLS 1.3 (insecure versions SSLv3, TLS 1.0, and TLS 1.1 are rejected at the protocol layer).
- ALPN auto-negotiation supports both HTTP/2 (`h2`) and HTTP/1.1 (`http/1.1`).
- Certificates and private keys are reloaded dynamically on SIGHUP or config change without dropping active connections.

### Token Bucket Rate Limiting
- Partitioned across 64 concurrent shards to eliminate lock contention.
- Independent token bucket tracking per client IP (`IpAddr`).
- Rejections return HTTP 429 Too Many Requests with a numeric `Retry-After: <seconds>` header.
- Bounded memory footprint: automatically prunes idle buckets older than 60 seconds and enforces maximum capacity per shard.

### Concurrency Limits and Load Shedding
- Configurable `max_concurrent_connections` threshold.
- Evaluated via atomic compare-and-swap (CAS) before allocating connection buffers.
- Rejects excess traffic cleanly with HTTP 503 Service Unavailable ("Server Overloaded: global concurrency limit reached"), ensuring predictable degradation rather than memory exhaustion.

---

## Observability and Control Plane

### Prometheus Metrics (`GET /metrics`)
Exposed on the administrative control plane port (default `127.0.0.1:9090`):

| Metric Name | Type | Labels | Description |
|---|---|---|---|
| `http_requests_total` | Counter | `pool`, `status` | Total HTTP requests handled by status code and pool |
| `http_request_duration_seconds` | Histogram | `pool`, `status` | Request latency distribution across standard Prometheus buckets |
| `active_connections` | Gauge | `pool`, `backend` | Real-time active connection count per backend |
| `backend_health` | Gauge | `pool`, `backend` | Real-time backend health flag (1.0 = healthy, 0.0 = unhealthy) |
| `backend_circuit_state` | Gauge | `pool`, `backend` | Circuit breaker status (0.0 = closed, 1.0 = half-open, 2.0 = open) |
| `http_retries_total` | Counter | `pool` | Total count of request retries executed |
| `rate_limited_requests_total` | Counter | none | Total requests rejected by per-IP rate limiting (429) |
| `load_shed_requests_total` | Counter | none | Total requests shed due to global concurrency saturation (503) |

### Structured JSON Access Logging
Every request produces a structured JSON access log entry tagged with a unique request ID:

```json
{
  "timestamp": 1700000000000,
  "request_id": "a1b2c3d4e5f67890a1b2c3d4e5f67890",
  "client_ip": "192.168.1.50",
  "method": "GET",
  "path": "/api/v1/users?page=2",
  "status": 200,
  "pool": "api",
  "backend": "127.0.0.1:9011",
  "latency_ms": 2.34,
  "retries": 0
}
```

- If the client transmits `X-Request-Id`, it is preserved. If absent, a random 32-character hexadecimal request ID is generated.
- The request ID is injected upstream to backends and returned downstream to clients in response headers.

### Grafana Dashboard
A ready-to-import dashboard is provided in [`docs/grafana-dashboard.json`](docs/grafana-dashboard.json) providing:
- Overview row: QPS stat, Active Connections, 5xx Error Rate %, p99 Latency.
- Traffic & Latency: Throughput by pool and status code, Latency percentiles (p50, p90, p99).
- Fleet Health: Per-backend active connections and real-time health step graphs.
- Traffic Control: Retries per second, rate-limited request counts, and load-shedding activity.

---

## Admin API Reference

The administrative server runs on a dedicated, isolated port (default `127.0.0.1:9090`):

```bash
# 1. Scrape Prometheus metrics
curl http://127.0.0.1:9090/metrics

# 2. Inspect real-time fleet health, active connections, circuit state, and EWMA latency
curl http://127.0.0.1:9090/admin/backends

# 3. Inspect active configuration state
curl http://127.0.0.1:9090/admin/config

# 4. Gracefully drain a backend (stops receiving new traffic, in-flight completes)
curl -X POST http://127.0.0.1:9090/admin/backends/127.0.0.1:9001/drain

# 5. Re-enable a drained backend
curl -X POST http://127.0.0.1:9090/admin/backends/127.0.0.1:9001/enable
```

---

## Configuration Reference

A complete multi-pool configuration is defined in [`config.multi-pool.toml`](config.multi-pool.toml):

```toml
listen_addr = "0.0.0.0:8080"
default_pool = "web"

[admin]
enabled = true
addr = "127.0.0.1:9090"

[timeouts]
connect_secs = 3
idle_keepalive_secs = 60
request_timeout_secs = 10
upstream_response_header_secs = 5
client_header_read_secs = 5

[limits]
max_request_header_size_bytes = 65536
max_body_size_bytes = 10485760 # 10 MB (0 for unlimited)
max_concurrent_connections = 5000 # 0 for unlimited

[tls]
enabled = false
cert_path = "certs/cert.pem"
key_path = "certs/key.pem"
alpn = ["h2", "http/1.1"]

[rate_limit]
enabled = true
requests_per_second = 100.0
burst = 50
max_entries = 100000
cleanup_interval_secs = 60

[access_log]
enabled = true
format = "json" # "json" or "text"

[retry]
enabled = true
max_attempts = 3
initial_backoff_ms = 50
max_backoff_ms = 1000
retry_budget_percent = 20

[circuit_breaker]
enabled = true
consecutive_failures = 5
cooldown_secs = 15
half_open_max_trials = 3
half_open_successes_to_close = 2

[slow_start]
enabled = true
window_secs = 30

[health_check]
path = "/health"
interval_secs = 5
timeout_secs = 2
unhealthy_threshold = 3
healthy_threshold = 2

# Routing rules (evaluated in order, longest prefix match wins)
[[routes]]
host = "api.example.com"
path_prefix = "/api/v1/"
pool = "api"
strip_prefix = true

[[routes]]
path_prefix = "/user/"
pool = "stateful"
rewrite_prefix = "/internal/user/"

# Pools
[pools.web]
algorithm = "least_response_time"

[[pools.web.backends]]
addr = "127.0.0.1:9001"
weight = 1

[[pools.web.backends]]
addr = "127.0.0.1:9002"
weight = 2

[pools.api]
algorithm = "power_of_two_choices"

[[pools.api.backends]]
addr = "127.0.0.1:9011"
weight = 1

[[pools.api.backends]]
addr = "127.0.0.1:9012"
weight = 1

[pools.stateful]
algorithm = "consistent_hash"
hash_key = "cookie:SESSIONID" # or "client_ip", "header:X-User-Id"

[[pools.stateful.backends]]
addr = "127.0.0.1:9021"
weight = 1
```

---

## Behavior and Limitations

- **All Backends Down**: If all backends in a pool are unhealthy or open in circuit breaking, `l7lb` returns a clean HTTP `503 Service Unavailable` with body `503 Service Unavailable: No healthy backends\n`.
- **Streamed Request Safety**: Requests with streaming request bodies where `is_end_stream()` is false are never retried after initial transmission starts, preventing duplicate payload delivery to multiple backends.
- **WebSocket / HTTP Upgrade**: `l7lb` supports 101 Switching Protocols. Once upgraded, bytes stream bidirectionally through `tokio::io::copy_bidirectional` without HTTP framing inspection.
- **Hop-by-Hop Headers**: RFC 7230 / RFC 9110 compliant removal ensures hop-by-hop headers are never forwarded upstream or downstream.

---

## Getting Started

### 1. Build
```bash
cargo build --release
```

### 2. Start Mock Backends
Run mock backends in separate terminals:
```bash
cargo run --bin mock_backend -- 9001
cargo run --bin mock_backend -- 9002
cargo run --bin mock_backend -- 9003
```

### 3. Launch Load Balancer
```bash
cargo run --bin l7lb -- --config config.multi-pool.toml
```

### 4. Send Test Traffic
```bash
# Standard request
curl -i http://127.0.0.1:8080/

# Custom trace request
curl -i -H "X-Request-Id: my-trace-id-123" http://127.0.0.1:8080/

# Inspect metrics
curl -s http://127.0.0.1:9090/metrics | grep http_requests_total
```

---

## Running Test Suite

Run the full automated test suite covering all phases (reliability, routing algorithms, security, and observability):

```bash
cargo test
```
