import { NextResponse } from "next/server";

const ADMIN_URL = process.env.L7LB_ADMIN_URL || "http://127.0.0.1:8081";
const ADMIN_TOKEN = process.env.L7LB_ADMIN_TOKEN;

export async function GET() {
  try {
    const headers: Record<string, string> = {};
    if (ADMIN_TOKEN) {
      headers["Authorization"] = `Bearer ${ADMIN_TOKEN}`;
    }

    const res = await fetch(`${ADMIN_URL}/admin/config`, {
      headers,
      cache: "no-store",
    });

    if (!res.ok) {
      return NextResponse.json(
        { error: `Upstream error: ${res.statusText}` },
        { status: res.status }
      );
    }

    const data = await res.json();
    return NextResponse.json(data);
  } catch (err: unknown) {
    const message = err instanceof Error ? err.message : "Failed to connect to l7lb admin API";
    return NextResponse.json({ error: message }, { status: 502 });
  }
}

