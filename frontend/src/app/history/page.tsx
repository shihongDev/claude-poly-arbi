"use client";

import { useState, useMemo } from "react";
import dynamic from "next/dynamic";
import { useDashboardStore } from "@/store";
import { MetricCard } from "@/components/metric-card";
import { DataTable, type Column } from "@/components/data-table";
import { Badge } from "@/components/ui/badge";
import {
  Dialog,
  DialogContent,
  DialogHeader,
  DialogTitle,
  DialogDescription,
} from "@/components/ui/dialog";
import {
  Table,
  TableBody,
  TableCell,
  TableHead,
  TableHeader,
  TableRow,
} from "@/components/ui/table";
import { Button } from "@/components/ui/button";
import {
  formatUsd,
  formatPnl,
  formatDecimal,
  timeAgo,
  cn,
} from "@/lib/utils";
import type { ExecutionReport, LegReport, TradingMode } from "@/lib/types";

const ReactECharts = dynamic(() => import("echarts-for-react"), { ssr: false });

type ModeFilter = "All" | "Paper" | "Live";

const MODE_FILTERS: ModeFilter[] = ["All", "Paper", "Live"];

function truncateId(id: string, chars = 8): string {
  if (id.length <= chars) return id;
  return id.slice(0, chars);
}

function modeBadge(mode: TradingMode) {
  return mode === "Paper" ? (
    <Badge className="bg-blue-500/15 text-blue-400 border-blue-500/25">
      Paper
    </Badge>
  ) : (
    <Badge className="bg-emerald-500/15 text-emerald-400 border-emerald-500/25">
      Live
    </Badge>
  );
}

function statusBadge(status: LegReport["status"]) {
  switch (status) {
    case "FullyFilled":
      return (
        <Badge className="bg-emerald-500/15 text-emerald-400 border-emerald-500/25">
          Filled
        </Badge>
      );
    case "PartiallyFilled":
      return (
        <Badge className="bg-amber-500/15 text-amber-400 border-amber-500/25">
          Partial
        </Badge>
      );
    case "Rejected":
      return (
        <Badge className="bg-red-500/15 text-red-400 border-red-500/25">
          Rejected
        </Badge>
      );
    case "Cancelled":
      return (
        <Badge className="bg-zinc-500/15 text-zinc-400 border-zinc-500/25">
          Cancelled
        </Badge>
      );
  }
}

function sideBadge(side: LegReport["side"]) {
  return side === "Buy" ? (
    <Badge className="bg-emerald-500/15 text-emerald-400 border-emerald-500/25">
      Buy
    </Badge>
  ) : (
    <Badge className="bg-red-500/15 text-red-400 border-red-500/25">
      Sell
    </Badge>
  );
}

const HISTORY_COLUMNS: Column<ExecutionReport>[] = [
  {
    key: "timestamp",
    header: "Time",
    sortable: true,
    render: (row) => (
      <span className="text-zinc-300">{timeAgo(row.timestamp)}</span>
    ),
    getValue: (row) => new Date(row.timestamp).getTime(),
  },
  {
    key: "opportunity_id",
    header: "Opportunity",
    sortable: true,
    mono: true,
    render: (row) => (
      <span className="text-zinc-300" title={row.opportunity_id}>
        {truncateId(row.opportunity_id)}
      </span>
    ),
    getValue: (row) => row.opportunity_id,
  },
  {
    key: "mode",
    header: "Mode",
    sortable: false,
    render: (row) => modeBadge(row.mode),
  },
  {
    key: "legs",
    header: "Legs",
    sortable: true,
    mono: true,
    render: (row) => (
      <span className="text-zinc-300">{row.legs.length}</span>
    ),
    getValue: (row) => row.legs.length,
  },
  {
    key: "realized_edge",
    header: "Realized Edge",
    sortable: true,
    mono: true,
    render: (row) => {
      const val = parseFloat(row.realized_edge);
      return (
        <span className={val >= 0 ? "text-emerald-500" : "text-red-500"}>
          {formatPnl(row.realized_edge)}
        </span>
      );
    },
    getValue: (row) => parseFloat(row.realized_edge),
  },
  {
    key: "slippage",
    header: "Slippage",
    sortable: true,
    mono: true,
    render: (row) => {
      const val = Math.abs(parseFloat(row.slippage));
      const isHigh = val > 0.005;
      return (
        <span className={isHigh ? "text-red-500" : "text-zinc-300"}>
          {formatDecimal(row.slippage, 4)}
        </span>
      );
    },
    getValue: (row) => parseFloat(row.slippage),
  },
  {
    key: "total_fees",
    header: "Total Fees",
    sortable: true,
    mono: true,
    render: (row) => (
      <span className="text-zinc-300">{formatUsd(row.total_fees)}</span>
    ),
    getValue: (row) => parseFloat(row.total_fees),
  },
];

