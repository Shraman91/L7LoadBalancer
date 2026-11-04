"use client";

import React from "react";
import {
  LineChart,
  Line,
  XAxis,
  YAxis,
  Tooltip,
  ResponsiveContainer,
  CartesianGrid,
} from "recharts";
import { Card, CardHeader, CardTitle, CardDescription } from "@/components/ui/card";

interface LatencyChartProps {
  data: Array<{
    timestamp: string;
    p50: number;
    p90: number;
    p99: number;
  }>;
}

export function LatencyChart({ data }: LatencyChartProps) {
  return (
    <Card className="flex flex-col">
      <CardHeader className="flex flex-row items-center justify-between space-y-0 pb-2">
        <div>
          <CardTitle>Latency Distribution</CardTitle>
          <CardDescription>Response time percentiles over time (ms)</CardDescription>
        </div>
        <div className="flex items-center gap-4 text-xs font-mono">
          <div className="flex items-center gap-1.5">
            <span className="w-2 h-2 rounded-full bg-[#0A0A0A]" />
            <span className="text-foreground-secondary">p50</span>
          </div>
          <div className="flex items-center gap-1.5">
            <span className="w-2 h-2 rounded-full bg-[#71717A]" />
            <span className="text-foreground-secondary">p90</span>
          </div>
          <div className="flex items-center gap-1.5">
            <span className="w-2 h-2 rounded-full bg-[#B45309]" />
            <span className="text-[#B45309]">p99</span>
          </div>
        </div>
      </CardHeader>

      <div className="h-64 w-full pt-4">
        <ResponsiveContainer width="100%" height="100%">
          <LineChart data={data} margin={{ top: 5, right: 10, left: -20, bottom: 0 }}>
            <CartesianGrid strokeDasharray="3 3" stroke="#E4E4E7" vertical={false} />
            <XAxis
              dataKey="timestamp"
              tickLine={false}
              axisLine={false}
              tick={{ fill: "#A1A1AA", fontSize: 11, fontFamily: "JetBrains Mono" }}
            />
            <YAxis
              tickLine={false}
              axisLine={false}
              unit="ms"
              tick={{ fill: "#A1A1AA", fontSize: 11, fontFamily: "JetBrains Mono" }}
            />
            <Tooltip
              content={({ active, payload, label }) => {
                if (active && payload && payload.length) {
                  return (
                    <div className="bg-surface border border-border p-2.5 rounded-card shadow-md text-xs font-mono">
                      <p className="text-foreground-muted mb-1.5">{label}</p>
                      {payload.map((p, idx) => (
                        <div key={idx} className="flex items-center justify-between gap-4 py-0.5">
                          <span className="text-foreground-secondary">{p.name}:</span>
                          <span className="font-bold tabular-nums text-foreground">
                            {p.value} ms
                          </span>
                        </div>
                      ))}
                    </div>
                  );
                }
                return null;
              }}
            />
            <Line
              type="monotone"
              dataKey="p50"
              name="p50"
              stroke="#0A0A0A"
              strokeWidth={1.5}
              dot={false}
            />
            <Line
              type="monotone"
              dataKey="p90"
              name="p90"
              stroke="#71717A"
              strokeWidth={1.5}
              dot={false}
            />
            <Line
              type="monotone"
              dataKey="p99"
              name="p99"
              stroke="#B45309"
              strokeWidth={1.5}
              dot={false}
            />
          </LineChart>
        </ResponsiveContainer>
      </div>
    </Card>
  );
}

