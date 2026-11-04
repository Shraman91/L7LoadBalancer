"use client";

import React, { useState } from "react";
import { RefreshCw } from "lucide-react";
import { cn } from "@/lib/utils";

interface HeaderProps {
  title: string;
  subtitle?: string;
  onRefresh?: () => void;
  isRefreshing?: boolean;
  lastUpdated?: Date | null;
}

export function Header({
  title,
  subtitle,
  onRefresh,
  isRefreshing = false,
  lastUpdated,
}: HeaderProps) {
  const [timeRange, setTimeRange] = useState<"5m" | "15m" | "1h">("5m");
  const [mounted, setMounted] = useState(false);

  React.useEffect(() => {
    setMounted(true);
  }, []);

  return (
    <header className="h-14 px-8 border-b border-border bg-surface flex items-center justify-between shrink-0 select-none">
      <div className="flex items-center gap-3">
        <h1 className="text-sm font-semibold tracking-tight text-foreground">{title}</h1>
        {subtitle && (
          <span className="text-xs text-foreground-muted hidden sm:inline">
            / {subtitle}
          </span>
        )}
      </div>

      <div className="flex items-center gap-3">
        {/* Live indicator */}
        <div className="flex items-center gap-1.5 px-2 py-1 rounded bg-[#F0FDF4] border border-[#BBF7D0] text-[11px] font-medium text-[#15803D]">
          <span className="w-1.5 h-1.5 rounded-full bg-[#15803D] animate-pulse" />
          <span>Live</span>
        </div>

        {/* Time range selector */}
        <div className="flex items-center p-0.5 rounded-btn bg-surface-subtle border border-border">
          {(["5m", "15m", "1h"] as const).map((r) => (
            <button
              key={r}
              onClick={() => setTimeRange(r)}
              className={cn(
                "px-2.5 py-1 text-xs font-mono font-medium rounded-sm transition-colors",
                timeRange === r
                  ? "bg-surface text-foreground shadow-sm"
                  : "text-foreground-secondary hover:text-foreground"
              )}
            >
              {r}
            </button>
          ))}
        </div>

        {/* Refresh button */}
        <button
          onClick={onRefresh}
          disabled={isRefreshing}
          title="Manual poll refresh"
          className="p-1.5 rounded-btn border border-border text-foreground-secondary hover:text-foreground hover:bg-surface-subtle transition-colors disabled:opacity-50"
        >
          <RefreshCw className={cn("w-3.5 h-3.5", isRefreshing && "animate-spin text-foreground")} />
        </button>

        {/* Timestamp */}
        {mounted && lastUpdated && (
          <span
            suppressHydrationWarning
            className="text-[11px] text-foreground-muted font-mono hidden md:inline"
          >
            Updated {lastUpdated.toLocaleTimeString("en-US", { hour12: false })}
          </span>
        )}
      </div>
    </header>
  );
}

