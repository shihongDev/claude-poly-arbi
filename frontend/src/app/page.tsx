"use client";

import { useMemo } from "react";
import { useDashboardStore } from "@/store";
import { MetricCard } from "@/components/metric-card";
import { PnlChart } from "@/components/pnl-chart";
import { RiskGauge } from "@/components/risk-gauge";
import { OpportunityRow } from "@/components/opportunity-row";
import {
  formatUsd,
  formatPnl,
  formatPercent,
  formatDecimal,
  cn,
} from "@/lib/utils";
import type { Position } from "@/lib/types";

const MAX_TOTAL_EXPOSURE = 5000;

export default function DashboardPage() {
  const metrics = useDashboardStore((s) => s.metrics);
  const opportunities = useDashboardStore((s) => s.opportunities);
  const positions = useDashboardStore((s) => s.positions);
  const history = useDashboardStore((s) => s.history);

  // Build equity curve from execution history
  const equityCurve = useMemo(() => {
    if (!history || history.length === 0) return [];

    // Sort by timestamp ascending
    const sorted = [...history].sort(
      (a, b) =>
        new Date(a.timestamp).getTime() - new Date(b.timestamp).getTime()
    );

    let cumulative = 0;
    return sorted.map((report) => {
      cumulative += parseFloat(report.realized_edge) - parseFloat(report.total_fees);
      return {
        time: report.timestamp.slice(0, 10), // YYYY-MM-DD for lightweight-charts
        value: cumulative,
      };
    });
  }, [history]);

  // Recent opportunities (last 10)
  const recentOpportunities = useMemo(
    () => opportunities.slice(0, 10),
    [opportunities]
  );

  // Top 5 positions by absolute unrealized P&L
  const topPositions = useMemo(() => {
    if (!positions || positions.length === 0) return [];
    return [...positions]
      .sort(
        (a, b) =>
          Math.abs(parseFloat(b.unrealized_pnl)) -
          Math.abs(parseFloat(a.unrealized_pnl))
      )
      .slice(0, 5);
  }, [positions]);

  // Derived values
  const currentExposure = metrics
    ? parseFloat(metrics.current_exposure)
    : 0;
  const drawdownPct = metrics ? metrics.drawdown_pct : 0;
  const brierScore = metrics ? metrics.brier_score : null;

  return (
    <div className="space-y-6">
      {/* Header */}
      <div>
        <h1 className="text-2xl font-bold text-white">Dashboard</h1>
        <p className="mt-1 text-sm text-zinc-400">
          Real-time arbitrage monitoring and execution overview
        </p>
      </div>

      {/* Row 1: 5 KPI MetricCards */}
      <div className="grid gap-4 sm:grid-cols-2 lg:grid-cols-5">
        <MetricCard
          title="Total P&L"
          value={metrics ? formatPnl(metrics.total_pnl) : "\u2014"}
          delta={
            metrics ? formatPnl(metrics.daily_pnl) + " today" : undefined
          }
          deltaType={
            metrics
              ? parseFloat(metrics.total_pnl) >= 0
                ? "positive"
                : "negative"
              : undefined
          }
        />
        <MetricCard
          title="Daily P&L"
          value={metrics ? formatPnl(metrics.daily_pnl) : "\u2014"}
          deltaType={
            metrics
              ? parseFloat(metrics.daily_pnl) >= 0
                ? "positive"
                : "negative"
              : undefined
          }
        />
        <MetricCard
          title="Open Positions"
          value={positions.length.toLocaleString()}
        />
        <MetricCard
          title="Active Opportunities"
          value={opportunities.length.toLocaleString()}
        />
        <MetricCard
          title="Brier Score"
          value={brierScore !== null ? formatDecimal(String(brierScore), 4) : "\u2014"}
          delta={brierScore !== null ? "0.2500 random baseline" : undefined}
          deltaType={
            brierScore !== null
              ? brierScore < 0.25
                ? "positive"
                : "negative"
              : undefined
          }
        />
      </div>

      {/* Row 2: Chart + Risk Gauges */}
      <div className="grid gap-6 lg:grid-cols-3">
        {/* Left: P&L Chart (2/3 width) */}
        <div className="lg:col-span-2 rounded-lg border border-zinc-800 bg-zinc-900 p-5">
          <h2 className="text-sm font-medium uppercase tracking-wider text-zinc-400">
            Equity Curve
          </h2>
          <div className="mt-4 h-[280px]">
            {equityCurve.length > 0 ? (
              <PnlChart data={equityCurve} />
            ) : (
              <div className="flex h-full items-center justify-center text-sm text-zinc-600">
                No trade history yet
              </div>
            )}
          </div>
        </div>

        {/* Right: Risk Gauges (1/3 width) */}
        <div className="flex flex-col gap-6">
          <div className="flex-1 rounded-lg border border-zinc-800 bg-zinc-900 p-5">
            <h2 className="text-sm font-medium uppercase tracking-wider text-zinc-400">
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
              className="mt-1 text-center text-xs text-zinc-500"
              style={{ fontFamily: "var(--font-mono)" }}
            >
              {formatUsd(String(currentExposure))} / {formatUsd(String(MAX_TOTAL_EXPOSURE))}
            </p>
          </div>
          <div className="flex-1 rounded-lg border border-zinc-800 bg-zinc-900 p-5">
            <h2 className="text-sm font-medium uppercase tracking-wider text-zinc-400">
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
              className="mt-1 text-center text-xs text-zinc-500"
              style={{ fontFamily: "var(--font-mono)" }}
            >
              {formatPercent(drawdownPct)}
            </p>
          </div>
        </div>
      </div>

      {/* Row 3: Recent Opportunities + Top Positions */}
      <div className="grid gap-6 lg:grid-cols-2">
        {/* Left: Recent Opportunities */}
        <div className="rounded-lg border border-zinc-800 bg-zinc-900">
          <div className="border-b border-zinc-800 px-5 py-4">
            <h2 className="text-sm font-medium uppercase tracking-wider text-zinc-400">
              Recent Opportunities
            </h2>
          </div>
          {recentOpportunities.length > 0 ? (
            <div className="overflow-x-auto">
              <table className="w-full text-sm">
                <thead>
                  <tr className="border-b border-zinc-800">
                    <th className="px-3 py-2.5 text-left text-xs font-medium uppercase tracking-wider text-zinc-500">
                      Type
                    </th>
                    <th className="px-3 py-2.5 text-left text-xs font-medium uppercase tracking-wider text-zinc-500">
                      Markets
                    </th>
                    <th className="px-3 py-2.5 text-left text-xs font-medium uppercase tracking-wider text-zinc-500">
                      Net Edge
                    </th>
                    <th className="px-3 py-2.5 text-left text-xs font-medium uppercase tracking-wider text-zinc-500">
                      Confidence
                    </th>
                    <th className="px-3 py-2.5 text-left text-xs font-medium uppercase tracking-wider text-zinc-500">
                      Size
                    </th>
                    <th className="px-3 py-2.5 text-left text-xs font-medium uppercase tracking-wider text-zinc-500">
                      Time
                    </th>
                  </tr>
                </thead>
                <tbody>
                  {recentOpportunities.map((opp) => (
                    <OpportunityRow key={opp.id} opportunity={opp} />
                  ))}
                </tbody>
              </table>
            </div>
          ) : (
            <div className="flex h-[240px] items-center justify-center text-sm text-zinc-600">
              No opportunities detected yet
            </div>
          )}
        </div>

        {/* Right: Top Positions */}
        <div className="rounded-lg border border-zinc-800 bg-zinc-900">
          <div className="border-b border-zinc-800 px-5 py-4">
            <h2 className="text-sm font-medium uppercase tracking-wider text-zinc-400">
              Top Positions
            </h2>
          </div>
          {topPositions.length > 0 ? (
            <div className="overflow-x-auto">
              <table className="w-full text-sm">
                <thead>
                  <tr className="border-b border-zinc-800">
                    <th className="px-4 py-2.5 text-left text-xs font-medium uppercase tracking-wider text-zinc-500">
                      Token
                    </th>
                    <th className="px-4 py-2.5 text-right text-xs font-medium uppercase tracking-wider text-zinc-500">
                      Size
                    </th>
                    <th className="px-4 py-2.5 text-right text-xs font-medium uppercase tracking-wider text-zinc-500">
                      Entry
                    </th>
                    <th className="px-4 py-2.5 text-right text-xs font-medium uppercase tracking-wider text-zinc-500">
                      Current
                    </th>
                    <th className="px-4 py-2.5 text-right text-xs font-medium uppercase tracking-wider text-zinc-500">
                      Unrealized P&L
                    </th>
                  </tr>
                </thead>
                <tbody>
                  {topPositions.map((pos) => (
                    <PositionRow key={pos.token_id} position={pos} />
                  ))}
                </tbody>
              </table>
            </div>
          ) : (
            <div className="flex h-[240px] items-center justify-center text-sm text-zinc-600">
              No open positions
            </div>
          )}
        </div>
      </div>
    </div>
  );
}

