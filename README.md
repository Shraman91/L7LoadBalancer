# l7lb — Production-Grade Layer 7 (HTTP) Load Balancer in Rust

A high-performance, fault-tolerant Layer 7 reverse proxy and load balancer built directly on `tokio` and `hyper` (no web framework). Designed with lock-free atomics on the hot path, streaming request/response bodies, configurable active/passive health checks, circuit breakers, idempotent retries with jitter & token budgeting, slow-start ramping, graceful shutdown, multi-pool content-based routing, advanced balancing algorithms, atomic hot reload, and an administrative control plane.

## Table of Contents

- [What is a Layer 7 load balancer?](#what-is-a-layer-7-load-balancer)
- [Key Features](#key-features)
- [Architecture](#architecture)
- [Balancing Algorithms](#balancing-algorithms)
- [Configuration Reference](#configuration-reference)
- [Getting Started](#getting-started)
- [Admin API](#admin-api)

---

## What is a Layer 7 load balancer?

A Layer 4 (TCP) load balancer forwards raw bytes and knows nothing about what's inside them. A **Layer 7** load balancer operates at the application layer: it terminates the client's HTTP connection, understands the request (host, path, headers, cookies), picks a backend based on content routing rules and health metrics, and forwards the request on the client's behalf.

```
                    ┌──────────► Backend :9001 (Web Pool)
Client ──► l7lb ────┼──────────► Backend :9011 (API Pool)
          :8080     └──────────► Backend :9021 (Stateful Pool)
```

---

## Key Features

### 1. Proxy Correctness & Protocol Support
- **End-to-End Streaming**: Request and response bodies stream without buffering in memory.
- **Hop-by-Hop Header Sanitation**: RFC 7230 / 9110 compliant removal of hop-by-hop headers (`Connection`, `Keep-Alive`, `Proxy-*`, `TE`, `Trailer`, `Transfer-Encoding`, `Upgrade`, and custom headers listed in `Connection`).
- **Standard Forwarding Headers**: Injects or appends `X-Forwarded-For`, `X-Forwarded-Proto`, and `X-Forwarded-Host`.
- **WebSocket & HTTP Upgrade Passthrough**: Full 101 Switching Protocols upgrade negotiation and bidirectional byte streaming via `tokio::io::copy_bidirectional`.
- **Standard Error Responses**: Returns clean 503 (No healthy backends), 502 (Upstream connection failure), 504 (Upstream timeout), 431 (Headers too large), and 413 (Payload too large).

### 2. Timeouts & Payload Limits
- Granular timeouts for connect, upstream response headers, client header reading, idle keep-alive, and overall requests.
- Header byte size caps (`max_request_header_size_bytes`) and optional body size limits (`max_body_size_bytes`).

### 3. Fault Tolerance & Health Checks
- **Active Health Checks**: Periodic probes to backend health paths with configurable intervals, timeouts, and success/failure thresholds to prevent flapping.
- **Passive Health Checks**: Live request monitoring that marks backends unhealthy immediately upon consecutive failures (connect errors, timeouts, 5xx) without waiting for background probes.
- **Lock-Free Circuit Breaker**: State machine (`Closed` $\to$ `Open` $\to$ `HalfOpen` $\to$ `Closed`) implemented using atomic operations per backend.
- **Idempotent Retries with Token Budget & Jitter**: Retries only idempotent HTTP methods (`GET`, `HEAD`, `OPTIONS`, `PUT`, `DELETE`, `TRACE`) across alternate healthy backends using exponential backoff with full jitter and a moving-window retry token budget (~20% limit) to prevent retry storms.
- **Slow-Start Ramping**: Gradually ramps effective traffic weights for backends recovering from unhealthy states.

### 4. Dynamic Reconfiguration & Zero-Downtime Hot Reload
- **Atomic State Swapping**: Powered by `arc-swap`, configuration updates swap the routing table, pool definitions, and thresholds atomically without dropping in-flight connections.
- **Health State Preservation**: Reloading retains the existing `Arc<Backend>` instances and their health, circuit breaker state, active connections, and latency metrics across reloads (updating weights dynamically).
- **Triggers**: Supports `SIGHUP` signal (Unix) and automatic config file change watching via `notify`.

### 5. L7 Content-Based Routing
- **Host Matching**: Route rules can match specific domains (e.g. `api.example.com` vs `web.example.com`).
- **Longest-Prefix Path Matching**: Routes requests to designated backend pools based on path prefixes with longest-match precedence (e.g. `/api/v2/*` wins over `/api/*`).
- **Path Transformation**: Supports prefix stripping (`strip_prefix = true`) and prefix rewriting (`rewrite_prefix = "/internal/..."`), preserving query strings.
- **Default Fallback**: Transparently routes unmatched traffic to `default_pool`.

---

## Architecture

```
            ┌──────────────────────────────────────────────┐
            │                    l7lb                      │
            │                                              │
 request ──►│  server ──► router ──► pool.balancer.select()┼──► backend
            │  (accept)   (host/path) (rr/wrr/lc/p2c/ch/lrt)│
            │                              ▲               │
            │                              │ healthy?      │
            │  health checker (1 task per backend) ────────┼──► GET /health
            └──────────────────────────────────────────────┘
```

---

## Balancing Algorithms

| Algorithm | How it picks a backend | Best for |
| --- | --- | --- |
| `round_robin` | Cycles through healthy backends in order. | Homogeneous backends with similar request costs. |
| `weighted_round_robin` | Smooth WRR (interleaves heavy nodes evenly) with slow-start weight ramping. | Backends with different hardware capacities. |
| `least_connections` | Chooses the healthy backend with the fewest active connections. | Requests with widely varying durations. |
| `power_of_two_choices` | Samples 2 distinct random healthy backends and selects the one with fewer active connections. | High-throughput distributed clusters avoiding herd behavior. |
| `consistent_hash` | Virtual node hash ring (128 vnodes per backend). Sticky routing keys via client IP, header, or cookie, with automatic ring failover. | Stateful services and caching layers. |
| `least_response_time` | Exponentially weighted moving average (EWMA, $\alpha = 0.2$) latency tracking per backend, with active connection tie-breaker. | Dynamic workloads with varying backend response latencies. |

---

## Configuration Reference

### Multi-Pool & Routing Configuration (`config.multi-pool.toml`)

```toml
listen_addr = "0.0.0.0:8080"
default_pool = "web"

[admin]
enabled = true
addr = "127.0.0.1:9090"

# Routing rules (longest prefix match)
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

## Getting Started

### Prerequisites

- [Rust](https://www.rust-lang.org/tools/install) (stable toolchain, installed via `rustup`)

### 1. Build

```bash
cargo build --release
```

### 2. Start mock backends

In separate terminals:
```bash
cargo run --bin mock_backend -- 9001
cargo run --bin mock_backend -- 9002
cargo run --bin mock_backend -- 9003
```

### 3. Start the load balancer

```bash
cargo run --bin l7lb
# or with a custom config:
# cargo run --bin l7lb -- --config config.multi-pool.toml
```

### 4. Send traffic

```bash
curl -s http://127.0.0.1:8080/
```

---

## Admin API

When enabled in config (`[admin] enabled = true`), the admin server exposes:

```bash
# View all backends, health status, active connections, circuit state, and EWMA latency
curl http://127.0.0.1:9090/admin/backends

# View active configuration
curl http://127.0.0.1:9090/admin/config

# Gracefully drain a backend
curl -X POST http://127.0.0.1:9090/admin/backends/127.0.0.1:9001/drain

# Re-enable a backend
curl -X POST http://127.0.0.1:9090/admin/backends/127.0.0.1:9001/enable
```

---

## Running Tests

```bash
cargo test
```
