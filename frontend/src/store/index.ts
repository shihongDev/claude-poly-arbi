import { create } from "zustand";
import type {
  Opportunity,
  ExecutionReport,
  Position,
  MetricsSnapshot,
  StatusResponse,
  MarketState,
  WsEvent,
} from "@/lib/types";

interface DashboardStore {
  wsStatus: "connected" | "connecting" | "disconnected";
  setWsStatus: (s: "connected" | "connecting" | "disconnected") => void;

  status: StatusResponse | null;
  setStatus: (s: StatusResponse) => void;

  opportunities: Opportunity[];
  addOpportunity: (o: Opportunity) => void;
  setOpportunities: (o: Opportunity[]) => void;

  positions: Position[];
  setPositions: (p: Position[]) => void;

  metrics: MetricsSnapshot | null;
  setMetrics: (m: MetricsSnapshot) => void;

  markets: MarketState[];
  setMarkets: (m: MarketState[]) => void;

  history: ExecutionReport[];
  addExecution: (e: ExecutionReport) => void;
  setHistory: (h: ExecutionReport[]) => void;

  killSwitchActive: boolean;
  killSwitchReason: string | null;
  setKillSwitch: (active: boolean, reason?: string | null) => void;

  handleWsEvent: (event: WsEvent) => void;
}

export const useDashboardStore = create<DashboardStore>((set) => ({
  wsStatus: "disconnected",
  setWsStatus: (wsStatus) => set({ wsStatus }),

  status: null,
  setStatus: (status) =>
    set({
      status,
      killSwitchActive: status.kill_switch_active,
      killSwitchReason: status.kill_switch_reason,
    }),

  opportunities: [],
  addOpportunity: (o) =>
    set((s) => ({
      opportunities: [o, ...s.opportunities].slice(0, 200),
    })),
  setOpportunities: (opportunities) => set({ opportunities }),

  positions: [],
  setPositions: (positions) => set({ positions }),

  metrics: null,
  setMetrics: (metrics) => set({ metrics }),

  markets: [],
  setMarkets: (markets) => set({ markets }),

  history: [],
  addExecution: (e) =>
    set((s) => ({ history: [e, ...s.history].slice(0, 500) })),
  setHistory: (history) => set({ history }),

  killSwitchActive: false,
  killSwitchReason: null,
  setKillSwitch: (active, reason) =>
    set({ killSwitchActive: active, killSwitchReason: reason ?? null }),

  handleWsEvent: (event) => {
    const { type, data } = event;
    set((s) => {
      switch (type) {
        case "opportunity_detected":
          return {
            opportunities: [
              data as Opportunity,
              ...s.opportunities,
            ].slice(0, 200),
          };
        case "trade_executed":
          return {
            history: [data as ExecutionReport, ...s.history].slice(0, 500),
          };
        case "position_update":
          return { positions: data as Position[] };
        case "metrics_update":
          return { metrics: data as MetricsSnapshot };
        case "kill_switch_change": {
          const ks = data as { active: boolean; reason?: string };
          return {
            killSwitchActive: ks.active,
            killSwitchReason: ks.reason ?? null,
          };
        }
        case "markets_loaded": {
          // Merge incoming markets with existing: update matches, append new
          const incoming = data as MarketState[];
          const existing = new Map(
            s.markets.map((m) => [m.condition_id, m])
          );
          for (const m of incoming) {
            existing.set(m.condition_id, m);
          }
          return { markets: Array.from(existing.values()) };
        }
        case "market_update": {
          const market = data as MarketState;
          const idx = s.markets.findIndex(
            (m) => m.condition_id === market.condition_id
          );
          if (idx >= 0) {
            const updated = [...s.markets];
            updated[idx] = market;
            return { markets: updated };
          }
          return { markets: [...s.markets, market] };
        }
        case "market_count_update":
          return {};
        default:
          return {};
      }
    });
  },
}));
