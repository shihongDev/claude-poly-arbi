"use client";

import { useMemo, useState, useCallback, useRef, useEffect } from "react";
import dynamic from "next/dynamic";
import { useDashboardStore } from "@/store";
import { MetricCard } from "@/components/metric-card";
import { PnlChart } from "@/components/pnl-chart";
import { RiskGauge } from "@/components/risk-gauge";
import { DataTable, type Column } from "@/components/data-table";
import { SimulationStatusPanel } from "@/components/simulation-status";
import { PositionPnlWaterfall } from "@/components/position-pnl-waterfall";
import { WinRateByStrategy } from "@/components/win-rate-by-strategy";
import { Button } from "@/components/ui/button";
import { Loader2, X } from "lucide-react";
import { toast } from "sonner";
import { closePosition, closeAllPositions } from "@/lib/api";
import {
  formatUsd,
  formatPnl,
  formatPercent,
  formatDecimal,
  truncateId,
  MONO_FONT,
  cn,
} from "@/lib/utils";
import type { Position } from "@/lib/types";
import { strategyLabels } from "@/lib/strategy-utils";
import type { EChartsOption } from "echarts";

const ReactECharts = dynamic(() => import("echarts-for-react"), { ssr: false });

// ---------------------------------------------------------------------------
// Constants & Helpers
// ---------------------------------------------------------------------------

const MAX_TOTAL_EXPOSURE = 5000;

function exposure(pos: Position): number {
  return Math.abs((parseFloat(pos.size) || 0) * (parseFloat(pos.current_price) || 0));
}

/** Null-safe wrapper for chart sections */
function ChartPanel({
  title,
  children,
  hasData,
}: {
  title: string;
  children: React.ReactNode;
  hasData: boolean;
}) {
  return (
    <div className="rounded-2xl bg-white p-5">
      <h2 className="text-[11px] font-medium uppercase tracking-wider text-[#9B9B9B]">
        {title}
      </h2>
      <div className="mt-3 h-[280px]">
        {hasData ? (
          children
        ) : (
          <div className="flex h-full items-center justify-center text-sm text-[#9B9B9B]">
            Awaiting data...
          </div>
        )}
      </div>
    </div>
  );
}

// ---------------------------------------------------------------------------
// Close Position Button (per-row action)
// ---------------------------------------------------------------------------

function ClosePositionButton({ tokenId }: { tokenId: string }) {
  const [loading, setLoading] = useState(false);

  const handleClose = useCallback(async () => {
    setLoading(true);
    try {
      await closePosition(tokenId);
      toast.success("Position closed");
    } catch (err) {
      toast.error(err instanceof Error ? err.message : "Failed to close position");
    } finally {
      setLoading(false);
    }
  }, [tokenId]);

  return (
    <Button
      variant="ghost"
      size="xs"
      className="text-[#B44C3F] hover:text-[#B44C3F] hover:bg-[#B44C3F]/10"
      disabled={loading}
      onClick={handleClose}
    >
      {loading ? <Loader2 className="animate-spin" /> : <X />}
      Close
    </Button>
  );
}

// ---------------------------------------------------------------------------
// Close All Button (card header action)
// ---------------------------------------------------------------------------

function CloseAllButton({ count }: { count: number }) {
  const [confirming, setConfirming] = useState(false);
  const [loading, setLoading] = useState(false);
  const timerRef = useRef<ReturnType<typeof setTimeout> | null>(null);

  // Auto-cancel confirmation after 3s
  useEffect(() => {
    if (confirming) {
      timerRef.current = setTimeout(() => setConfirming(false), 3000);
      return () => {
        if (timerRef.current) clearTimeout(timerRef.current);
      };
    }
  }, [confirming]);

  const handleClick = useCallback(async () => {
    if (!confirming) {
      setConfirming(true);
      return;
    }

    // Second click — execute
    if (timerRef.current) clearTimeout(timerRef.current);
    setConfirming(false);
    setLoading(true);
    try {
      const result = await closeAllPositions();
      toast.success(`Closed ${result.closed} position${result.closed === 1 ? "" : "s"}`);
    } catch (err) {
      toast.error(err instanceof Error ? err.message : "Failed to close positions");
    } finally {
      setLoading(false);
    }
  }, [confirming]);

  if (count === 0) return null;

  return (
    <Button
      variant={confirming ? "destructive" : "outline"}
      size="xs"
      disabled={loading}
      onClick={handleClick}
    >
      {loading ? (
        <Loader2 className="animate-spin" />
      ) : confirming ? (
        "Confirm Close All"
      ) : (
        `Close All (${count})`
      )}
    </Button>
  );
}

