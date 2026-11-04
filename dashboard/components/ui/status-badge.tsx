import { cn } from "@/lib/utils";
import React from "react";

export type StatusType = "healthy" | "degraded" | "unhealthy" | "draining";

interface StatusBadgeProps {
  status: StatusType;
  label?: string;
  className?: string;
}

export function StatusBadge({ status, label, className }: StatusBadgeProps) {
  const configs: Record<
    StatusType,
    { dot: string; bg: string; text: string; border: string; defaultLabel: string }
  > = {
    healthy: {
      dot: "bg-[#15803D]",
      bg: "bg-[#F0FDF4]",
      text: "text-[#15803D]",
      border: "border-[#BBF7D0]",
      defaultLabel: "Healthy",
    },
    degraded: {
      dot: "bg-[#B45309]",
      bg: "bg-[#FFFBEB]",
      text: "text-[#B45309]",
      border: "border-[#FDE68A]",
      defaultLabel: "Degraded",
    },
    unhealthy: {
      dot: "bg-[#B91C1C]",
      bg: "bg-[#FEF2F2]",
      text: "text-[#B91C1C]",
      border: "border-[#FECACA]",
      defaultLabel: "Unhealthy",
    },
    draining: {
      dot: "bg-[#52525B]",
      bg: "bg-[#F4F4F5]",
      text: "text-[#52525B]",
      border: "border-[#E4E4E7]",
      defaultLabel: "Draining",
    },
  };

  const config = configs[status];

  return (
    <span
      className={cn(
        "inline-flex items-center gap-1.5 px-2.5 py-0.5 text-xs font-medium rounded-full border",
        config.bg,
        config.text,
        config.border,
        className
      )}
    >
      <span className={cn("w-1.5 h-1.5 rounded-full shrink-0", config.dot)} />
      {label || config.defaultLabel}
    </span>
  );
}

export function CircuitBadge({ state }: { state: "closed" | "open" | "half_open" | string }) {
  if (state === "closed") {
    return (
      <span className="inline-flex items-center gap-1 px-2 py-0.5 text-xs font-mono font-medium rounded-full bg-[#F0FDF4] text-[#15803D] border border-[#BBF7D0]">
        <span className="w-1.5 h-1.5 rounded-full bg-[#15803D]" />
        closed
      </span>
    );
  }
  if (state === "half_open") {
    return (
      <span className="inline-flex items-center gap-1 px-2 py-0.5 text-xs font-mono font-medium rounded-full bg-[#FFFBEB] text-[#B45309] border border-[#FDE68A]">
        <span className="w-1.5 h-1.5 rounded-full bg-[#B45309]" />
        half_open
      </span>
    );
  }
  return (
    <span className="inline-flex items-center gap-1 px-2 py-0.5 text-xs font-mono font-medium rounded-full bg-[#FEF2F2] text-[#B91C1C] border border-[#FECACA]">
      <span className="w-1.5 h-1.5 rounded-full bg-[#B91C1C]" />
      open
    </span>
  );
}

