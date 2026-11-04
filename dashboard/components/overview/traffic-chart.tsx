"use client";

import React from "react";
import {
  AreaChart,
  Area,
  XAxis,
  YAxis,
  Tooltip,
  ResponsiveContainer,
  CartesianGrid,
} from "recharts";
import { Card, CardHeader, CardTitle, CardDescription } from "@/components/ui/card";

interface TrafficChartProps {
  data: Array<{
    timestamp: string;
    rate2xx: number;
    rate4xx: number;
    rate5xx: number;
  }>;
}

export function TrafficChart({ data }: TrafficChartProps) {
  const [mounted, setMounted] = React.useState(false);

  React.useEffect(() => {
    setMounted(true);
  }, []);

  return (
    <Card className="flex flex-col">
      <CardHeader className="flex flex-row items-center justify-between space-y-0 pb-2">
        <div>
          <CardTitle>Throughput</CardTitle>
          <CardDescription>HTTP request rate partitioned by status class (req/s)</CardDescription>
        </div>
        <div className="flex items-center gap-4 text-xs font-mono">
          <div className="flex items-center gap-1.5">
            <span className="w-2 h-2 rounded-full bg-[#0A0A0A]" />
            <span className="text-foreground-secondary">2xx Success</span>
          </div>
          <div className="flex items-center gap-1.5">
            <span className="w-2 h-2 rounded-full bg-[#71717A]" />
            <span className="text-foreground-secondary">4xx Client</span>
          </div>
          <div className="flex items-center gap-1.5">
            <span className="w-2 h-2 rounded-full bg-[#B91C1C]" />
            <span className="text-[#B91C1C]">5xx Error</span>
          </div>
        </div>
      </CardHeader>

      <div className="h-64 w-full pt-4">
        {mounted ? (
          <ResponsiveContainer width="100%" height="100%">
            <AreaChart data={data} margin={{ top: 5, right: 10, left: -20, bottom: 0 }}>
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
                              {p.value} req/s
                            </span>
                          </div>
                        ))}
                      </div>
                    );
                  }
                  return null;
                }}
              />
              {/* Solid fills only - NO gradients per strict design rules */}
              <Area
                type="monotone"
                dataKey="rate2xx"
                name="2xx"
                stackId="1"
                stroke="#0A0A0A"
                strokeWidth={1.5}
                fill="#0A0A0A"
                fillOpacity={0.08}
              />
              <Area
                type="monotone"
                dataKey="rate4xx"
                name="4xx"
                stackId="1"
                stroke="#71717A"
                strokeWidth={1.5}
                fill="#71717A"
                fillOpacity={0.08}
              />
              <Area
                type="monotone"
                dataKey="rate5xx"
                name="5xx"
                stackId="1"
                stroke="#B91C1C"
                strokeWidth={1.5}
                fill="#B91C1C"
                fillOpacity={0.15}
              />
            </AreaChart>
          </ResponsiveContainer>
        ) : (
          <div className="h-full w-full flex items-center justify-center text-xs text-foreground-muted font-mono">
            Loading chart telemetry...
          </div>
        )}
      </div>
    </Card>
  );
}