function PositionRow({ position }: { position: Position }) {
  const pnl = parseFloat(position.unrealized_pnl);
  const isPositive = pnl >= 0;

  return (
    <tr className="border-b border-zinc-800 bg-zinc-950 transition-colors hover:bg-zinc-800/50">
      <td className="px-4 py-3">
        <span
          className="text-xs text-zinc-400"
          style={{ fontFamily: "var(--font-mono)" }}
        >
          {position.token_id.slice(0, 10)}...
        </span>
      </td>
      <td
        className="px-4 py-3 text-right text-zinc-300"
        style={{ fontFamily: "var(--font-mono)" }}
      >
        {formatUsd(position.size)}
      </td>
      <td
        className="px-4 py-3 text-right text-zinc-300"
        style={{ fontFamily: "var(--font-mono)" }}
      >
        {parseFloat(position.avg_entry_price).toFixed(4)}
      </td>
      <td
        className="px-4 py-3 text-right text-zinc-300"
        style={{ fontFamily: "var(--font-mono)" }}
      >
        {parseFloat(position.current_price).toFixed(4)}
      </td>
      <td
        className={cn(
          "px-4 py-3 text-right font-bold",
          isPositive ? "text-emerald-500" : "text-red-500"
        )}
        style={{ fontFamily: "var(--font-mono)" }}
      >
        {formatPnl(position.unrealized_pnl)}
      </td>
    </tr>
  );
}
