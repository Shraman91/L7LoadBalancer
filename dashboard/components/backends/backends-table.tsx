"use client";

import React, { useState, useMemo } from "react";
import { BackendStatus } from "@/lib/types";
import { StatusBadge, CircuitBadge } from "@/components/ui/status-badge";
import { formatLatency, cn } from "@/lib/utils";
import { Search, ArrowUpDown, Shield, PowerOff, Power } from "lucide-react";

interface BackendsTableProps {
  backends: BackendStatus[];
  onSelectBackend: (backend: BackendStatus) => void;
  onInitiateDrainAction: (backend: BackendStatus) => void;
}

type SortField = "addr" | "pool" | "active_connections" | "weight" | "ewma_latency_ms";
type SortOrder = "asc" | "desc";

export function BackendsTable({
  backends,
  onSelectBackend,
  onInitiateDrainAction,
}: BackendsTableProps) {
  const [search, setSearch] = useState("");
  const [selectedPool, setSelectedPool] = useState<string>("all");
  const [selectedStatus, setSelectedStatus] = useState<string>("all");
  const [sortField, setSortField] = useState<SortField>("addr");
  const [sortOrder, setSortOrder] = useState<SortOrder>("asc");

  const pools = useMemo(() => {
    const set = new Set<string>();
    backends.forEach((b) => b.pool && set.add(b.pool));
    return Array.from(set);
  }, [backends]);

  const filtered = useMemo(() => {
    return backends.filter((b) => {
      const matchSearch =
        b.addr.toLowerCase().includes(search.toLowerCase()) ||
        (b.pool && b.pool.toLowerCase().includes(search.toLowerCase()));

      const matchPool = selectedPool === "all" || b.pool === selectedPool;

      let matchStatus = true;
      if (selectedStatus === "healthy") {
        matchStatus = b.healthy && !b.draining && b.circuit_state === "closed";
      } else if (selectedStatus === "degraded") {
        matchStatus = b.circuit_state === "half_open";
      } else if (selectedStatus === "unhealthy") {
        matchStatus = !b.healthy || b.circuit_state === "open";
      } else if (selectedStatus === "draining") {
        matchStatus = b.draining;
      }

      return matchSearch && matchPool && matchStatus;
    });
  }, [backends, search, selectedPool, selectedStatus]);

  const sorted = useMemo(() => {
    return [...filtered].sort((a, b) => {
      let cmp = 0;
      if (sortField === "addr") {
        cmp = a.addr.localeCompare(b.addr);
      } else if (sortField === "pool") {
        cmp = (a.pool || "").localeCompare(b.pool || "");
      } else if (sortField === "active_connections") {
        cmp = a.active_connections - b.active_connections;
      } else if (sortField === "weight") {
        cmp = a.weight - b.weight;
      } else if (sortField === "ewma_latency_ms") {
        cmp = a.ewma_latency_ms - b.ewma_latency_ms;
      }
      return sortOrder === "asc" ? cmp : -cmp;
    });
  }, [filtered, sortField, sortOrder]);

  const toggleSort = (field: SortField) => {
    if (sortField === field) {
      setSortOrder(sortOrder === "asc" ? "desc" : "asc");
    } else {
      setSortField(field);
      setSortOrder("asc");
    }
  };

  return (
    <div className="space-y-4">
      {/* Controls Bar */}
      <div className="flex flex-col sm:flex-row items-stretch sm:items-center justify-between gap-3">
        <div className="relative flex-1 max-w-sm">
          <Search className="w-3.5 h-3.5 text-foreground-muted absolute left-3 top-1/2 -translate-y-1/2" />
          <input
            type="text"
            placeholder="Search backends by address or pool..."
            value={search}
            onChange={(e) => setSearch(e.target.value)}
            className="w-full pl-9 pr-3 py-1.5 rounded-btn border border-border bg-surface text-xs text-foreground placeholder:text-foreground-muted focus:border-border-strong transition-colors"
          />
        </div>

        <div className="flex items-center gap-2">
          {/* Pool Filter */}
          <select
            value={selectedPool}
            onChange={(e) => setSelectedPool(e.target.value)}
            className="px-2.5 py-1.5 rounded-btn border border-border bg-surface text-xs font-mono text-foreground focus:border-border-strong transition-colors"
          >
            <option value="all">Pool: All</option>
            {pools.map((p) => (
              <option key={p} value={p}>
                Pool: {p}
              </option>
            ))}
          </select>

          {/* Status Filter */}
          <select
            value={selectedStatus}
            onChange={(e) => setSelectedStatus(e.target.value)}
            className="px-2.5 py-1.5 rounded-btn border border-border bg-surface text-xs font-mono text-foreground focus:border-border-strong transition-colors"
          >
            <option value="all">Status: All</option>
            <option value="healthy">Healthy</option>
            <option value="degraded">Degraded (Half-Open)</option>
            <option value="unhealthy">Unhealthy (Open)</option>
            <option value="draining">Draining</option>
          </select>
        </div>
      </div>

      {/* Table */}
      <div className="border border-border rounded-card bg-surface overflow-hidden shadow-card">
        <table className="w-full text-left border-collapse">
          <thead>
            <tr className="border-b border-border bg-surface-subtle text-[11px] font-medium text-foreground-secondary uppercase tracking-wider select-none">
              <th
                onClick={() => toggleSort("addr")}
                className="py-3 px-4 cursor-pointer hover:text-foreground transition-colors"
              >
                <div className="flex items-center gap-1.5">
                  <span>Backend Address</span>
                  <ArrowUpDown className="w-3 h-3 text-foreground-muted" />
                </div>
              </th>
              <th
                onClick={() => toggleSort("pool")}
                className="py-3 px-3 cursor-pointer hover:text-foreground transition-colors"
              >
                <div className="flex items-center gap-1.5">
                  <span>Pool</span>
                  <ArrowUpDown className="w-3 h-3 text-foreground-muted" />
                </div>
              </th>
              <th className="py-3 px-3">Health</th>
              <th className="py-3 px-3">Circuit State</th>
              <th
                onClick={() => toggleSort("active_connections")}
                className="py-3 px-3 text-right cursor-pointer hover:text-foreground transition-colors"
              >
                <div className="flex items-center justify-end gap-1.5">
                  <span>Active Conns</span>
                  <ArrowUpDown className="w-3 h-3 text-foreground-muted" />
                </div>
              </th>
              <th
                onClick={() => toggleSort("weight")}
                className="py-3 px-3 text-right cursor-pointer hover:text-foreground transition-colors"
              >
                <div className="flex items-center justify-end gap-1.5">
                  <span>Weight</span>
                  <ArrowUpDown className="w-3 h-3 text-foreground-muted" />
                </div>
              </th>
              <th
                onClick={() => toggleSort("ewma_latency_ms")}
                className="py-3 px-3 text-right cursor-pointer hover:text-foreground transition-colors"
              >
                <div className="flex items-center justify-end gap-1.5">
                  <span>Latency EWMA</span>
                  <ArrowUpDown className="w-3 h-3 text-foreground-muted" />
                </div>
              </th>
              <th className="py-3 px-4 text-right">Actions</th>
            </tr>
          </thead>
          <tbody className="divide-y divide-border text-xs">
            {sorted.map((backend) => {
              const status = backend.draining
                ? "draining"
                : !backend.healthy || backend.circuit_state === "open"
                ? "unhealthy"
                : backend.circuit_state === "half_open"
                ? "degraded"
                : "healthy";

              return (
                <tr
                  key={backend.addr}
                  onClick={() => onSelectBackend(backend)}
                  className="hover:bg-surface-subtle cursor-pointer transition-colors duration-100 group"
                >
                  <td className="py-3.5 px-4 font-mono font-medium text-foreground">
                    {backend.addr}
                    {backend.tls && (
                      <span className="ml-2 text-[10px] font-mono px-1 py-0.5 rounded bg-surface-subtle border border-border text-foreground-secondary">
                        TLS
                      </span>
                    )}
                  </td>
                  <td className="py-3.5 px-3 font-mono text-foreground-secondary">
                    {backend.pool || "web"}
                  </td>
                  <td className="py-3.5 px-3">
                    <StatusBadge status={status} />
                  </td>
                  <td className="py-3.5 px-3">
                    <CircuitBadge state={backend.circuit_state} />
                  </td>
                  <td className="py-3.5 px-3 text-right font-mono font-semibold tabular-nums text-foreground">
                    {backend.active_connections}
                  </td>
                  <td className="py-3.5 px-3 text-right font-mono tabular-nums text-foreground-secondary">
                    {backend.weight}
                    {backend.slow_start_progress !== undefined && (
                      <span className="ml-1 text-[10px] text-[#B45309] font-mono">
                        ({backend.slow_start_progress}%)
                      </span>
                    )}
                  </td>
                  <td className="py-3.5 px-3 text-right font-mono tabular-nums text-foreground">
                    {formatLatency(backend.ewma_latency_ms)}
                  </td>
                  <td
                    className="py-3.5 px-4 text-right"
                    onClick={(e) => e.stopPropagation()}
                  >
                    <button
                      onClick={() => onInitiateDrainAction(backend)}
                      className={cn(
                        "inline-flex items-center gap-1 px-2.5 py-1 rounded-btn text-xs font-medium border transition-colors",
                        backend.draining
                          ? "border-[#BBF7D0] bg-[#F0FDF4] text-[#15803D] hover:bg-[#DCFCE7]"
                          : "border-border text-foreground-secondary hover:text-foreground hover:bg-surface-subtle"
                      )}
                    >
                      {backend.draining ? (
                        <>
                          <Power className="w-3 h-3 text-[#15803D]" />
                          Enable
                        </>
                      ) : (
                        <>
                          <PowerOff className="w-3 h-3 text-foreground-muted" />
                          Drain
                        </>
                      )}
                    </button>
                  </td>
                </tr>
              );
            })}

            {sorted.length === 0 && (
              <tr>
                <td colSpan={8} className="py-12 text-center text-xs text-foreground-muted">
                  No backends match the selected filter criteria
                </td>
              </tr>
            )}
          </tbody>
        </table>
      </div>
    </div>
  );
}

