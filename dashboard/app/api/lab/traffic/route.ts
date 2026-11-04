import { NextRequest, NextResponse } from "next/server";

const PROXY_URL = process.env.L7LB_PROXY_URL || "http://127.0.0.1:8080";

export async function POST(req: NextRequest) {
  try {
    const { path = "/", method = "GET", rate = 50, durationSecs = 5 } = await req.json();

    const targetUrl = `${PROXY_URL}${path.startsWith("/") ? path : `/${path}`}`;
    const totalRequests = Math.min(1000, Math.max(1, rate * durationSecs));

    let success2xx = 0;
    let failed5xx = 0;
    let rateLimited429 = 0;
    let otherCodes = 0;
    const latencies: number[] = [];

    const batchSize = Math.min(20, Math.max(1, rate));
    const batches = Math.ceil(totalRequests / batchSize);

    for (let b = 0; b < batches; b++) {
      const currentBatch = Math.min(batchSize, totalRequests - b * batchSize);
      const promises = Array.from({ length: currentBatch }).map(async () => {
        const start = performance.now();
        try {
          const res = await fetch(targetUrl, {
            method,
            headers: {
              "User-Agent": "l7lb-traffic-lab/1.0",
            },
            cache: "no-store",
          });
          const elapsed = performance.now() - start;
          latencies.push(elapsed);

          if (res.status >= 200 && res.status < 300) {
            success2xx++;
          } else if (res.status === 429) {
            rateLimited429++;
          } else if (res.status >= 500) {
            failed5xx++;
          } else {
            otherCodes++;
          }
        } catch {
          failed5xx++;
          latencies.push(performance.now() - start);
        }
      });

      await Promise.all(promises);
      // Brief spacing between batches to approximate rate
      await new Promise((r) => setTimeout(r, 100));
    }

    latencies.sort((a, b) => a - b);
    const p50Ms = latencies[Math.floor(latencies.length * 0.5)] || 0;
    const p99Ms = latencies[Math.floor(latencies.length * 0.99)] || 0;

    return NextResponse.json({
      totalSent: totalRequests,
      success2xx,
      failed5xx,
      rateLimited429,
      otherCodes,
      retried: failed5xx,
      p50Ms: Math.round(p50Ms * 100) / 100,
      p99Ms: Math.round(p99Ms * 100) / 100,
    });
  } catch (err: unknown) {
    const message = err instanceof Error ? err.message : "Traffic generation failed";
    return NextResponse.json({ error: message }, { status: 500 });
  }
}

