"use client";

import { useState, useMemo, useCallback, useEffect } from "react";
import dynamic from "next/dynamic";
import {
  Loader2,
  AlertTriangle,
  XCircle,
  SearchX,
  Play,
  RotateCcw,
  Upload,
  FlaskConical,
} from "lucide-react";
import { useDashboardStore } from "@/store";
import { fetchApi, sandboxDetect, sandboxBacktest, runSimulation } from "@/lib/api";
import { cn, formatBps, formatUsd, formatDecimal, timeAgo } from "@/lib/utils";
import { DataTable, type Column } from "@/components/data-table";
import { MetricCard } from "@/components/metric-card";
import { Badge } from "@/components/ui/badge";
import { Button } from "@/components/ui/button";
import { Input } from "@/components/ui/input";
import { Label } from "@/components/ui/label";
import { Switch } from "@/components/ui/switch";
import { Tabs, TabsList, TabsTrigger, TabsContent } from "@/components/ui/tabs";
import {
  Select,
  SelectContent,
  SelectItem,
  SelectTrigger,
  SelectValue,
} from "@/components/ui/select";
import {
  Dialog,
  DialogContent,
  DialogDescription,
  DialogFooter,
  DialogHeader,
  DialogTitle,
} from "@/components/ui/dialog";
import {
  Sheet,
  SheetContent,
  SheetHeader,
  SheetTitle,
  SheetDescription,
} from "@/components/ui/sheet";
import { ScrollArea } from "@/components/ui/scroll-area";
import {
  Table,
  TableBody,
  TableCell,
  TableHead,
  TableHeader,
  TableRow,
} from "@/components/ui/table";
import type {
  SandboxConfigOverrides,
  DetectResponse,
  BacktestResponse,
  BacktestTrade,
  Opportunity,
  ArbType,
  SimulateParams,
} from "@/lib/types";

const ReactECharts = dynamic(() => import("echarts-for-react"), { ssr: false });

// ---------------------------------------------------------------------------
// Arb-type badge config (shared with detail sheet)
// ---------------------------------------------------------------------------

const arbTypeConfig: Record<ArbType, { label: string; className: string }> = {
  IntraMarket: { label: "Intra-Market", className: "bg-blue-50 text-blue-600" },
  CrossMarket: { label: "Cross-Market", className: "bg-purple-50 text-purple-600" },
  MultiOutcome: { label: "Multi-Outcome", className: "bg-amber-50 text-amber-600" },
};

// ---------------------------------------------------------------------------
// Simulation result types (reused from simulation page)
// ---------------------------------------------------------------------------

interface MethodResult {
  probability: number;
  std_error?: number;
  ci_lower: number;
  ci_upper: number;
}

interface RawSimulationResponse {
  condition_id: string;
  initial_price: number;
  monte_carlo: {
    probability: number;
    standard_error: number;
    confidence_interval: [number, number];
    n_paths: number;
  };
  particle_filter: {
    probability: number[];
    confidence_interval: number[][];
    method: string;
  };
}

interface SimulationDisplayResult {
  condition_id: string;
  market_price: number;
  monte_carlo: MethodResult;
  particle_filter: MethodResult;
}

function normalizeSimResult(raw: RawSimulationResponse): SimulationDisplayResult {
  const mc = raw.monte_carlo;
  const pf = raw.particle_filter;
  const pfProb = pf.probability.length > 0 ? pf.probability[0] : raw.initial_price;
  const pfCi =
    pf.confidence_interval.length > 0
      ? pf.confidence_interval[0]
      : [pfProb - 0.05, pfProb + 0.05];
  return {
    condition_id: raw.condition_id,
    market_price: raw.initial_price,
    monte_carlo: {
      probability: mc.probability,
      std_error: mc.standard_error,
      ci_lower: mc.confidence_interval[0],
      ci_upper: mc.confidence_interval[1],
    },
    particle_filter: {
      probability: pfProb,
      ci_lower: pfCi[0] ?? pfProb - 0.05,
      ci_upper: pfCi[1] ?? pfProb + 0.05,
    },
  };
}

function pct(v: number): string {
  return `${(v * 100).toFixed(2)}%`;
}

// ---------------------------------------------------------------------------
// Default sandbox config
// ---------------------------------------------------------------------------

const DEFAULT_CONFIG: SandboxConfigOverrides = {
  min_edge_bps: 50,
  intra_market_enabled: true,
  cross_market_enabled: true,
  multi_outcome_enabled: true,
  intra_min_deviation: "0.005",
  cross_min_implied_edge: "0.02",
  multi_min_deviation: "0.01",
  max_slippage_bps: 100,
  vwap_depth_levels: 10,
  max_position_per_market: "500",
  max_total_exposure: "5000",
  daily_loss_limit: "200",
};

// ---------------------------------------------------------------------------
// Page Component
// ---------------------------------------------------------------------------