// ---------------------------------------------------------------------------
// ECharts Option Builders (from Performance)
// ---------------------------------------------------------------------------

function buildBrierGaugeOption(value: number): EChartsOption {
  return {
    backgroundColor: "transparent",
    series: [
      {
        type: "gauge",
        startAngle: 200,
        endAngle: -20,
        min: 0,
        max: 1,
        radius: "90%",
        progress: {
          show: true,
          width: 14,
          roundCap: true,
          itemStyle: {
            color:
              value < 0.15
                ? "#2D6A4F"
                : value < 0.25
                  ? "#D97706"
                  : "#B44C3F",
          },
        },
        pointer: { show: false },
        axisLine: {
          lineStyle: { width: 14, color: [[1, "#F0EEEA"]] },
          roundCap: true,
        },
        axisTick: { show: false },
        splitLine: { show: false },
        axisLabel: { show: false },
        title: {
          show: true,
          offsetCenter: [0, "75%"],
          fontSize: 11,
          color: "#6B6B6B",
          fontFamily: "Space Grotesk, sans-serif",
        },
        detail: {
          offsetCenter: [0, "20%"],
          fontSize: 28,
          fontWeight: "bold",
          fontFamily: MONO_FONT,
          color: "#1A1A19",
          formatter: (v: number) => v.toFixed(4),
        },
        data: [{ value, name: "0.25 = random baseline" }],
      },
    ],
  };
}

function buildExecutionQualityOption(
  dataPoints: { index: number; quality: number }[],
): EChartsOption {
  return {
    backgroundColor: "transparent",
    tooltip: {
      trigger: "axis",
      backgroundColor: "#FFFFFF",
      borderColor: "#E6E4DF",
      textStyle: { color: "#1A1A19", fontFamily: MONO_FONT, fontSize: 12 },
      formatter: (params: unknown) => {
        const p = (params as { value: number }[])[0];
        return `Execution Quality: ${(p.value * 100).toFixed(1)}%`;
      },
    },
    grid: { top: 20, right: 20, bottom: 30, left: 50 },
    xAxis: {
      type: "category",
      data: dataPoints.map((d) => d.index),
      axisLine: { lineStyle: { color: "#E6E4DF" } },
      axisTick: { show: false },
      axisLabel: { color: "#9B9B9B", fontFamily: MONO_FONT, fontSize: 10 },
    },
    yAxis: {
      type: "value",
      min: 0,
      max: 1,
      axisLine: { show: false },
      axisTick: { show: false },
      splitLine: { lineStyle: { color: "#F0EEEA" } },
      axisLabel: {
        color: "#9B9B9B",
        fontFamily: MONO_FONT,
        fontSize: 10,
        formatter: (v: number) => `${(v * 100).toFixed(0)}%`,
      },
    },
    series: [
      {
        type: "line",
        data: dataPoints.map((d) => d.quality),
        smooth: true,
        symbol: "circle",
        symbolSize: 4,
        lineStyle: { color: "#2D6A4F", width: 2 },
        itemStyle: { color: "#2D6A4F" },
        areaStyle: {
          color: {
            type: "linear",
            x: 0, y: 0, x2: 0, y2: 1,
            colorStops: [
              { offset: 0, color: "rgba(45, 106, 79, 0.25)" },
              { offset: 1, color: "rgba(45, 106, 79, 0.02)" },
            ],
          },
        },
      },
    ],
  };
}

