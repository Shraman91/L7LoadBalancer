import { computeRatesFromMetrics, parsePrometheusText } from "@/lib/prometheus-parser";
import { NextResponse } from "next/server";

const ADMIN_URL = process.env.L7LB_ADMIN_URL || "http://127.0.0.1:8081";

interface TimelinePoint {
  timestamp: string;
  rate2xx: number;
  rate4xx: number;
  rate5xx: number;
}

interface LatencyPoint {
  timestamp: string;
  p50: number;
  p90: number;
  p99: number;
}

let trafficTimeline: TimelinePoint[] = [];
let latencyTimeline: LatencyPoint[] = [];

export async function GET() {
  try {
    const res = await fetch(`${ADMIN_URL}/metrics`, { cache: "no-store" });
    if (!res.ok) {
      return NextResponse.json(
        { error: `Metrics endpoint error: ${res.statusText}` },
        { status: res.status }
      );
    }

    const text = await res.text();
    const rawMetrics = parsePrometheusText(text);
    const rates = computeRatesFromMetrics(rawMetrics);

    const now = new Date();
    const timeStr = now.toLocaleTimeString("en-US", {
      hour12: false,
      hour: "2-digit",
      minute: "2-digit",
      second: "2-digit",
    });

    trafficTimeline.push({
      timestamp: timeStr,
      rate2xx: rates.rate2xx,
      rate4xx: rates.rate4xx,
      rate5xx: rates.rate5xx,
    });
    if (trafficTimeline.length > 30) {
      trafficTimeline.shift();
    }

    latencyTimeline.push({
      timestamp: timeStr,
      p50: rates.p50Ms,
      p90: rates.p90Ms,
      p99: rates.p99Ms,
    });
    if (latencyTimeline.length > 30) {
      latencyTimeline.shift();
    }

    return NextResponse.json({
      totalRequests: rates.totalRate * 60, // approximate or total counter
      requestsPerSec: rates.totalRate,
      rate2xx: rates.rate2xx,
      rate4xx: rates.rate4xx,
      rate5xx: rates.rate5xx,
      errorRate5xxPct: rates.errorRate5xxPct,
      p50LatencyMs: rates.p50Ms,
      p90LatencyMs: rates.p90Ms,
      p99LatencyMs: rates.p99Ms,
      totalActiveConnections: rates.activeConnections,
      rateLimitedTotal: rates.rateLimitedTotal,
      loadShedTotal: rates.loadShedTotal,
      retriesTotal: rates.retriesTotal,
      trafficTimeline,
      latencyTimeline,
    });
  } catch (err: unknown) {
    const message = err instanceof Error ? err.message : "Failed to fetch metrics";
    return NextResponse.json({ error: message }, { status: 502 });
  }
}

