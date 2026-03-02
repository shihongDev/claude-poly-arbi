"use client";

import { useDashboardStore } from "@/store";
import { MetricCard } from "@/components/metric-card";
import { formatUsd, formatPnl, formatPercent } from "@/lib/utils";

export default function DashboardPage() {
  const metrics = useDashboardStore((s) => s.metrics);
  const status = useDashboardStore((s) => s.status);
  const opportunities = useDashboardStore((s) => s.opportunities);
  const positions = useDashboardStore((s) => s.positions);

  return (
    <div className="space-y-6">
      <div>
        <h1 className="text-2xl font-bold text-white">Dashboard</h1>
        <p className="mt-1 text-sm text-zinc-400">
          Real-time arbitrage monitoring and execution overview
        </p>
      </div>

      {/* KPI Cards */}
      <div className="grid gap-4 sm:grid-cols-2 lg:grid-cols-4">
        <MetricCard
          title="Total P&L"
          value={metrics ? formatPnl(metrics.total_pnl) : "\u2014"}
          delta={metrics ? formatPnl(metrics.daily_pnl) + " today" : undefined}
          deltaType={
            metrics
              ? parseFloat(metrics.daily_pnl) >= 0
                ? "positive"
                : "negative"
              : undefined
          }
        />
        <MetricCard
          title="Current Equity"
          value={metrics ? formatUsd(metrics.current_equity) : "\u2014"}
        />
        <MetricCard
          title="Drawdown"
          value={metrics ? formatPercent(metrics.drawdown_pct) : "\u2014"}
          deltaType={
            metrics
              ? metrics.drawdown_pct > 5
                ? "negative"
                : "neutral"
              : undefined
          }
        />
        <MetricCard
          title="Trade Count"
          value={metrics ? metrics.trade_count.toLocaleString() : "\u2014"}
        />
      </div>

      {/* Secondary metrics */}
      <div className="grid gap-4 sm:grid-cols-2 lg:grid-cols-4">
        <MetricCard
          title="Active Markets"
          value={status ? status.market_count.toLocaleString() : "\u2014"}
        />
        <MetricCard
          title="Open Opportunities"
          value={opportunities.length.toLocaleString()}
        />
        <MetricCard
          title="Open Positions"
          value={positions.length.toLocaleString()}
        />
        <MetricCard
          title="Brier Score"
          value={metrics ? metrics.brier_score.toFixed(4) : "\u2014"}
        />
      </div>

      {/* Placeholder sections */}
      <div className="grid gap-6 lg:grid-cols-2">
        <div className="rounded-lg border border-zinc-800 bg-zinc-900 p-5">
          <h2 className="text-sm font-medium uppercase tracking-wider text-zinc-400">
            P&L Chart
          </h2>
          <div className="mt-4 flex h-[240px] items-center justify-center text-sm text-zinc-600">
            Chart renders when data is available
          </div>
        </div>
        <div className="rounded-lg border border-zinc-800 bg-zinc-900 p-5">
          <h2 className="text-sm font-medium uppercase tracking-wider text-zinc-400">
            Recent Opportunities
          </h2>
          <div className="mt-4 flex h-[240px] items-center justify-center text-sm text-zinc-600">
            Opportunities will appear here in real-time
          </div>
        </div>
      </div>
    </div>
  );
}
