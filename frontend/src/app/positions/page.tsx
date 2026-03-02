"use client";

import { useMemo } from "react";
import dynamic from "next/dynamic";
import { useDashboardStore } from "@/store";
import { MetricCard } from "@/components/metric-card";
import { DataTable, type Column } from "@/components/data-table";
import { formatUsd, formatPnl, formatDecimal } from "@/lib/utils";
import type { Position, ArbType } from "@/lib/types";

const ReactECharts = dynamic(() => import("echarts-for-react"), { ssr: false });

const ARB_TYPES: ArbType[] = ["IntraMarket", "CrossMarket", "MultiOutcome"];

const ARB_TYPE_LABELS: Record<ArbType, string> = {
  IntraMarket: "Intra-Market",
  CrossMarket: "Cross-Market",
  MultiOutcome: "Multi-Outcome",
};

function truncateId(id: string, chars = 8): string {
  if (id.length <= chars * 2 + 3) return id;
  return `${id.slice(0, chars)}...${id.slice(-chars)}`;
}

function exposure(pos: Position): number {
  return Math.abs(parseFloat(pos.size) * parseFloat(pos.current_price));
}

const POSITION_COLUMNS: Column<Position>[] = [
  {
    key: "token_id",
    header: "Token ID",
    sortable: true,
    mono: true,
    render: (row) => (
      <span className="text-zinc-300" title={row.token_id}>
        {truncateId(row.token_id)}
      </span>
    ),
    getValue: (row) => row.token_id,
  },
  {
    key: "condition_id",
    header: "Condition ID",
    sortable: true,
    mono: true,
    render: (row) => (
      <span className="text-zinc-300" title={row.condition_id}>
        {truncateId(row.condition_id)}
      </span>
    ),
    getValue: (row) => row.condition_id,
  },
  {
    key: "size",
    header: "Size",
    sortable: true,
    mono: true,
    render: (row) => (
      <span className="text-zinc-200">{formatDecimal(row.size, 4)}</span>
    ),
    getValue: (row) => parseFloat(row.size),
  },
  {
    key: "entry_price",
    header: "Entry Price",
    sortable: true,
    mono: true,
    render: (row) => (
      <span className="text-zinc-200">
        {formatDecimal(row.avg_entry_price, 4)}
      </span>
    ),
    getValue: (row) => parseFloat(row.avg_entry_price),
  },
  {
    key: "current_price",
    header: "Current Price",
    sortable: true,
    mono: true,
    render: (row) => (
      <span className="text-zinc-200">
        {formatDecimal(row.current_price, 4)}
      </span>
    ),
    getValue: (row) => parseFloat(row.current_price),
  },
  {
    key: "unrealized_pnl",
    header: "Unrealized P&L",
    sortable: true,
    mono: true,
    render: (row) => {
      const pnl = parseFloat(row.unrealized_pnl);
      return (
        <span className={pnl >= 0 ? "text-emerald-500" : "text-red-500"}>
          {formatPnl(row.unrealized_pnl)}
        </span>
      );
    },
    getValue: (row) => parseFloat(row.unrealized_pnl),
  },
];

