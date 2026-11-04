import React from "react";
import { Card } from "@/components/ui/card";
import { cn } from "@/lib/utils";

interface KpiCardProps {
  label: string;
  value: string | number;
  unit?: string;
  delta?: string;
  deltaType?: "positive" | "negative" | "neutral";
  sparklineData?: number[];
  subtext?: string;
}

export function KpiCard({
  label,
  value,
  unit,
  delta,
  deltaType = "neutral",
  sparklineData = [12, 14, 13, 15, 18, 17, 19, 21, 20, 22],
  subtext,
}: KpiCardProps) {
  // Simple monochrome SVG sparkline
  const min = Math.min(...sparklineData);
  const max = Math.max(...sparklineData);
  const range = max - min || 1;
  const width = 64;
  const height = 24;

  const points = sparklineData
    .map((val, idx) => {
      const x = (idx / (sparklineData.length - 1)) * width;
      const y = height - ((val - min) / range) * (height - 4) - 2;
      return `${x},${y}`;
    })
    .join(" ");

  return (
    <Card className="p-4 flex flex-col justify-between">
      <div className="flex items-center justify-between">
        <span className="text-xs font-medium text-foreground-secondary">{label}</span>
        {/* Sparkline */}
        <svg width={width} height={height} className="overflow-visible">
          <polyline
            fill="none"
            stroke="#71717A"
            strokeWidth="1.5"
            strokeLinecap="round"
            strokeLinejoin="round"
            points={points}
          />
        </svg>
      </div>

      <div className="mt-2 flex items-baseline gap-1">
        <span className="text-2xl font-bold font-mono tracking-tight tabular-nums text-foreground">
          {value}
        </span>
        {unit && <span className="text-xs font-mono text-foreground-muted">{unit}</span>}
      </div>

      <div className="mt-2 flex items-center justify-between text-[11px]">
        {delta && (
          <span
            className={cn(
              "font-mono font-medium",
              deltaType === "positive" && "text-[#15803D]",
              deltaType === "negative" && "text-[#B91C1C]",
              deltaType === "neutral" && "text-foreground-secondary"
            )}
          >
            {delta}
          </span>
        )}
        {subtext && <span className="text-foreground-muted">{subtext}</span>}
      </div>
    </Card>
  );
}

