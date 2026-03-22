"use client";

import { useState, useMemo } from "react";
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
  truncateId,
  cn,
} from "@/lib/utils";
import type { ExecutionReport, LegReport, TradingMode } from "@/lib/types";

import { DailyPnlCalendar } from "@/components/daily-pnl-calendar";
import { SlippageScatterEnhanced } from "@/components/slippage-scatter-enhanced";

type ModeFilter = "All" | "Paper" | "Live";

const MODE_FILTERS: ModeFilter[] = ["All", "Paper", "Live"];


function modeBadge(mode: TradingMode) {
  return mode === "Paper" ? (
    <Badge className="bg-blue-50 text-blue-600">
      Paper
    </Badge>
  ) : (
    <Badge className="bg-[#DAE9E0] text-[#2D6A4F]">
      Live
    </Badge>
  );
}

function statusBadge(status: LegReport["status"]) {
  switch (status) {
    case "FullyFilled":
      return (
        <Badge className="bg-[#DAE9E0] text-[#2D6A4F]">
          Filled
        </Badge>
      );
    case "PartiallyFilled":
      return (
        <Badge className="bg-amber-50 text-amber-600">
          Partial
        </Badge>
      );
    case "Rejected":
      return (
        <Badge className="bg-[#F5E0DD] text-[#B44C3F]">
          Rejected
        </Badge>
      );
    case "Cancelled":
      return (
        <Badge className="bg-[#F0EEEA] text-[#6B6B6B]">
          Cancelled
        </Badge>
      );
  }
}

