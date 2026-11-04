import { NextRequest, NextResponse } from "next/server";

const ADMIN_URL = process.env.L7LB_ADMIN_URL || "http://127.0.0.1:8081";
const ADMIN_TOKEN = process.env.L7LB_ADMIN_TOKEN;

export async function POST(
  _req: NextRequest,
  { params }: { params: { addr: string } }
) {
  try {
    const addr = decodeURIComponent(params.addr);
    const headers: Record<string, string> = {
      "Content-Type": "application/json",
    };
    if (ADMIN_TOKEN) {
      headers["Authorization"] = `Bearer ${ADMIN_TOKEN}`;
    }

    const res = await fetch(`${ADMIN_URL}/admin/backends/${addr}/enable`, {
      method: "POST",
      headers,
    });

    const data = await res.json().catch(() => ({}));
    return NextResponse.json(data, { status: res.status });
  } catch (err: unknown) {
    const message = err instanceof Error ? err.message : "Failed to execute enable";
    return NextResponse.json({ error: message }, { status: 502 });
  }
}

