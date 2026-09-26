# l7lb — a small Layer 7 (HTTP) load balancer in Rust

A reverse proxy / load balancer built on `tokio` + `hyper` directly (no
web framework), with pluggable balancing algorithms and active health
checks. Built as a portfolio project to demonstrate async Rust, trait-based
design, and basic distributed-systems concepts.

## Features

- **Reverse proxy** — accepts HTTP/1.1 (and HTTP/2 via `hyper-util`'s auto
  builder) connections and forwards them to a backend pool, streaming
  request/response bodies rather than buffering them in memory.
- **Pluggable algorithms** via a `LoadBalancer` trait:
  - `round_robin` — cycles through healthy backends in order
  - `weighted_round_robin` — smooth WRR (same algorithm nginx uses), so
    higher-weight backends get proportionally more traffic without bursting
  - `least_connections` — routes to the healthy backend with the fewest
    active connections
- **Active health checks** — a background task per backend polls a
  configurable path on an interval, with configurable healthy/unhealthy
  thresholds so a single flaky probe doesn't flip state.
- **Config-driven** — backends, algorithm, and health check parameters
  live in `config.toml`, no recompilation needed to change topology.

## Folder structure

```
l7-load-balancer/
├── Cargo.toml
├── config.toml              # sample config: backends, algorithm, health checks
├── README.md
└── src/
    ├── main.rs               # CLI parsing, wiring, startup
    ├── config.rs             # TOML config structs + loading
    ├── backend.rs            # runtime Backend state (health, conn count)
    ├── state.rs              # shared AppState (backends, balancer, client)
    ├── proxy.rs               # per-request proxy handler
    ├── server.rs              # TCP accept loop / hyper server wiring
    ├── health.rs               # background health-check tasks
    ├── balancer/
    │   ├── mod.rs              # LoadBalancer trait
    │   ├── round_robin.rs
    │   ├── weighted_round_robin.rs
    │   └── least_connections.rs
    └── bin/
        └── mock_backend.rs      # tiny test backend for local demos
```

## Running it locally

Start three mock backends in separate terminals:

```bash
cargo run --bin mock_backend -- 9001
cargo run --bin mock_backend -- 9002
cargo run --bin mock_backend -- 9003
```

Then start the load balancer (uses `config.toml` by default):

```bash
cargo run --bin l7lb
# or: cargo run --bin l7lb -- --config path/to/other.toml
```

Hit it and watch requests get distributed:

```bash
for i in {1..6}; do curl -s http://127.0.0.1:8080/; done
```

Kill one of the mock backends and watch the health checker mark it
unhealthy (log line) and stop routing to it; bring it back and it
recovers after `healthy_threshold` consecutive successful probes.

## Design notes / things worth mentioning in an interview

- **Why hyper directly instead of a framework**: a load balancer's hot
  path is "accept connection → pick backend → forward" — there's no
  routing/templating/middleware need that would justify a framework, and
  building on raw `hyper` demonstrates understanding of the underlying
  primitives (`Body`/`Incoming`, connection builders, services).
- **Streaming bodies**: request/response bodies are passed through as
  `hyper::body::Incoming` / boxed bodies rather than buffered into
  `Vec<u8>`, so the proxy doesn't blow up memory on large uploads/downloads
  and doesn't add latency waiting for a full body before forwarding.
- **Health check debouncing**: threshold-based flapping prevention
  (`unhealthy_threshold` / `healthy_threshold`) rather than flipping state
  on a single probe result, which is how most production load balancers
  (nginx, HAProxy, ALB) behave.
- **Smooth weighted round robin**: implemented the nginx algorithm
  (accumulate weight each tick, pick the max, subtract total) rather than
  naive "repeat backend N times in a list", because the naive version
  bursts N consecutive requests at the heavy backend instead of spreading
  them out.
- **Lock-free hot path**: backend health/connection-count state uses
  atomics (`AtomicBool`, `AtomicUsize`, `AtomicI64`) instead of
  `Mutex`/`RwLock`, since the balancer's `select()` is called on every
  single request.

## Possible extensions (good "future work" talking points)

- TLS termination (`rustls`) on the listener side
- Sticky sessions (consistent hashing on client IP or a cookie)
- Circuit breaking / retries with backoff on 5xx from a backend
- Passive health checks (mark unhealthy based on live request failures,
  not just the background prober)
- Prometheus metrics endpoint (`/metrics`) — request counts, latency
  histograms, per-backend health state
- Graceful shutdown / connection draining on SIGTERM
- Config hot-reload (watch `config.toml`, add/remove backends without restart)
