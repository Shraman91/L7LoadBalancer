import React from "react";
import { Card, CardHeader, CardTitle, CardDescription } from "@/components/ui/card";
import { ShieldCheck, ArrowUpRight } from "lucide-react";
import { formatNumber } from "@/lib/utils";

interface ProtectionPanelProps {
  rateLimitedTotal: number;
  loadShedTotal: number;
  retriesTotal: number;
}

export function ProtectionPanel({
  rateLimitedTotal,
  loadShedTotal,
  retriesTotal,
}: ProtectionPanelProps) {
  const items = [
    {
      label: "Rate-Limited Requests",
      description: "HTTP 429 emitted via client IP token-bucket",
      count: rateLimitedTotal,
      subtext: "Capacity: 100 req/s, Burst: 150",
    },
    {
      label: "Load-Shed Requests",
      description: "HTTP 503 emitted via global concurrency CAS",
      count: loadShedTotal,
      subtext: "Max concurrency threshold: 1,000",
    },
    {
      label: "Upstream Retries",
      description: "Idempotent requests retried on 5xx / timeout",
      count: retriesTotal,
      subtext: "Budget ratio capped at 20%",
    },
  ];

  return (
    <Card className="flex flex-col justify-between">
      <CardHeader className="pb-3">
        <div className="flex items-center gap-2">
          <ShieldCheck className="w-4 h-4 text-foreground" />
          <CardTitle>Protection & Mitigation</CardTitle>
        </div>
        <CardDescription>
          Active resilience mechanisms mitigating downstream and upstream distress
        </CardDescription>
      </CardHeader>

      <div className="grid grid-cols-1 md:grid-cols-3 gap-3 pt-2">
        {items.map((item, idx) => (
          <div
            key={idx}
            className="p-3.5 rounded-card border border-border bg-surface-subtle flex flex-col justify-between"
          >
            <div>
              <div className="flex items-center justify-between text-xs font-medium text-foreground">
                <span>{item.label}</span>
                <ArrowUpRight className="w-3.5 h-3.5 text-foreground-muted" />
              </div>
              <p className="text-[11px] text-foreground-muted mt-0.5">{item.description}</p>
            </div>

            <div className="mt-3">
              <span className="text-xl font-bold font-mono tabular-nums text-foreground">
                {formatNumber(item.count)}
              </span>
              <p className="text-[10px] text-foreground-secondary font-mono mt-0.5">
                {item.subtext}
              </p>
            </div>
          </div>
        ))}
      </div>
    </Card>
  );
}

