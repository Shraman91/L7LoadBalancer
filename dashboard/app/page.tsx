"use client";

import React, { useState } from "react";
import { useQuery } from "@tanstack/react-query";
import { fetchBackends, fetchEvents, fetchMetrics } from "@/lib/api";
import { Header } from "@/components/layout/header";
import { KpiCard } from "@/components/overview/kpi-card";
import { TrafficChart } from "@/components/overview/traffic-chart";
import { LatencyChart } from "@/components/overview/latency-chart";
import { ProtectionPanel } from "@/components/overview/protection-panel";
import { RecentEventsList } from "@/components/overview/recent-events";
import { formatNumber } from "@/lib/utils";

export default function OverviewPage() {
  const [lastRefreshed, setLastRefreshed] = useState<Date>(new Date());

  const {
    data: metrics,
    isLoading: metricsLoading,
    isError: metricsError,
    refetch: refetchMetrics,
    isRefetching,
  } = useQuery({
    queryKey: ["metrics"],
    queryFn: async () => {
      const res = await fetchMetrics();
      setLastRefreshed(new Date());
      return res;
    },
  });

  const { data: backends } = useQuery({
    queryKey: ["backends"],
    queryFn: fetchBackends,
  });

  const { data: events } = useQuery({
    queryKey: ["events"],
    queryFn: fetchEvents,
  });

  const totalBackends = backends?.length || 0;
  const healthyBackends = backends?.filter((b) => b.healthy && !b.draining).length || 0;

  const handleManualRefresh = () => {
    refetchMetrics();
  };

  return (
    <div className="flex-1 flex flex-col h-full overflow-hidden">
      <Header
        title="Fleet Overview"
        subtitle="Global Traffic & Health"
        onRefresh={handleManualRefresh}
        isRefreshing={isRefetching}
        lastUpdated={lastRefreshed}
      />

      <main className="flex-1 overflow-y-auto p-8 max-w-[1400px] w-full mx-auto space-y-6">
        {/* KPI Grid */}
        <section className="grid grid-cols-2 md:grid-cols-3 lg:grid-cols-6 gap-3">
          <KpiCard
            label="Throughput"
            value={formatNumber(metrics?.requestsPerSec || 0)}
            unit="req/s"
            delta="+3.8%"
            deltaType="positive"
            sparklineData={[180, 192, 185, 204, 210, 195, 220, 215]}
            subtext="vs 5m window"
          />
          <KpiCard
            label="5xx Error Rate"
            value={(metrics?.errorRate5xxPct || 0).toFixed(2)}
            unit="%"
            delta={(metrics?.errorRate5xxPct || 0) > 1 ? "+0.4%" : "-0.1%"}
            deltaType={(metrics?.errorRate5xxPct || 0) > 1 ? "negative" : "positive"}
            sparklineData={[0.2, 0.4, 0.1, 0.8, 0.3, 0.5, 0.2]}
            subtext="upstream errors"
          />
          <KpiCard
            label="p50 Latency"
            value={(metrics?.p50LatencyMs || 0).toFixed(2)}
            unit="ms"
            delta="-0.04 ms"
            deltaType="positive"
            sparklineData={[0.75, 0.72, 0.70, 0.69, 0.68, 0.71]}
            subtext="median response"
          />
          <KpiCard
            label="p99 Latency"
            value={(metrics?.p99LatencyMs || 0).toFixed(2)}
            unit="ms"
            delta="+0.12 ms"
            deltaType="negative"
            sparklineData={[1.8, 1.9, 1.85, 2.1, 2.05, 2.2]}
            subtext="tail response"
          />
          <KpiCard
            label="Active Connections"
            value={formatNumber(metrics?.totalActiveConnections || 0)}
            delta="In-flight"
            deltaType="neutral"
            sparklineData={[80, 85, 82, 91, 88, 93]}
            subtext="pooled sockets"
          />
          <KpiCard
            label="Backend Fleet"
            value={`${healthyBackends}/${totalBackends}`}
            delta={healthyBackends === totalBackends ? "100% UP" : "DEGRADED"}
            deltaType={healthyBackends === totalBackends ? "positive" : "negative"}
            sparklineData={[7, 7, 7, 6, 6, 7]}
            subtext="healthy nodes"
          />
        </section>

        {/* Primary Traffic & Latency Visualizations */}
        <section className="grid grid-cols-1 lg:grid-cols-2 gap-6">
          <TrafficChart data={metrics?.trafficTimeline || []} />
          <LatencyChart data={metrics?.latencyTimeline || []} />
        </section>

        {/* Protection Panel and Audit Feed */}
        <section className="grid grid-cols-1 lg:grid-cols-3 gap-6">
          <div className="lg:col-span-2">
            <ProtectionPanel
              rateLimitedTotal={metrics?.rateLimitedTotal || 0}
              loadShedTotal={metrics?.loadShedTotal || 0}
              retriesTotal={metrics?.retriesTotal || 0}
            />
          </div>
          <div className="lg:col-span-1">
            <RecentEventsList events={events || []} />
          </div>
        </section>
      </main>
    </div>
  );
}

