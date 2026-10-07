# Implementation Plan: Upgrading `l7lb` to Production-Grade

This plan outlines the 5-phase evolution of `l7lb` from a lightweight demo into a high-performance, resilient, production-ready Layer 7 load balancer.

---

## Architectural Principles & Rules
- **Lock-free hot path**: Backend selection, circuit breaking, metrics, connection tracking, and health checks use atomics and `ArcSwap` (no `Mutex`/`RwLock` in proxy dispatch).
- **Zero-buffering / Streaming**: Request/response bodies stream end-to-end via Hyper's frame streaming. For retries on idempotent methods with empty/replayable bodies, buffered streams are used only when retry is enabled and method is idempotent.
- **Backward Compatibility**: `config.toml` maintains sensible defaults so legacy configs continue running without alteration.
- **Error Handling & Observability**: Strict error typing with `thiserror`, structured logs with `tracing`, and zero unhandled panics (`unwrap`/`expect`).

---

## Phase Breakdown

### Phase 1: Reliability
**Objective**: Hardened proxying, hop-by-hop header management, timeouts, passive health checks, atomic circuit breaking, idempotent retries with budget/backoff, slow start, and graceful cancellation/shutdown.

**Files to Modify / Create**:
- `Cargo.toml`: Add `thiserror`, `tokio-util`, `rand`, `http-body-util`, `hyper-util`.
- `src/error.rs` (New): Custom typed error definitions for proxying, upstream connectivity, timeouts, and circuit breaker rejections.
- `src/config.rs`: Add timeout configs (connect, header, idle, request), payload limits (header size, body size), passive health check parameters, circuit breaker settings, retry policy, slow-start window, and shutdown drain timeouts.
- `src/backend.rs`: Add atomic circuit breaker state (`Closed`, `Open`, `HalfOpen`), passive failure counters, slow start timestamp/ramp calculation, and drain state.
- `src/circuit_breaker.rs` (New): Lock-free circuit breaker state machine (`AtomicU8`/`AtomicU64` timestamp).
- `src/retry.rs` (New): Idempotent method checks, exponential backoff with full jitter, and moving-window retry token budget.
- `src/proxy.rs`: Hop-by-hop header stripping (Connection, Keep-Alive, Proxy-*, TE, Trailer, Transfer-Encoding, Upgrade), `X-Forwarded-*` header handling, WebSocket / HTTP upgrade tunneling, timeout wrappers, passive check hooks, and retry loop.
- `src/server.rs`: Graceful shutdown with `tokio_util::sync::CancellationToken`, in-flight request counter, and timeout draining.
- `src/main.rs`: Integrate signal handlers (SIGINT/SIGTERM) and wiring.
- `tests/phase1_reliability_tests.rs` (New): Unit & integration tests for circuit breakers, passive health, retries, slow-start, timeouts, and graceful shutdown.
- `README.md`: Document Phase 1 features, config options, and failure modes.

---

### Phase 2: Config, Routing & Advanced Balancing Algorithms
**Objective**: SIGHUP / file watcher hot reload, multi-pool host & path routing with longest-prefix matching, rewrite rules, advanced lock-free algorithms (Power of Two Choices, Consistent Hash Ring, Least Response Time EWMA), and dedicated Admin HTTP API.

**Files to Modify / Create**:
- `Cargo.toml`: Add `arc-swap`.
- `src/config.rs`: Multi-pool definitions, route rule definitions (Host, Path prefix, strip/rewrite), admin server configuration.
- `src/router.rs` (New): Longest-prefix and Host-matching router mapping requests to backend pools.
- `src/balancer/power_of_two.rs` (New): Power of two random choices algorithm.
- `src/balancer/consistent_hash.rs` (New): Consistent hash ring with virtual nodes for sticky routing (IP, header, cookie).
- `src/balancer/least_response_time.rs` (New): EWMA latency tracking with lock-free atomic floating math.
- `src/balancer/mod.rs`: Register new algorithms under `LoadBalancer` trait.
- `src/admin.rs` (New): Dedicated admin server (`/admin/backends`, `/admin/config`, `/admin/backends/{addr}/drain`, `/admin/backends/{addr}/enable`).
- `src/state.rs`: Wrap router & pool configuration in `ArcSwap` for zero-downtime hot reload.
- `src/server.rs` & `src/main.rs`: Wire SIGHUP signal listener and admin service.
- `tests/phase2_routing_algo_tests.rs` (New): Comprehensive tests for routing, hot-reload preservation of health state, algorithms, and admin API endpoints.
- `README.md`: Update docs for routing syntax, admin API, and new algorithms.

