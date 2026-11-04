import React from "react";
import { Card, CardHeader, CardTitle, CardDescription } from "@/components/ui/card";
import { LogEvent } from "@/lib/types";
import { cn } from "@/lib/utils";
import Link from "next/link";
import { ChevronRight } from "lucide-react";

export function RecentEventsList({ events }: { events: LogEvent[] }) {
  const displayEvents = events.slice(0, 5);

  return (
    <Card className="flex flex-col">
      <CardHeader className="flex flex-row items-center justify-between space-y-0 pb-3">
        <div>
          <CardTitle>Recent Audit & State Events</CardTitle>
          <CardDescription>Real-time circuit transitions and health check state shifts</CardDescription>
        </div>
        <Link
          href="/events"
          className="text-xs font-medium text-foreground-secondary hover:text-foreground flex items-center gap-1 transition-colors"
        >
          View all
          <ChevronRight className="w-3.5 h-3.5" />
        </Link>
      </CardHeader>

      <div className="divide-y divide-border pt-1">
        {displayEvents.map((evt) => (
          <div key={evt.id} className="py-2.5 flex items-start justify-between gap-4 text-xs">
            <div className="flex items-start gap-3 min-w-0">
              <span
                className={cn(
                  "px-2 py-0.5 rounded-full text-[10px] font-mono font-medium border shrink-0",
                  evt.severity === "critical" && "bg-[#FEF2F2] text-[#B91C1C] border-[#FECACA]",
                  evt.severity === "warning" && "bg-[#FFFBEB] text-[#B45309] border-[#FDE68A]",
                  evt.severity === "info" && "bg-[#F0FDF4] text-[#15803D] border-[#BBF7D0]"
                )}
              >
                {evt.severity.toUpperCase()}
              </span>

              <div className="min-w-0">
                <p className="text-foreground font-medium truncate">{evt.message}</p>
                <div className="flex items-center gap-2 text-[11px] text-foreground-muted font-mono mt-0.5">
                  {evt.pool && <span>pool: {evt.pool}</span>}
                  {evt.backend && <span>node: {evt.backend}</span>}
                </div>
              </div>
            </div>

            <span className="text-[11px] font-mono text-foreground-muted shrink-0">
              {evt.timestamp}
            </span>
          </div>
        ))}

        {displayEvents.length === 0 && (
          <div className="py-6 text-center text-xs text-foreground-muted">
            No recent state events recorded
          </div>
        )}
      </div>
    </Card>
  );
}