function sideBadge(side: LegReport["side"]) {
  return side === "Buy" ? (
    <Badge className="bg-[#DAE9E0] text-[#2D6A4F]">
      Buy
    </Badge>
  ) : (
    <Badge className="bg-[#F5E0DD] text-[#B44C3F]">
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
      <span className="text-[#1A1A19]">{timeAgo(row.timestamp)}</span>
    ),
    getValue: (row) => new Date(row.timestamp).getTime(),
  },
  {
    key: "opportunity_id",
    header: "Opportunity",
    sortable: true,
    mono: true,
    render: (row) => (
      <span className="text-[#1A1A19]" title={row.opportunity_id}>
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
      <span className="text-[#1A1A19]">{row.legs.length}</span>
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
        <span className={val >= 0 ? "text-[#2D6A4F]" : "text-[#B44C3F]"}>
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
        <span className={isHigh ? "text-[#B44C3F]" : "text-[#1A1A19]"}>
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
      <span className="text-[#1A1A19]">{formatUsd(row.total_fees)}</span>
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

  return (
    <div className="space-y-6">
      {/* Header */}
      <div>
        <h1 className="text-2xl font-bold text-[#1A1A19]">Trade History</h1>
        <p className="mt-1 text-sm text-[#6B6B6B]">
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

      {/* Daily P&L Calendar */}
      <DailyPnlCalendar history={history} />

      {/* Filter Bar */}
      <div className="flex items-center gap-2">
        <span className="text-[11px] font-medium uppercase tracking-wider text-[#9B9B9B]">
          Mode:
        </span>
        {MODE_FILTERS.map((f) => (
          <Button
            key={f}
            variant={modeFilter === f ? "default" : "ghost"}
            size="xs"
            className={cn(
              modeFilter === f
                ? "bg-[#1A1A19] text-white hover:bg-[#333]"
                : "text-[#6B6B6B] hover:text-[#1A1A19] hover:bg-[#F0EEEA]"
            )}
            onClick={() => setModeFilter(f)}
          >
            {f}
          </Button>
        ))}
      </div>

      {/* Trade History Table */}
      <div className="rounded-2xl bg-white">
        <div className="border-b border-[#E6E4DF] px-5 py-4">
          <h2 className="text-[11px] font-medium uppercase tracking-wider text-[#9B9B9B]">
            Execution Reports
          </h2>
        </div>
        {filteredHistory.length === 0 ? (
          <div className="flex h-[300px] items-center justify-center text-sm text-[#9B9B9B]">
            No trades executed yet
          </div>
        ) : (
          <DataTable
            columns={HISTORY_COLUMNS}
            data={filteredHistory}
            pageSize={15}
            onRowClick={(row) => setSelectedReport(row)}
            keyExtractor={(row) => `${row.opportunity_id}-${row.timestamp}`}
          />
        )}
      </div>

      {/* Enhanced Slippage Scatter */}
      <SlippageScatterEnhanced history={filteredHistory} />

      {/* Detail Dialog */}
      <Dialog
        open={selectedReport !== null}
        onOpenChange={(open) => {
          if (!open) setSelectedReport(null);
        }}
      >
        <DialogContent className="sm:max-w-2xl max-h-[85vh] overflow-y-auto">
          <DialogHeader>
            <DialogTitle>
              Execution Report
            </DialogTitle>
            <DialogDescription>
              Full details for opportunity{" "}
              <span
                className="text-[#1A1A19]"
                style={{ fontFamily: "var(--font-jetbrains-mono)" }}
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
                  <span className="text-xs uppercase tracking-wider text-[#9B9B9B]">
                    Opportunity ID
                  </span>
                  <p
                    className="mt-1 text-[#1A1A19] break-all"
                    style={{ fontFamily: "var(--font-jetbrains-mono)" }}
                  >
                    {selectedReport.opportunity_id}
                  </p>
                </div>
                <div>
                  <span className="text-xs uppercase tracking-wider text-[#9B9B9B]">
                    Timestamp
                  </span>
                  <p className="mt-1 text-[#1A1A19]">
                    {new Date(selectedReport.timestamp).toLocaleString()}
                  </p>
                </div>
                <div>
                  <span className="text-xs uppercase tracking-wider text-[#9B9B9B]">
                    Mode
                  </span>
                  <div className="mt-1">{modeBadge(selectedReport.mode)}</div>
                </div>
                <div>
                  <span className="text-xs uppercase tracking-wider text-[#9B9B9B]">
                    Legs
                  </span>
                  <p
                    className="mt-1 text-[#1A1A19]"
                    style={{ fontFamily: "var(--font-jetbrains-mono)" }}
                  >
                    {selectedReport.legs.length}
                  </p>
                </div>
                <div>
                  <span className="text-xs uppercase tracking-wider text-[#9B9B9B]">
                    Realized Edge
                  </span>
                  <p
                    className={cn(
                      "mt-1 font-bold",
                      parseFloat(selectedReport.realized_edge) >= 0
                        ? "text-[#2D6A4F]"
                        : "text-[#B44C3F]"
                    )}
                    style={{ fontFamily: "var(--font-jetbrains-mono)" }}
                  >
                    {formatPnl(selectedReport.realized_edge)}
                  </p>
                </div>
                <div>
                  <span className="text-xs uppercase tracking-wider text-[#9B9B9B]">
                    Slippage
                  </span>
                  <p
                    className="mt-1 text-[#1A1A19]"
                    style={{ fontFamily: "var(--font-jetbrains-mono)" }}
                  >
                    {formatDecimal(selectedReport.slippage, 4)}
                  </p>
                </div>
                <div>
                  <span className="text-xs uppercase tracking-wider text-[#9B9B9B]">
                    Total Fees
                  </span>
                  <p
                    className="mt-1 text-[#1A1A19]"
                    style={{ fontFamily: "var(--font-jetbrains-mono)" }}
                  >
                    {formatUsd(selectedReport.total_fees)}
                  </p>
                </div>
              </div>

              {/* Legs table */}
              <div>
                <h3 className="mb-3 text-[11px] font-medium uppercase tracking-wider text-[#9B9B9B]">
                  Leg Details
                </h3>
                <div className="rounded-md border border-[#E6E4DF] overflow-x-auto">
                  <Table>
                    <TableHeader>
                      <TableRow className="border-[#E6E4DF] hover:bg-transparent">
                        <TableHead className="bg-white text-xs font-medium uppercase tracking-wider text-[#9B9B9B]">
                          Order ID
                        </TableHead>
                        <TableHead className="bg-white text-xs font-medium uppercase tracking-wider text-[#9B9B9B]">
                          Token ID
                        </TableHead>
                        <TableHead className="bg-white text-xs font-medium uppercase tracking-wider text-[#9B9B9B]">
                          Side
                        </TableHead>
                        <TableHead className="bg-white text-xs font-medium uppercase tracking-wider text-[#9B9B9B]">
                          Exp. VWAP
                        </TableHead>
                        <TableHead className="bg-white text-xs font-medium uppercase tracking-wider text-[#9B9B9B]">
                          Act. Fill
                        </TableHead>
                        <TableHead className="bg-white text-xs font-medium uppercase tracking-wider text-[#9B9B9B]">
                          Size
                        </TableHead>
                        <TableHead className="bg-white text-xs font-medium uppercase tracking-wider text-[#9B9B9B]">
                          Status
                        </TableHead>
                      </TableRow>
                    </TableHeader>
                    <TableBody>
                      {selectedReport.legs.map((leg, i) => (
                        <TableRow
                          key={i}
                          className="border-[#E6E4DF] bg-[#F8F7F4] hover:bg-[#F8F7F4]"
                        >
                          <TableCell
                            className="text-[#1A1A19]"
                            style={{ fontFamily: "var(--font-jetbrains-mono)" }}
                          >
                            <span title={leg.order_id}>
                              {truncateId(leg.order_id)}
                            </span>
                          </TableCell>
                          <TableCell
                            className="text-[#1A1A19]"
                            style={{ fontFamily: "var(--font-jetbrains-mono)" }}
                          >
                            <span title={leg.token_id}>
                              {truncateId(leg.token_id)}
                            </span>
                          </TableCell>
                          <TableCell>{sideBadge(leg.side)}</TableCell>
                          <TableCell
                            className="text-[#1A1A19]"
                            style={{ fontFamily: "var(--font-jetbrains-mono)" }}
                          >
                            {formatDecimal(leg.expected_vwap, 4)}
                          </TableCell>
                          <TableCell
                            className="text-[#1A1A19]"
                            style={{ fontFamily: "var(--font-jetbrains-mono)" }}
                          >
                            {formatDecimal(leg.actual_fill_price, 4)}
                          </TableCell>
                          <TableCell
                            className="text-[#1A1A19]"
                            style={{ fontFamily: "var(--font-jetbrains-mono)" }}
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