function buildPnlByStrategyOption(
  pnlByType: Record<string, string>,
): EChartsOption {
  const entries = Object.entries(pnlByType).map(([key, val]) => ({
    label: strategyLabels[key] ?? key,
    value: parseFloat(val),
  }));

  entries.sort((a, b) => Math.abs(b.value) - Math.abs(a.value));

  return {
    backgroundColor: "transparent",
    tooltip: {
      trigger: "axis",
      axisPointer: { type: "shadow" },
      backgroundColor: "#FFFFFF",
      borderColor: "#E6E4DF",
      textStyle: { color: "#1A1A19", fontFamily: MONO_FONT, fontSize: 12 },
      formatter: (params: unknown) => {
        const p = (params as { name: string; value: number }[])[0];
        const prefix = p.value >= 0 ? "+" : "";
        return `${p.name}: ${prefix}$${p.value.toFixed(2)}`;
      },
    },
    grid: { top: 10, right: 30, bottom: 10, left: 110, containLabel: false },
    xAxis: {
      type: "value",
      axisLine: { lineStyle: { color: "#E6E4DF" } },
      axisTick: { show: false },
      splitLine: { lineStyle: { color: "#F0EEEA" } },
      axisLabel: {
        color: "#9B9B9B",
        fontFamily: MONO_FONT,
        fontSize: 10,
        formatter: (v: number) => {
          if (v === 0) return "$0";
          return v > 0 ? `+$${v}` : `-$${Math.abs(v)}`;
        },
      },
    },
    yAxis: {
      type: "category",
      data: entries.map((e) => e.label),
      axisLine: { show: false },
      axisTick: { show: false },
      axisLabel: {
        color: "#6B6B6B",
        fontFamily: "Space Grotesk, sans-serif",
        fontSize: 12,
      },
    },
    series: [
      {
        type: "bar",
        data: entries.map((e) => ({
          value: e.value,
          itemStyle: {
            color: e.value >= 0 ? "#2D6A4F" : "#B44C3F",
            borderRadius: e.value >= 0 ? [0, 4, 4, 0] : [4, 0, 0, 4],
          },
        })),
        barWidth: 20,
      },
    ],
  };
}

// ---------------------------------------------------------------------------
// Exposure Donut Builder (from Positions)
// ---------------------------------------------------------------------------

const SLICE_COLORS = [
  "#2D6A4F", "#3b82f6", "#D97706", "#8b5cf6",
  "#ec4899", "#06b6d4", "#f97316", "#14b8a6",
];

function buildExposureDonutOption(positions: Position[]) {
  const dataByMarket = new Map<string, { label: string; value: number }>();
  for (const pos of positions) {
    const key = pos.condition_id;
    const existing = dataByMarket.get(key);
    dataByMarket.set(key, {
      label: existing?.label ?? truncateId(pos.condition_id, 6),
      value: (existing?.value ?? 0) + exposure(pos),
    });
  }

  const seriesData = Array.from(dataByMarket.values()).map(
    ({ label, value }) => ({ name: label, value: Math.round(value * 100) / 100 })
  );

  return {
    tooltip: {
      trigger: "item" as const,
      backgroundColor: "#FFFFFF",
      borderColor: "#E6E4DF",
      textStyle: { color: "#1A1A19", fontFamily: MONO_FONT },
      formatter: (params: { name: string; value: number; percent: number }) =>
        `${params.name}<br/>$${params.value.toFixed(2)} (${params.percent.toFixed(1)}%)`,
    },
    legend: {
      bottom: 0,
      textStyle: { color: "#6B6B6B", fontSize: 11 },
      itemWidth: 10,
      itemHeight: 10,
    },
    series: [
      {
        type: "pie" as const,
        radius: ["45%", "70%"],
        center: ["50%", "45%"],
        avoidLabelOverlap: true,
        itemStyle: {
          borderRadius: 4,
          borderColor: "#F8F7F4",
          borderWidth: 2,
        },
        label: { show: false },
        emphasis: {
          label: {
            show: true,
            fontSize: 12,
            fontWeight: "bold",
            color: "#1A1A19",
          },
        },
        data: seriesData.map((d, i) => ({
          ...d,
          itemStyle: { color: SLICE_COLORS[i % SLICE_COLORS.length] },
        })),
      },
    ],
  };
}

// ---------------------------------------------------------------------------
// Page Component
// ---------------------------------------------------------------------------

