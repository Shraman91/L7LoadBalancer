# L7LB: Technical Architecture and System Manual

This document provides a comprehensive technical overview of the `l7lb` Layer 7 HTTP reverse proxy and load balancer codebase, its module architecture, concurrency model, data pathways, and runtime behavior.

---

## 1. Project Overview

`l7lb` is an industrial-grade Layer 7 (HTTP/1.1 and HTTP/2) reverse proxy and load balancer implemented in Rust. It is engineered directly on top of the asynchronous runtime `tokio` and low-level HTTP primitives from `hyper` 1.x without any high-level web frameworks (such as Axum or Actix).

### Core Design Constraints

1. **Lock-Free Hot Path**: The request-forwarding hot path avoids standard mutual exclusion locks (`std::sync::Mutex` or `parking_lot::RwLock`). Routing table lookups and backend selections leverage atomic variables (`AtomicUsize`, `AtomicBool`, `AtomicU64`), atomic compare-and-swap (CAS) loops, and pointer swaps via `arc-swap::ArcSwap`.
2. **Zero-Copy Streaming Bodies**: Request and response bodies stream continuously between client sockets and upstream connections. Full bodies are never buffered into heap memory, maintaining a flat memory profile under multi-gigabyte payload transfers and high concurrency.
3. **Cascading Failure Protection**: Per-backend atomic circuit breakers, moving-window retry budgets with jitter, passive failure detection, and slow-start traffic ramp-up protect upstream services from retry storms and thundering herds.
4. **Zero-Downtime Configuration Hot Reload**: Configuration changes trigger an atomic pointer swap on `SIGHUP` or via file-watching without interrupting in-flight requests or losing established backend health history.
5. **Defense in Depth**: Front-end TLS termination (TLS 1.2+ only) via `rustls`, 64-shard concurrent token-bucket IP rate limiting, and global concurrency caps with 503 load shedding.

---

## 2. Workspace Directory Structure

