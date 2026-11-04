"use client";

import React, { useMemo } from "react";
import { useQuery } from "@tanstack/react-query";
import { fetchBackends, fetchConfig } from "@/lib/api";
import { Header } from "@/components/layout/header";
import {
  ReactFlow,
  Background,
  Controls,
  Edge,
  Node,
} from "@xyflow/react";
import "@xyflow/react/dist/style.css";
import {
  ListenerNode,
  RouteNode,
  PoolNode,
  BackendNode,
} from "@/components/topology/custom-nodes";

const nodeTypes = {
  listener: ListenerNode,
  route: RouteNode,
  pool: PoolNode,
  backend: BackendNode,
};

export default function TopologyPage() {
  const { data: backends = [], refetch: refetchBackends, isRefetching } = useQuery({
    queryKey: ["backends"],
    queryFn: fetchBackends,
  });

  const { data: config } = useQuery({
    queryKey: ["config"],
    queryFn: fetchConfig,
  });

  const { nodes, edges } = useMemo(() => {
    const generatedNodes: Node[] = [];
    const generatedEdges: Edge[] = [];

    // 1. Listener Node
    generatedNodes.push({
      id: "node-listener",
      type: "listener",
      position: { x: 40, y: 220 },
      data: {
        label: "HTTP Listener",
        port: 8080,
        tls: config?.tls?.enabled || false,
      },
    });

    const routes = config?.routes || [
      { path_prefix: "/api/", pool: "api" },
      { path_prefix: "/user/", pool: "stateful" },
      { path_prefix: "/", pool: "web" },
    ];

    // 2. Route Nodes
    routes.forEach((r, idx) => {
      const rId = `route-${idx}`;
      const yPos = 80 + idx * 140;
      generatedNodes.push({
        id: rId,
        type: "route",
        position: { x: 260, y: yPos },
        data: {
          pathPrefix: r.path_prefix,
          pool: r.pool,
          host: r.host,
        },
      });

      generatedEdges.push({
        id: `e-listener-${rId}`,
        source: "node-listener",
        target: rId,
        type: "smoothstep",
        style: { stroke: "#D4D4D8", strokeWidth: 1.5 },
      });
    });

    // 3. Pool Nodes
    const pools = Object.entries(config?.pools || {
      web: { algorithm: "round_robin", backends: [] },
      api: { algorithm: "power_of_two_choices", backends: [] },
      stateful: { algorithm: "consistent_hash", backends: [] },
    });

    pools.forEach(([name, poolCfg], idx) => {
      const pId = `pool-${name}`;
      const yPos = 80 + idx * 150;
      const poolBackends = backends.filter((b) => b.pool === name);

      generatedNodes.push({
        id: pId,
        type: "pool",
        position: { x: 520, y: yPos },
        data: {
          name,
          algorithm: poolCfg.algorithm,
          backendCount: poolBackends.length,
        },
      });

      // Connect corresponding routes to this pool
      routes.forEach((r, rIdx) => {
        if (r.pool === name) {
          generatedEdges.push({
            id: `e-route-${rIdx}-${pId}`,
            source: `route-${rIdx}`,
            target: pId,
            type: "smoothstep",
            style: { stroke: "#A1A1AA", strokeWidth: 1.5 },
          });
        }
      });
    });

    // 4. Backend Nodes
    backends.forEach((backend, idx) => {
      const bId = `backend-${backend.addr}`;
      const yPos = 40 + idx * 85;
      const isBad = !backend.healthy || backend.circuit_state === "open";

      generatedNodes.push({
        id: bId,
        type: "backend",
        position: { x: 800, y: yPos },
        data: {
          addr: backend.addr,
          healthy: backend.healthy,
          draining: backend.draining,
          circuitState: backend.circuit_state,
          conns: backend.active_connections,
          latencyMs: backend.ewma_latency_ms,
        },
      });

      // Connect from pool
      const poolName = backend.pool || "web";
      generatedEdges.push({
        id: `e-pool-${poolName}-${bId}`,
        source: `pool-${poolName}`,
        target: bId,
        type: "smoothstep",
        style: isBad
          ? { stroke: "#B91C1C", strokeWidth: 1.5, strokeDasharray: "4 4" }
          : { stroke: "#A1A1AA", strokeWidth: 1.5 },
      });
    });

    return { nodes: generatedNodes, edges: generatedEdges };
  }, [backends, config]);

  return (
    <div className="flex-1 flex flex-col h-full overflow-hidden">
      <Header
        title="Topology Visualizer"
        subtitle="Listener -> Routes -> Pools -> Backends"
        onRefresh={() => refetchBackends()}
        isRefreshing={isRefetching}
      />

      <main className="flex-1 w-full h-full bg-[#FAFAFA] relative">
        <ReactFlow
          nodes={nodes}
          edges={edges}
          nodeTypes={nodeTypes}
          fitView
          fitViewOptions={{ padding: 0.2 }}
          minZoom={0.2}
          maxZoom={1.5}
          proOptions={{ hideAttribution: true }}
        >
          <Background color="#E4E4E7" gap={20} size={1} />
          <Controls showInteractive={false} className="!bg-surface !border-border !rounded-card !shadow-sm" />
        </ReactFlow>
      </main>
    </div>
  );
}