export default function PlaygroundPage() {
  const markets = useDashboardStore((s) => s.markets);

  // ── Sandbox config (local state, isolated from live engine) ──
  const [config, setConfig] = useState<SandboxConfigOverrides>({ ...DEFAULT_CONFIG });

  // Load live config on mount to seed defaults
  useEffect(() => {
    fetchApi<Record<string, unknown>>("/api/config")
      .then((live) => {
        setConfig((prev) => ({
          ...prev,
          min_edge_bps: (live.min_edge_bps as number) ?? prev.min_edge_bps,
          intra_market_enabled:
            (live.intra_market_enabled as boolean) ?? prev.intra_market_enabled,
          cross_market_enabled:
            (live.cross_market_enabled as boolean) ?? prev.cross_market_enabled,
          multi_outcome_enabled:
            (live.multi_outcome_enabled as boolean) ?? prev.multi_outcome_enabled,
        }));
      })
      .catch(() => {});
  }, []);

  // ── Tab state ──
  const [activeTab, setActiveTab] = useState<string>("detect");

  // ── Detect state ──
  const [detectResult, setDetectResult] = useState<DetectResponse | null>(null);
  const [detectLoading, setDetectLoading] = useState(false);
  const [detectError, setDetectError] = useState<string | null>(null);
  const [selectedOpp, setSelectedOpp] = useState<Opportunity | null>(null);

  // ── Backtest state ──
  const [backtestResult, setBacktestResult] = useState<BacktestResponse | null>(null);
  const [backtestLoading, setBacktestLoading] = useState(false);
  const [backtestError, setBacktestError] = useState<string | null>(null);

  // ── Simulate state ──
  const [simConditionId, setSimConditionId] = useState("");
  const [simParams, setSimParams] = useState<SimulateParams>({
    num_paths: 10000,
    volatility: 0.4,
    drift: 0.0,
    time_horizon: 0.5,
    particle_count: 1000,
    process_noise: 0.05,
    observation_noise: 0.03,
  });
  const [simResult, setSimResult] = useState<SimulationDisplayResult | null>(null);
  const [simLoading, setSimLoading] = useState(false);
  const [simError, setSimError] = useState<string | null>(null);

  // ── Apply to Live dialog ──
  const [applyDialogOpen, setApplyDialogOpen] = useState(false);
  const [applyLoading, setApplyLoading] = useState(false);

  // ── Handlers ──

  const handleDetect = useCallback(async () => {
    setDetectLoading(true);
    setDetectError(null);
    try {
      const res = await sandboxDetect(config);
      setDetectResult(res);
    } catch (err) {
      setDetectError(err instanceof Error ? err.message : "Detection failed");
    } finally {
      setDetectLoading(false);
    }
  }, [config]);

  const handleBacktest = useCallback(async () => {
    setBacktestLoading(true);
    setBacktestError(null);
    try {
      const res = await sandboxBacktest(config);
      setBacktestResult(res);
    } catch (err) {
      setBacktestError(err instanceof Error ? err.message : "Backtest failed");
    } finally {
      setBacktestLoading(false);
    }
  }, [config]);

  const handleSimulate = useCallback(async () => {
    if (!simConditionId) return;
    setSimLoading(true);
    setSimError(null);
    try {
      const raw = (await runSimulation(simConditionId, simParams)) as RawSimulationResponse;
      setSimResult(normalizeSimResult(raw));
    } catch (err) {
      setSimError(err instanceof Error ? err.message : "Simulation failed");
    } finally {
      setSimLoading(false);
    }
  }, [simConditionId, simParams]);

  const handleApplyToLive = useCallback(async () => {
    setApplyLoading(true);
    try {
      await fetchApi("/api/config", {
        method: "PUT",
        body: JSON.stringify(config),
      });
      setApplyDialogOpen(false);
    } catch {
      // toast would go here
    } finally {
      setApplyLoading(false);
    }
  }, [config]);

  const handleReset = useCallback(() => {
    setConfig({ ...DEFAULT_CONFIG });
    setDetectResult(null);
    setBacktestResult(null);
    setSimResult(null);
  }, []);

  // ── Config updater helpers ──

  const updateNum = (key: keyof SandboxConfigOverrides, val: string) => {
    const num = parseInt(val);
    if (!isNaN(num)) setConfig((c) => ({ ...c, [key]: num }));
  };

  const updateStr = (key: keyof SandboxConfigOverrides, val: string) => {
    setConfig((c) => ({ ...c, [key]: val }));
  };

  const updateBool = (key: keyof SandboxConfigOverrides, val: boolean) => {
    setConfig((c) => ({ ...c, [key]: val }));
  };

  // ── Detect tab columns ──

  const detectColumns: Column<Opportunity>[] = useMemo(
    () => [
      {
        key: "type",
        header: "Type",
        render: (row) => {
          const cfg = arbTypeConfig[row.arb_type];
          return <Badge className={cn("text-xs", cfg.className)}>{cfg.label}</Badge>;
        },
      },
      {
        key: "markets",
        header: "Markets",
        render: (row) => (
          <span
            className="inline-block max-w-[180px] truncate text-sm text-[#1A1A19]"
            title={row.markets.join(", ")}
          >
            {row.markets
              .map((m) => (m.length > 10 ? `${m.slice(0, 6)}...${m.slice(-4)}` : m))
              .join(", ")}
          </span>
        ),
      },
      {
        key: "net_edge",
        header: "Net Edge (bps)",
        sortable: true,
        mono: true,
        render: (row) => {
          const val = parseFloat(row.net_edge);
          return (
            <span className={cn("text-sm font-bold", val > 0 ? "text-[#2D6A4F]" : "text-[#B44C3F]")}>
              {formatBps(row.net_edge)}
            </span>
          );
        },
        getValue: (row) => parseFloat(row.net_edge),
      },
      {
        key: "confidence",
        header: "Confidence",
        sortable: true,
        render: (row) => (
          <div className="flex items-center gap-2">
            <div className="h-1.5 w-16 overflow-hidden rounded-full bg-[#F0EEEA]">
              <div
                className={cn(
                  "h-full rounded-full",
                  row.confidence > 0.8
                    ? "bg-[#2D6A4F]"
                    : row.confidence > 0.5
                      ? "bg-amber-500"
                      : "bg-[#B44C3F]"
                )}
                style={{ width: `${row.confidence * 100}%` }}
              />
            </div>
            <span className="text-xs text-[#6B6B6B]" style={{ fontFamily: "var(--font-jetbrains-mono)" }}>
              {(row.confidence * 100).toFixed(0)}%
            </span>
          </div>
        ),
        getValue: (row) => row.confidence,
      },
      {
        key: "size",
        header: "Size",
        sortable: true,
        mono: true,
        render: (row) => <span className="text-sm text-[#1A1A19]">{formatUsd(row.size_available)}</span>,
        getValue: (row) => parseFloat(row.size_available),
      },
      {
        key: "legs",
        header: "Legs",
        mono: true,
        render: (row) => <span className="text-sm text-[#6B6B6B]">{row.legs.length}</span>,
      },
      {
        key: "simulate",
        header: "",
        render: (row) => (
          <Button
            variant="ghost"
            size="sm"
            className="text-[#6B6B6B] hover:text-[#2D6A4F]"
            onClick={(e) => {
              e.stopPropagation();
              if (row.markets.length > 0) {
                setSimConditionId(row.markets[0]);
                setActiveTab("simulate");
              }
            }}
          >
            <FlaskConical className="mr-1 h-3.5 w-3.5" />
            Sim
          </Button>
        ),
      },
    ],
    []
  );

  // ── Backtest daily chart option ──

  const backtestChartOption = useMemo(() => {
    if (!backtestResult?.daily_breakdown?.length) return null;
    const days = backtestResult.daily_breakdown;
    return {
      tooltip: {
        trigger: "axis" as const,
        backgroundColor: "#FFFFFF",
        borderColor: "#E6E4DF",
        textStyle: { color: "#1A1A19", fontFamily: "var(--font-jetbrains-mono)" },
      },
      grid: { left: 60, right: 20, top: 20, bottom: 40 },
      xAxis: {
        type: "category" as const,
        data: days.map((d) => d.date),
        axisLine: { lineStyle: { color: "#E6E4DF" } },
        axisLabel: { color: "#6B6B6B", fontSize: 11 },
      },
      yAxis: {
        type: "value" as const,
        axisLine: { lineStyle: { color: "#E6E4DF" } },
        axisLabel: {
          color: "#6B6B6B",
          fontSize: 11,
          fontFamily: "var(--font-jetbrains-mono)",
          formatter: (v: number) => `$${v.toFixed(0)}`,
        },
        splitLine: { lineStyle: { color: "#F0EEEA" } },
      },
      series: [
        {
          type: "bar" as const,
          data: days.map((d) => ({
            value: parseFloat(d.pnl),
            itemStyle: {
              color: parseFloat(d.pnl) >= 0 ? "#2D6A4F" : "#B44C3F",
              borderRadius: [4, 4, 0, 0],
            },
          })),
        },
      ],
    };
  }, [backtestResult]);

  // ── Simulation comparison chart ──

  const simChartOption = useMemo(() => {
    if (!simResult) return null;
    const methods = [
      { label: "Monte Carlo", data: simResult.monte_carlo, color: "#3b82f6" },
      { label: "Particle Filter", data: simResult.particle_filter, color: "#f59e0b" },
    ];
    return {
      tooltip: {
        trigger: "axis" as const,
        backgroundColor: "#FFFFFF",
        borderColor: "#E6E4DF",
        textStyle: { color: "#1A1A19", fontFamily: "var(--font-jetbrains-mono)" },
        formatter: (params: Array<{ name: string; value: number }>) => {
          const idx = methods.findIndex((m) => m.label === params[0].name);
          if (idx === -1) return "";
          const d = methods[idx].data;
          return `<b>${params[0].name}</b><br/>Prob: ${pct(d.probability)}<br/>CI: [${pct(d.ci_lower)}, ${pct(d.ci_upper)}]`;
        },
      },
      grid: { left: 130, right: 40, top: 24, bottom: 32 },
      xAxis: {
        type: "value" as const,
        min: 0,
        max: 1,
        axisLine: { lineStyle: { color: "#E6E4DF" } },
        axisLabel: { color: "#6B6B6B", fontSize: 11, fontFamily: "var(--font-jetbrains-mono)", formatter: (v: number) => pct(v) },
        splitLine: { lineStyle: { color: "#F0EEEA" } },
      },
      yAxis: {
        type: "category" as const,
        data: methods.map((m) => m.label),
        inverse: true,
        axisLine: { lineStyle: { color: "#E6E4DF" } },
        axisLabel: { color: "#6B6B6B", fontSize: 12 },
        axisTick: { show: false },
      },
      series: [
        {
          name: "CI Lower",
          type: "bar" as const,
          stack: "ci",
          silent: true,
          itemStyle: { color: "transparent" },
          data: methods.map((m) => m.data.ci_lower),
          barWidth: 12,
        },
        {
          name: "CI Range",
          type: "bar" as const,
          stack: "ci",
          silent: true,
          itemStyle: { color: "rgba(214,210,206,0.3)", borderRadius: 2 },
          data: methods.map((m) => m.data.ci_upper - m.data.ci_lower),
          barWidth: 12,
        },
        {
          name: "Probability",
          type: "scatter" as const,
          symbolSize: 14,
          data: methods.map((m) => ({
            value: [m.data.probability, m.label],
            itemStyle: { color: m.color, borderColor: "#F8F7F4", borderWidth: 2 },
          })),
          z: 10,
        },
        {
          name: "Market Price",
          type: "scatter" as const,
          symbolSize: 0,
          data: [],
          markLine: {
            silent: true,
            symbol: "none",
            lineStyle: { color: "#B44C3F", type: "dashed" as const, width: 1.5 },
            data: [{ xAxis: simResult.market_price }],
            label: {
              formatter: `Market ${pct(simResult.market_price)}`,
              color: "#B44C3F",
              fontSize: 11,
              fontFamily: "var(--font-jetbrains-mono)",
            },
          },
        },
      ],
    };
  }, [simResult]);

  // ── Render ──

  return (
    <div className="flex gap-6">
      {/* ── Config Sidebar ── */}
      <aside className="w-[280px] shrink-0 space-y-5">
        <div className="rounded-2xl bg-white p-5">
          <h2 className="text-[11px] font-medium uppercase tracking-wider text-[#9B9B9B]">
            Sandbox Config
          </h2>

          {/* Strategy Section */}
          <div className="mt-4 space-y-3">
            <p className="text-xs font-medium text-[#1A1A19]">Strategy</p>

            <div className="space-y-1.5">
              <Label className="text-xs text-[#6B6B6B]">Min Edge (bps)</Label>
              <Input
                type="number"
                value={config.min_edge_bps ?? ""}
                onChange={(e) => updateNum("min_edge_bps", e.target.value)}
                className="h-8 border-[#E6E4DF] bg-[#F8F7F4] text-sm"
                style={{ fontFamily: "var(--font-jetbrains-mono)" }}
                min={0}
              />
            </div>

            <div className="flex items-center justify-between">
              <Label className="text-xs text-[#6B6B6B]">Intra-Market</Label>
              <Switch
                size="sm"
                checked={config.intra_market_enabled ?? true}
                onCheckedChange={(v) => updateBool("intra_market_enabled", v)}
              />
            </div>
            {config.intra_market_enabled && (
              <div className="space-y-1.5 pl-3">
                <Label className="text-xs text-[#6B6B6B]">Min Deviation</Label>
                <Input
                  type="text"
                  value={config.intra_min_deviation ?? ""}
                  onChange={(e) => updateStr("intra_min_deviation", e.target.value)}
                  className="h-8 border-[#E6E4DF] bg-[#F8F7F4] text-sm"
                  style={{ fontFamily: "var(--font-jetbrains-mono)" }}
                />
              </div>
            )}

            <div className="flex items-center justify-between">
              <Label className="text-xs text-[#6B6B6B]">Cross-Market</Label>
              <Switch
                size="sm"
                checked={config.cross_market_enabled ?? true}
                onCheckedChange={(v) => updateBool("cross_market_enabled", v)}
              />
            </div>
            {config.cross_market_enabled && (
              <div className="space-y-1.5 pl-3">
                <Label className="text-xs text-[#6B6B6B]">Min Implied Edge</Label>
                <Input
                  type="text"
                  value={config.cross_min_implied_edge ?? ""}
                  onChange={(e) => updateStr("cross_min_implied_edge", e.target.value)}
                  className="h-8 border-[#E6E4DF] bg-[#F8F7F4] text-sm"
                  style={{ fontFamily: "var(--font-jetbrains-mono)" }}
                />
              </div>
            )}

            <div className="flex items-center justify-between">
              <Label className="text-xs text-[#6B6B6B]">Multi-Outcome</Label>
              <Switch
                size="sm"
                checked={config.multi_outcome_enabled ?? true}
                onCheckedChange={(v) => updateBool("multi_outcome_enabled", v)}
              />
            </div>
            {config.multi_outcome_enabled && (
              <div className="space-y-1.5 pl-3">
                <Label className="text-xs text-[#6B6B6B]">Min Deviation</Label>
                <Input
                  type="text"
                  value={config.multi_min_deviation ?? ""}
                  onChange={(e) => updateStr("multi_min_deviation", e.target.value)}
                  className="h-8 border-[#E6E4DF] bg-[#F8F7F4] text-sm"
                  style={{ fontFamily: "var(--font-jetbrains-mono)" }}
                />
              </div>
            )}
          </div>

          {/* Slippage Section */}
          <div className="mt-5 space-y-3 border-t border-[#E6E4DF] pt-4">
            <p className="text-xs font-medium text-[#1A1A19]">Slippage</p>

            <div className="space-y-1.5">
              <Label className="text-xs text-[#6B6B6B]">Max Slippage (bps)</Label>
              <Input
                type="number"
                value={config.max_slippage_bps ?? ""}
                onChange={(e) => updateNum("max_slippage_bps", e.target.value)}
                className="h-8 border-[#E6E4DF] bg-[#F8F7F4] text-sm"
                style={{ fontFamily: "var(--font-jetbrains-mono)" }}
                min={0}
              />
            </div>

            <div className="space-y-1.5">
              <Label className="text-xs text-[#6B6B6B]">VWAP Depth Levels</Label>
              <Input
                type="number"
                value={config.vwap_depth_levels ?? ""}
                onChange={(e) => updateNum("vwap_depth_levels", e.target.value)}
                className="h-8 border-[#E6E4DF] bg-[#F8F7F4] text-sm"
                style={{ fontFamily: "var(--font-jetbrains-mono)" }}
                min={1}
                max={50}
              />
            </div>
          </div>

          {/* Risk Section */}
          <div className="mt-5 space-y-3 border-t border-[#E6E4DF] pt-4">
            <p className="text-xs font-medium text-[#1A1A19]">Risk</p>

            <div className="space-y-1.5">
              <Label className="text-xs text-[#6B6B6B]">Max Position / Market</Label>
              <Input
                type="text"
                value={config.max_position_per_market ?? ""}
                onChange={(e) => updateStr("max_position_per_market", e.target.value)}
                className="h-8 border-[#E6E4DF] bg-[#F8F7F4] text-sm"
                style={{ fontFamily: "var(--font-jetbrains-mono)" }}
              />
            </div>

            <div className="space-y-1.5">
              <Label className="text-xs text-[#6B6B6B]">Max Total Exposure</Label>
              <Input
                type="text"
                value={config.max_total_exposure ?? ""}
                onChange={(e) => updateStr("max_total_exposure", e.target.value)}
                className="h-8 border-[#E6E4DF] bg-[#F8F7F4] text-sm"
                style={{ fontFamily: "var(--font-jetbrains-mono)" }}
              />
            </div>

            <div className="space-y-1.5">
              <Label className="text-xs text-[#6B6B6B]">Daily Loss Limit</Label>
              <Input
                type="text"
                value={config.daily_loss_limit ?? ""}
                onChange={(e) => updateStr("daily_loss_limit", e.target.value)}
                className="h-8 border-[#E6E4DF] bg-[#F8F7F4] text-sm"
                style={{ fontFamily: "var(--font-jetbrains-mono)" }}
              />
            </div>
          </div>

          {/* Actions */}
          <div className="mt-5 space-y-2 border-t border-[#E6E4DF] pt-4">
            <Button
              onClick={handleDetect}
              disabled={detectLoading}
              className="w-full bg-[#2D6A4F] text-white hover:bg-[#245840]"
            >
              {detectLoading ? <Loader2 className="mr-2 h-4 w-4 animate-spin" /> : <Play className="mr-2 h-4 w-4" />}
              Detect
            </Button>
            <Button
              onClick={handleBacktest}
              disabled={backtestLoading}
              variant="outline"
              className="w-full border-[#E6E4DF] text-[#1A1A19] hover:bg-[#F8F7F4]"
            >
              {backtestLoading ? <Loader2 className="mr-2 h-4 w-4 animate-spin" /> : <RotateCcw className="mr-2 h-4 w-4" />}
              Backtest
            </Button>
          </div>

          {/* Apply to Live + Reset */}
          <div className="mt-4 space-y-2 border-t border-[#E6E4DF] pt-4">
            <Button
              onClick={() => setApplyDialogOpen(true)}
              variant="outline"
              className="w-full border-amber-300 text-amber-700 hover:bg-amber-50"
            >
              <Upload className="mr-2 h-4 w-4" />
              Apply to Live
            </Button>
            <Button onClick={handleReset} variant="ghost" className="w-full text-[#6B6B6B] hover:text-[#1A1A19]">
              Reset Defaults
            </Button>
          </div>
        </div>
      </aside>

      {/* ── Main Content (Tabbed) ── */}
      <div className="min-w-0 flex-1 space-y-4">
        <div>
          <h1 className="text-2xl font-bold text-[#1A1A19]">Strategy Playground</h1>
          <p className="mt-1 text-sm text-[#6B6B6B]">
            Tune parameters, detect opportunities, backtest history, and run simulations
          </p>
        </div>

        <Tabs value={activeTab} onValueChange={setActiveTab}>
          <TabsList>
            <TabsTrigger value="detect">Detect</TabsTrigger>
            <TabsTrigger value="backtest">Backtest</TabsTrigger>
            <TabsTrigger value="simulate">Simulate</TabsTrigger>
          </TabsList>

          {/* ── Detect Tab ── */}
          <TabsContent value="detect">
            <div className="space-y-4">
              {/* Stats Banner */}
              {detectResult && (
                <div className="rounded-2xl bg-white px-5 py-3">
                  <p className="text-sm text-[#1A1A19]">
                    <span className="font-bold text-[#2D6A4F]">{detectResult.opportunities.length}</span>{" "}
                    opportunities found in{" "}
                    <span className="font-mono text-[#6B6B6B]">{detectResult.detection_time_ms}ms</span> —{" "}
                    <span className="font-mono text-[#6B6B6B]">{detectResult.markets_scanned}</span> markets
                    scanned
                  </p>
                </div>
              )}

              {/* Error */}
              {detectError && <ErrorBanner message={detectError} />}

              {/* Loading */}
              {detectLoading && <div className="h-48 animate-pulse rounded-2xl bg-white" />}

              {/* Results Table */}
              {!detectLoading && detectResult && (
                <div className="overflow-hidden rounded-2xl bg-white">
                  {detectResult.opportunities.length === 0 ? (
                    <EmptyState message="No opportunities found with current parameters" hint="Try lowering min edge or enabling more arb types" />
                  ) : (
                    <DataTable
                      columns={detectColumns}
                      data={detectResult.opportunities}
                      pageSize={25}
                      onRowClick={(row) => setSelectedOpp(row)}
                    />
                  )}
                </div>
              )}

              {/* Initial state */}
              {!detectLoading && !detectResult && !detectError && (
                <div className="overflow-hidden rounded-2xl bg-white">
                  <EmptyState
                    message="Click Detect to scan markets"
                    hint="Adjust parameters in the sidebar and run detection"
                  />
                </div>
              )}
            </div>
          </TabsContent>

          {/* ── Backtest Tab ── */}
          <TabsContent value="backtest">
            <div className="space-y-4">
              {/* Error */}
              {backtestError && <ErrorBanner message={backtestError} />}

              {/* Loading */}
              {backtestLoading && (
                <div className="space-y-4">
                  <div className="h-24 animate-pulse rounded-2xl bg-white" />
                  <div className="h-64 animate-pulse rounded-2xl bg-white" />
                </div>
              )}

              {/* Results */}
              {!backtestLoading && backtestResult && (
                <>
                  {/* KPI Cards */}
                  <div className="grid grid-cols-4 gap-4 rounded-2xl bg-white">
                    <MetricCard
                      title="Original Trades"
                      value={backtestResult.total_trades_original.toString()}
                    />
                    <MetricCard
                      title="Filtered Trades"
                      value={backtestResult.total_trades_filtered.toString()}
                      delta={`${backtestResult.trades_rejected} rejected`}
                      deltaType={backtestResult.trades_rejected > 0 ? "negative" : "neutral"}
                    />
                    <MetricCard
                      title="Sandbox P&L"
                      value={formatUsd(backtestResult.aggregate_pnl)}
                      deltaType={parseFloat(backtestResult.aggregate_pnl) >= 0 ? "positive" : "negative"}
                    />
                    <MetricCard
                      title="P&L Delta"
                      value={formatUsd(
                        (
                          parseFloat(backtestResult.aggregate_pnl) -
                          parseFloat(backtestResult.aggregate_pnl_original)
                        ).toFixed(2)
                      )}
                      deltaType={
                        parseFloat(backtestResult.aggregate_pnl) >=
                        parseFloat(backtestResult.aggregate_pnl_original)
                          ? "positive"
                          : "negative"
                      }
                    />
                  </div>

                  {/* Daily P&L Chart */}
                  {backtestChartOption && (
                    <div className="rounded-2xl bg-white">
                      <div className="border-b border-[#E6E4DF] px-5 py-4">
                        <h2 className="text-[11px] font-medium uppercase tracking-wider text-[#9B9B9B]">
                          Daily P&L
                        </h2>
                      </div>
                      <div className="p-4">
                        <ReactECharts
                          option={backtestChartOption}
                          style={{ height: 220, width: "100%" }}
                          opts={{ renderer: "canvas" }}
                        />
                      </div>
                    </div>
                  )}

                  {/* Trade Log */}
                  <div className="rounded-2xl bg-white">
                    <div className="border-b border-[#E6E4DF] px-5 py-4">
                      <h2 className="text-[11px] font-medium uppercase tracking-wider text-[#9B9B9B]">
                        Trade Log ({backtestResult.trades.length})
                      </h2>
                    </div>
                    <div className="overflow-x-auto">
                      <Table>
                        <TableHeader>
                          <TableRow className="hover:bg-transparent">
                            <TableHead className="text-xs">Opportunity</TableHead>
                            <TableHead className="text-right text-xs">Edge</TableHead>
                            <TableHead className="text-right text-xs">Fees</TableHead>
                            <TableHead className="text-right text-xs">Net P&L</TableHead>
                            <TableHead className="text-xs">Status</TableHead>
                            <TableHead className="text-xs">Reason</TableHead>
                          </TableRow>
                        </TableHeader>
                        <TableBody>
                          {backtestResult.trades.map((trade, i) => (
                            <BacktestTradeRow key={i} trade={trade} />
                          ))}
                        </TableBody>
                      </Table>
                    </div>
                  </div>
                </>
              )}

              {/* Initial state */}
              {!backtestLoading && !backtestResult && !backtestError && (
                <div className="overflow-hidden rounded-2xl bg-white">
                  <EmptyState
                    message="Click Backtest to replay history"
                    hint="Adjust parameters and re-score execution history under new thresholds"
                  />
                </div>
              )}
            </div>
          </TabsContent>

          {/* ── Simulate Tab ── */}
          <TabsContent value="simulate">
            <div className="space-y-4">
              {/* Config */}
              <div className="rounded-2xl bg-white p-5">
                <h2 className="text-[11px] font-medium uppercase tracking-wider text-[#9B9B9B]">
                  Simulation Config
                </h2>

                {markets.length === 0 ? (
                  <div className="mt-4 flex items-center gap-2 text-sm text-[#9B9B9B]">
                    <AlertTriangle className="h-4 w-4 text-amber-600" />
                    No markets available — start the arb engine first
                  </div>
                ) : (
                  <div className="mt-4 space-y-4">
                    {/* Market selector */}
                    <div className="space-y-2">
                      <Label className="text-[#1A1A19]">Market</Label>
                      <Select value={simConditionId} onValueChange={setSimConditionId}>
                        <SelectTrigger className="w-full max-w-xl border-[#E6E4DF] bg-[#F8F7F4] text-[#1A1A19]">
                          <SelectValue placeholder="Select a market..." />
                        </SelectTrigger>
                        <SelectContent className="max-h-72 border-[#E6E4DF] bg-white">
                          {markets.map((m) => (
                            <SelectItem key={m.condition_id} value={m.condition_id} className="text-[#1A1A19]">
                              <span className="line-clamp-1">{m.question}</span>
                            </SelectItem>
                          ))}
                        </SelectContent>
                      </Select>
                    </div>

                    {/* Parameter grid */}
                    <div className="grid grid-cols-4 gap-3">
                      <SimParamInput
                        label="Num Paths"
                        value={simParams.num_paths ?? 10000}
                        type="number"
                        onChange={(v) => setSimParams((p) => ({ ...p, num_paths: parseInt(v) || 10000 }))}
                      />
                      <SimParamInput
                        label="Volatility"
                        value={simParams.volatility ?? 0.4}
                        onChange={(v) => setSimParams((p) => ({ ...p, volatility: parseFloat(v) || 0.4 }))}
                      />
                      <SimParamInput
                        label="Drift"
                        value={simParams.drift ?? 0}
                        onChange={(v) => setSimParams((p) => ({ ...p, drift: parseFloat(v) || 0 }))}
                      />
                      <SimParamInput
                        label="Time Horizon"
                        value={simParams.time_horizon ?? 0.5}
                        onChange={(v) => setSimParams((p) => ({ ...p, time_horizon: parseFloat(v) || 0.5 }))}
                      />
                      <SimParamInput
                        label="Particles"
                        value={simParams.particle_count ?? 1000}
                        type="number"
                        onChange={(v) => setSimParams((p) => ({ ...p, particle_count: parseInt(v) || 1000 }))}
                      />
                      <SimParamInput
                        label="Process Noise"
                        value={simParams.process_noise ?? 0.05}
                        onChange={(v) => setSimParams((p) => ({ ...p, process_noise: parseFloat(v) || 0.05 }))}
                      />
                      <SimParamInput
                        label="Obs. Noise"
                        value={simParams.observation_noise ?? 0.03}
                        onChange={(v) => setSimParams((p) => ({ ...p, observation_noise: parseFloat(v) || 0.03 }))}
                      />
                    </div>

                    <Button
                      onClick={handleSimulate}
                      disabled={!simConditionId || simLoading}
                      className="bg-[#2D6A4F] text-white hover:bg-[#245840]"
                    >
                      {simLoading && <Loader2 className="mr-2 h-4 w-4 animate-spin" />}
                      {simLoading ? "Running..." : "Run Simulation"}
                    </Button>
                  </div>
                )}
              </div>

              {/* Error */}
              {simError && <ErrorBanner message={simError} />}

              {/* Loading */}
              {simLoading && (
                <div className="space-y-4">
                  <div className="h-48 animate-pulse rounded-2xl bg-white" />
                  <div className="h-48 animate-pulse rounded-2xl bg-white" />
                </div>
              )}

              {/* Results */}
              {!simLoading && simResult && (
                <>
                  {/* Comparison Table */}
                  <div className="rounded-2xl bg-white">
                    <div className="border-b border-[#E6E4DF] px-5 py-4">
                      <h2 className="text-[11px] font-medium uppercase tracking-wider text-[#9B9B9B]">
                        Method Comparison
                      </h2>
                    </div>
                    <div className="overflow-x-auto">
                      <Table>
                        <TableHeader>
                          <TableRow className="hover:bg-transparent">
                            <TableHead className="text-xs">Method</TableHead>
                            <TableHead className="text-right text-xs">Probability</TableHead>
                            <TableHead className="text-right text-xs">Std Error</TableHead>
                            <TableHead className="text-right text-xs">95% CI</TableHead>
                          </TableRow>
                        </TableHeader>
                        <TableBody>
                          <TableRow className="bg-[#F8F7F4]">
                            <TableCell className="text-sm text-[#6B6B6B]">Market Price</TableCell>
                            <TableCell className="text-right font-mono text-sm">{pct(simResult.market_price)}</TableCell>
                            <TableCell className="text-right font-mono text-sm text-[#9B9B9B]">&mdash;</TableCell>
                            <TableCell className="text-right font-mono text-sm text-[#9B9B9B]">&mdash;</TableCell>
                          </TableRow>
                          <SimMethodRow label="Monte Carlo" data={simResult.monte_carlo} color="#3b82f6" />
                          <SimMethodRow label="Particle Filter" data={simResult.particle_filter} color="#f59e0b" />
                        </TableBody>
                      </Table>
                    </div>
                  </div>

                  {/* Chart */}
                  {simChartOption && (
                    <div className="rounded-2xl bg-white">
                      <div className="border-b border-[#E6E4DF] px-5 py-4">
                        <h2 className="text-[11px] font-medium uppercase tracking-wider text-[#9B9B9B]">
                          Probability Comparison
                        </h2>
                      </div>
                      <div className="p-4">
                        <ReactECharts
                          option={simChartOption}
                          style={{ height: 200, width: "100%" }}
                          opts={{ renderer: "canvas" }}
                        />
                      </div>
                    </div>
                  )}
                </>
              )}
            </div>
          </TabsContent>
        </Tabs>
      </div>

      {/* ── Opportunity Detail Sheet ── */}
      <Sheet open={selectedOpp !== null} onOpenChange={(open) => { if (!open) setSelectedOpp(null); }}>
        <SheetContent side="right" className="w-full border-[#E6E4DF] bg-[#F8F7F4] sm:max-w-lg">
          {selectedOpp && (
            <>
              <SheetHeader>
                <SheetTitle className="text-[#1A1A19]">Opportunity Detail</SheetTitle>
                <SheetDescription className="text-[#6B6B6B]">{selectedOpp.id}</SheetDescription>
              </SheetHeader>
              <ScrollArea className="flex-1 px-4">
                <div className="space-y-6 pb-6">
                  <div className="grid grid-cols-2 gap-4">
                    <DetailField label="Type">
                      <Badge className={cn("text-xs", arbTypeConfig[selectedOpp.arb_type].className)}>
                        {arbTypeConfig[selectedOpp.arb_type].label}
                      </Badge>
                    </DetailField>
                    <DetailField label="Detected">{timeAgo(selectedOpp.detected_at)}</DetailField>
                    <DetailField label="Gross Edge" mono>{formatDecimal(selectedOpp.gross_edge, 6)}</DetailField>
                    <DetailField label="Net Edge" mono>
                      <span className={cn("font-bold", parseFloat(selectedOpp.net_edge) > 0 ? "text-[#2D6A4F]" : "text-[#B44C3F]")}>
                        {formatBps(selectedOpp.net_edge)}
                      </span>
                    </DetailField>
                    <DetailField label="Confidence" mono>{(selectedOpp.confidence * 100).toFixed(1)}%</DetailField>
                    <DetailField label="Size Available" mono>{formatUsd(selectedOpp.size_available)}</DetailField>
                  </div>

                  {/* Markets */}
                  <div className="space-y-2">
                    <p className="text-[11px] font-medium uppercase tracking-wider text-[#9B9B9B]">Markets</p>
                    <div className="space-y-1">
                      {selectedOpp.markets.map((m, i) => (
                        <div
                          key={i}
                          className="rounded border border-[#E6E4DF] bg-white px-3 py-2 text-xs text-[#1A1A19]"
                          style={{ fontFamily: "var(--font-jetbrains-mono)" }}
                        >
                          {m}
                        </div>
                      ))}
                    </div>
                  </div>

                  {/* VWAP */}
                  {selectedOpp.estimated_vwap.length > 0 && (
                    <div className="space-y-2">
                      <p className="text-[11px] font-medium uppercase tracking-wider text-[#9B9B9B]">Estimated VWAP</p>
                      <div className="flex flex-wrap gap-2">
                        {selectedOpp.estimated_vwap.map((v, i) => (
                          <span
                            key={i}
                            className="rounded border border-[#E6E4DF] bg-white px-2 py-1 text-xs text-[#1A1A19]"
                            style={{ fontFamily: "var(--font-jetbrains-mono)" }}
                          >
                            {formatDecimal(v, 6)}
                          </span>
                        ))}
                      </div>
                    </div>
                  )}

                  {/* Legs */}
                  <div className="space-y-2">
                    <p className="text-[11px] font-medium uppercase tracking-wider text-[#9B9B9B]">
                      Trade Legs ({selectedOpp.legs.length})
                    </p>
                    <div className="space-y-2">
                      {selectedOpp.legs.map((leg, i) => (
                        <div key={i} className="rounded-2xl bg-white p-3">
                          <div className="mb-2 flex items-center gap-3">
                            <Badge
                              className={cn(
                                "text-xs",
                                leg.side === "Buy" ? "bg-[#DAE9E0] text-[#2D6A4F]" : "bg-[#F5E0DD] text-[#B44C3F]"
                              )}
                            >
                              {leg.side}
                            </Badge>
                            <span
                              className="text-xs text-[#6B6B6B]"
                              style={{ fontFamily: "var(--font-jetbrains-mono)" }}
                              title={leg.token_id}
                            >
                              {leg.token_id.length > 16
                                ? `${leg.token_id.slice(0, 8)}...${leg.token_id.slice(-6)}`
                                : leg.token_id}
                            </span>
                          </div>
                          <div className="grid grid-cols-3 gap-3 text-xs" style={{ fontFamily: "var(--font-jetbrains-mono)" }}>
                            <div>
                              <span className="text-[#6B6B6B]">Price</span>
                              <p className="mt-0.5 text-[#1A1A19]">{parseFloat(leg.target_price).toFixed(4)}</p>
                            </div>
                            <div>
                              <span className="text-[#6B6B6B]">Size</span>
                              <p className="mt-0.5 text-[#1A1A19]">{formatUsd(leg.target_size)}</p>
                            </div>
                            <div>
                              <span className="text-[#6B6B6B]">VWAP Est.</span>
                              <p className="mt-0.5 text-[#1A1A19]">{parseFloat(leg.vwap_estimate).toFixed(4)}</p>
                            </div>
                          </div>
                        </div>
                      ))}
                    </div>
                  </div>

                  {/* Simulate button in sheet */}
                  {selectedOpp.markets.length > 0 && (
                    <Button
                      onClick={() => {
                        setSimConditionId(selectedOpp.markets[0]);
                        setActiveTab("simulate");
                        setSelectedOpp(null);
                      }}
                      variant="outline"
                      className="w-full border-[#E6E4DF] text-[#1A1A19]"
                    >
                      <FlaskConical className="mr-2 h-4 w-4" />
                      Simulate This Market
                    </Button>
                  )}
                </div>
              </ScrollArea>
            </>
          )}
        </SheetContent>
      </Sheet>

      {/* ── Apply to Live Confirmation Dialog ── */}
      <Dialog open={applyDialogOpen} onOpenChange={setApplyDialogOpen}>
        <DialogContent>
          <DialogHeader>
            <DialogTitle>Apply to Live Engine</DialogTitle>
            <DialogDescription>
              This will update the live engine configuration. Changes take effect immediately.
              The sandbox parameters will be pushed to the production config.
            </DialogDescription>
          </DialogHeader>
          <DialogFooter>
            <Button variant="outline" onClick={() => setApplyDialogOpen(false)} className="border-[#E6E4DF]">
              Cancel
            </Button>
            <Button
              onClick={handleApplyToLive}
              disabled={applyLoading}
              className="bg-amber-600 text-white hover:bg-amber-700"
            >
              {applyLoading && <Loader2 className="mr-2 h-4 w-4 animate-spin" />}
              Confirm Apply
            </Button>
          </DialogFooter>
        </DialogContent>
      </Dialog>
    </div>
  );
}