```
l7-load-balancer/
|-- .github/
|   `-- workflows/
|       `-- ci.yml                   # CI/CD: fmt, clippy, tests, cross-platform release builds
|-- certs/                          # Test and sample TLS certificates and private keys
|-- docker/
|   |-- config.toml                 # Multi-backend containerized proxy configuration
|   |-- prometheus.yml              # Prometheus scraper configuration
|   `-- grafana/
|       `-- provisioning/
|           |-- dashboards/         # Automated Grafana dashboard provider definition
|           `-- datasources/        # Automated Prometheus datasource provider definition
|-- docs/
|   |-- benchmarks.md               # Throughput, latency percentiles, and memory comparisons
|   `-- grafana-dashboard.json      # Production-grade Grafana dashboard definition
|-- fuzz/
|   |-- Cargo.toml                  # LibFuzzer manifest
|   `-- fuzz_targets/
|       |-- fuzz_config_parser.rs   # Fuzz target for TOML config deserialization and validation
|       `-- fuzz_header_sanitization.rs # Fuzz target for request/response header stripping
|-- scripts/
|   `-- bench.sh                    # Automated benchmarking script (oha / wrk / curl)
|-- src/
|   |-- bin/
|   |   `-- mock_backend.rs         # Standalone lightweight HTTP backend for tests and demo
|   |-- admin.rs                    # Administrative control plane and Prometheus exporter
|   |-- backend.rs                  # Upstream backend node representation with atomic telemetry
|   |-- balancer/                   # Load balancing algorithm implementations
|   |   |-- mod.rs                  # LoadBalancer trait definition
|   |   |-- round_robin.rs          # Atomic round-robin
|   |   |-- weighted_round_robin.rs # Smooth interleaved weighted round-robin with slow-start
|   |   |-- least_connections.rs    # Atomic least active connections selection
|   |   |-- power_of_two_choices.rs # P2C randomized minimum-connection selection
|   |   |-- consistent_hash.rs      # Virtual node ring consistent hashing
|   |   `-- least_response_time.rs  # Latency EWMA load balancing
|   |-- circuit_breaker.rs          # Lock-free three-state circuit breaker state machine
|   |-- config.rs                   # TOML configuration schema, defaults, and validation
|   |-- error.rs                    # Custom error types (thiserror)
|   |-- health.rs                   # Active background HTTP health probe supervisor
|   |-- lib.rs                      # Library root exposing core modules
|   |-- main.rs                     # CLI entrypoint, signal handling, and runtime supervisor
|   |-- metrics.rs                  # Prometheus metrics registry and instruments
|   |-- proxy.rs                    # Core request dispatcher, header sanitizer, and retrier
|   |-- rate_limit.rs               # Sharded concurrent token bucket rate limiter
|   |-- retry.rs                    # Idempotent method filters, token budget, and backoff jitter
|   |-- router.rs                   # Content-based Host and longest-prefix path router
|   |-- server.rs                   # TCP listener loop, TLS handshake, and graceful drain
|   |-- state.rs                    # AppState and AppStateHolder coordinating runtime components
|   |-- tls.rs                      # Rustls server and client TLS configuration loaders
|-- tests/
|   |-- phase1_reliability_tests.rs # Headers, timeouts, circuit breaker, retries, slow-start
|   |-- phase2_routing_algo_tests.rs# Multi-pool routing, new algorithms, hot reload, admin API
|   |-- phase3_security_tests.rs    # TLS termination, upstream TLS, rate limiting, load shedding
|   |-- phase4_observability_tests.rs # Metrics scraping, structured access logs, trace propagation
|   `-- phase5_proof_and_chaos_tests.rs # Chaos testing, WebSocket upgrade, large streaming bodies
|-- Cargo.toml                      # Project package manifest and dependencies
|-- CHANGELOG.md                    # Changelog detailing features across all phases
|-- Dockerfile                      # Multi-stage release container build
|-- docker-compose.yml              # One-command demo (l7lb, 3 backends, Prometheus, Grafana)
`-- README.md                       # Public documentation and manual
```

---

## 3. High-Level Architecture and Data Flow

```
                                  INCOMING CLIENT CONNECTION
                                              |
                                              v
                              +-------------------------------+
                              |    TCP Listener (server.rs)   |
                              +-------------------------------+
                                              |
                                              v
                              +-------------------------------+
                              | TLS Termination (tls.rs)      |
                              |   Negotiates TLS 1.2/1.3      |
                              |   ALPN: HTTP/2 or HTTP/1.1    |
                              +-------------------------------+
                                              |
                                              v
                              +-------------------------------+
                              |  Global Concurrency Check     |
                              |  Active < max_connections?    |
                              |    No  -> HTTP 503 Overload   |
                              |    Yes -> Proceed             |
                              +-------------------------------+
                                              |
                                              v
                              +-------------------------------+
                              |  Rate Limiter (rate_limit.rs) |
                              |  Client IP Token Bucket Check |
                              |    Exceeded -> HTTP 429       |
                              +-------------------------------+
                                              |
                                              v
                              +-------------------------------+
                              |    L7 Router (router.rs)      |
                              |  Match Host & Longest Path    |
                              |  Determine Target Pool        |
                              +-------------------------------+
                                              |
                                              v
                              +-------------------------------+
                              | Load Balancer (balancer/)     |
                              |  RR / WRR / LC / P2C / CH / LRT
                              |  Select Non-Tripped Backend   |
                              +-------------------------------+
                                              |
                                              v
                              +-------------------------------+
                              | Circuit Breaker Check (cb.rs) |
                              |  Closed -> Forward            |
                              |  Open   -> Fast-fail / Next   |
                              |  HalfOpen -> Limited Probe    |
                              +-------------------------------+
                                              |
                                              v
                              +-------------------------------+
                              | Forwarding Client (proxy.rs)  |
                              |  - Strip Hop-by-Hop Headers   |
                              |  - Inject X-Forwarded-*       |
                              |  - Inject/Propagate Request ID|
                              |  - Optional Upstream TLS      |
                              |  - Stream Request Body        |
                              +-------------------------------+
                                              |
                       +----------------------+----------------------+
                       |                                             |
           [Success / Non-Retryable]                         [502/503/504 / Error]
                       |                                             |
                       v                                             v
        +-----------------------------+               +-----------------------------+
        | If 101 Switching Protocols  |               | Retry Check:                |
        | -> copy_bidirectional tunnel|               | Idempotent method?          |
        | Else                        |               | Body already closed?        |
        | -> Stream Response Body     |               | Budget tokens available?    |
        +-----------------------------+               |   Yes -> Backoff + Next Node|
                       |                              |   No  -> Emit Error to User |
                       v                              +-----------------------------+
        +-----------------------------+
        | Telemetry (metrics & logs)  |
        | - Increment Prometheus QPS  |
        | - Record Latency Histogram  |
        | - Emit JSON Access Log      |
        +-----------------------------+