export default function PortfolioPage() {
  const metrics = useDashboardStore((s) => s.metrics);
  const positions = useDashboardStore((s) => s.positions);
  const history = useDashboardStore((s) => s.history);
  const opportunities = useDashboardStore((s) => s.opportunities);

  // -- Equity curve (from Dashboard) --
  const equityCurve = useMemo(() => {
    if (!history || history.length === 0) return [];
    const sorted = [...history].sort(
      (a, b) =>
        new Date(a.timestamp).getTime() - new Date(b.timestamp).getTime()
    );
    let cumulative = 0;
    return sorted.map((report) => {
      cumulative += parseFloat(report.realized_edge) - parseFloat(report.total_fees);
      return { time: report.timestamp.slice(0, 10), value: cumulative };
    });
  }, [history]);

  // -- Active positions (size > 0) --
  const activePositions = useMemo(
    () => positions.filter((p) => parseFloat(p.size) > 0),
    [positions]
  );

  // -- Positions table columns --
  const positionColumns: Column<Position>[] = useMemo(
    () => [
      {
        key: "token_id",
        header: "Token ID",
        sortable: true,
        mono: true,
        render: (row: Position) => (
          <span className="text-[#1A1A19]" title={row.token_id}>
            {truncateId(row.token_id)}
          </span>
        ),
        getValue: (row: Position) => row.token_id,
      },
      {
        key: "condition_id",
        header: "Condition ID",
        sortable: true,
        mono: true,
        render: (row: Position) => (
          <span className="text-[#1A1A19]" title={row.condition_id}>
            {truncateId(row.condition_id)}
          </span>
        ),
        getValue: (row: Position) => row.condition_id,
      },
      {
        key: "size",
        header: "Size",
        sortable: true,
        mono: true,
        render: (row: Position) => (
          <span className="text-[#1A1A19]">{formatDecimal(row.size, 4)}</span>
        ),
        getValue: (row: Position) => parseFloat(row.size),
      },
      {
        key: "entry_price",
        header: "Entry Price",
        sortable: true,
        mono: true,
        render: (row: Position) => (
          <span className="text-[#1A1A19]">
            {formatDecimal(row.avg_entry_price, 4)}
          </span>
        ),
        getValue: (row: Position) => parseFloat(row.avg_entry_price),
      },
      {
        key: "current_price",
        header: "Current Price",
        sortable: true,
        mono: true,
        render: (row: Position) => (
          <span className="text-[#1A1A19]">
            {formatDecimal(row.current_price, 4)}
          </span>
        ),
        getValue: (row: Position) => parseFloat(row.current_price),
      },
      {
        key: "unrealized_pnl",
        header: "Unrealized P&L",
        sortable: true,
        mono: true,
        render: (row: Position) => {
          const pnl = parseFloat(row.unrealized_pnl);
          return (
            <span className={pnl >= 0 ? "text-[#2D6A4F]" : "text-[#B44C3F]"}>
              {formatPnl(row.unrealized_pnl)}
            </span>
          );
        },
        getValue: (row: Position) => parseFloat(row.unrealized_pnl),
      },
      {
        key: "actions",
        header: "",
        sortable: false,
        render: (row: Position) => <ClosePositionButton tokenId={row.token_id} />,
      },
    ],
    []
  );

  // -- Positions aggregates (from Positions) --
  const totalExposure = useMemo(
    () => activePositions.reduce((sum, p) => sum + exposure(p), 0),
    [activePositions]
  );

  const totalUnrealizedPnl = useMemo(
    () => activePositions.reduce((sum, p) => sum + parseFloat(p.unrealized_pnl), 0),
    [activePositions]
  );

  // -- Exposure donut (from Positions) --
  const exposureChartOption = useMemo(
    () => (activePositions.length > 0 ? buildExposureDonutOption(activePositions) : null),
    [activePositions]
  );

  // -- Execution quality trend (from Performance) --
  const executionQualityData = useMemo(
    () =>
      history
        .slice()
        .reverse()
        .map((report, idx) => {
          const realized = parseFloat(report.realized_edge);
          const slippage = parseFloat(report.slippage);
          const fees = parseFloat(report.total_fees);
          const denominator = realized + slippage + fees;
          const quality = denominator > 0 ? realized / denominator : 0;
          return { index: idx + 1, quality: Math.max(0, Math.min(1, quality)) };
        }),
    [history]
  );

  // Treat metrics as unavailable if the server returned an empty object (before first engine cycle)
  const metricsReady =
    metrics != null && metrics.brier_score != null && metrics.total_pnl != null;
  const currentExposure = metricsReady ? parseFloat(metrics.current_exposure) : 0;
  const drawdownPct = metricsReady ? metrics.drawdown_pct : 0;
  const hasPnlByType =
    metricsReady &&
    metrics.pnl_by_type != null &&
    Object.keys(metrics.pnl_by_type).length > 0;

  // Memoize ECharts options to prevent full chart re-initialization on every render
  const brierOption = useMemo(
    () => (metricsReady ? buildBrierGaugeOption(metrics.brier_score) : null),
    [metricsReady, metrics?.brier_score]
  );

  const execQualityOption = useMemo(
    () =>
      executionQualityData.length > 0
        ? buildExecutionQualityOption(executionQualityData)
        : null,
    [executionQualityData]
  );

  const pnlByStrategyOption = useMemo(
    () =>
      hasPnlByType ? buildPnlByStrategyOption(metrics.pnl_by_type) : null,
    [hasPnlByType, metrics?.pnl_by_type]
  );

  return (
    <div className="space-y-8">
      {/* Header */}
      <div>
        <h1 className="text-2xl font-bold text-[#1A1A19]">Portfolio</h1>
        <p className="mt-1 text-sm text-[#6B6B6B]">
          Positions, performance, and risk
        </p>
      </div>

      {/* Hero P&L */}
      {metricsReady && (
        <div className="py-4">
          <p className="text-[11px] font-medium uppercase tracking-wider text-[#9B9B9B]">
            Total P&L
          </p>
          <p
            className={cn(
              "mt-2 text-[56px] font-bold leading-none",
              parseFloat(metrics.total_pnl) >= 0 ? "text-[#2D6A4F]" : "text-[#B44C3F]"
            )}
            style={{ fontFamily: "var(--font-space-grotesk)" }}
          >
            {formatPnl(metrics.total_pnl)}
          </p>
          <p className="mt-2 text-sm text-[#6B6B6B]">
            {formatPnl(metrics.daily_pnl)} today
          </p>
        </div>
      )}

      {/* Section 1: 6 KPI Cards */}
      <div className="grid gap-4 sm:grid-cols-2 lg:grid-cols-3 xl:grid-cols-6">
        <MetricCard
          title="Daily P&L"
          value={metricsReady ? formatPnl(metrics.daily_pnl) : "\u2014"}
          deltaType={
            metricsReady
              ? parseFloat(metrics.daily_pnl) >= 0
                ? "positive"
                : "negative"
              : undefined
          }
        />
        <MetricCard
          title="Total Exposure"
          value={formatUsd(totalExposure.toFixed(2))}
        />
        <MetricCard
          title="Unrealized P&L"
          value={formatPnl(totalUnrealizedPnl.toFixed(2))}
          deltaType={totalUnrealizedPnl >= 0 ? "positive" : "negative"}
        />
        <MetricCard
          title="Open Positions"
          value={activePositions.length.toLocaleString()}
        />
        <MetricCard
          title="Brier Score"
          value={metricsReady ? metrics.brier_score.toFixed(4) : "\u2014"}
          delta={metricsReady ? "<0.25 is better than random" : undefined}
          deltaType={
            metricsReady
              ? metrics.brier_score < 0.25
                ? "positive"
                : "negative"
              : undefined
          }
        />
        <MetricCard
          title="Total Trades"
          value={metricsReady ? metrics.trade_count.toLocaleString() : "\u2014"}
        />
      </div>

      {/* Section 2: Equity Curve + Risk Gauges */}
      <div className="grid gap-6 lg:grid-cols-3">
        <div className="lg:col-span-2 rounded-2xl bg-white p-5">
          <h2 className="text-[11px] font-medium uppercase tracking-wider text-[#9B9B9B]">
            Equity Curve
          </h2>
          <div className="mt-4 h-[280px]">
            {equityCurve.length > 0 ? (
              <PnlChart data={equityCurve} />
            ) : (
              <div className="flex h-full items-center justify-center text-sm text-[#9B9B9B]">
                No trade history yet
              </div>
            )}
          </div>
        </div>

        <div className="flex flex-col gap-6">
          <div className="flex-1 rounded-2xl bg-white p-5">
            <h2 className="text-[11px] font-medium uppercase tracking-wider text-[#9B9B9B]">
              Exposure
            </h2>
            <div className="mt-2 h-[120px]">
              <RiskGauge
                value={currentExposure}
                max={MAX_TOTAL_EXPOSURE}
                label="Exposure ($)"
                warningThreshold={0.6}
                criticalThreshold={0.8}
              />
            </div>
            <p
              className="mt-1 text-center text-xs text-[#9B9B9B]"
              style={{ fontFamily: MONO_FONT }}
            >
              {formatUsd(String(currentExposure))} / {formatUsd(String(MAX_TOTAL_EXPOSURE))}
            </p>
          </div>
          <div className="flex-1 rounded-2xl bg-white p-5">
            <h2 className="text-[11px] font-medium uppercase tracking-wider text-[#9B9B9B]">
              Drawdown
            </h2>
            <div className="mt-2 h-[120px]">
              <RiskGauge
                value={drawdownPct}
                max={100}
                label="Drawdown (%)"
                warningThreshold={0.05}
                criticalThreshold={0.1}
              />
            </div>
            <p
              className="mt-1 text-center text-xs text-[#9B9B9B]"
              style={{ fontFamily: MONO_FONT }}
            >
              {formatPercent(drawdownPct)}
            </p>
          </div>
        </div>
      </div>

      {/* Section 3: Positions Table + Exposure Donut */}
      <div className="grid gap-6 lg:grid-cols-5">
        <div className="rounded-2xl bg-white lg:col-span-3">
          <div className="flex items-center justify-between border-b border-[#E6E4DF] px-5 py-4">
            <h2 className="text-[11px] font-medium uppercase tracking-wider text-[#9B9B9B]">
              Open Positions
            </h2>
            <CloseAllButton count={activePositions.length} />
          </div>
          {activePositions.length === 0 ? (
            <div className="flex h-[300px] items-center justify-center text-sm text-[#9B9B9B]">
              No open positions
            </div>
          ) : (
            <DataTable
              columns={positionColumns}
              data={activePositions}
              pageSize={10}
            />
          )}
        </div>

        <div className="rounded-2xl bg-white lg:col-span-2">
          <div className="border-b border-[#E6E4DF] px-5 py-4">
            <h2 className="text-[11px] font-medium uppercase tracking-wider text-[#9B9B9B]">
              Exposure by Market
            </h2>
          </div>
          <div className="p-4">
            {exposureChartOption ? (
              <ReactECharts
                option={exposureChartOption}
                style={{ height: 300, width: "100%" }}
                opts={{ renderer: "canvas" }}
              />
            ) : (
              <div className="flex h-[300px] items-center justify-center text-sm text-[#9B9B9B]">
                No position data to chart
              </div>
            )}
          </div>
        </div>
      </div>

      {/* Section 3b: Position PnL Waterfall */}
      <PositionPnlWaterfall positions={positions} />

      {/* Section 4: Performance Charts — Brier + Exec Quality + P&L by Strategy */}
      <div className="grid gap-6 lg:grid-cols-3">
        <ChartPanel title="Brier Score" hasData={brierOption !== null}>
          {brierOption && (
            <ReactECharts
              option={brierOption}
              style={{ height: "100%", width: "100%" }}
              opts={{ renderer: "canvas" }}
            />
          )}
        </ChartPanel>

        <ChartPanel
          title="Execution Quality Trend"
          hasData={execQualityOption !== null}
        >
          {execQualityOption && (
            <ReactECharts
              option={execQualityOption}
              style={{ height: "100%", width: "100%" }}
              opts={{ renderer: "canvas" }}
            />
          )}
        </ChartPanel>

        <ChartPanel title="P&L by Strategy" hasData={pnlByStrategyOption !== null}>
          {pnlByStrategyOption && (
            <ReactECharts
              option={pnlByStrategyOption}
              style={{ height: "100%", width: "100%" }}
              opts={{ renderer: "canvas" }}
            />
          )}
        </ChartPanel>
      </div>

      {/* Section 5: Win Rate by Strategy */}
      <WinRateByStrategy history={history} opportunities={opportunities} />

      {/* Section 6: Simulation Engine Status */}
      <SimulationStatusPanel />
    </div>
  );
}
