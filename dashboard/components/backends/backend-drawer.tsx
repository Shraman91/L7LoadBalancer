"use client";

import React from "react";
import { BackendStatus } from "@/lib/types";
import { X, Server, Activity, ShieldCheck, Zap } from "lucide-react";
import { StatusBadge, CircuitBadge } from "@/components/ui/status-badge";
import { formatLatency } from "@/lib/utils";
import { ResponsiveContainer, LineChart, Line, XAxis, YAxis, Tooltip } from "recharts";

interface BackendDrawerProps {
  backend: BackendStatus | null;
  onClose: () => void;
  onToggleDrain: (backend: BackendStatus) => void;
}

export function BackendDrawer({ backend, onClose, onToggleDrain }: BackendDrawerProps) {
  const [mounted, setMounted] = React.useState(false);

  React.useEffect(() => {
    setMounted(true);
  }, []);

  if (!backend) return null;

  // Mock historical active connection trend for this backend
  const connHistory = [
    { t: "18:20", conns: Math.max(0, backend.active_connections - 4) },
    { t: "18:21", conns: Math.max(0, backend.active_connections - 2) },
    { t: "18:22", conns: Math.max(0, backend.active_connections + 3) },
    { t: "18:23", conns: Math.max(0, backend.active_connections - 1) },
    { t: "18:24", conns: backend.active_connections },
  ];

  const overallStatus = backend.draining
    ? "draining"
    : !backend.healthy || backend.circuit_state === "open"
    ? "unhealthy"
    : backend.circuit_state === "half_open"
    ? "degraded"
    : "healthy";

  return (
    <div className="fixed inset-0 z-50 flex justify-end bg-black/30 backdrop-blur-[1px]">
      <div className="w-full max-w-lg bg-surface border-l border-border h-full shadow-2xl flex flex-col justify-between animate-in slide-in-from-right duration-200">
        <div>
          {/* Header */}
          <div className="p-6 border-b border-border flex items-start justify-between">
            <div className="flex items-center gap-3">
              <div className="w-10 h-10 rounded-card bg-surface-subtle border border-border flex items-center justify-center">
                <Server className="w-5 h-5 text-foreground" />
              </div>
              <div>
                <h2 className="text-sm font-semibold font-mono text-foreground tracking-tight">
                  {backend.addr}
                </h2>
                <div className="flex items-center gap-2 mt-1">
                  <StatusBadge status={overallStatus} />
                  <span className="text-xs text-foreground-muted font-mono">
                    pool: {backend.pool || "web"}
                  </span>
                </div>
              </div>
            </div>
            <button
              onClick={onClose}
              className="p-1 rounded-btn text-foreground-muted hover:text-foreground transition-colors"
            >
              <X className="w-4 h-4" />
            </button>
          </div>

          {/* Body stats */}
          <div className="p-6 space-y-6 overflow-y-auto">
            {/* Quick Metrics */}
            <div className="grid grid-cols-2 gap-3">
              <div className="p-3.5 rounded-card border border-border bg-surface-subtle">
                <span className="text-xs text-foreground-secondary">Active Connections</span>
                <p className="text-xl font-bold font-mono tabular-nums text-foreground mt-1">
                  {backend.active_connections}
                </p>
              </div>
              <div className="p-3.5 rounded-card border border-border bg-surface-subtle">
                <span className="text-xs text-foreground-secondary">EWMA Latency</span>
                <p className="text-xl font-bold font-mono tabular-nums text-foreground mt-1">
                  {formatLatency(backend.ewma_latency_ms)}
                </p>
              </div>
            </div>

            {/* Slow Start Progress if active */}
            {backend.slow_start_progress !== undefined && (
              <div className="p-4 rounded-card border border-[#FDE68A] bg-[#FFFBEB]">
                <div className="flex items-center justify-between text-xs font-medium text-[#B45309] mb-1.5">
                  <span className="flex items-center gap-1.5">
                    <Zap className="w-3.5 h-3.5" />
                    Slow-Start Traffic Ramp
                  </span>
                  <span className="font-mono">{backend.slow_start_progress}%</span>
                </div>
                <div className="w-full bg-[#FEF3C7] h-1.5 rounded-full overflow-hidden">
                  <div
                    className="bg-[#B45309] h-full transition-all duration-300"
                    style={{ width: `${backend.slow_start_progress}%` }}
                  />
                </div>
                <p className="text-[11px] text-[#B45309] mt-2">
                  Effective weight ramping linearly over 15s window to avoid overloading recovered backend.
                </p>
              </div>
            )}

            {/* Connection Trend */}
            <div>
              <div className="flex items-center justify-between text-xs mb-2">
                <span className="font-medium text-foreground flex items-center gap-1.5">
                  <Activity className="w-3.5 h-3.5 text-foreground-muted" />
                  Active Concurrency Timeline
                </span>
                <span className="text-[11px] text-foreground-muted font-mono">recent window</span>
              </div>
              <div className="h-28 w-full border border-border rounded-card p-2 bg-surface-subtle">
                {mounted ? (
                  <ResponsiveContainer width="100%" height="100%">
                    <LineChart data={connHistory}>
                      <XAxis
                        dataKey="t"
                        tickLine={false}
                        axisLine={false}
                        tick={{ fill: "#A1A1AA", fontSize: 10, fontFamily: "JetBrains Mono" }}
                      />
                      <YAxis
                        tickLine={false}
                        axisLine={false}
                        tick={{ fill: "#A1A1AA", fontSize: 10, fontFamily: "JetBrains Mono" }}
                      />
                      <Tooltip
                        content={({ active, payload, label }) => {
                          if (active && payload && payload.length) {
                            return (
                              <div className="bg-surface border border-border p-1.5 rounded text-[11px] font-mono shadow">
                                {label}: {payload[0].value} conns
                              </div>
                            );
                          }
                          return null;
                        }}
                      />
                      <Line
                        type="monotone"
                        dataKey="conns"
                        stroke="#0A0A0A"
                        strokeWidth={1.5}
                        dot={{ r: 2, fill: "#0A0A0A" }}
                      />
                    </LineChart>
                  </ResponsiveContainer>
                ) : (
                  <div className="w-full h-full flex items-center justify-center text-xs text-foreground-muted font-mono">
                    Loading trend...
                  </div>
                )}
              </div>
            </div>

            {/* Circuit Breaker State & Parameters */}
            <div className="border border-border rounded-card p-4 space-y-3">
              <div className="flex items-center justify-between">
                <span className="text-xs font-medium text-foreground flex items-center gap-1.5">
                  <ShieldCheck className="w-3.5 h-3.5 text-foreground-muted" />
                  Circuit Breaker Machine
                </span>
                <CircuitBadge state={backend.circuit_state} />
              </div>
              <div className="text-xs divide-y divide-border pt-1">
                <div className="py-1.5 flex items-center justify-between font-mono">
                  <span className="text-foreground-secondary">State</span>
                  <span className="text-foreground font-semibold">{backend.circuit_state}</span>
                </div>
                <div className="py-1.5 flex items-center justify-between font-mono">
                  <span className="text-foreground-secondary">Assigned Weight</span>
                  <span className="text-foreground">{backend.weight}</span>
                </div>
                <div className="py-1.5 flex items-center justify-between font-mono">
                  <span className="text-foreground-secondary">TLS Upstream</span>
                  <span className="text-foreground">{backend.tls ? "Enabled" : "Disabled (Plain HTTP)"}</span>
                </div>
                <div className="py-1.5 flex items-center justify-between font-mono">
                  <span className="text-foreground-secondary">Drain Flag</span>
                  <span className="text-foreground">{backend.draining ? "TRUE (Draining)" : "FALSE"}</span>
                </div>
              </div>
            </div>
          </div>
        </div>

        {/* Action footer */}
        <div className="p-6 border-t border-border bg-surface-subtle flex items-center justify-between">
          <span className="text-xs text-foreground-muted">Operator Control</span>
          <button
            onClick={() => onToggleDrain(backend)}
            className="px-4 py-2 rounded-btn bg-foreground text-primary-foreground hover:bg-primary-hover text-xs font-medium transition-colors"
          >
            {backend.draining ? "Enable Backend" : "Drain Backend"}
          </button>
        </div>
      </div>
    </div>
  );
}

