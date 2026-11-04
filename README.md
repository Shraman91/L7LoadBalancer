# l7lb: Production-Grade Layer 7 HTTP Reverse Proxy and Load Balancer

[![CI / CD](https://github.com/Shraman91/L7LoadBalancer/actions/workflows/ci.yml/badge.svg)](https://github.com/Shraman91/L7LoadBalancer/actions/workflows/ci.yml)
[![Rust](https://img.shields.io/badge/rust-1.80%2B-blue.svg)](https://www.rust-lang.org)
[![License: MIT](https://img.shields.io/badge/License-MIT-green.svg)](LICENSE)

A high-performance, fault-tolerant Layer 7 (HTTP/1.1 and HTTP/2) reverse proxy and load balancer written in Rust directly on top of `tokio` and `hyper` (no web framework). Built for extreme reliability, low latency, and zero-downtime operations, `l7lb` enforces lock-free atomic operations on the hot path, end-to-end streaming bodies with zero heap buffering, multi-pool content-based routing, adaptive load balancing algorithms, per-client IP rate limiting, circuit breakers with retries and jitter, and full Prometheus observability.

---

## Architectural Tenets

- **Lock-Free Hot Path**: Routing and backend selection execute without acquiring `Mutex` or `RwLock`. Shared state is coordinated via atomic operations (`AtomicUsize`, `AtomicBool`), atomic compare-and-swap (CAS), and `arc-swap` pointer swaps.
- **End-to-End Streaming**: Request and response payloads stream continuously between client and upstream backends. Payloads are never buffered into memory, preserving a flat memory profile even under multi-gigabyte transfers.
- **Resilience Against Cascading Failures**: Independent per-backend atomic circuit breakers, passive failure tracking, moving-window retry budgets with jitter, and slow-start traffic ramping prevent thundering herds and retry storms.
- **Zero-Downtime Dynamic Configuration**: SIGHUP signals and administrative actions trigger atomic pointer swaps (`ArcSwap`) that update routes, pools, and thresholds without dropping in-flight connections or losing backend health history.
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
                        |  - WebSocket / Upgrade Passthrough    |
                        +---------------------------------------+
                              |             |             |
                              v             v             v
                        +-----------+ +-----------+ +-----------+
                        | Backend 1 | | Backend 2 | | Backend 3 |
                        |  :9001    | |  :9002    | |  :9003    |
                        +-----------+ +-----------+ +-----------+

                       ADMIN & OBSERVABILITY CONTROL PLANE (:8081)
                        +---------------------------------------+
                        | GET  /metrics (Prometheus Exporter)   |
                        | GET  /admin/backends (Health & State) |
                        | GET  /admin/config (Active Config)    |
                        | POST /admin/backends/{addr}/drain     |
                        | POST /admin/backends/{addr}/enable    |
                        +---------------------------------------+
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
8. **WebSocket / Upgrade**: For 101 Switching Protocols responses, the connection transitions to a raw full-duplex tunnel using `tokio::io::copy_bidirectional`.
9. **Observability & Logging**: The response body streams back to the client. Prometheus counters, latency histograms, and structured JSON access logs are emitted asynchronously.

---

## Load Balancing Algorithms

Every pool defines its own load balancing strategy implementing the `LoadBalancer` trait:

| Algorithm Key | Selection Strategy | Primary Use Case |
| :--- | :--- | :--- |
| `round_robin` | Atomic round-robin cycling through all healthy backends. | Uniform backend clusters with uniform request processing costs. |
| `weighted_round_robin` | Smooth interleaved weighted round-robin (Nginx-style) with dynamic slow-start weight ramping. | Clusters with heterogeneous hardware specifications. |
| `least_connections` | Lock-free scan selecting the healthy backend with the fewest active connections. | Workloads with variable request execution times (e.g., long-lived queries). |
| `power_of_two_choices` | Randomly samples two distinct healthy nodes and selects the one with fewer active connections. | Large-scale distributed deployments to avoid herd oscillations. |
| `consistent_hash` | Ketama-style virtual node ring (160 vnodes per node) hashing on client IP, HTTP header, or cookie. | Stateful services, cache tiers, and sticky user sessions with ring failover. |
| `least_response_time` | Exponentially Weighted Moving Average (EWMA, alpha = 0.2) tracking backend latency, tie-breaking on active connections. | Dynamic cloud workloads with fluctuating upstream network or CPU performance. |

---

## Reliability and Fault Tolerance

### Atomic Circuit Breaker
Every backend maintains an atomic circuit breaker operating across three states:
- **Closed**: Requests pass normally. Consecutive upstream failures increment an atomic counter. When the failure threshold is reached, the state transitions to `Open`.
- **Open**: Requests fast-fail immediately without attempting upstream connections, protecting backends from overload. After a configured cooldown window, the state transitions to `HalfOpen`.
- **HalfOpen**: A bounded number of trial requests are admitted. If trial requests succeed, the breaker resets to `Closed`. If any trial request fails, it transitions immediately back to `Open`.

### Idempotent Retries with Token Budget and Jitter
- **Safety Guarantee**: Only idempotent HTTP methods (`GET`, `HEAD`, `OPTIONS`, `PUT`, `DELETE`) with completed bodies are eligible for retry. Non-idempotent methods (`POST`, `PATCH`) and active streaming bodies are never retried to prevent double-execution.
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
Exposed on the administrative control plane port (default `127.0.0.1:8081`):

| Metric Name | Type | Labels | Description |
| :--- | :--- | :--- | :--- |
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

---

## Performance Benchmarks

Comparative benchmark run using `oha` (50 concurrent connections, HTTP/1.1 keep-alive, 256-byte payload, 12-core x86_64 system):

| Target Architecture | Throughput (req/sec) | Latency p50 | Latency p90 | Latency p99 | Latency p99.9 | Peak Memory (RSS) |
| :--- | :--- | :--- | :--- | :--- | :--- |
| **Direct Backend (Baseline)** | 82,450 req/s | 0.52 ms | 0.88 ms | 1.28 ms | 2.10 ms | 14.2 MB |
| **L7LB (Rust / Tokio / Hyper)** | 71,820 req/s | 0.68 ms | 1.15 ms | 1.84 ms | 3.45 ms | 21.6 MB |
| **NGINX 1.25 (Alpine Default)** | 64,310 req/s | 0.76 ms | 1.42 ms | 2.21 ms | 4.12 ms | 38.5 MB |

For detailed analysis, latency histograms, and methodology, see [`docs/benchmarks.md`](docs/benchmarks.md).

---

## One-Command Demo (Docker Compose)

Launch a full production-like environment with `l7lb`, three mock upstream backends, Prometheus, and Grafana:

```bash
docker compose up --build
```

### Endpoints in Demo Environment

- **Load Balancer Proxy**: `http://localhost:8080/`
- **Admin Dashboard**: `http://localhost:3001/` (Next.js App Router, control plane)
- **Admin API & Prometheus Exporter**: `http://localhost:8081/admin/backends` and `http://localhost:8081/metrics`
- **Prometheus UI**: `http://localhost:9090/`
- **Grafana Dashboard**: `http://localhost:3000/` (credentials: `admin` / `admin`, auto-provisioned with `docs/grafana-dashboard.json`)

---

## Admin & Observability Dashboard

The repository includes a production-grade administrative dashboard located in [`dashboard/`](dashboard/), styled after modern B2B SaaS control planes (Linear, Vercel, Stripe).

### Features & Capabilities

- **Overview (`/`)**: Key metrics (req/s, 5xx error rate %, p50/p99 latency, active connections, healthy nodes) with sparklines, stacked throughput chart (2xx/4xx/5xx), latency percentiles, and protection counters (rate limits, load shedding, retries).
- **Backend Fleet (`/backends`)**: Searchable, filterable, and sortable table with status badges, circuit breaker indicators, active connections, and EWMA latency. Supports operator drain and enable actions with confirmation modals, optimistic updates, and slide-over telemetry drawers.
- **Topology Visualizer (`/topology`)**: Interactive React Flow diagram rendering `Listener -> Routes -> Pools -> Backends` with live status dots and dashed degradation links.
- **Routes & Config (`/config`)**: Visual breakdown of active routing tables, pools, timeouts, and limits with syntax-highlighted TOML/JSON views and one-click copy.
- **Audit & Incident Log (`/events`)**: Real-time event log tracking circuit state transitions, health checks, rate limit spikes, and administrative drain actions.
- **Traffic Lab (`/lab`)**: Built-in HTTP load generator and operator chaos walkthrough demonstrating circuit breaker opening, retry storms mitigation, and slow-start recovery.
- **Settings (`/settings`)**: Control plane connection settings, polling interval controls, and a standalone mock-mode toggle (`NEXT_PUBLIC_USE_MOCK=true`).

### Running the Dashboard Locally

```bash
cd dashboard
npm install
npm run dev
```

Navigate to `http://localhost:3001`. To run in standalone mock mode without running the backend proxy:

```bash
NEXT_PUBLIC_USE_MOCK=true npm run dev
```

---

## Configuration Reference

A complete multi-pool configuration is defined below:

```toml
listen_addr = "0.0.0.0:8080"
default_pool = "web"

[admin]
enabled = true
addr = "0.0.0.0:8081"

[timeouts]
connect_secs = 3
upstream_response_header_secs = 5
client_header_read_secs = 5
idle_keepalive_secs = 60

[limits]
max_request_header_size_bytes = 16384
max_body_size_bytes = 10485760 # 10 MB (0 for unlimited)
max_concurrent_connections = 1000

[tls]
enabled = false
cert_path = "certs/cert.pem"
key_path = "certs/key.pem"
alpn = ["h2", "http/1.1"]

[rate_limit]
enabled = true
requests_per_second = 100.0
burst = 150
max_entries = 10000
cleanup_interval_secs = 30

[circuit_breaker]
failure_threshold = 3
cooldown_secs = 10
half_open_max_trials = 2
half_open_successes_to_close = 2

[retry]
enabled = true
max_attempts = 3
initial_backoff_ms = 100
max_backoff_ms = 1000
budget_ratio = 0.20
min_retry_tokens = 10

[passive_health_check]
enabled = true
unhealthy_threshold = 3

[slow_start]
enabled = true
window_secs = 15

[shutdown]
drain_timeout_secs = 10

[access_log]
enabled = true
format = "json"

[pools.web]
algorithm = "round_robin"

[pools.web.health_check]
path = "/health"
interval_secs = 3
timeout_secs = 2
unhealthy_threshold = 2
healthy_threshold = 2

[[pools.web.backends]]
addr = "127.0.0.1:9001"
weight = 3

[[pools.web.backends]]
addr = "127.0.0.1:9002"
weight = 2

[[pools.web.backends]]
addr = "127.0.0.1:9003"
weight = 1

[[routes]]
path_prefix = "/"
pool = "web"
```

---

## Behaviour & Limitations

- **All Backends Down**: When no healthy backends exist in the selected pool, `l7lb` returns a clean `503 Service Unavailable` with body `503 Service Unavailable: No healthy backends\n`.
- **Connect Failures vs Upstream Timeouts**:
  - Upstream connection failures emit `502 Bad Gateway`.
  - Upstream response header timeouts emit `504 Gateway Timeout`.
  - Client header read timeouts emit `408 Request Timeout`.
  - Excessive request header byte lengths emit `431 Request Header Fields Too Large`.
  - Excessive request body byte lengths emit `413 Payload Too Large`.
- **Idempotent Retries**: Only `GET`, `HEAD`, `OPTIONS`, `PUT`, and `DELETE` requests with complete bodies are retried upon connect failure or upstream `502`/`503`/`504`. Streaming bodies (`is_end_stream() == false`) are never buffered or retried to preserve deterministic upstream semantics.
- **WebSocket / Upgrade**: Requests bearing `Upgrade: websocket` are forwarded with `101 Switching Protocols` support. Upon upgrade confirmation, client and upstream connections are bridged using full-duplex TCP streaming (`tokio::io::copy_bidirectional`).
- **Hop-by-Hop Header Sanitization**: In compliance with RFC 7230, `Connection`, `Keep-Alive`, `Proxy-Authenticate`, `Proxy-Authorization`, `TE`, `Trailer`, `Transfer-Encoding`, and `Upgrade` (unless an upgrade is negotiated) are stripped in both inbound and outbound directions.
- **Forwarded Headers**: Inbound requests have `X-Forwarded-For` appended with the client IP, `X-Forwarded-Proto` set to `https` or `http`, and `X-Forwarded-Host` set to the incoming `Host` header.

---

## Continuous Fuzzing

The repository includes LibFuzzer targets under `fuzz/`:

```bash
# Fuzz TOML configuration parser and validator
cargo fuzz run fuzz_config_parser

# Fuzz HTTP header sanitization routines
cargo fuzz run fuzz_header_sanitization
```

---

## Running the Automated Test Suite

Run the full end-to-end test suite across all 5 phases:

```bash
cargo test
```
