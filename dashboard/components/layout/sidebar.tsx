"use client";

import Link from "next/link";
import { usePathname } from "next/navigation";
import {
  LayoutDashboard,
  Server,
  Network,
  Sliders,
  Activity,
  Zap,
  Settings,
  Circle,
} from "lucide-react";
import { cn } from "@/lib/utils";
import { isMockMode } from "@/lib/api";
import { useEffect, useState } from "react";

const NAV_ITEMS = [
  { href: "/", label: "Overview", icon: LayoutDashboard },
  { href: "/backends", label: "Backends", icon: Server },
  { href: "/topology", label: "Topology", icon: Network },
  { href: "/config", label: "Routes & Config", icon: Sliders },
  { href: "/events", label: "Events", icon: Activity },
  { href: "/lab", label: "Traffic Lab", icon: Zap },
  { href: "/settings", label: "Settings", icon: Settings },
];

export function Sidebar() {
  const pathname = usePathname();
  const [mockActive, setMockActive] = useState(false);

  useEffect(() => {
    setMockActive(isMockMode());
  }, []);

  return (
    <aside className="w-60 bg-surface border-r border-border h-screen flex flex-col justify-between shrink-0 select-none">
      <div>
        {/* Brand header */}
        <div className="h-14 px-5 flex items-center gap-2.5 border-b border-border">
          <div className="w-5 h-5 rounded-btn bg-foreground flex items-center justify-center text-primary-foreground font-mono text-xs font-bold tracking-tighter">
            L7
          </div>
          <div className="flex items-center gap-2">
            <span className="font-mono font-bold tracking-tight text-foreground text-sm">
              l7lb
            </span>
            <span className="text-[10px] font-mono font-medium px-1.5 py-0.5 rounded bg-surface-subtle text-foreground-secondary border border-border">
              v0.5.0
            </span>
          </div>
        </div>

        {/* Navigation list */}
        <nav className="p-3 space-y-1">
          {NAV_ITEMS.map((item) => {
            const Icon = item.icon;
            const active =
              item.href === "/"
                ? pathname === "/"
                : pathname === item.href || pathname.startsWith(`${item.href}/`);

            return (
              <Link
                key={item.href}
                href={item.href}
                className={cn(
                  "flex items-center gap-3 px-3 py-2 rounded-btn text-xs font-medium transition-colors duration-150",
                  active
                    ? "bg-foreground text-primary-foreground shadow-sm"
                    : "text-foreground-secondary hover:text-foreground hover:bg-surface-subtle"
                )}
              >
                <Icon className={cn("w-4 h-4 shrink-0", active ? "text-primary-foreground" : "text-foreground-muted")} />
                <span>{item.label}</span>
              </Link>
            );
          })}
        </nav>
      </div>

      {/* Footer system status */}
      <div className="p-3 m-3 border border-border rounded-card bg-surface-subtle">
        <div className="flex items-center justify-between text-xs mb-1.5">
          <span className="text-foreground-secondary text-[11px] font-medium">Status</span>
          <span className="flex items-center gap-1 text-[11px] font-mono text-foreground">
            <Circle className="w-2 h-2 fill-[#15803D] text-[#15803D]" />
            {mockActive ? "Mock Mode" : "Online"}
          </span>
        </div>
        <div className="flex items-center justify-between text-[11px] text-foreground-muted font-mono">
          <span>Admin</span>
          <span>127.0.0.1:8081</span>
        </div>
      </div>
    </aside>
  );
}

