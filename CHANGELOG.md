# Changelog

All notable changes to the L7 Load Balancer (`l7lb`) project will be documented in this file.

The format is based on [Keep a Changelog](https://keepachangelog.com/en/1.0.0/), and this project adheres to [Semantic Versioning](https://semver.org/spec/v2.0.0.html).

---

## [0.5.0] - Phase 5: Proof, Chaos Testing, Benchmarks & Polish

### Added
- **Integration & Chaos Test Suite (`tests/phase5_proof_and_chaos_tests.rs`)**:
  - Algorithm distribution tests verifying deterministic balancing across Weighted Round Robin and Power of Two Choices.
  - WebSocket / HTTP Upgrade full-duplex tunnel verification with bidirectional data transmission.
  - Large streaming body tests ensuring flat user-space memory with zero payload buffering.
  - Abrupt backend termination chaos test verifying immediate TCP failure handling and safe failover.
  - Backend timeout chaos test asserting proper 504 Gateway Timeout emission upon upstream delay.
  - Intermittent 5xx error injection chaos test verifying idempotent method retries and subsequent circuit breaker tripping.
  - Graceful connection draining verification during proxy shutdown.
- **Continuous Fuzzing Suite (`fuzz/`)**:
  - `fuzz_config_parser`: Fuzzes arbitrary inputs to the TOML configuration parser and validator.
  - `fuzz_header_sanitization`: Fuzzes HTTP request and response header sanitization routines against malformed header permutations.
- **Performance Benchmarking Suite**:
  - `scripts/bench.sh`: Automated load generation and latency measurement script supporting `oha`, `wrk`, and `curl`.
  - `docs/benchmarks.md`: Benchmark documentation and comparison table evaluating Direct Backend vs L7LB vs NGINX 1.25.
- **Containerization & Demo**:
  - Multi-stage `Dockerfile` optimizing build cache and producing a minimal Debian Slim runtime image.
  - Complete `docker-compose.yml` demo orchestrating `l7lb`, 3 mock backends, Prometheus metric scraping, and Grafana dashboard visualization.
- **GitHub Actions CI/CD Pipeline (`.github/workflows/ci.yml`)**:
  - Automated formatting checks (`cargo fmt`), strict linting (`cargo clippy -- -D warnings`), full test suite execution, and cross-platform release binary artifact generation (Linux, Windows, macOS).

---

## [0.4.0] - Phase 4: Observability & Metrics

### Added
- **Prometheus Metrics Registry (`src/metrics.rs`)**:
  - `http_requests_total`: Counter tracking HTTP request volume partitioned by status code and backend pool.
  - `http_request_duration_seconds`: High-resolution latency histogram partitioned by status code and backend pool.
  - `active_connections`: Gauge tracking active client connections across worker threads.
  - `backend_health`: Gauge reflecting real-time active and passive health state per backend address.
  - `backend_circuit_state`: Gauge monitoring circuit breaker state transitions (Closed, Open, HalfOpen).
  - `http_retries_total`: Counter recording upstream request retries by pool.
  - `rate_limited_requests_total`: Counter tracking client requests throttled by IP rate limiting.
  - `load_shed_requests_total`: Counter tracking requests shed due to global concurrency limits.
- **Prometheus `/metrics` Scraping Endpoint (`src/admin.rs`)**:
  - Integrated Prometheus metrics exporter served over the Admin HTTP API.
- **Structured JSON Access Logging (`src/proxy.rs`)**:
  - Structured JSON access logging emitting `request_id`, `client_ip`, `method`, `path`, `status`, `pool`, `backend`, `latency_ms`, and `retries`.
- **Distributed Tracing Header (`X-Request-Id`)**:
  - Automatic UUID generation for inbound requests lacking `X-Request-Id` and bidirectional propagation to upstream backends and downstream responses.
- **Grafana Dashboard Definition (`docs/grafana-dashboard.json`)**:
  - Production-grade dashboard visualizing request throughput, p50/p90/p99 latency distributions, error rates, circuit breaker statuses, and load-shedding counters.

---

## [0.3.0] - Phase 3: Security & Traffic Control

### Added
- **TLS Termination with Rustls (`src/tls.rs`)**:
  - TLS 1.2 and TLS 1.3 termination using modern safe Rust cryptography (`rustls`).
  - ALPN protocol negotiation supporting HTTP/2 (`h2`) and HTTP/1.1.
  - In-memory certificate and private key hot-reloading without dropping active client connections.
- **Upstream Backend TLS Support**:
  - Configurable TLS upstream forwarding with custom Server Name Indication (SNI) and optional insecure certificate bypassing for local dev/test environments.
- **Token Bucket IP Rate Limiting (`src/rate_limit.rs`)**:
  - Lock-free, atomic token bucket rate limiter tracking requests per second and burst capacity per client IP.
  - Automatic eviction and garbage collection of inactive client state to guarantee bounded memory usage.
  - HTTP 429 Too Many Requests response emission with dynamic `Retry-After` header calculation.
- **Global Concurrency Limiting & Load Shedding**:
  - High-concurrency load shedding using atomic connection counting returning HTTP 503 Service Unavailable when the maximum concurrency threshold is exceeded.

---

## [0.2.0] - Phase 2: Configuration, Routing & Algorithms

### Added
- **Dynamic Configuration Hot Reloading**:
  - Atomic config swapping on `SIGHUP` using `arc-swap` without dropping in-flight traffic.
  - Backend health state preservation across configuration swaps.
- **Multi-Pool Routing Engine (`src/router.rs`)**:
  - Exact Host header and longest-prefix path matching routing requests to dedicated backend pools.
  - Configurable path prefix stripping and URL prefix rewriting.
- **Advanced Load Balancing Algorithms (`src/balancer/`)**:
  - Power of Two Choices (P2C): Picks two random healthy backends and assigns the request to the backend with fewer active connections.
  - Consistent Hashing: Hash ring with 160 virtual nodes per backend supporting sticky routing by client IP, arbitrary HTTP request header, or HTTP cookie.
  - Least Response Time: Selects backends based on an exponentially weighted moving average (EWMA) of past response latency.
- **Admin Management API (`src/admin.rs`)**:
  - Dedicated administrative HTTP server listening on an isolated port.
  - Endpoints: `GET /admin/backends`, `GET /admin/config`, `POST /admin/backends/{addr}/drain`, and `POST /admin/backends/{addr}/enable`.

---

## [0.1.0] - Phase 1: Reliability & Proxy Hardening

### Added
- **Hop-by-Hop Header Sanitization**:
  - Stripping RFC 7230 hop-by-hop headers in both upstream and downstream directions.
  - Injection of standard proxy forwarding headers: `X-Forwarded-For`, `X-Forwarded-Proto`, and `X-Forwarded-Host`.
- **Configurable Timeouts & Frame Limits**:
  - Fine-grained controls for connection timeout, upstream response-header timeout, client header read timeout, and idle keep-alive duration.
  - Request header size limit (431 Request Header Fields Too Large) and payload body size limit (413 Payload Too Large).
- **Passive Health Checking**:
  - Tracking consecutive runtime failures (TCP connection errors, timeouts, HTTP 502/503/504) per backend to immediately isolate dead instances without waiting for active health probe intervals.
- **Three-State Atomic Circuit Breaker (`src/circuit_breaker.rs`)**:
  - Lock-free state machine transitions: `Closed` -> `Open` -> `HalfOpen` -> `Closed`.
  - Configurable consecutive failure threshold, cooldown backoff period, and trial request success thresholds.
- **Resilient Upstream Retries with Exponential Backoff & Jitter**:
  - Strictly limited to idempotent HTTP methods (`GET`, `HEAD`, `OPTIONS`, `PUT`, `DELETE`).
  - Retry budget tokens preventing cascading retry storms under upstream distress.
  - Zero buffering requirement: Non-idempotent streaming bodies are forwarded once and never buffered.
- **Slow-Start Traffic Ramp-Up**:
  - Linear traffic ramping for recovered backends over a configurable ramp window to prevent thundering-herd overload.
- **Graceful Shutdown & Connection Draining (`src/server.rs`)**:
  - Signal-aware shutdown (`SIGINT` / `SIGTERM`) halting inbound accept loops and draining in-flight requests until a configurable deadline.

