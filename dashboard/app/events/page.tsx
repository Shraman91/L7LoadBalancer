"use client";

import React, { useState, useEffect } from "react";
import { useQuery } from "@tanstack/react-query";
import { fetchEvents, fetchBackends } from "@/lib/api";
import { Header } from "@/components/layout/header";
import { LogEvent, EventSeverity } from "@/lib/types";
import { cn } from "@/lib/utils";
import { Play, Pause, Trash2, Filter } from "lucide-react";

export default function EventsPage() {
  const [isPaused, setIsPaused] = useState(false);
  const [eventList, setEventList] = useState<LogEvent[]>([]);
  const [selectedSeverity, setSelectedSeverity] = useState<string>("all");
  const [selectedPool, setSelectedPool] = useState<string>("all");

  const { data: polledEvents = [] } = useQuery({
    queryKey: ["events"],
    queryFn: fetchEvents,
    enabled: !isPaused,
  });

  const { data: backends = [] } = useQuery({
    queryKey: ["backends"],
    queryFn: fetchBackends,
  });

  // Client-side diffing engine across successive backend polls
  useEffect(() => {
    if (!isPaused && polledEvents.length > 0) {
      setEventList(polledEvents);
    }
  }, [polledEvents, isPaused]);

  const clearEvents = () => {
    setEventList([]);
  };

  const filtered = eventList.filter((evt) => {
    const matchSeverity = selectedSeverity === "all" || evt.severity === selectedSeverity;
    const matchPool = selectedPool === "all" || evt.pool === selectedPool;
    return matchSeverity && matchPool;
  });

  return (
    <div className="flex-1 flex flex-col h-full overflow-hidden">
      <Header
        title="Audit & Incident Log"
        subtitle="Circuit State, Health Shifting & Concurrency Signals"
      />

      <main className="flex-1 overflow-y-auto p-8 max-w-[1400px] w-full mx-auto space-y-6">
        {/* Controls Bar */}
        <div className="flex flex-col sm:flex-row items-stretch sm:items-center justify-between gap-3">
          <div className="flex items-center gap-2">
            <button
              onClick={() => setIsPaused(!isPaused)}
              className={cn(
                "flex items-center gap-1.5 px-3 py-1.5 rounded-btn border text-xs font-medium transition-colors",
                isPaused
                  ? "border-[#BBF7D0] bg-[#F0FDF4] text-[#15803D]"
                  : "border-border bg-surface text-foreground hover:bg-surface-subtle"
              )}
            >
              {isPaused ? (
                <>
                  <Play className="w-3.5 h-3.5 fill-current" />
                  Resume Feed
                </>
              ) : (
                <>
                  <Pause className="w-3.5 h-3.5 fill-current" />
                  Pause Stream
                </>
              )}
            </button>

            <button
              onClick={clearEvents}
              className="flex items-center gap-1.5 px-3 py-1.5 rounded-btn border border-border bg-surface hover:bg-surface-subtle text-xs font-medium text-foreground-secondary hover:text-foreground transition-colors"
            >
              <Trash2 className="w-3.5 h-3.5" />
              Clear Log
            </button>
          </div>

          <div className="flex items-center gap-2">
            {/* Severity Filter */}
            <select
              value={selectedSeverity}
              onChange={(e) => setSelectedSeverity(e.target.value)}
              className="px-2.5 py-1.5 rounded-btn border border-border bg-surface text-xs font-mono text-foreground focus:border-border-strong transition-colors"
            >
              <option value="all">Severity: All</option>
              <option value="critical">Critical</option>
              <option value="warning">Warning</option>
              <option value="info">Info</option>
            </select>

            {/* Pool Filter */}
            <select
              value={selectedPool}
              onChange={(e) => setSelectedPool(e.target.value)}
              className="px-2.5 py-1.5 rounded-btn border border-border bg-surface text-xs font-mono text-foreground focus:border-border-strong transition-colors"
            >
              <option value="all">Pool: All</option>
              <option value="web">web</option>
              <option value="api">api</option>
              <option value="stateful">stateful</option>
            </select>
          </div>
        </div>

        {/* Events Feed Container */}
        <div className="border border-border rounded-card bg-surface overflow-hidden shadow-card">
          <div className="divide-y divide-border">
            {filtered.map((evt) => (
              <div
                key={evt.id}
                className="p-4 flex items-start justify-between gap-4 hover:bg-surface-subtle transition-colors"
              >
                <div className="flex items-start gap-3 min-w-0">
                  <span
                    className={cn(
                      "px-2 py-0.5 rounded-full text-[10px] font-mono font-bold tracking-wider border shrink-0 mt-0.5",
                      evt.severity === "critical" && "bg-[#FEF2F2] text-[#B91C1C] border-[#FECACA]",
                      evt.severity === "warning" && "bg-[#FFFBEB] text-[#B45309] border-[#FDE68A]",
                      evt.severity === "info" && "bg-[#F0FDF4] text-[#15803D] border-[#BBF7D0]"
                    )}
                  >
                    {evt.severity.toUpperCase()}
                  </span>

                  <div className="min-w-0">
                    <p className="text-xs font-medium text-foreground">{evt.message}</p>
                    <div className="flex items-center gap-3 text-[11px] text-foreground-muted font-mono mt-1">
                      {evt.pool && <span>pool: <strong className="text-foreground-secondary">{evt.pool}</strong></span>}
                      {evt.backend && <span>backend: <strong className="text-foreground-secondary">{evt.backend}</strong></span>}
                      <span>type: {evt.type}</span>
                    </div>
                  </div>
                </div>

                <span className="text-xs font-mono text-foreground-muted shrink-0 tabular-nums">
                  {evt.timestamp}
                </span>
              </div>
            ))}

            {filtered.length === 0 && (
              <div className="py-16 text-center text-xs text-foreground-muted">
                No events match the selected filters
              </div>
            )}
          </div>
        </div>
      </main>
    </div>
  );
}