export default function PositionsPage() {
  const positions = useDashboardStore((s) => s.positions);
  const metrics = useDashboardStore((s) => s.metrics);

  const totalExposure = useMemo(
    () => positions.reduce((sum, p) => sum + exposure(p), 0),
    [positions]
  );

  const totalUnrealizedPnl = useMemo(
    () =>
      positions.reduce((sum, p) => sum + parseFloat(p.unrealized_pnl), 0),
    [positions]
  );

  const pnlDeltaType = totalUnrealizedPnl >= 0 ? "positive" : "negative";

  // Exposure donut chart options
  const exposureChartOption = useMemo(() => {
    if (positions.length === 0) return null;

    const sliceColors = [
      "#10b981", // emerald-500
      "#3b82f6", // blue-500
      "#f59e0b", // amber-500
      "#8b5cf6", // violet-500
      "#ec4899", // pink-500
      "#06b6d4", // cyan-500
      "#f97316", // orange-500
      "#14b8a6", // teal-500
    ];

    const dataByMarket = new Map<string, number>();
    for (const pos of positions) {
      const key = truncateId(pos.condition_id, 6);
      dataByMarket.set(key, (dataByMarket.get(key) ?? 0) + exposure(pos));
    }

    const seriesData = Array.from(dataByMarket.entries()).map(
      ([name, value]) => ({ name, value: Math.round(value * 100) / 100 })
    );

    return {
      tooltip: {
        trigger: "item" as const,
        backgroundColor: "#18181b",
        borderColor: "#3f3f46",
        textStyle: { color: "#d4d4d8", fontFamily: "var(--font-mono)" },
        formatter: (params: { name: string; value: number; percent: number }) =>
          `${params.name}<br/>$${params.value.toFixed(2)} (${params.percent.toFixed(1)}%)`,
      },
      legend: {
        bottom: 0,
        textStyle: { color: "#a1a1aa", fontSize: 11 },
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
            borderColor: "#09090b",
            borderWidth: 2,
          },
          label: { show: false },
          emphasis: {
            label: {
              show: true,
              fontSize: 12,
              fontWeight: "bold",
              color: "#fafafa",
            },
          },
          data: seriesData.map((d, i) => ({
            ...d,
            itemStyle: { color: sliceColors[i % sliceColors.length] },
          })),
        },
      ],
    };
  }, [positions]);

  // P&L by Strategy bar chart options
  const pnlByTypeChartOption = useMemo(() => {
    if (!metrics?.pnl_by_type) return null;

    const categories: string[] = [];
    const values: number[] = [];
    const colors: string[] = [];

    for (const arbType of ARB_TYPES) {
      const raw = metrics.pnl_by_type[arbType];
      const val = raw !== undefined ? parseFloat(raw) : 0;
      categories.push(ARB_TYPE_LABELS[arbType]);
      values.push(Math.round(val * 100) / 100);
      colors.push(val >= 0 ? "#10b981" : "#ef4444");
    }

    return {
      tooltip: {
        trigger: "axis" as const,
        backgroundColor: "#18181b",
        borderColor: "#3f3f46",
        textStyle: { color: "#d4d4d8", fontFamily: "var(--font-mono)" },
        formatter: (params: Array<{ name: string; value: number }>) => {
          const p = params[0];
          const prefix = p.value >= 0 ? "+" : "";
          return `${p.name}<br/>${prefix}$${p.value.toFixed(2)}`;
        },
      },
      grid: {
        left: 60,
        right: 24,
        top: 16,
        bottom: 40,
      },
      xAxis: {
        type: "category" as const,
        data: categories,
        axisLine: { lineStyle: { color: "#3f3f46" } },
        axisLabel: { color: "#a1a1aa", fontSize: 11 },
        axisTick: { show: false },
      },
      yAxis: {
        type: "value" as const,
        axisLine: { show: false },
        axisLabel: {
          color: "#a1a1aa",
          fontSize: 11,
          fontFamily: "var(--font-mono)",
          formatter: (v: number) => `$${v}`,
        },
        splitLine: { lineStyle: { color: "#27272a" } },
      },
      series: [
        {
          type: "bar" as const,
          data: values.map((v, i) => ({
            value: v,
            itemStyle: { color: colors[i], borderRadius: [4, 4, 0, 0] },
          })),
          barWidth: "40%",
        },
      ],
    };
  }, [metrics]);

  return (
    <div className="space-y-6">
      <div>
        <h1 className="text-2xl font-bold text-white">Positions</h1>
        <p className="mt-1 text-sm text-zinc-400">
          Open positions, exposure breakdown, and strategy performance
        </p>
      </div>

      {/* KPI Row */}
      <div className="grid gap-4 sm:grid-cols-2 lg:grid-cols-3">
        <MetricCard
          title="Total Exposure"
          value={formatUsd(totalExposure.toFixed(2))}
        />
        <MetricCard
          title="Total Unrealized P&L"
          value={formatPnl(totalUnrealizedPnl.toFixed(2))}
          deltaType={pnlDeltaType}
        />
        <MetricCard
          title="Open Positions"
          value={positions.length.toLocaleString()}
        />
      </div>

      {/* Two-column: Table + Donut */}
      <div className="grid gap-6 lg:grid-cols-5">
        {/* Left: Positions table (60%) */}
        <div className="rounded-lg border border-zinc-800 bg-zinc-900 lg:col-span-3">
          <div className="border-b border-zinc-800 px-5 py-4">
            <h2 className="text-sm font-medium uppercase tracking-wider text-zinc-400">
              Open Positions
            </h2>
          </div>
          {positions.length === 0 ? (
            <div className="flex h-[300px] items-center justify-center text-sm text-zinc-600">
              No open positions
            </div>
          ) : (
            <DataTable
              columns={POSITION_COLUMNS}
              data={positions}
              pageSize={10}
            />
          )}
        </div>

        {/* Right: Exposure donut (40%) */}
        <div className="rounded-lg border border-zinc-800 bg-zinc-900 lg:col-span-2">
          <div className="border-b border-zinc-800 px-5 py-4">
            <h2 className="text-sm font-medium uppercase tracking-wider text-zinc-400">
              Exposure by Market
            </h2>
          </div>
          <div className="p-4">
            {exposureChartOption ? (
              <ReactECharts
                option={exposureChartOption}
                style={{ height: 300, width: "100%" }}
                opts={{ renderer: "canvas" }}
                theme="dark"
              />
            ) : (
              <div className="flex h-[300px] items-center justify-center text-sm text-zinc-600">
                No position data to chart
              </div>
            )}
          </div>
        </div>
      </div>

      {/* Bottom: P&L by Strategy bar chart */}
      <div className="rounded-lg border border-zinc-800 bg-zinc-900">
        <div className="border-b border-zinc-800 px-5 py-4">
          <h2 className="text-sm font-medium uppercase tracking-wider text-zinc-400">
            P&L by Strategy
          </h2>
        </div>
        <div className="p-4">
          {pnlByTypeChartOption ? (
            <ReactECharts
              option={pnlByTypeChartOption}
              style={{ height: 280, width: "100%" }}
              opts={{ renderer: "canvas" }}
              theme="dark"
            />
          ) : (
            <div className="flex h-[280px] items-center justify-center text-sm text-zinc-600">
              No strategy performance data available
            </div>
          )}
        </div>
      </div>
    </div>
  );
}
