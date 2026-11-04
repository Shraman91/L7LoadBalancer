"use client";

import React, { useState, useEffect } from "react";
import { Header } from "@/components/layout/header";
import { Card, CardHeader, CardTitle, CardDescription } from "@/components/ui/card";
import { isMockMode, setMockMode } from "@/lib/api";
import { Settings as SettingsIcon, Server, Shield, SlidersHorizontal, RefreshCw } from "lucide-react";
import { useToast } from "@/components/ui/toast";

export default function SettingsPage() {
  const { toast } = useToast();

  const [mockActive, setMockActive] = useState(false);
  const [adminUrl, setAdminUrl] = useState("http://127.0.0.1:8081");
  const [pollInterval, setPollInterval] = useState("2000");
  const [density, setDensity] = useState("comfortable");

  useEffect(() => {
    setMockActive(isMockMode());
  }, []);

  const handleToggleMock = (e: React.ChangeEvent<HTMLInputElement>) => {
    const val = e.target.checked;
    setMockActive(val);
    setMockMode(val);
  };

  const handleSaveSettings = (e: React.FormEvent) => {
    e.preventDefault();
    toast({
      type: "success",
      title: "Settings Saved",
      description: "Preferences and connection parameters updated",
    });
  };

  return (
    <div className="flex-1 flex flex-col h-full overflow-hidden">
      <Header
        title="Settings"
        subtitle="Control Plane Configuration & Environment"
      />

      <main className="flex-1 overflow-y-auto p-8 max-w-[1000px] w-full mx-auto space-y-6">
        <form onSubmit={handleSaveSettings} className="space-y-6">
          {/* Connection Parameters */}
          <Card>
            <CardHeader className="pb-3">
              <div className="flex items-center gap-2">
                <Server className="w-4 h-4 text-foreground" />
                <CardTitle>Admin API Connection</CardTitle>
              </div>
              <CardDescription>
                Server-side target URL for proxying administrative control requests
              </CardDescription>
            </CardHeader>

            <div className="space-y-4 pt-2 text-xs">
              <div>
                <label className="font-medium text-foreground block mb-1.5">Admin Base URL</label>
                <input
                  type="text"
                  value={adminUrl}
                  onChange={(e) => setAdminUrl(e.target.value)}
                  className="w-full px-3 py-1.5 rounded-btn border border-border bg-surface font-mono text-xs text-foreground focus:border-border-strong"
                />
                <span className="text-[11px] text-foreground-muted mt-1 block">
                  Configurable via server environment variable: <code>L7LB_ADMIN_URL</code>
                </span>
              </div>

              <div>
                <label className="font-medium text-foreground block mb-1.5">Bearer Token</label>
                <input
                  type="password"
                  placeholder="Configured via L7LB_ADMIN_TOKEN"
                  disabled
                  className="w-full px-3 py-1.5 rounded-btn border border-border bg-surface-subtle font-mono text-xs text-foreground-muted cursor-not-allowed"
                />
                <span className="text-[11px] text-foreground-muted mt-1 block">
                  Secrets are never passed to the browser; read from server environment.
                </span>
              </div>
            </div>
          </Card>

          {/* Polling & Density */}
          <Card>
            <CardHeader className="pb-3">
              <div className="flex items-center gap-2">
                <SlidersHorizontal className="w-4 h-4 text-foreground" />
                <CardTitle>Interface & Telemetry</CardTitle>
              </div>
              <CardDescription>Telemetry polling frequency and display density</CardDescription>
            </CardHeader>

            <div className="space-y-4 pt-2 text-xs">
              <div>
                <label className="font-medium text-foreground block mb-1.5">Telemetry Polling Interval</label>
                <div className="grid grid-cols-3 gap-3">
                  {[
                    { label: "1 second (Fast)", val: "1000" },
                    { label: "2 seconds (Default)", val: "2000" },
                    { label: "5 seconds (Low Load)", val: "5000" },
                  ].map((item) => (
                    <button
                      key={item.val}
                      type="button"
                      onClick={() => setPollInterval(item.val)}
                      className={`p-3 rounded-btn border text-xs font-mono text-left transition-colors ${
                        pollInterval === item.val
                          ? "bg-foreground text-primary-foreground border-foreground shadow-sm"
                          : "bg-surface border-border text-foreground-secondary hover:text-foreground hover:bg-surface-subtle"
                      }`}
                    >
                      {item.label}
                    </button>
                  ))}
                </div>
              </div>

              <div>
                <label className="font-medium text-foreground block mb-1.5">Table Density</label>
                <div className="grid grid-cols-2 gap-3">
                  {[
                    { label: "Comfortable", val: "comfortable" },
                    { label: "Compact", val: "compact" },
                  ].map((item) => (
                    <button
                      key={item.val}
                      type="button"
                      onClick={() => setDensity(item.val)}
                      className={`py-2 px-3 rounded-btn border text-xs font-medium text-left transition-colors ${
                        density === item.val
                          ? "bg-foreground text-primary-foreground border-foreground shadow-sm"
                          : "bg-surface border-border text-foreground-secondary hover:text-foreground hover:bg-surface-subtle"
                      }`}
                    >
                      {item.label}
                    </button>
                  ))}
                </div>
              </div>
            </div>
          </Card>

          {/* Mock Mode Control */}
          <Card>
            <CardHeader className="pb-3">
              <div className="flex items-center gap-2">
                <Shield className="w-4 h-4 text-foreground" />
                <CardTitle>Standalone Demo / Mock Mode</CardTitle>
              </div>
              <CardDescription>
                Toggle realistic simulated data generation for standalone demonstrations without running the backend
              </CardDescription>
            </CardHeader>

            <div className="pt-2 flex items-center justify-between text-xs">
              <div>
                <p className="font-medium text-foreground">Simulated Backend Fleet</p>
                <p className="text-foreground-muted mt-0.5">
                  Simulates 3 pools, 7 backends with varying healthy, degraded, and draining states
                </p>
              </div>
              <label className="relative inline-flex items-center cursor-pointer">
                <input
                  type="checkbox"
                  checked={mockActive}
                  onChange={handleToggleMock}
                  className="sr-only peer"
                />
                <div className="w-11 h-6 bg-border peer-focus:outline-none rounded-full peer peer-checked:after:translate-x-full peer-checked:after:border-white after:content-[''] after:absolute after:top-[2px] after:left-[2px] after:bg-white after:border-gray-300 after:border after:rounded-full after:h-5 after:w-5 after:transition-all peer-checked:bg-foreground"></div>
              </label>
            </div>
          </Card>

          <div className="flex justify-end">
            <button
              type="submit"
              className="px-4 py-2 rounded-btn bg-foreground text-primary-foreground hover:bg-primary-hover text-xs font-medium transition-colors"
            >
              Save Preferences
            </button>
          </div>
        </form>
      </main>
    </div>
  );
}

