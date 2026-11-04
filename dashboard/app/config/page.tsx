"use client";

import React, { useState } from "react";
import { useQuery } from "@tanstack/react-query";
import { fetchConfig } from "@/lib/api";
import { Header } from "@/components/layout/header";
import { Card, CardHeader, CardTitle, CardDescription } from "@/components/ui/card";
import { Copy, Check, Code, FileText, ChevronDown, ChevronRight } from "lucide-react";
import { useToast } from "@/components/ui/toast";

export default function ConfigPage() {
  const { toast } = useToast();
  const [viewMode, setViewMode] = useState<"structured" | "raw">("structured");
  const [rawFormat, setRawFormat] = useState<"json" | "toml">("json");
  const [copied, setCopied] = useState(false);

  const [expandedSections, setExpandedSections] = useState<Record<string, boolean>>({
    routes: true,
    pools: true,
    protection: true,
    timeouts: true,
    tls: true,
  });

  const { data: config, isLoading, refetch, isRefetching } = useQuery({
    queryKey: ["config"],
    queryFn: fetchConfig,
  });

  const toggleSection = (s: string) => {
    setExpandedSections((prev) => ({ ...prev, [s]: !prev[s] }));
  };

  const copyToClipboard = (text: string) => {
    navigator.clipboard.writeText(text);
    setCopied(true);
    toast({
      type: "success",
      title: "Config Copied",
      description: "Active configuration copied to clipboard",
    });
    setTimeout(() => setCopied(false), 2000);
  };

  const rawJson = config ? JSON.stringify(config, null, 2) : "{}";

  // Simple TOML serialization preview
  const rawToml = config
    ? `listen_addr = "${config.listen_addr}"
default_pool = "${config.default_pool}"

[admin]
enabled = ${config.admin.enabled}
addr = "${config.admin.addr}"

[timeouts]
connect_secs = ${config.timeouts.connect_secs}
upstream_response_header_secs = ${config.timeouts.upstream_response_header_secs}
client_header_read_secs = ${config.timeouts.client_header_read_secs}
idle_keepalive_secs = ${config.timeouts.idle_keepalive_secs}

[limits]
max_request_header_size_bytes = ${config.limits.max_request_header_size_bytes}
max_body_size_bytes = ${config.limits.max_body_size_bytes}
max_concurrent_connections = ${config.limits.max_concurrent_connections}

[rate_limit]
enabled = ${config.rate_limit.enabled}
requests_per_second = ${config.rate_limit.requests_per_second}
burst = ${config.rate_limit.burst}

[circuit_breaker]
failure_threshold = ${config.circuit_breaker.failure_threshold}
cooldown_secs = ${config.circuit_breaker.cooldown_secs}
half_open_max_trials = ${config.circuit_breaker.half_open_max_trials}

[retry]
enabled = ${config.retry.enabled}
max_attempts = ${config.retry.max_attempts}
budget_ratio = ${config.retry.budget_ratio}

[slow_start]
enabled = ${config.slow_start.enabled}
window_secs = ${config.slow_start.window_secs}
`
    : "";

  return (
    <div className="flex-1 flex flex-col h-full overflow-hidden">
      <Header
        title="Routing & Configuration"
        subtitle="Active Reverse Proxy Parameters"
        onRefresh={() => refetch()}
        isRefreshing={isRefetching}
      />

      <main className="flex-1 overflow-y-auto p-8 max-w-[1400px] w-full mx-auto space-y-6">
        {/* Controls Bar */}
        <div className="flex items-center justify-between">
          <div className="flex items-center gap-2 p-0.5 rounded-btn bg-surface-subtle border border-border">
            <button
              onClick={() => setViewMode("structured")}
              className={`flex items-center gap-1.5 px-3 py-1.5 rounded-sm text-xs font-medium transition-colors ${
                viewMode === "structured"
                  ? "bg-surface text-foreground shadow-sm"
                  : "text-foreground-secondary hover:text-foreground"
              }`}
            >
              <FileText className="w-3.5 h-3.5" />
              Structured
            </button>
            <button
              onClick={() => setViewMode("raw")}
              className={`flex items-center gap-1.5 px-3 py-1.5 rounded-sm text-xs font-medium transition-colors ${
                viewMode === "raw"
                  ? "bg-surface text-foreground shadow-sm"
                  : "text-foreground-secondary hover:text-foreground"
              }`}
            >
              <Code className="w-3.5 h-3.5" />
              Raw Source
            </button>
          </div>

          <div className="flex items-center gap-2">
            {viewMode === "raw" && (
              <div className="flex items-center p-0.5 rounded-btn bg-surface-subtle border border-border text-xs font-mono">
                <button
                  onClick={() => setRawFormat("json")}
                  className={`px-2 py-1 rounded-sm ${
                    rawFormat === "json"
                      ? "bg-surface text-foreground shadow-sm"
                      : "text-foreground-secondary"
                  }`}
                >
                  JSON
                </button>
                <button
                  onClick={() => setRawFormat("toml")}
                  className={`px-2 py-1 rounded-sm ${
                    rawFormat === "toml"
                      ? "bg-surface text-foreground shadow-sm"
                      : "text-foreground-secondary"
                  }`}
                >
                  TOML
                </button>
              </div>
            )}

            <button
              onClick={() => copyToClipboard(rawFormat === "toml" ? rawToml : rawJson)}
              className="flex items-center gap-1.5 px-3 py-1.5 rounded-btn border border-border bg-surface hover:bg-surface-subtle text-xs font-medium text-foreground transition-colors"
            >
              {copied ? <Check className="w-3.5 h-3.5 text-[#15803D]" /> : <Copy className="w-3.5 h-3.5" />}
              <span>Copy</span>
            </button>
          </div>
        </div>

        {/* Structured Mode */}
        {viewMode === "structured" ? (
          <div className="space-y-4">
            {/* 1. Routes */}
            <Card className="p-0 overflow-hidden">
              <div
                onClick={() => toggleSection("routes")}
                className="p-4 flex items-center justify-between cursor-pointer hover:bg-surface-subtle select-none border-b border-border"
              >
                <div>
                  <h3 className="text-sm font-semibold text-foreground">Content-Based Routes</h3>
                  <p className="text-xs text-foreground-muted">
                    Exact Host matching and longest path-prefix routing rules
                  </p>
                </div>
                {expandedSections.routes ? (
                  <ChevronDown className="w-4 h-4 text-foreground-muted" />
                ) : (
                  <ChevronRight className="w-4 h-4 text-foreground-muted" />
                )}
              </div>

              {expandedSections.routes && (
                <div className="divide-y divide-border">
                  {config?.routes?.map((r, idx) => (
                    <div key={idx} className="p-4 flex items-center justify-between text-xs font-mono">
                      <div>
                        <span className="font-bold text-foreground">{r.path_prefix}</span>
                        <span className="text-foreground-muted ml-3">
                          {r.host ? `host: ${r.host}` : "host: * (all)"}
                        </span>
                      </div>
                      <div className="flex items-center gap-2">
                        {r.strip_prefix && (
                          <span className="px-1.5 py-0.5 rounded bg-surface-subtle border border-border text-[11px] text-foreground-secondary">
                            strip_prefix
                          </span>
                        )}
                        <span className="px-2 py-0.5 rounded bg-surface-subtle border border-border text-[11px] text-foreground font-semibold">
                          pool: {r.pool}
                        </span>
                      </div>
                    </div>
                  ))}
                </div>
              )}
            </Card>

            {/* 2. Pools */}
            <Card className="p-0 overflow-hidden">
              <div
                onClick={() => toggleSection("pools")}
                className="p-4 flex items-center justify-between cursor-pointer hover:bg-surface-subtle select-none border-b border-border"
              >
                <div>
                  <h3 className="text-sm font-semibold text-foreground">Backend Pools</h3>
                  <p className="text-xs text-foreground-muted">
                    Algorithms, active health probe parameters, and hash keys
                  </p>
                </div>
                {expandedSections.pools ? (
                  <ChevronDown className="w-4 h-4 text-foreground-muted" />
                ) : (
                  <ChevronRight className="w-4 h-4 text-foreground-muted" />
                )}
              </div>

              {expandedSections.pools && (
                <div className="p-4 grid grid-cols-1 md:grid-cols-3 gap-4">
                  {Object.entries(config?.pools || {}).map(([name, pool]) => (
                    <div key={name} className="p-4 rounded-card border border-border bg-surface-subtle space-y-2">
                      <div className="flex items-center justify-between">
                        <span className="font-bold text-xs uppercase tracking-wide text-foreground">
                          {name}
                        </span>
                        <span className="text-[10px] font-mono px-1.5 py-0.5 rounded bg-surface border border-border text-foreground-secondary">
                          {pool.backends.length} nodes
                        </span>
                      </div>
                      <div className="text-xs font-mono space-y-1 pt-1 text-foreground-secondary">
                        <p>algo: <span className="text-foreground font-medium">{pool.algorithm}</span></p>
                        {pool.hash_key && <p>hash_key: <span className="text-foreground">{pool.hash_key}</span></p>}
                        {pool.health_check && (
                          <p>
                            probe: <span className="text-foreground">{pool.health_check.path}</span> (every {pool.health_check.interval_secs}s)
                          </p>
                        )}
                      </div>
                    </div>
                  ))}
                </div>
              )}
            </Card>

            {/* 3. Protection & Resilience */}
            <Card className="p-0 overflow-hidden">
              <div
                onClick={() => toggleSection("protection")}
                className="p-4 flex items-center justify-between cursor-pointer hover:bg-surface-subtle select-none border-b border-border"
              >
                <div>
                  <h3 className="text-sm font-semibold text-foreground">Traffic Protection & Circuit Breakers</h3>
                  <p className="text-xs text-foreground-muted">
                    Rate limiting, concurrency limits, circuit breaker cooldowns, and retries
                  </p>
                </div>
                {expandedSections.protection ? (
                  <ChevronDown className="w-4 h-4 text-foreground-muted" />
                ) : (
                  <ChevronRight className="w-4 h-4 text-foreground-muted" />
                )}
              </div>

              {expandedSections.protection && (
                <div className="p-4 grid grid-cols-1 md:grid-cols-3 gap-4 text-xs font-mono">
                  <div className="p-3.5 rounded-card border border-border bg-surface-subtle space-y-1.5">
                    <span className="font-semibold text-foreground">Rate Limiter</span>
                    <p className="text-foreground-secondary">RPS: {config?.rate_limit.requests_per_second}</p>
                    <p className="text-foreground-secondary">Burst: {config?.rate_limit.burst}</p>
                    <p className="text-foreground-secondary">Max Buckets: {config?.rate_limit.max_entries}</p>
                  </div>
                  <div className="p-3.5 rounded-card border border-border bg-surface-subtle space-y-1.5">
                    <span className="font-semibold text-foreground">Circuit Breaker</span>
                    <p className="text-foreground-secondary">Failure Threshold: {config?.circuit_breaker.failure_threshold}</p>
                    <p className="text-foreground-secondary">Cooldown: {config?.circuit_breaker.cooldown_secs}s</p>
                    <p className="text-foreground-secondary">Half-Open Trials: {config?.circuit_breaker.half_open_max_trials}</p>
                  </div>
                  <div className="p-3.5 rounded-card border border-border bg-surface-subtle space-y-1.5">
                    <span className="font-semibold text-foreground">Idempotent Retries</span>
                    <p className="text-foreground-secondary">Max Attempts: {config?.retry.max_attempts}</p>
                    <p className="text-foreground-secondary">
                      Budget Ratio: {((config?.retry?.budget_ratio ?? 0) * 100).toFixed(0)}%
                    </p>
                    <p className="text-foreground-secondary">Backoff: {config?.retry.initial_backoff_ms} - {config?.retry.max_backoff_ms}ms</p>
                  </div>
                </div>
              )}
            </Card>

            {/* 4. Timeouts & Limits */}
            <Card className="p-0 overflow-hidden">
              <div
                onClick={() => toggleSection("timeouts")}
                className="p-4 flex items-center justify-between cursor-pointer hover:bg-surface-subtle select-none border-b border-border"
              >
                <div>
                  <h3 className="text-sm font-semibold text-foreground">Timeouts & Frame Limits</h3>
                  <p className="text-xs text-foreground-muted">
                    Socket timeouts, header sizes, and payload limits
                  </p>
                </div>
                {expandedSections.timeouts ? (
                  <ChevronDown className="w-4 h-4 text-foreground-muted" />
                ) : (
                  <ChevronRight className="w-4 h-4 text-foreground-muted" />
                )}
              </div>

              {expandedSections.timeouts && (
                <div className="p-4 grid grid-cols-2 md:grid-cols-4 gap-4 text-xs font-mono">
                  <div className="p-3 rounded-card border border-border bg-surface-subtle">
                    <span className="text-foreground-muted">Connect Timeout</span>
                    <p className="text-foreground font-bold mt-1">{config?.timeouts.connect_secs}s</p>
                  </div>
                  <div className="p-3 rounded-card border border-border bg-surface-subtle">
                    <span className="text-foreground-muted">Upstream Header</span>
                    <p className="text-foreground font-bold mt-1">{config?.timeouts.upstream_response_header_secs}s</p>
                  </div>
                  <div className="p-3 rounded-card border border-border bg-surface-subtle">
                    <span className="text-foreground-muted">Max Concurrency</span>
                    <p className="text-foreground font-bold mt-1">{config?.limits.max_concurrent_connections}</p>
                  </div>
                  <div className="p-3 rounded-card border border-border bg-surface-subtle">
                    <span className="text-foreground-muted">Max Body Size</span>
                    <p className="text-foreground font-bold mt-1">
                      {((config?.limits.max_body_size_bytes || 0) / (1024 * 1024)).toFixed(0)} MB
                    </p>
                  </div>
                </div>
              )}
            </Card>
          </div>
        ) : (
          /* Raw View Mode */
          <Card className="p-0 overflow-hidden font-mono text-xs">
            <pre className="p-6 bg-surface-subtle text-foreground overflow-x-auto leading-relaxed">
              {rawFormat === "toml" ? rawToml : rawJson}
            </pre>
          </Card>
        )}
      </main>
    </div>
  );
}

