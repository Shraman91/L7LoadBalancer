"use client";

import React from "react";
import { Handle, Position } from "@xyflow/react";
import { Globe, GitFork, Layers, Server } from "lucide-react";
import { cn } from "@/lib/utils";

export function ListenerNode({ data }: { data: { label: string; port: number; tls: boolean } }) {
  return (
    <div className="bg-surface border border-border rounded-card p-3 shadow-sm w-44 select-none">
      <div className="flex items-center gap-2 mb-1">
        <Globe className="w-3.5 h-3.5 text-foreground" />
        <span className="text-[11px] font-semibold tracking-tight text-foreground">
          HTTP Listener
        </span>
      </div>
      <p className="text-xs font-mono font-medium text-foreground">0.0.0.0:{data.port}</p>
      <div className="flex items-center gap-1.5 mt-2 text-[10px] font-mono text-foreground-secondary">
        <span className="px-1 py-0.5 rounded bg-surface-subtle border border-border">
          {data.tls ? "TLS 1.2/1.3" : "Plain HTTP"}
        </span>
        <span className="px-1 py-0.5 rounded bg-surface-subtle border border-border">
          ALPN h2/1.1
        </span>
      </div>
      <Handle type="source" position={Position.Right} className="!bg-foreground !w-2 !h-2" />
    </div>
  );
}

export function RouteNode({
  data,
}: {
  data: { pathPrefix: string; pool: string; host?: string | null };
}) {
  return (
    <div className="bg-surface border border-border rounded-card p-3 shadow-sm w-48 select-none">
      <Handle type="target" position={Position.Left} className="!bg-foreground !w-2 !h-2" />
      <div className="flex items-center gap-2 mb-1">
        <GitFork className="w-3.5 h-3.5 text-foreground-secondary" />
        <span className="text-[11px] font-semibold tracking-tight text-foreground">
          Route Rule
        </span>
      </div>
      <p className="text-xs font-mono font-bold text-foreground truncate">
        {data.pathPrefix}
      </p>
      <div className="mt-1.5 text-[10px] font-mono text-foreground-muted truncate">
        {data.host ? `host: ${data.host}` : "host: * (any)"}
      </div>
      <Handle type="source" position={Position.Right} className="!bg-foreground !w-2 !h-2" />
    </div>
  );
}

export function PoolNode({
  data,
}: {
  data: { name: string; algorithm: string; backendCount: number };
}) {
  return (
    <div className="bg-surface border border-border rounded-card p-3.5 shadow-sm w-52 select-none">
      <Handle type="target" position={Position.Left} className="!bg-foreground !w-2 !h-2" />
      <div className="flex items-center justify-between mb-1">
        <div className="flex items-center gap-2">
          <Layers className="w-3.5 h-3.5 text-foreground" />
          <span className="text-xs font-semibold tracking-tight text-foreground capitalize">
            {data.name} Pool
          </span>
        </div>
        <span className="text-[10px] font-mono px-1.5 py-0.5 rounded bg-surface-subtle border border-border text-foreground-secondary">
          {data.backendCount} nodes
        </span>
      </div>
      <p className="text-[11px] font-mono text-foreground-secondary mt-1">
        algo: {data.algorithm}
      </p>
      <Handle type="source" position={Position.Right} className="!bg-foreground !w-2 !h-2" />
    </div>
  );
}

export function BackendNode({
  data,
}: {
  data: {
    addr: string;
    healthy: boolean;
    draining: boolean;
    circuitState: string;
    conns: number;
    latencyMs: number;
  };
}) {
  const isHealthy = data.healthy && !data.draining && data.circuitState === "closed";
  const isDegraded = data.circuitState === "half_open";
  const isDead = !data.healthy || data.circuitState === "open";

  return (
    <div
      className={cn(
        "bg-surface border rounded-card p-3 shadow-sm w-52 select-none transition-colors",
        data.draining
          ? "border-border bg-surface-subtle"
          : isDead
          ? "border-[#FECACA] bg-[#FEF2F2]/30"
          : isDegraded
          ? "border-[#FDE68A]"
          : "border-border"
      )}
    >
      <Handle type="target" position={Position.Left} className="!bg-foreground !w-2 !h-2" />
      <div className="flex items-center justify-between">
        <div className="flex items-center gap-2 min-w-0">
          <Server className="w-3.5 h-3.5 text-foreground-secondary shrink-0" />
          <span className="text-xs font-mono font-medium text-foreground truncate">
            {data.addr}
          </span>
        </div>
        <span
          className={cn(
            "w-2 h-2 rounded-full shrink-0",
            data.draining
              ? "bg-[#52525B]"
              : isHealthy
              ? "bg-[#15803D]"
              : isDegraded
              ? "bg-[#B45309]"
              : "bg-[#B91C1C]"
          )}
        />
      </div>

      <div className="mt-2.5 pt-2 border-t border-border flex items-center justify-between text-[11px] font-mono">
        <span className="text-foreground-secondary">{data.conns} conns</span>
        <span className="text-foreground-muted">{data.latencyMs.toFixed(1)} ms</span>
      </div>
    </div>
  );
}

