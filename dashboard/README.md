# L7LB Admin & Observability Dashboard

A high-performance, B2B SaaS-grade administrative control plane and observability dashboard for `l7lb`. Built with Next.js (App Router), TypeScript (strict), Tailwind CSS, TanStack Query, Recharts, and React Flow.

---

## Features

- **Fleet Overview**: Aggregated requests/sec, 5xx error rate %, median p50 and tail p99 latency distributions, active in-flight connection counters, and healthy node ratios with sparklines.
- **Backend Fleet Table**: Live status, circuit breaker states (`closed`, `half_open`, `open`), slow-start ramp progress, active concurrency, and EWMA latency. Search, filter by pool/state, and sort.
- **Operator Actions**: Drain and enable backends with confirmation dialogs, optimistic UI updates, and rollback on failure.
- **Topology Visualizer**: Interactive node graph mapping `Listener (:8080) -> Routes -> Pools -> Backends` with status dots and dashed failure links.
- **Routes & Config Viewer**: Formatted collapsible cards for routes, pools, timeouts, frame limits, rate limiting, and circuit breakers, plus raw TOML/JSON copy/toggle.
- **Live Audit & Incident Feed**: Real-time event log tracking circuit transitions, health check shifts, administrative actions, and load shedding signals.
- **Traffic Lab**: Integrated load generator and chaos walkthrough to trigger traffic, simulate container failure, and observe circuit breaker opening and slow-start recovery.

---

## Design System

Strict Light Theme with Black Undertones:
- **Canvas / Page**: `#FAFAFA`
- **Cards & Surfaces**: `#FFFFFF` with `#E4E4E7` borders (no gradients, no heavy drop shadows)
- **Text**: `#0A0A0A` (primary), `#52525B` (secondary), `#A1A1AA` (muted)
- **Primary Actions**: Solid `#0A0A0A` with `#FAFAFA` text (hover `#27272A`)
- **Status Indicators**: Muted, subtle badges and 8px dots only (never large fills)
  - Healthy: `#15803D` on `#F0FDF4`
  - Degraded / Half-Open: `#B45309` on `#FFFBEB`
  - Unhealthy / Open: `#B91C1C` on `#FEF2F2`
  - Draining: `#52525B` on `#F4F4F5`
- **Typography**: Inter (UI) and JetBrains Mono (metrics, addresses, timestamps). Tabular numbers on all metrics.
- **Zero Emojis**: Complete typographical restraint across UI copy, logs, and empty states.

---

## Environment Variables

| Variable | Default | Purpose |
| :--- | :--- | :--- |
| `L7LB_ADMIN_URL` | `http://127.0.0.1:8081` | Upstream URL for `l7lb` Admin API server |
| `L7LB_PROXY_URL` | `http://127.0.0.1:8080` | Upstream URL for proxying Traffic Lab load tests |
| `L7LB_ADMIN_TOKEN` | *(empty)* | Optional Bearer token for authenticating mutating control plane requests |
| `NEXT_PUBLIC_USE_MOCK` | `false` | When `true`, uses realistic simulated data without requiring a running proxy |

---

## Getting Started

### 1. Install Dependencies
```bash
npm install
```

### 2. Run in Standalone Mock Mode
To run without running `l7lb` locally:
```bash
NEXT_PUBLIC_USE_MOCK=true npm run dev
```
Navigate to `http://localhost:3001`.

### 3. Run with Live Backend
Start `l7lb` on `127.0.0.1:8080` and `127.0.0.1:8081`, then:
```bash
npm run dev
```

### 4. Build for Production
```bash
npm run build
npm start
```

---

## Visual Previews

<!-- Screenshot placeholder: Fleet Overview -->
<!-- Screenshot placeholder: Backend Table & Slide-Over Drawer -->
<!-- Screenshot placeholder: Topology Visualizer -->
<!-- Screenshot placeholder: Traffic Lab & Chaos Demo -->