```

---

## 4. Detailed Module Breakdown

### 4.1. Core Server & Lifecycle (`src/server.rs` & `src/main.rs`)

- **Signal Trapping**: `main.rs` listens for termination signals (`SIGINT`, `SIGTERM`) and configuration reload signals (`SIGHUP`).
- **Connection Processing**: `server::run` binds to the configured socket address. Incoming connections are handled by spawning lightweight asynchronous Tokio tasks.
- **Protocol Negotiation**: Each incoming stream is checked against the optional `TlsAcceptor`. If TLS is configured, `rustls` negotiates either `h2` or `http/1.1` via ALPN.
- **HTTP Auto-Engine**: `hyper_util::server::conn::auto::Builder` handles both HTTP/1.1 and HTTP/2 wire formats automatically with configured header read timeouts. Connections are wrapped with `serve_connection_with_upgrades` to support protocol upgrades (such as WebSockets).
- **Graceful Draining**: On cancellation, the accept loop immediately terminates. The server tracks in-flight requests through an `AtomicUsize`. It enters a drain loop, waiting up to `drain_timeout_secs` for active operations to complete before exiting cleanly.

### 4.2. Request Dispatcher & Proxying Engine (`src/proxy.rs`)

- **Hop-by-Hop Header Sanitization**: Conforming to RFC 7230 and RFC 9110, headers specific to a single transport hop are stripped in both inbound and outbound directions:
  - `Connection`, `Keep-Alive`, `Proxy-Authenticate`, `Proxy-Authorization`, `TE`, `Trailer`, `Transfer-Encoding`, `Upgrade` (retained only when negotiating an upgrade).
- **Forwarding Header Injection**:
  - `X-Forwarded-For`: Appends the immediate client IP address.
  - `X-Forwarded-Proto`: Set to `https` if the downstream connection was terminated over TLS; otherwise `http`.
  - `X-Forwarded-Host`: Set to the incoming `Host` header.
  - `X-Request-Id`: Preserved if transmitted by the client; otherwise populated with a unique 32-character hexadecimal UUID.
- **Limits Verification**:
  - Emits `431 Request Header Fields Too Large` if total header octets exceed `limits.max_request_header_size_bytes`.
  - Emits `413 Payload Too Large` if `Content-Length` exceeds `limits.max_body_size_bytes`.
- **WebSocket Passthrough**: When upstream responds with `101 Switching Protocols`, hyper's upgrade mechanisms yield raw underlying IO handles (`TokioIo`). The proxy immediately connects client and server streams via `tokio::io::copy_bidirectional`.

### 4.3. State Management & Dynamic Swapping (`src/state.rs`)

- **`AppState`**: Holds reference-counted instances of the router, backend pools, HTTP forwarding client, retry budget, rate limiter, Prometheus metrics registry, and atomic telemetry counters.
- **`AppStateHolder`**: Houses `inner: ArcSwap<AppState>`.
- **Hot Reload Guarantee**: When a new configuration is applied:
  1. The new TOML configuration is validated.
  2. For every backend address already present in the existing state, its operational state (active connection count, consecutive failure counters, health flags, and circuit breaker status) is preserved.
  3. New pools and routing tables are constructed.
  4. `inner.store(new_state)` atomically swaps the root pointer. Active in-flight requests continue executing against their existing `Arc<AppState>`, guaranteeing zero dropped connections.

### 4.4. Content-Based Routing Engine (`src/router.rs`)

- **Route Rules**: Each route defines an optional `host`, a required `path_prefix`, a destination `pool`, and optional `strip_prefix` or `rewrite_prefix` directives.
- **Evaluation Priority**:
  1. Rules matching the exact incoming `Host` header take precedence over wildcard host rules.
  2. Longest path prefix matching wins.
- **Path Manipulation**: If `strip_prefix` is set, the matched prefix is removed from the request URI before upstream dispatch. If `rewrite_prefix` is provided, the matched prefix is replaced with the replacement string.

### 4.5. Upstream Backend Node (`src/backend.rs`)

Each upstream target is represented by a `Backend` struct:
- `addr`: Target `SocketAddr`.
- `healthy`: `AtomicBool` reflecting the combined outcome of active and passive health assessments.
- `draining`: `AtomicBool` indicating administrative drain status. When true, the backend is excluded from selection for new requests while allowing existing requests to finish.
- `active_connections`: `AtomicUsize` tracking currently executing requests on that node.
- `ewma_latency_micros`: `AtomicU64` recording an Exponentially Weighted Moving Average of latency ($\alpha = 0.2$).
- `healthy_since_millis`: `AtomicU64` timestamp used to compute dynamic slow-start traffic weight ramping.
- `circuit_breaker`: Dedicated per-backend instance of `CircuitBreaker`.

### 4.6. Load Balancing Algorithms (`src/balancer/`)

All algorithms implement the `LoadBalancer` trait:

```rust
pub trait LoadBalancer: Send + Sync {
    fn select(&self, backends: &[Arc<Backend>], excluded: &[SocketAddr]) -> Option<Arc<Backend>>;
    fn select_with_key(&self, backends: &[Arc<Backend>], excluded: &[SocketAddr], key: Option<&str>) -> Option<Arc<Backend>>;
}
```

1. **Round Robin (`round_robin.rs`)**:
   - Cycles through healthy backends using an atomic counter (`AtomicUsize`).
2. **Weighted Round Robin with Slow-Start (`weighted_round_robin.rs`)**:
   - Implements smooth interleaved weighted selection (Nginx-style).
   - If slow-start is enabled, a backend recovering from an unhealthy state ramps its effective weight linearly from 1 to its configured target over `slow_start.window_secs`.
3. **Least Connections (`least_connections.rs`)**:
   - Scans healthy backends and selects the instance with the lowest `active_connections` value.
4. **Power of Two Choices / P2C (`power_of_two_choices.rs`)**:
   - Samples two distinct candidate nodes at random and selects the candidate with fewer active connections, preventing herd oscillations.
5. **Consistent Hashing (`consistent_hash.rs`)**:
   - Builds a virtual node ring using 160 virtual points per backend hashed via standard 64-bit hashing.
   - Extracts keys from client IP addresses, designated HTTP headers, or cookies.
   - Supports seamless failover: if the primary hashed backend is unhealthy, the ring advances clockwise to the next healthy node.
6. **Least Response Time (`least_response_time.rs`)**:
   - Evaluates the EWMA latency (`ewma_latency_micros`). Selects the lowest-latency healthy backend, tie-breaking on active connection count.

### 4.7. Circuit Breaker (`src/circuit_breaker.rs`)

The circuit breaker prevents repeated calls to failing backends using an atomic state machine:
- **`Closed` (State 0)**: Normal operation. Every failure increments `consecutive_failures`. If failures reach `failure_threshold`, it transitions to `Open`.
- **`Open` (State 1)**: Rejects traffic immediately. Stores `opened_at_millis`. After `cooldown_duration` elapses, the next request transitions the breaker to `HalfOpen`.
- **`HalfOpen` (State 2)**: Permits a limited number of trial requests (`half_open_max_trials`). If `half_open_successes_to_close` consecutive successes occur, the breaker transitions back to `Closed`. If any trial request fails, it reverts immediately to `Open`.

### 4.8. Idempotent Retries & Token Budget (`src/retry.rs`)

- **Idempotency Gate**: Upstream retries are strictly restricted to idempotent HTTP methods: `GET`, `HEAD`, `OPTIONS`, `PUT`, and `DELETE`.
- **Streaming Guard**: Requests with non-empty streaming bodies where `is_end_stream()` is false are never retried, preventing duplicate or corrupt upstream payload processing.
- **Exponential Backoff with Jitter**:
  $$\text{Backoff} = \min\left(\text{max\_backoff}, \text{initial\_backoff} \times 2^{\text{attempt}-1}\right) \times \text{random\_factor}$$
- **Retry Budget Token Bucket**: A moving-window budget limits retries to a configured fraction (e.g. 20%) of total traffic, preventing cascading retry storms when backends experience systemic degradation.

### 4.9. Concurrency & Rate Limiting (`src/rate_limit.rs`)

- **Sharded Token Bucket**: The rate limiter maintains 64 independent shards (`parking_lot::Mutex<HashMap<IpAddr, Bucket>>`) indexed by hashing client IPs. This eliminates global lock contention across worker threads.
- **Token Bucket Algorithm**: Replenishes tokens at `requests_per_second` up to `burst`. If tokens are depleted, requests are rejected with `429 Too Many Requests` along with a numeric `Retry-After: <secs>` header.
- **Bounded Memory Eviction**: Inactive entries older than 60 seconds are periodically purged to ensure predictable memory usage.
- **Global Concurrency Load Shedding**: Evaluates `in_flight_requests` via atomic CAS against `limits.max_concurrent_connections`. Excess requests are shed immediately with `503 Service Unavailable`.

### 4.10. Observability & Control Plane (`src/metrics.rs` & `src/admin.rs`)

- **Admin Server**: Runs on an isolated administrative interface (default `127.0.0.1:8081`).
- **Prometheus Scraper (`GET /metrics`)**:
  - `http_requests_total`: Labeled by status code and pool.
  - `http_request_duration_seconds`: Histogram measuring response latency across standard Prometheus buckets.
  - `active_connections`: Gauge measuring concurrent requests per backend.
  - `backend_health`: Real-time health state (1.0 = healthy, 0.0 = unhealthy).
  - `backend_circuit_state`: Circuit status (0 = closed, 1 = half-open, 2 = open).
  - `http_retries_total`: Retried request counter.
  - `rate_limited_requests_total`: Count of 429 rejections.
  - `load_shed_requests_total`: Count of 503 load-shedding rejections.
- **Administrative Endpoints**:
  - `GET /admin/backends`: Inspects active connections, health flags, and circuit states across all backends.
  - `GET /admin/config`: Retrieves currently active configuration.
  - `POST /admin/backends/{addr}/drain`: Stops new traffic allocation to a backend while allowing in-flight requests to complete.
  - `POST /admin/backends/{addr}/enable`: Restores a drained backend to service.
- **Structured JSON Logging**: Every request outputs a structured JSON log entry containing `request_id`, `client_ip`, `method`, `path`, `status`, `pool`, `backend`, `latency_ms`, and `retries`.

---

## 5. Performance and Resilience Characteristics

### 5.1. Lock-Free Synchronization Model

| Path / Mechanism | Concurrency Primitive | Contention Scope |
| :--- | :--- | :--- |
| Config & Routing Table | `arc_swap::ArcSwap` | Zero contention; hot path performs atomic pointer loads |
| Global Concurrency Guard | `AtomicUsize::compare_exchange_weak` | Global lock-free CAS loop |
| Rate Limiting | 64-shard concurrent `Mutex` | Partitioned by `hash(client_ip) % 64` |
| Backend Selection (RR) | `AtomicUsize::fetch_add` | Lock-free atomic increment |
| Backend Selection (WRR) | Atomic weight calculation | Thread-safe atomic access |
| Circuit Breaker Transitions | Atomic CAS (`compare_exchange`) | Per-backend atomic state machine |
| In-Flight Request Tracking | `AtomicUsize::fetch_add` / `fetch_sub` | Per-backend atomic counters |

### 5.2. Comparative Performance (from `docs/benchmarks.md`)

Benchmarked using `oha` with 50 concurrent connections, keep-alive enabled, and minimal payloads on a 12-core x86_64 system:

| Target Architecture | Throughput | p50 Latency | p99 Latency | p99.9 Latency | Resident Memory |
| :--- | :--- | :--- | :--- | :--- | :--- |
| **Direct Backend** | 82,450 req/s | 0.52 ms | 1.28 ms | 2.10 ms | 14.2 MB |
| **L7LB (Rust / Hyper)** | 71,820 req/s | 0.68 ms | 1.84 ms | 3.45 ms | 21.6 MB |
| **NGINX 1.25** | 64,310 req/s | 0.76 ms | 2.21 ms | 4.12 ms | 38.5 MB |

---

## 6. How to Run, Test, and Verify

### 6.1. Build and Test

```bash
# Verify code formatting
cargo fmt --all -- --check

# Run strict linter
cargo clippy --all-targets -- -D warnings

# Execute all unit, integration, and chaos test suites
cargo test
```

### 6.2. Local Multi-Backend Execution

```bash
# Terminal 1, 2, 3: Start mock backends
cargo run --bin mock_backend -- 9001
cargo run --bin mock_backend -- 9002
cargo run --bin mock_backend -- 9003

# Terminal 4: Launch proxy
cargo run --bin l7lb -- --config config.toml
```

### 6.3. Docker Compose One-Command Demo

```bash
docker compose up --build
```

- Proxy service: `http://localhost:8080/`
- Administrative API & metrics: `http://localhost:8081/admin/backends` and `http://localhost:8081/metrics`
- Prometheus interface: `http://localhost:9090/`
- Grafana dashboard: `http://localhost:3000/` (credentials: `admin` / `admin`)