---

### Phase 3: Security & Traffic Control
**Objective**: Rustls TLS 1.2/1.3 termination with ALPN HTTP/2, upstream TLS support, atomic token bucket IP rate limiting, and global connection concurrency bounding with load-shedding (503).

**Files to Modify / Create**:
- `Cargo.toml`: Add `tokio-rustls`, `rustls-pemfile`, `rustls`.
- `src/tls.rs` (New): TLS listener acceptor, client TLS connector, and certificate reloader.
- `src/rate_limit.rs` (New): Per-IP token bucket with atomic rate limiting and background eviction of stale entries.
- `src/concurrency.rs` (New): Atomic concurrency guard and load-shedding gatekeeper.
- `src/config.rs`: TLS config, rate limit config, and max concurrency settings.
- `src/server.rs` & `src/proxy.rs`: Enforce TLS termination, rate limits (429 with `Retry-After`), and concurrency caps (503).
- `tests/phase3_security_tests.rs` (New): Rate limit tests, concurrency cap tests, TLS handshake tests.
- `README.md`: Document TLS setup, rate limiting, and load shedding.

---

### Phase 4: Observability
**Objective**: Prometheus metrics scraping endpoint on the admin port, structured JSON access logs with distributed `X-Request-Id` tracing, latency histograms, and Grafana dashboard assets.

**Files to Modify / Create**:
- `Cargo.toml`: Add `metrics`, `metrics-exporter-prometheus`.
- `src/metrics.rs` (New): Metrics registry, counters (requests, errors, retries, rate limits), gauges (active connections, circuit states, backend health), and latency histograms.
- `src/access_log.rs` (New): Structured JSON access logging with request ID generation and propagation.
- `src/admin.rs`: Expose `GET /metrics` on admin interface.
- `docs/grafana-dashboard.json` (New): Complete Grafana dashboard definition.
- `README.md`: Document metrics, logging format, and Grafana import guide.

---

### Phase 5: Proof, Benchmarking & Production Polish
**Objective**: End-to-end integration tests, chaos testing (intermittent failures, slow backends, abruptly closed connections), fuzz targets (`cargo-fuzz`), performance benchmarks (`oha`/`wrk`), multi-stage Docker build, docker-compose full stack demo, GitHub Actions CI, and final documentation.

**Files to Modify / Create**:
- `tests/chaos_tests.rs` (New): Chaos testing scenarios.
- `tests/streaming_tests.rs` (New): High-throughput large payload streaming tests.
- `fuzz/` (New): Fuzz targets for config parser and HTTP header sanitation.
- `scripts/bench.sh` (New): Automated benchmarking script.
- `docs/benchmarks.md` (New): Benchmarking results comparing direct vs `l7lb` vs nginx.
- `Dockerfile` & `docker-compose.yml` (New): Production deployment demo.
- `.github/workflows/ci.yml` (New): CI pipeline with `fmt`, `clippy`, unit/integration tests, and release artifacts.
- `README.md` & `CHANGELOG.md`: Final overhaul with architecture diagram, reference guides, and release notes.

---

## Assumptions
1. **Windows & Unix compatibility**: Signal handling will utilize `tokio::signal::ctrl_c` on Windows and `tokio::signal::unix::Signal` for SIGHUP/SIGTERM where supported on Unix, falling back gracefully.
2. **Streaming and Retries**: Non-idempotent methods (POST, PATCH) or idempotent requests with large streaming bodies that cannot be buffered safely without memory impact are not retried if the stream was already partially consumed. Empty or small rewindable payloads on idempotent methods (GET, HEAD, OPTIONS, PUT, DELETE) will be retried against alternate backends within the retry budget.
