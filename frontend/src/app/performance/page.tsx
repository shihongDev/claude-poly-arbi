"use client";

import dynamic from "next/dynamic";
import { useDashboardStore } from "@/store";
import { MetricCard } from "@/components/metric-card";
import { formatPercent } from "@/lib/utils";
import type { EChartsOption } from "echarts";

const ReactECharts = dynamic(() => import("echarts-for-react"), { ssr: false });

// ---------------------------------------------------------------------------
// Helpers
// ---------------------------------------------------------------------------

const MONO_FONT = "var(--font-mono), JetBrains Mono, monospace";

/** Null-safe placeholder wrapper for chart panels */
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
    <div className="rounded-lg border border-zinc-800 bg-zinc-900 p-5">
      <h2 className="text-sm font-medium uppercase tracking-wider text-zinc-400">
        {title}
      </h2>
      <div className="mt-3 h-[280px]">
        {hasData ? (
          children
        ) : (
          <div className="flex h-full items-center justify-center text-sm text-zinc-600">
            Awaiting data...
          </div>
        )}
      </div>
    </div>
  );
}

// ---------------------------------------------------------------------------
// Chart option builders
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
                ? "#10b981" // emerald-500
                : value < 0.25
                  ? "#f59e0b" // amber-500
                  : "#ef4444", // red-500
          },
        },
        pointer: { show: false },
        axisLine: {
          lineStyle: {
            width: 14,
            color: [[1, "#27272a"]],
          },
          roundCap: true,
        },
        axisTick: { show: false },
        splitLine: { show: false },
        axisLabel: { show: false },
        markLine: {
          silent: true,
          symbol: "none",
          data: [{ yAxis: 0.25 }],
          lineStyle: { color: "#f59e0b", type: "dashed", width: 1 },
        },
        title: {
          show: true,
          offsetCenter: [0, "75%"],
          fontSize: 11,
          color: "#a1a1aa",
          fontFamily: "Inter, sans-serif",
        },
        detail: {
          offsetCenter: [0, "20%"],
          fontSize: 28,
          fontWeight: "bold",
          fontFamily: MONO_FONT,
          color: "#fafafa",
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
      backgroundColor: "#18181b",
      borderColor: "#3f3f46",
      textStyle: { color: "#fafafa", fontFamily: MONO_FONT, fontSize: 12 },
      formatter: (params: unknown) => {
        const p = (params as { value: number }[])[0];
        return `Execution Quality: ${(p.value * 100).toFixed(1)}%`;
      },
    },
    grid: {
      top: 20,
      right: 20,
      bottom: 30,
      left: 50,
    },
    xAxis: {
      type: "category",
      data: dataPoints.map((d) => d.index),
      axisLine: { lineStyle: { color: "#3f3f46" } },
      axisTick: { show: false },
      axisLabel: {
        color: "#71717a",
        fontFamily: MONO_FONT,
        fontSize: 10,
      },
    },
    yAxis: {
      type: "value",
      min: 0,
      max: 1,
      axisLine: { show: false },
      axisTick: { show: false },
      splitLine: { lineStyle: { color: "#27272a" } },
      axisLabel: {
        color: "#71717a",
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
        lineStyle: { color: "#10b981", width: 2 },
        itemStyle: { color: "#10b981" },
        areaStyle: {
          color: {
            type: "linear",
            x: 0,
            y: 0,
            x2: 0,
            y2: 1,
            colorStops: [
              { offset: 0, color: "rgba(16, 185, 129, 0.25)" },
              { offset: 1, color: "rgba(16, 185, 129, 0.02)" },
            ],
          },
        },
      },
    ],
  };
}

function buildDrawdownGaugeOption(value: number): EChartsOption {
  return {
    backgroundColor: "transparent",
    series: [
      {
        type: "gauge",
        startAngle: 200,
        endAngle: -20,
        min: 0,
        max: 25,
        radius: "90%",
        progress: {
          show: true,
          width: 14,
          roundCap: true,
          itemStyle: {
            color:
              value < 5
                ? "#10b981" // emerald-500
                : value < 10
                  ? "#f59e0b" // amber-500
                  : "#ef4444", // red-500
          },
        },
        pointer: { show: false },
        axisLine: {
          lineStyle: {
            width: 14,
            color: [[1, "#27272a"]],
          },
          roundCap: true,
        },
        axisTick: { show: false },
        splitLine: { show: false },
        axisLabel: { show: false },
        title: {
          show: true,
          offsetCenter: [0, "75%"],
          fontSize: 11,
          color: "#a1a1aa",
          fontFamily: "Inter, sans-serif",
        },
        detail: {
          offsetCenter: [0, "20%"],
          fontSize: 28,
          fontWeight: "bold",
          fontFamily: MONO_FONT,
          color: "#fafafa",
          formatter: (v: number) => `${v.toFixed(2)}%`,
        },
        data: [{ value, name: "Current Drawdown" }],
      },
    ],
  };
}

