"use client";

import { useEffect } from "react";
import { useWebSocket } from "@/hooks/use-websocket";
import { useDashboardStore } from "@/store";
import { fetchApi } from "@/lib/api";
import type {
  StatusResponse,
  Opportunity,
  Position,
  MetricsSnapshot,
  MarketState,
  ExecutionReport,
} from "@/lib/types";

function DataInitializer() {
  const setStatus = useDashboardStore((s) => s.setStatus);
  const setOpportunities = useDashboardStore((s) => s.setOpportunities);
  const setPositions = useDashboardStore((s) => s.setPositions);
  const setMetrics = useDashboardStore((s) => s.setMetrics);
  const setMarkets = useDashboardStore((s) => s.setMarkets);
  const setHistory = useDashboardStore((s) => s.setHistory);

  useEffect(() => {
    async function loadInitialData() {
      try {
        const [status, opportunities, positions, metrics, markets, history] =
          await Promise.allSettled([
            fetchApi<StatusResponse>("/api/status"),
            fetchApi<Opportunity[]>("/api/opportunities"),
            fetchApi<Position[]>("/api/positions"),
            fetchApi<MetricsSnapshot>("/api/metrics"),
            fetchApi<MarketState[]>("/api/markets"),
            fetchApi<ExecutionReport[]>("/api/history"),
          ]);

        if (status.status === "fulfilled") setStatus(status.value);
        if (opportunities.status === "fulfilled") setOpportunities(opportunities.value);
        if (positions.status === "fulfilled") setPositions(positions.value);
        if (metrics.status === "fulfilled") setMetrics(metrics.value);
        if (markets.status === "fulfilled") setMarkets(markets.value);
        if (history.status === "fulfilled") setHistory(history.value);
      } catch {
        /* Backend not available yet — WebSocket will handle reconnection */
      }
    }

    loadInitialData();
  }, [setStatus, setOpportunities, setPositions, setMetrics, setMarkets, setHistory]);

  return null;
}

function WebSocketConnector() {
  useWebSocket();
  return null;
}

export function Providers({ children }: { children: React.ReactNode }) {
  return (
    <>
      <WebSocketConnector />
      <DataInitializer />
      {children}
    </>
  );
}
