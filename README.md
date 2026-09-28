# l7lb — A Layer 7 (HTTP) Load Balancer in Rust

A lightweight HTTP reverse proxy and load balancer built directly on [`tokio`](https://tokio.rs) and [`hyper`](https://hyper.rs) — no web framework. It distributes incoming requests across a pool of backend servers using pluggable balancing algorithms, and automatically stops routing to backends that fail their health checks.

## Table of Contents

- [What is a Layer 7 load balancer?](#what-is-a-layer-7-load-balancer)
- [Features](#features)
- [Architecture](#architecture)
- [Project structure](#project-structure)
- [Getting started](#getting-started)
- [Configuration](#configuration)
- [Balancing algorithms](#balancing-algorithms)
- [Health checks](#health-checks)
- [Design decisions](#design-decisions)
- [Roadmap](#roadmap)

## What is a Layer 7 load balancer?

A Layer 4 (TCP) load balancer forwards raw bytes and knows nothing about what's inside them. A **Layer 7** load balancer operates at the application layer: it terminates the client's HTTP connection, understands the request, picks a backend, and forwards the request on the client's behalf. That's what lets it make smarter routing decisions and hide the backend pool behind a single address.

```
                    ┌──────────► Backend :9001
Client ──► l7lb ────┼──────────► Backend :9002
          :8080     └──────────► Backend :9003
```

## Features

- **Reverse proxy** — accepts HTTP/1.1 and HTTP/2 connections (via `hyper-util`'s auto builder) and forwards them to the backend pool.
- **Streaming bodies** — request and response bodies are streamed through rather than buffered in memory, so large uploads and downloads don't inflate memory usage or add latency.
- **Three balancing algorithms** behind a common `LoadBalancer` trait:
  - `round_robin`
  - `weighted_round_robin` (smooth weighted round robin, the same approach nginx uses)
  - `least_connections`
- **Active health checks** — a background task per backend probes a configurable path on a fixed interval, with separate healthy/unhealthy thresholds to prevent flapping.
- **Automatic failover and recovery** — unhealthy backends are removed from rotation and re-added once they pass enough consecutive probes.
- **Config-driven** — backends, algorithm, and health-check parameters live in a TOML file; change the topology without recompiling.
- **Lock-free hot path** — per-backend health and connection counts are atomics, so backend selection doesn't contend on a mutex.
- **Built-in mock backend** — a tiny test server is included so you can try everything locally.

## Architecture

```
            ┌──────────────────────────────────────────────┐
            │                    l7lb                      │
            │                                              │
 request ──►│  server ──► proxy ──► balancer.select() ─────┼──► backend
            │  (accept)   (handler)   (rr / wrr / lc)      │
            │                              ▲               │
            │                              │ healthy?      │
            │  health checker (1 task per backend) ────────┼──► GET /health
            └──────────────────────────────────────────────┘
```

1. **Server** accepts TCP connections and serves them with hyper.
2. **Proxy handler** takes each request and asks the configured balancer for a backend.
3. **Balancer** picks from the currently *healthy* backends according to its algorithm.
4. The request is forwarded to the chosen backend and the response is streamed back to the client.
5. Meanwhile, **health-check tasks** continuously probe every backend and update its shared health state.

## Project structure

```
L7LoadBalancer/
├── Cargo.toml
├── config.toml                  # sample config: backends, algorithm, health checks
├── README.md
└── src/
    ├── main.rs                  # CLI parsing, wiring, startup
    ├── config.rs                # TOML config structs + loading
    ├── backend.rs               # runtime backend state (health, connection count)
    ├── state.rs                 # shared AppState (backends, balancer, HTTP client)
    ├── proxy.rs                 # per-request proxy handler
    ├── server.rs                # TCP accept loop / hyper server wiring
    ├── health.rs                # background health-check tasks
    ├── balancer/
    │   ├── mod.rs               # LoadBalancer trait
    │   ├── round_robin.rs
    │   ├── weighted_round_robin.rs
    │   └── least_connections.rs
    └── bin/
        └── mock_backend.rs      # tiny test backend for local demos
```

## Getting started

### Prerequisites

- [Rust](https://www.rust-lang.org/tools/install) (stable toolchain, installed via `rustup`)

### 1. Clone and build

```bash
git clone https://github.com/Shraman91/L7LoadBalancer.git
cd L7LoadBalancer
cargo build --release
```

### 2. Start some backends

Run three mock backends, each in its own terminal:

```bash
cargo run --bin mock_backend -- 9001
cargo run --bin mock_backend -- 9002
cargo run --bin mock_backend -- 9003
```

### 3. Start the load balancer

It reads `config.toml` from the current directory by default:

```bash
cargo run --bin l7lb
```

To use a different config file:

```bash
cargo run --bin l7lb -- --config path/to/other.toml
```

### 4. Send traffic

```bash
for i in {1..6}; do curl -s http://127.0.0.1:8080/; done
```

You should see the responses spread across the backends.

### 5. Try failover

1. Stop one of the mock backends (`Ctrl+C`).
2. After `unhealthy_threshold` failed probes, `l7lb` logs the backend as unhealthy and stops routing to it.
3. Re-run the curl loop — traffic now goes only to the remaining backends.
4. Restart the backend. After `healthy_threshold` consecutive successful probes, it rejoins the rotation.

## Configuration

`config.toml`:

```toml
listen_addr = "0.0.0.0:8080"
algorithm   = "round_robin"

[health_check]
path                = "/health"
interval_secs       = 5
timeout_secs        = 2
unhealthy_threshold = 3
healthy_threshold   = 2

[[backends]]
addr   = "127.0.0.1:9001"
weight = 1

[[backends]]
addr   = "127.0.0.1:9002"
weight = 1

[[backends]]
addr   = "127.0.0.1:9003"
weight = 2
```

| Key | Description |
| --- | --- |
| `listen_addr` | Address and port the load balancer listens on. |
| `algorithm` | `round_robin`, `weighted_round_robin`, or `least_connections`. |
| `health_check.path` | HTTP path probed on every backend. |
| `health_check.interval_secs` | Seconds between probes. |
| `health_check.timeout_secs` | Seconds before a probe is counted as failed. |
| `health_check.unhealthy_threshold` | Consecutive failures before a backend is marked unhealthy. |
| `health_check.healthy_threshold` | Consecutive successes before an unhealthy backend is marked healthy again. |
| `backends[].addr` | Backend address (`host:port`). |
| `backends[].weight` | Relative share of traffic. Used by `weighted_round_robin`. |

## Balancing algorithms

| Algorithm | How it picks a backend | Best for |
| --- | --- | --- |
| `round_robin` | Cycles through healthy backends in order. | Homogeneous backends with similar request costs. |
| `weighted_round_robin` | Smooth WRR: each tick every backend's running weight increases by its configured weight, the highest is chosen, then the total weight is subtracted from it. | Backends with different capacities. |
| `least_connections` | Chooses the healthy backend with the fewest active connections. | Requests with widely varying durations. |

> **Note:** `weight` only has an effect with `weighted_round_robin`. To see it in action with the sample config (backend `:9003` has weight 2), set `algorithm = "weighted_round_robin"` and re-run the curl loop — `:9003` should receive about half the traffic.

## Health checks

Each backend gets its own background task that requests `health_check.path` every `interval_secs`. A probe fails if the backend doesn't respond successfully within `timeout_secs`.

State changes are **debounced** with thresholds instead of flipping on a single result:

- A healthy backend becomes **unhealthy** after `unhealthy_threshold` consecutive failures.
- An unhealthy backend becomes **healthy** after `healthy_threshold` consecutive successes.

With the sample config, a backend is removed after 3 failed probes (~15 s) and restored after 2 successful ones (~10 s).

## Design decisions

- **`hyper` directly instead of a web framework.** The hot path of a load balancer is "accept → pick backend → forward." There's no routing, templating, or middleware to justify a framework, and working with hyper's primitives keeps the proxy small and explicit.
- **Streaming instead of buffering.** Bodies are passed through as streams, so memory stays flat regardless of payload size and forwarding starts before the full body arrives.
- **Threshold-based health checks.** Debouncing avoids flapping on a single slow or dropped probe, matching how production load balancers such as nginx, HAProxy, and AWS ALB behave.
- **Smooth weighted round robin.** The naive approach (repeat each backend `weight` times in a list) sends bursts of consecutive requests to heavy backends. The smooth variant interleaves them evenly.
- **Atomics on the hot path.** `select()` runs on every request, so backend state uses `AtomicBool`, `AtomicUsize`, and `AtomicI64` rather than `Mutex`/`RwLock`.
- **Trait-based algorithms.** Adding a new strategy means implementing the `LoadBalancer` trait in `src/balancer/` — nothing else in the proxy changes.

## Roadmap

- [ ] TLS termination (`rustls`)
- [ ] Sticky sessions (consistent hashing on client IP or cookie)
- [ ] Retries with backoff and circuit breaking on backend 5xx
- [ ] Passive health checks based on live request failures
- [ ] Prometheus `/metrics` endpoint (request counts, latency histograms, per-backend health)
- [ ] Graceful shutdown and connection draining on SIGTERM
- [ ] Config hot-reload without restart