export default function HistoryPage() {
  const history = useDashboardStore((s) => s.history);
  const [modeFilter, setModeFilter] = useState<ModeFilter>("All");
  const [selectedReport, setSelectedReport] =
    useState<ExecutionReport | null>(null);

  const filteredHistory = useMemo(() => {
    if (modeFilter === "All") return history;
    return history.filter((r) => r.mode === modeFilter);
  }, [history, modeFilter]);

  // KPI computations
  const totalTrades = filteredHistory.length;

  const totalRealizedEdge = useMemo(
    () => filteredHistory.reduce((s, r) => s + parseFloat(r.realized_edge), 0),
    [filteredHistory]
  );

  const avgSlippage = useMemo(() => {
    if (filteredHistory.length === 0) return 0;
    const sum = filteredHistory.reduce(
      (s, r) => s + parseFloat(r.slippage),
      0
    );
    return sum / filteredHistory.length;
  }, [filteredHistory]);

  // Slippage scatter data
  const scatterChartOption = useMemo(() => {
    const overpaid: [number, number][] = [];
    const underpaid: [number, number][] = [];

    for (const report of filteredHistory) {
      for (const leg of report.legs) {
        const expected = parseFloat(leg.expected_vwap);
        const actual = parseFloat(leg.actual_fill_price);
        if (isNaN(expected) || isNaN(actual)) continue;
        if (actual >= expected) {
          overpaid.push([expected, actual]);
        } else {
          underpaid.push([expected, actual]);
        }
      }
    }

    if (overpaid.length === 0 && underpaid.length === 0) return null;

    // Compute axis range for the diagonal line
    const allX = [...overpaid, ...underpaid].map((p) => p[0]);
    const allY = [...overpaid, ...underpaid].map((p) => p[1]);
    const minVal = Math.min(...allX, ...allY);
    const maxVal = Math.max(...allX, ...allY);
    const padding = (maxVal - minVal) * 0.05 || 0.01;
    const axisMin = Math.max(0, minVal - padding);
    const axisMax = maxVal + padding;

    return {
      tooltip: {
        trigger: "item" as const,
        backgroundColor: "#18181b",
        borderColor: "#3f3f46",
        textStyle: { color: "#d4d4d8", fontFamily: "var(--font-mono)" },
        formatter: (params: { value: [number, number] }) => {
          const [expected, actual] = params.value;
          const diff = actual - expected;
          const sign = diff >= 0 ? "+" : "";
          return [
            `Expected VWAP: ${expected.toFixed(4)}`,
            `Actual Fill: ${actual.toFixed(4)}`,
            `Slippage: ${sign}${diff.toFixed(4)}`,
          ].join("<br/>");
        },
      },
      grid: { left: 60, right: 24, top: 24, bottom: 48 },
      xAxis: {
        type: "value" as const,
        name: "Expected VWAP",
        nameLocation: "center" as const,
        nameGap: 32,
        nameTextStyle: { color: "#a1a1aa", fontSize: 11 },
        min: axisMin,
        max: axisMax,
        axisLine: { lineStyle: { color: "#3f3f46" } },
        axisLabel: {
          color: "#a1a1aa",
          fontSize: 11,
          fontFamily: "var(--font-mono)",
        },
        splitLine: { lineStyle: { color: "#27272a" } },
      },
      yAxis: {
        type: "value" as const,
        name: "Actual Fill Price",
        nameLocation: "center" as const,
        nameGap: 44,
        nameTextStyle: { color: "#a1a1aa", fontSize: 11 },
        min: axisMin,
        max: axisMax,
        axisLine: { lineStyle: { color: "#3f3f46" } },
        axisLabel: {
          color: "#a1a1aa",
          fontSize: 11,
          fontFamily: "var(--font-mono)",
        },
        splitLine: { lineStyle: { color: "#27272a" } },
      },
      series: [
        {
          name: "Perfect Execution",
          type: "line" as const,
          data: [
            [axisMin, axisMin],
            [axisMax, axisMax],
          ],
          lineStyle: { color: "#52525b", type: "dashed" as const, width: 1 },
          symbol: "none",
          silent: true,
          z: 1,
        },
        {
          name: "Overpaid",
          type: "scatter" as const,
          data: overpaid,
          itemStyle: { color: "#ef4444", opacity: 0.7 },
          symbolSize: 6,
          z: 2,
        },
        {
          name: "Underpaid",
          type: "scatter" as const,
          data: underpaid,
          itemStyle: { color: "#10b981", opacity: 0.7 },
          symbolSize: 6,
          z: 2,
        },
      ],
      legend: {
        bottom: 0,
        textStyle: { color: "#a1a1aa", fontSize: 11 },
        itemWidth: 10,
        itemHeight: 10,
        data: [
          { name: "Overpaid", icon: "circle" },
          { name: "Underpaid", icon: "circle" },
        ],
      },
    };
  }, [filteredHistory]);

  return (
    <div className="space-y-6">
      {/* Header */}
      <div>
        <h1 className="text-2xl font-bold text-white">Trade History</h1>
        <p className="mt-1 text-sm text-zinc-400">
          Execution reports, slippage analysis, and historical performance
        </p>
      </div>

      {/* KPI Row */}
      <div className="grid gap-4 sm:grid-cols-2 lg:grid-cols-3">
        <MetricCard
          title="Total Trades"
          value={totalTrades.toLocaleString()}
        />
        <MetricCard
          title="Total Realized Edge"
          value={formatPnl(totalRealizedEdge.toFixed(2))}
          deltaType={totalRealizedEdge >= 0 ? "positive" : "negative"}
        />
        <MetricCard
          title="Average Slippage"
          value={
            filteredHistory.length > 0
              ? formatDecimal(avgSlippage.toFixed(6), 4)
              : "\u2014"
          }
        />
      </div>

      {/* Filter Bar */}
      <div className="flex items-center gap-2">
        <span className="text-xs font-medium uppercase tracking-wider text-zinc-400">
          Mode:
        </span>
        {MODE_FILTERS.map((f) => (
          <Button
            key={f}
            variant={modeFilter === f ? "default" : "ghost"}
            size="xs"
            className={cn(
              modeFilter === f
                ? "bg-zinc-700 text-white hover:bg-zinc-600"
                : "text-zinc-400 hover:text-white hover:bg-zinc-800"
            )}
            onClick={() => setModeFilter(f)}
          >
            {f}
          </Button>
        ))}
      </div>

      {/* Trade History Table */}
      <div className="rounded-lg border border-zinc-800 bg-zinc-900">
        <div className="border-b border-zinc-800 px-5 py-4">
          <h2 className="text-sm font-medium uppercase tracking-wider text-zinc-400">
            Execution Reports
          </h2>
        </div>
        {filteredHistory.length === 0 ? (
          <div className="flex h-[300px] items-center justify-center text-sm text-zinc-600">
            No trades executed yet
          </div>
        ) : (
          <DataTable
            columns={HISTORY_COLUMNS}
            data={filteredHistory}
            pageSize={15}
            onRowClick={(row) => setSelectedReport(row)}
          />
        )}
      </div>

      {/* Slippage Analysis Chart */}
      <div className="rounded-lg border border-zinc-800 bg-zinc-900">
        <div className="border-b border-zinc-800 px-5 py-4">
          <h2 className="text-sm font-medium uppercase tracking-wider text-zinc-400">
            Slippage Analysis
          </h2>
        </div>
        <div className="p-4">
          {scatterChartOption ? (
            <ReactECharts
              option={scatterChartOption}
              style={{ height: 320, width: "100%" }}
              opts={{ renderer: "canvas" }}
              theme="dark"
            />
          ) : (
            <div className="flex h-[320px] items-center justify-center text-sm text-zinc-600">
              No leg data to chart
            </div>
          )}
        </div>
      </div>

      {/* Detail Dialog */}
      <Dialog
        open={selectedReport !== null}
        onOpenChange={(open) => {
          if (!open) setSelectedReport(null);
        }}
      >
        <DialogContent className="border-zinc-800 bg-zinc-900 text-zinc-100 sm:max-w-2xl max-h-[85vh] overflow-y-auto">
          <DialogHeader>
            <DialogTitle className="text-white">
              Execution Report
            </DialogTitle>
            <DialogDescription className="text-zinc-400">
              Full details for opportunity{" "}
              <span
                className="text-zinc-300"
                style={{ fontFamily: "var(--font-mono)" }}
              >
                {selectedReport?.opportunity_id}
              </span>
            </DialogDescription>
          </DialogHeader>

          {selectedReport && (
            <div className="space-y-5">
              {/* Summary fields */}
              <div className="grid grid-cols-2 gap-4 text-sm">
                <div>
                  <span className="text-xs uppercase tracking-wider text-zinc-500">
                    Opportunity ID
                  </span>
                  <p
                    className="mt-1 text-zinc-200 break-all"
                    style={{ fontFamily: "var(--font-mono)" }}
                  >
                    {selectedReport.opportunity_id}
                  </p>
                </div>
                <div>
                  <span className="text-xs uppercase tracking-wider text-zinc-500">
                    Timestamp
                  </span>
                  <p className="mt-1 text-zinc-200">
                    {new Date(selectedReport.timestamp).toLocaleString()}
                  </p>
                </div>
                <div>
                  <span className="text-xs uppercase tracking-wider text-zinc-500">
                    Mode
                  </span>
                  <div className="mt-1">{modeBadge(selectedReport.mode)}</div>
                </div>
                <div>
                  <span className="text-xs uppercase tracking-wider text-zinc-500">
                    Legs
                  </span>
                  <p
                    className="mt-1 text-zinc-200"
                    style={{ fontFamily: "var(--font-mono)" }}
                  >
                    {selectedReport.legs.length}
                  </p>
                </div>
                <div>
                  <span className="text-xs uppercase tracking-wider text-zinc-500">
                    Realized Edge
                  </span>
                  <p
                    className={cn(
                      "mt-1 font-bold",
                      parseFloat(selectedReport.realized_edge) >= 0
                        ? "text-emerald-500"
                        : "text-red-500"
                    )}
                    style={{ fontFamily: "var(--font-mono)" }}
                  >
                    {formatPnl(selectedReport.realized_edge)}
                  </p>
                </div>
                <div>
                  <span className="text-xs uppercase tracking-wider text-zinc-500">
                    Slippage
                  </span>
                  <p
                    className="mt-1 text-zinc-200"
                    style={{ fontFamily: "var(--font-mono)" }}
                  >
                    {formatDecimal(selectedReport.slippage, 4)}
                  </p>
                </div>
                <div>
                  <span className="text-xs uppercase tracking-wider text-zinc-500">
                    Total Fees
                  </span>
                  <p
                    className="mt-1 text-zinc-200"
                    style={{ fontFamily: "var(--font-mono)" }}
                  >
                    {formatUsd(selectedReport.total_fees)}
                  </p>
                </div>
              </div>

              {/* Legs table */}
              <div>
                <h3 className="mb-3 text-xs font-medium uppercase tracking-wider text-zinc-400">
                  Leg Details
                </h3>
                <div className="rounded-md border border-zinc-800 overflow-x-auto">
                  <Table>
                    <TableHeader>
                      <TableRow className="border-zinc-800 hover:bg-transparent">
                        <TableHead className="bg-zinc-900 text-xs font-medium uppercase tracking-wider text-zinc-500">
                          Order ID
                        </TableHead>
                        <TableHead className="bg-zinc-900 text-xs font-medium uppercase tracking-wider text-zinc-500">
                          Token ID
                        </TableHead>
                        <TableHead className="bg-zinc-900 text-xs font-medium uppercase tracking-wider text-zinc-500">
                          Side
                        </TableHead>
                        <TableHead className="bg-zinc-900 text-xs font-medium uppercase tracking-wider text-zinc-500">
                          Exp. VWAP
                        </TableHead>
                        <TableHead className="bg-zinc-900 text-xs font-medium uppercase tracking-wider text-zinc-500">
                          Act. Fill
                        </TableHead>
                        <TableHead className="bg-zinc-900 text-xs font-medium uppercase tracking-wider text-zinc-500">
                          Size
                        </TableHead>
                        <TableHead className="bg-zinc-900 text-xs font-medium uppercase tracking-wider text-zinc-500">
                          Status
                        </TableHead>
                      </TableRow>
                    </TableHeader>
                    <TableBody>
                      {selectedReport.legs.map((leg, i) => (
                        <TableRow
                          key={i}
                          className="border-zinc-800 bg-zinc-950 hover:bg-zinc-800/50"
                        >
                          <TableCell
                            className="text-zinc-300"
                            style={{ fontFamily: "var(--font-mono)" }}
                          >
                            <span title={leg.order_id}>
                              {truncateId(leg.order_id)}
                            </span>
                          </TableCell>
                          <TableCell
                            className="text-zinc-300"
                            style={{ fontFamily: "var(--font-mono)" }}
                          >
                            <span title={leg.token_id}>
                              {truncateId(leg.token_id)}
                            </span>
                          </TableCell>
                          <TableCell>{sideBadge(leg.side)}</TableCell>
                          <TableCell
                            className="text-zinc-300"
                            style={{ fontFamily: "var(--font-mono)" }}
                          >
                            {formatDecimal(leg.expected_vwap, 4)}
                          </TableCell>
                          <TableCell
                            className="text-zinc-300"
                            style={{ fontFamily: "var(--font-mono)" }}
                          >
                            {formatDecimal(leg.actual_fill_price, 4)}
                          </TableCell>
                          <TableCell
                            className="text-zinc-300"
                            style={{ fontFamily: "var(--font-mono)" }}
                          >
                            {formatDecimal(leg.filled_size, 4)}
                          </TableCell>
                          <TableCell>{statusBadge(leg.status)}</TableCell>
                        </TableRow>
                      ))}
                    </TableBody>
                  </Table>
                </div>
              </div>
            </div>
          )}
        </DialogContent>
      </Dialog>
    </div>
  );
}