// ---------------------------------------------------------------------------
// Sub-components
// ---------------------------------------------------------------------------

function DetailField({
  label,
  mono,
  children,
}: {
  label: string;
  mono?: boolean;
  children: React.ReactNode;
}) {
  return (
    <div>
      <p className="text-xs text-[#6B6B6B]">{label}</p>
      <div
        className={cn("mt-0.5 text-sm text-[#1A1A19]", mono && "font-mono")}
        style={mono ? { fontFamily: "var(--font-jetbrains-mono)" } : undefined}
      >
        {children}
      </div>
    </div>
  );
}

function EmptyState({ message, hint }: { message: string; hint: string }) {
  return (
    <div className="flex flex-col items-center justify-center py-20 text-[#6B6B6B]">
      <SearchX className="mb-3 h-10 w-10 text-[#9B9B9B]" />
      <p className="text-sm">{message}</p>
      <p className="mt-1 text-xs text-[#9B9B9B]">{hint}</p>
    </div>
  );
}

function ErrorBanner({ message }: { message: string }) {
  return (
    <div className="rounded-lg border border-[#B44C3F]/30 bg-[#F5E0DD] p-5">
      <div className="flex items-start gap-3">
        <XCircle className="mt-0.5 h-5 w-5 shrink-0 text-[#B44C3F]" />
        <div>
          <h3 className="text-sm font-medium text-[#B44C3F]">Error</h3>
          <p className="mt-1 text-sm text-[#B44C3F]/80">{message}</p>
        </div>
      </div>
    </div>
  );
}

