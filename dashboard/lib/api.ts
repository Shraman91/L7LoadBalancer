import {
  getMockBackends,
  getMockMetrics,
  MOCK_CONFIG,
  MOCK_EVENTS,
  setMockBackendDrain,
} from "./mock";
import { BackendStatus, LogEvent, ParsedMetrics, ProxyConfig } from "./types";

export function isMockMode(): boolean {
  if (typeof window !== "undefined") {
    const local = localStorage.getItem("l7lb_use_mock");
    if (local !== null) {
      return local === "true";
    }
  }
  return process.env.NEXT_PUBLIC_USE_MOCK === "true";
}

export function setMockMode(val: boolean) {
  if (typeof window !== "undefined") {
    localStorage.setItem("l7lb_use_mock", val ? "true" : "false");
    window.location.reload();
  }
}

export async function fetchBackends(): Promise<BackendStatus[]> {
  if (isMockMode()) {
    return Promise.resolve(getMockBackends());
  }

  const res = await fetch("/api/backends", { cache: "no-store" });
  if (!res.ok) {
    throw new Error(`Failed to fetch backends: ${res.statusText}`);
  }
  return res.json();
}

export async function fetchConfig(): Promise<ProxyConfig> {
  if (isMockMode()) {
    return Promise.resolve(MOCK_CONFIG);
  }

  const res = await fetch("/api/config", { cache: "no-store" });
  if (!res.ok) {
    throw new Error(`Failed to fetch config: ${res.statusText}`);
  }
  return res.json();
}

export async function fetchMetrics(): Promise<ParsedMetrics> {
  if (isMockMode()) {
    return Promise.resolve(getMockMetrics());
  }

  const res = await fetch("/api/metrics", { cache: "no-store" });
  if (!res.ok) {
    throw new Error(`Failed to fetch metrics: ${res.statusText}`);
  }
  return res.json();
}

export async function drainBackend(addr: string): Promise<{ status: string; message: string }> {
  if (isMockMode()) {
    setMockBackendDrain(addr, true);
    return Promise.resolve({
      status: "ok",
      message: "Backend marked draining (mock mode)",
    });
  }

  const res = await fetch(`/api/backends/${encodeURIComponent(addr)}/drain`, {
    method: "POST",
    headers: { "Content-Type": "application/json" },
  });
  if (!res.ok) {
    const err = await res.json().catch(() => ({ error: res.statusText }));
    throw new Error(err.error || `Drain failed: ${res.statusText}`);
  }
  return res.json();
}

export async function enableBackend(addr: string): Promise<{ status: string; message: string }> {
  if (isMockMode()) {
    setMockBackendDrain(addr, false);
    return Promise.resolve({
      status: "ok",
      message: "Backend enabled (mock mode)",
    });
  }

  const res = await fetch(`/api/backends/${encodeURIComponent(addr)}/enable`, {
    method: "POST",
    headers: { "Content-Type": "application/json" },
  });
  if (!res.ok) {
    const err = await res.json().catch(() => ({ error: res.statusText }));
    throw new Error(err.error || `Enable failed: ${res.statusText}`);
  }
  return res.json();
}

export async function fetchEvents(): Promise<LogEvent[]> {
  if (isMockMode()) {
    return Promise.resolve(MOCK_EVENTS);
  }

  const res = await fetch("/api/events", { cache: "no-store" });
  if (!res.ok) {
    return MOCK_EVENTS;
  }
  return res.json();
}

export async function triggerTrafficLab(params: {
  path: string;
  method: string;
  rate: number;
  durationSecs: number;
}): Promise<{
  totalSent: number;
  success2xx: number;
  failed5xx: number;
  rateLimited429: number;
  retried: number;
  p50Ms: number;
  p99Ms: number;
}> {
  if (isMockMode()) {
    // Return realistic simulated execution result
    const total = params.rate * params.durationSecs;
    const failed = Math.floor(total * 0.04);
    const retried = failed;
    const rateLimited = params.rate > 80 ? Math.floor(total * 0.02) : 0;
    const success = total - failed - rateLimited;
    return new Promise((resolve) => {
      setTimeout(() => {
        resolve({
          totalSent: total,
          success2xx: success,
          failed5xx: failed,
          rateLimited429: rateLimited,
          retried,
          p50Ms: 0.85,
          p99Ms: 2.4,
        });
      }, 1500);
    });
  }

  const res = await fetch("/api/lab/traffic", {
    method: "POST",
    headers: { "Content-Type": "application/json" },
    body: JSON.stringify(params),
  });
  if (!res.ok) {
    throw new Error(`Traffic test failed: ${res.statusText}`);
  }
  return res.json();
}

