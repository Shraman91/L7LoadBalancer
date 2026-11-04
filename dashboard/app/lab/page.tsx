"use client";

import React, { useState } from "react";
import { Header } from "@/components/layout/header";
import { Card, CardHeader, CardTitle, CardDescription } from "@/components/ui/card";
import { triggerTrafficLab } from "@/lib/api";
import { Play, RotateCcw, Zap, Terminal, CheckCircle2, AlertOctagon } from "lucide-react";
import { formatNumber } from "@/lib/utils";
import { useToast } from "@/components/ui/toast";

export default function TrafficLabPage() {
  const { toast } = useToast();

  const [path, setPath] = useState("/");
  const [method, setMethod] = useState("GET");
  const [rate, setRate] = useState(50);
  const [durationSecs, setDurationSecs] = useState(5);

  const [isRunning, setIsRunning] = useState(false);
  const [result, setResult] = useState<{
    totalSent: number;
    success2xx: number;
    failed5xx: number;
    rateLimited429: number;
    retried: number;
    p50Ms: number;
    p99Ms: number;
  } | null>(null);

  const handleStartTraffic = async () => {
    setIsRunning(true);
    setResult(null);

    toast({
      type: "info",
      title: "Traffic Load Triggered",
      description: `Sending ${rate} req/s to ${path} for ${durationSecs}s...`,
    });

    try {
      const res = await triggerTrafficLab({ path, method, rate, durationSecs });
      setResult(res);
      toast({
        type: "success",
        title: "Traffic Generation Completed",
        description: `Successfully processed ${res.totalSent} requests`,
      });
    } catch (err: unknown) {
      toast({
        type: "error",
        title: "Traffic Generation Error",
        description: err instanceof Error ? err.message : "Failed to run load test",
      });
    } finally {
      setIsRunning(false);
    }
  };

  return (
    <div className="flex-1 flex flex-col h-full overflow-hidden">
      <Header
        title="Traffic Lab"
        subtitle="Synthetic Traffic Injection & Chaos Simulation"
      />

      <main className="flex-1 overflow-y-auto p-8 max-w-[1400px] w-full mx-auto space-y-6">
        <div className="grid grid-cols-1 lg:grid-cols-3 gap-6">
          {/* Traffic Generator Controls */}
          <div className="lg:col-span-1 space-y-4">
            <Card>
              <CardHeader className="pb-3">
                <div className="flex items-center gap-2">
                  <Zap className="w-4 h-4 text-foreground" />
                  <CardTitle>Traffic Generator</CardTitle>
                </div>
                <CardDescription>
                  Inject HTTP traffic directly through proxy port :8080
                </CardDescription>
              </CardHeader>

              <div className="space-y-4 pt-2 text-xs">
                <div>
                  <label className="font-medium text-foreground block mb-1.5">HTTP Method</label>
                  <div className="grid grid-cols-4 gap-2">
                    {["GET", "POST", "PUT", "DELETE"].map((m) => (
                      <button
                        key={m}
                        type="button"
                        onClick={() => setMethod(m)}
                        className={`py-1.5 rounded-btn border text-xs font-mono font-medium transition-colors ${
                          method === m
                            ? "bg-foreground text-primary-foreground border-foreground"
                            : "bg-surface border-border text-foreground-secondary hover:text-foreground"
                        }`}
                      >
                        {m}
                      </button>
                    ))}
                  </div>
                </div>

                <div>
                  <label className="font-medium text-foreground block mb-1.5">Target Path</label>
                  <input
                    type="text"
                    value={path}
                    onChange={(e) => setPath(e.target.value)}
                    placeholder="/"
                    className="w-full px-3 py-1.5 rounded-btn border border-border bg-surface font-mono text-xs text-foreground focus:border-border-strong"
                  />
                  <span className="text-[11px] text-foreground-muted mt-1 block">
                    e.g. /, /api/v1/users, /user/profile
                  </span>
                </div>

                <div>
                  <div className="flex items-center justify-between mb-1.5">
                    <label className="font-medium text-foreground">Target Rate</label>
                    <span className="font-mono text-foreground font-bold">{rate} req/s</span>
                  </div>
                  <input
                    type="range"
                    min="1"
                    max="200"
                    value={rate}
                    onChange={(e) => setRate(parseInt(e.target.value, 10))}
                    className="w-full accent-foreground cursor-pointer"
                  />
                </div>

                <div>
                  <div className="flex items-center justify-between mb-1.5">
                    <label className="font-medium text-foreground">Duration</label>
                    <span className="font-mono text-foreground font-bold">{durationSecs}s</span>
                  </div>
                  <input
                    type="range"
                    min="1"
                    max="30"
                    value={durationSecs}
                    onChange={(e) => setDurationSecs(parseInt(e.target.value, 10))}
                    className="w-full accent-foreground cursor-pointer"
                  />
                </div>

                <div className="pt-2">
                  <button
                    onClick={handleStartTraffic}
                    disabled={isRunning}
                    className="w-full py-2.5 rounded-btn bg-foreground text-primary-foreground hover:bg-primary-hover font-medium text-xs flex items-center justify-center gap-2 transition-colors disabled:opacity-50"
                  >
                    {isRunning ? (
                      <>
                        <RotateCcw className="w-3.5 h-3.5 animate-spin" />
                        Generating Traffic...
                      </>
                    ) : (
                      <>
                        <Play className="w-3.5 h-3.5 fill-current" />
                        Execute Traffic Run
                      </>
                    )}
                  </button>
                </div>
              </div>
            </Card>

            {/* Run Results Card */}
            {result && (
              <Card className="animate-in fade-in duration-200">
                <CardHeader className="pb-3">
                  <CardTitle>Run Telemetry</CardTitle>
                  <CardDescription>Metrics captured from load run</CardDescription>
                </CardHeader>
                <div className="space-y-2 text-xs font-mono">
                  <div className="p-2.5 rounded bg-surface-subtle flex items-center justify-between">
                    <span className="text-foreground-secondary">Total Dispatched</span>
                    <span className="font-bold text-foreground">{formatNumber(result.totalSent)}</span>
                  </div>
                  <div className="p-2.5 rounded bg-[#F0FDF4] border border-[#BBF7D0] flex items-center justify-between text-[#15803D]">
                    <span>2xx Success</span>
                    <span className="font-bold">{formatNumber(result.success2xx)}</span>
                  </div>
                  <div className="p-2.5 rounded bg-[#FEF2F2] border border-[#FECACA] flex items-center justify-between text-[#B91C1C]">
                    <span>5xx Errors</span>
                    <span className="font-bold">{formatNumber(result.failed5xx)}</span>
                  </div>
                  {result.rateLimited429 > 0 && (
                    <div className="p-2.5 rounded bg-[#FFFBEB] border border-[#FDE68A] flex items-center justify-between text-[#B45309]">
                      <span>429 Rate Limited</span>
                      <span className="font-bold">{formatNumber(result.rateLimited429)}</span>
                    </div>
                  )}
                  <div className="p-2.5 rounded bg-surface-subtle flex items-center justify-between">
                    <span className="text-foreground-secondary">Retried Requests</span>
                    <span className="font-bold text-foreground">{formatNumber(result.retried)}</span>
                  </div>
                  <div className="p-2.5 rounded bg-surface-subtle flex items-center justify-between">
                    <span className="text-foreground-secondary">Latency p50 / p99</span>
                    <span className="font-bold text-foreground">
                      {result.p50Ms}ms / {result.p99Ms}ms
                    </span>
                  </div>
                </div>
              </Card>
            )}
          </div>

          {/* Interactive Chaos Demo Walkthrough Guide */}
          <div className="lg:col-span-2">
            <Card className="h-full flex flex-col justify-between">
              <div>
                <CardHeader className="pb-4">
                  <div className="flex items-center gap-2">
                    <Terminal className="w-4 h-4 text-foreground" />
                    <CardTitle>Operator Chaos Testing Guide</CardTitle>
                  </div>
                  <CardDescription>
                    Follow this walkthrough to witness circuit breakers, retries, and slow-start in action
                  </CardDescription>
                </CardHeader>

                <div className="space-y-6 text-xs leading-relaxed">
                  {/* Step 1 */}
                  <div className="flex items-start gap-3">
                    <div className="w-6 h-6 rounded-full bg-surface-subtle border border-border flex items-center justify-center font-mono font-bold text-foreground shrink-0 mt-0.5">
                      1
                    </div>
                    <div>
                      <h4 className="font-semibold text-foreground">Establish Baseline Load</h4>
                      <p className="text-foreground-secondary mt-1">
                        Use the generator panel on the left to start a 50 req/s load run against path{" "}
                        <code className="font-mono bg-surface-subtle px-1 py-0.5 rounded border border-border">/</code>{" "}
                        for 15 seconds. Observe throughput and steady-state latency in the Overview tab.
                      </p>
                    </div>
                  </div>

                  {/* Step 2 */}
                  <div className="flex items-start gap-3">
                    <div className="w-6 h-6 rounded-full bg-surface-subtle border border-border flex items-center justify-center font-mono font-bold text-foreground shrink-0 mt-0.5">
                      2
                    </div>
                    <div>
                      <h4 className="font-semibold text-foreground">Inject Upstream Chaos (Kill a Backend)</h4>
                      <p className="text-foreground-secondary mt-1">
                        In a terminal or via Docker, terminate backend 2 to simulate an unexpected crash:
                      </p>
                      <pre className="mt-2 p-2.5 rounded bg-surface-subtle border border-border font-mono text-[11px] text-foreground">
                        docker compose stop backend2
                      </pre>
                    </div>
                  </div>

                  {/* Step 3 */}
                  <div className="flex items-start gap-3">
                    <div className="w-6 h-6 rounded-full bg-surface-subtle border border-border flex items-center justify-center font-mono font-bold text-foreground shrink-0 mt-0.5">
                      3
                    </div>
                    <div>
                      <h4 className="font-semibold text-foreground">Observe Circuit Breaker & Retries</h4>
                      <p className="text-foreground-secondary mt-1">
                        Notice how the proxy retries idempotent requests against surviving nodes, increments the retry counter, and trips the circuit breaker to{" "}
                        <span className="font-mono px-1 py-0.5 rounded bg-[#FEF2F2] text-[#B91C1C] border border-[#FECACA]">
                          OPEN
                        </span>{" "}
                        after 3 consecutive failures.
                      </p>
                    </div>
                  </div>

                  {/* Step 4 */}
                  <div className="flex items-start gap-3">
                    <div className="w-6 h-6 rounded-full bg-surface-subtle border border-border flex items-center justify-center font-mono font-bold text-foreground shrink-0 mt-0.5">
                      4
                    </div>
                    <div>
                      <h4 className="font-semibold text-foreground">Observe Slow-Start Recovery</h4>
                      <p className="text-foreground-secondary mt-1">
                        Restart the backend container. After the 10-second cooldown, the breaker transitions to{" "}
                        <span className="font-mono px-1 py-0.5 rounded bg-[#FFFBEB] text-[#B45309] border border-[#FDE68A]">
                          HALF-OPEN
                        </span>
                        , admits trial requests, and ramps its traffic share smoothly over the 15-second slow-start window.
                      </p>
                      <pre className="mt-2 p-2.5 rounded bg-surface-subtle border border-border font-mono text-[11px] text-foreground">
                        docker compose start backend2
                      </pre>
                    </div>
                  </div>
                </div>
              </div>

              <div className="mt-6 p-4 rounded-card border border-border bg-surface-subtle text-xs text-foreground-muted flex items-center justify-between">
                <span>All proxy behaviors can also be verified with mock mode enabled</span>
                <span className="font-mono text-foreground font-medium">NEXT_PUBLIC_USE_MOCK=true</span>
              </div>
            </Card>
          </div>
        </div>
      </main>
    </div>
  );
}

