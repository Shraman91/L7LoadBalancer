import { MOCK_EVENTS } from "@/lib/mock";
import { NextResponse } from "next/server";

export async function GET() {
  return NextResponse.json(MOCK_EVENTS);
}