function SimParamInput({
  label,
  value,
  type = "text",
  onChange,
}: {
  label: string;
  value: number;
  type?: string;
  onChange: (val: string) => void;
}) {
  return (
    <div className="space-y-1.5">
      <Label className="text-xs text-[#6B6B6B]">{label}</Label>
      <Input
        type={type}
        value={value}
        onChange={(e) => onChange(e.target.value)}
        className="h-8 border-[#E6E4DF] bg-[#F8F7F4] text-sm"
        style={{ fontFamily: "var(--font-jetbrains-mono)" }}
      />
    </div>
  );
}

function BacktestTradeRow({ trade }: { trade: BacktestTrade }) {
  const netPnl = parseFloat(trade.net_pnl);
  return (
    <TableRow className="border-[#E6E4DF]">
      <TableCell>
        <span
          className="text-xs text-[#6B6B6B]"
          style={{ fontFamily: "var(--font-jetbrains-mono)" }}
          title={trade.opportunity_id}
        >
          {trade.opportunity_id.length > 12
            ? `${trade.opportunity_id.slice(0, 6)}...${trade.opportunity_id.slice(-4)}`
            : trade.opportunity_id}
        </span>
      </TableCell>
      <TableCell className="text-right font-mono text-sm">{formatDecimal(trade.realized_edge, 4)}</TableCell>
      <TableCell className="text-right font-mono text-sm text-[#6B6B6B]">{formatUsd(trade.total_fees)}</TableCell>
      <TableCell className={cn("text-right font-mono text-sm font-bold", netPnl >= 0 ? "text-[#2D6A4F]" : "text-[#B44C3F]")}>
        {formatUsd(trade.net_pnl)}
      </TableCell>
      <TableCell>
        <Badge className={cn("text-xs", trade.included ? "bg-[#DAE9E0] text-[#2D6A4F]" : "bg-[#F5E0DD] text-[#B44C3F]")}>
          {trade.included ? "Included" : "Rejected"}
        </Badge>
      </TableCell>
      <TableCell className="text-xs text-[#9B9B9B]">{trade.rejection_reason ?? "—"}</TableCell>
    </TableRow>
  );
}

function SimMethodRow({
  label,
  data,
  color,
}: {
  label: string;
  data: MethodResult;
  color: string;
}) {
  return (
    <TableRow className="border-[#E6E4DF]">
      <TableCell>
        <div className="flex items-center gap-2">
          <span className="inline-block h-2.5 w-2.5 rounded-full" style={{ backgroundColor: color }} />
          <span className="text-sm text-[#1A1A19]">{label}</span>
        </div>
      </TableCell>
      <TableCell className="text-right font-mono text-sm">{pct(data.probability)}</TableCell>
      <TableCell className="text-right font-mono text-sm text-[#6B6B6B]">
        {data.std_error !== undefined ? pct(data.std_error) : "\u2014"}
      </TableCell>
      <TableCell className="text-right font-mono text-sm text-[#6B6B6B]">
        [{pct(data.ci_lower)}, {pct(data.ci_upper)}]
      </TableCell>
    </TableRow>
  );
}
