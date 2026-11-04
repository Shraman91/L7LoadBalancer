"use client";

import React, { useState } from "react";
import { useQuery, useMutation, useQueryClient } from "@tanstack/react-query";
import { fetchBackends, drainBackend, enableBackend } from "@/lib/api";
import { Header } from "@/components/layout/header";
import { BackendsTable } from "@/components/backends/backends-table";
import { BackendDrawer } from "@/components/backends/backend-drawer";
import { DrainConfirmDialog } from "@/components/backends/drain-confirm-dialog";
import { BackendStatus } from "@/lib/types";
import { useToast } from "@/components/ui/toast";

export default function BackendsPage() {
  const queryClient = useQueryClient();
  const { toast } = useToast();

  const [selectedBackend, setSelectedBackend] = useState<BackendStatus | null>(null);
  const [actionBackend, setActionBackend] = useState<BackendStatus | null>(null);

  const {
    data: backends = [],
    isLoading,
    refetch,
    isRefetching,
  } = useQuery({
    queryKey: ["backends"],
    queryFn: fetchBackends,
  });

  // Optimistic drain mutation
  const drainMutation = useMutation({
    mutationFn: (addr: string) => drainBackend(addr),
    onMutate: async (addr) => {
      await queryClient.cancelQueries({ queryKey: ["backends"] });
      const previous = queryClient.getQueryData<BackendStatus[]>(["backends"]);
      queryClient.setQueryData<BackendStatus[]>(["backends"], (old) =>
        old ? old.map((b) => (b.addr === addr ? { ...b, draining: true } : b)) : []
      );
      return { previous };
    },
    onError: (err, addr, context) => {
      if (context?.previous) {
        queryClient.setQueryData(["backends"], context.previous);
      }
      toast({
        type: "error",
        title: "Drain Action Failed",
        description: err instanceof Error ? err.message : "Failed to mark backend draining",
      });
    },
    onSuccess: (_, addr) => {
      toast({
        type: "success",
        title: "Backend Marked Draining",
        description: `Traffic to ${addr} is halted. In-flight requests are draining.`,
      });
      queryClient.invalidateQueries({ queryKey: ["backends"] });
      setActionBackend(null);
      if (selectedBackend?.addr === addr) {
        setSelectedBackend((prev) => (prev ? { ...prev, draining: true } : null));
      }
    },
  });

  // Optimistic enable mutation
  const enableMutation = useMutation({
    mutationFn: (addr: string) => enableBackend(addr),
    onMutate: async (addr) => {
      await queryClient.cancelQueries({ queryKey: ["backends"] });
      const previous = queryClient.getQueryData<BackendStatus[]>(["backends"]);
      queryClient.setQueryData<BackendStatus[]>(["backends"], (old) =>
        old ? old.map((b) => (b.addr === addr ? { ...b, draining: false } : b)) : []
      );
      return { previous };
    },
    onError: (err, addr, context) => {
      if (context?.previous) {
        queryClient.setQueryData(["backends"], context.previous);
      }
      toast({
        type: "error",
        title: "Enable Action Failed",
        description: err instanceof Error ? err.message : "Failed to enable backend",
      });
    },
    onSuccess: (_, addr) => {
      toast({
        type: "success",
        title: "Backend Enabled",
        description: `Backend ${addr} is active and receiving traffic.`,
      });
      queryClient.invalidateQueries({ queryKey: ["backends"] });
      setActionBackend(null);
      if (selectedBackend?.addr === addr) {
        setSelectedBackend((prev) => (prev ? { ...prev, draining: false } : null));
      }
    },
  });

  const handleConfirmAction = () => {
    if (!actionBackend) return;
    if (actionBackend.draining) {
      enableMutation.mutate(actionBackend.addr);
    } else {
      drainMutation.mutate(actionBackend.addr);
    }
  };

  return (
    <div className="flex-1 flex flex-col h-full overflow-hidden">
      <Header
        title="Backend Fleet"
        subtitle="Nodes, Circuit Breakers & Concurrency"
        onRefresh={() => refetch()}
        isRefreshing={isRefetching}
      />

      <main className="flex-1 overflow-y-auto p-8 max-w-[1400px] w-full mx-auto space-y-6">
        <BackendsTable
          backends={backends}
          onSelectBackend={(b) => setSelectedBackend(b)}
          onInitiateDrainAction={(b) => setActionBackend(b)}
        />
      </main>

      {/* Backend Detail Slide-Over Drawer */}
      <BackendDrawer
        backend={selectedBackend}
        onClose={() => setSelectedBackend(null)}
        onToggleDrain={(b) => {
          setActionBackend(b);
        }}
      />

      {/* Drain/Enable Confirmation Modal */}
      <DrainConfirmDialog
        isOpen={Boolean(actionBackend)}
        backendAddr={actionBackend?.addr || ""}
        isDraining={actionBackend?.draining || false}
        isLoading={drainMutation.isPending || enableMutation.isPending}
        onClose={() => setActionBackend(null)}
        onConfirm={handleConfirmAction}
      />
    </div>
  );
}