function buildPnlByStrategyOption(
  pnlByType: Record<string, string>,
): EChartsOption {
  const labels: Record<string, string> = {
    IntraMarket: "Intra-Market",
    CrossMarket: "Cross-Market",
    MultiOutcome: "Multi-Outcome",
  };

  const entries = Object.entries(pnlByType).map(([key, val]) => ({
    label: labels[key] ?? key,
    value: parseFloat(val),
  }));

  // Sort so largest absolute value is at top
  entries.sort((a, b) => Math.abs(b.value) - Math.abs(a.value));

  return {
    backgroundColor: "transparent",
    tooltip: {
      trigger: "axis",
      axisPointer: { type: "shadow" },
      backgroundColor: "#18181b",
      borderColor: "#3f3f46",
      textStyle: { color: "#fafafa", fontFamily: MONO_FONT, fontSize: 12 },
      formatter: (params: unknown) => {
        const p = (params as { name: string; value: number }[])[0];
        const prefix = p.value >= 0 ? "+" : "";
        return `${p.name}: ${prefix}$${p.value.toFixed(2)}`;
      },
    },
    grid: {
      top: 10,
      right: 30,
      bottom: 10,
      left: 110,
      containLabel: false,
    },
    xAxis: {
      type: "value",
      axisLine: { lineStyle: { color: "#3f3f46" } },
      axisTick: { show: false },
      splitLine: { lineStyle: { color: "#27272a" } },
      axisLabel: {
        color: "#71717a",
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
        color: "#a1a1aa",
        fontFamily: "Inter, sans-serif",
        fontSize: 12,
      },
    },
    series: [
      {
        type: "bar",
        data: entries.map((e) => ({
          value: e.value,
          itemStyle: {
            color: e.value >= 0 ? "#10b981" : "#ef4444",
            borderRadius: e.value >= 0 ? [0, 4, 4, 0] : [4, 0, 0, 4],
          },
        })),
        barWidth: 20,
      },
    ],
  };
}

// ---------------------------------------------------------------------------
// Page Component
// ---------------------------------------------------------------------------

export default function PerformancePage() {
  const metrics = useDashboardStore((s) => s.metrics);
  const history = useDashboardStore((s) => s.history);

  // Derive execution quality per trade from history
  const executionQualityData = history
    .slice()
    .reverse()
    .map((report, idx) => {
      const realized = parseFloat(report.realized_edge);
      const slippage = parseFloat(report.slippage);
      const fees = parseFloat(report.total_fees);
      const denominator = realized + slippage + fees;
      const quality = denominator > 0 ? realized / denominator : 0;
      return { index: idx + 1, quality: Math.max(0, Math.min(1, quality)) };
    });

  const hasPnlByType =
    metrics !== null &&
    metrics.pnl_by_type !== null &&
    Object.keys(metrics.pnl_by_type).length > 0;

  return (
    <div className="space-y-6">
      {/* Header */}
      <div>
        <h1 className="text-2xl font-bold text-white">Performance</h1>
        <p className="mt-1 text-sm text-zinc-400">
          Calibration, execution quality, and strategy-level attribution
        </p>
      </div>

      {/* KPI Cards */}
      <div className="grid gap-4 sm:grid-cols-2 lg:grid-cols-4">
        <MetricCard
          title="Brier Score"
          value={metrics ? metrics.brier_score.toFixed(4) : "\u2014"}
          delta={metrics ? "<0.25 is better than random" : undefined}
          deltaType={
            metrics
              ? metrics.brier_score < 0.25
                ? "positive"
                : "negative"
              : undefined
          }
        />
        <MetricCard
          title="Execution Quality"
          value={
            metrics
              ? formatPercent(parseFloat(metrics.execution_quality) * 100)
              : "\u2014"
          }
        />
        <MetricCard
          title="Max Drawdown"
          value={metrics ? formatPercent(metrics.drawdown_pct) : "\u2014"}
          deltaType={
            metrics
              ? metrics.drawdown_pct < 5
                ? "positive"
                : metrics.drawdown_pct < 10
                  ? "neutral"
                  : "negative"
              : undefined
          }
        />
        <MetricCard
          title="Total Trades"
          value={metrics ? metrics.trade_count.toLocaleString() : "\u2014"}
        />
      </div>

      {/* 2x2 Chart Grid */}
      <div className="grid gap-6 lg:grid-cols-2">
        {/* Brier Score Gauge */}
        <ChartPanel title="Brier Score" hasData={metrics !== null}>
          {metrics !== null && (
            <ReactECharts
              option={buildBrierGaugeOption(metrics.brier_score)}
              style={{ height: "100%", width: "100%" }}
              opts={{ renderer: "canvas" }}
            />
          )}
        </ChartPanel>

        {/* Execution Quality Trend */}
        <ChartPanel
          title="Execution Quality Trend"
          hasData={executionQualityData.length > 0}
        >
          {executionQualityData.length > 0 && (
            <ReactECharts
              option={buildExecutionQualityOption(executionQualityData)}
              style={{ height: "100%", width: "100%" }}
              opts={{ renderer: "canvas" }}
            />
          )}
        </ChartPanel>

        {/* Drawdown Indicator */}
        <ChartPanel title="Drawdown" hasData={metrics !== null}>
          {metrics !== null && (
            <ReactECharts
              option={buildDrawdownGaugeOption(metrics.drawdown_pct)}
              style={{ height: "100%", width: "100%" }}
              opts={{ renderer: "canvas" }}
            />
          )}
        </ChartPanel>

        {/* P&L by Strategy */}
        <ChartPanel title="P&L by Strategy" hasData={hasPnlByType}>
          {hasPnlByType && (
            <ReactECharts
              option={buildPnlByStrategyOption(metrics.pnl_by_type)}
              style={{ height: "100%", width: "100%" }}
              opts={{ renderer: "canvas" }}
            />
          )}
        </ChartPanel>
      </div>
    </div>
  );
}
