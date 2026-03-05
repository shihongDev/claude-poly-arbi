"use client";

import { useState, useEffect, useCallback } from "react";
import {
  Activity,
  CheckCircle2,
  XCircle,
  AlertTriangle,
  RefreshCw,
  Loader2,
} from "lucide-react";
import { Badge } from "@/components/ui/badge";
import { Button } from "@/components/ui/button";
import {
  Table,
  TableBody,
  TableCell,
  TableHead,
  TableHeader,
  TableRow,
} from "@/components/ui/table";
import { cn } from "@/lib/utils";
import { fetchSimulationStatus } from "@/lib/api";
import type { SimulationStatus } from "@/lib/types";

const MONO = { fontFamily: "var(--font-jetbrains-mono)" };

function pct(v: number): string {
  return `${(v * 100).toFixed(2)}%`;
}

function truncateId(id: string, chars = 8): string {
  if (id.length <= chars * 2 + 3) return id;
  return `${id.slice(0, chars)}...${id.slice(-chars)}`;
}

// ---------------------------------------------------------------------------
// Sub-components
// ---------------------------------------------------------------------------

function SectionHeader({ title }: { title: string }) {
  return (
    <h3 className="text-[11px] font-medium uppercase tracking-wider text-[#9B9B9B]">
      {title}
    </h3>
  );
}

function ProbabilityDivergenceTable({
  estimates,
}: {
  estimates: SimulationStatus["estimates"];
}) {
  return (
    <div className="rounded-2xl bg-white">
      <div className="border-b border-[#E6E4DF] px-5 py-4">
        <SectionHeader title="Probability Divergence" />
      </div>
      <div className="overflow-x-auto">
        <Table>
          <TableHeader>
            <TableRow className="border-[#E6E4DF] hover:bg-transparent">
              <TableHead className="text-xs font-medium uppercase tracking-wider text-[#9B9B9B]">
                Market
              </TableHead>
              <TableHead className="text-right text-xs font-medium uppercase tracking-wider text-[#9B9B9B]">
                Market Price
              </TableHead>
              <TableHead className="text-right text-xs font-medium uppercase tracking-wider text-[#9B9B9B]">
                Model Est.
              </TableHead>
              <TableHead className="text-right text-xs font-medium uppercase tracking-wider text-[#9B9B9B]">
                Divergence
              </TableHead>
              <TableHead className="text-right text-xs font-medium uppercase tracking-wider text-[#9B9B9B]">
                95% CI
              </TableHead>
              <TableHead className="text-xs font-medium uppercase tracking-wider text-[#9B9B9B]">
                Method
              </TableHead>
            </TableRow>
          </TableHeader>
          <TableBody>
            {estimates.length === 0 ? (
              <TableRow>
                <TableCell
                  colSpan={6}
                  className="py-8 text-center text-sm text-[#9B9B9B]"
                >
                  No estimates available
                </TableCell>
              </TableRow>
            ) : (
              estimates.map((est) => {
                const absDivergence = Math.abs(est.divergence);
                const divColor =
                  absDivergence > 0.05
                    ? "text-[#B44C3F]"
                    : absDivergence > 0.02
                      ? "text-[#D97706]"
                      : "text-[#2D6A4F]";
                return (
                  <TableRow
                    key={est.condition_id}
                    className="border-[#E6E4DF]"
                  >
                    <TableCell>
                      <span
                        className="text-sm text-[#1A1A19]"
                        style={MONO}
                        title={est.condition_id}
                      >
                        {truncateId(est.condition_id)}
                      </span>
                    </TableCell>
                    <TableCell
                      className="text-right text-sm text-[#1A1A19]"
                      style={MONO}
                    >
                      {pct(est.market_price)}
                    </TableCell>
                    <TableCell
                      className="text-right text-sm text-[#1A1A19]"
                      style={MONO}
                    >
                      {pct(est.model_estimate)}
                    </TableCell>
                    <TableCell
                      className={cn("text-right text-sm", divColor)}
                      style={MONO}
                    >
                      {est.divergence >= 0 ? "+" : ""}
                      {pct(est.divergence)}
                    </TableCell>
                    <TableCell
                      className="text-right text-sm text-[#6B6B6B]"
                      style={MONO}
                    >
                      [{pct(est.confidence_interval[0])},{" "}
                      {pct(est.confidence_interval[1])}]
                    </TableCell>
                    <TableCell>
                      <Badge
                        className={cn(
                          "text-[10px]",
                          est.method === "Monte Carlo"
                            ? "bg-blue-50 text-blue-600"
                            : "bg-amber-50 text-amber-600"
                        )}
                      >
                        {est.method}
                      </Badge>
                    </TableCell>
                  </TableRow>
                );
              })
            )}
          </TableBody>
        </Table>
      </div>
    </div>
  );
}

function ConvergenceDiagnosticsCard({
  convergence,
}: {
  convergence: SimulationStatus["convergence"];
}) {
  return (
    <div className="rounded-2xl bg-white p-5">
      <SectionHeader title="Convergence Diagnostics" />
      <div className="mt-4 grid grid-cols-2 gap-4">
        <div>
          <p className="text-xs text-[#9B9B9B]">Paths Used</p>
          <p className="mt-1 text-lg font-semibold text-[#1A1A19]" style={MONO}>
            {convergence.paths_used.toLocaleString()}
          </p>
        </div>
        <div>
          <p className="text-xs text-[#9B9B9B]">Standard Error</p>
          <p className="mt-1 text-lg font-semibold text-[#1A1A19]" style={MONO}>
            {convergence.standard_error.toFixed(4)}
          </p>
        </div>
        <div>
          <p className="text-xs text-[#9B9B9B]">Converged</p>
          <div className="mt-1 flex items-center gap-1.5">
            {convergence.converged ? (
              <>
                <CheckCircle2 className="h-4 w-4 text-[#2D6A4F]" />
                <Badge className="bg-[#DAE9E0] text-[#2D6A4F] text-[10px]">
                  Yes
                </Badge>
              </>
            ) : (
              <>
                <XCircle className="h-4 w-4 text-[#B44C3F]" />
                <Badge className="bg-[#F5E0DD] text-[#B44C3F] text-[10px]">
                  No
                </Badge>
              </>
            )}
          </div>
        </div>
        <div>
          <p className="text-xs text-[#9B9B9B]">Gelman-Rubin</p>
          <p
            className={cn(
              "mt-1 text-lg font-semibold",
              convergence.gelman_rubin !== null && convergence.gelman_rubin < 1.1
                ? "text-[#2D6A4F]"
                : convergence.gelman_rubin !== null
                  ? "text-[#B44C3F]"
                  : "text-[#9B9B9B]"
            )}
            style={MONO}
          >
            {convergence.gelman_rubin !== null
              ? convergence.gelman_rubin.toFixed(3)
              : "\u2014"}
          </p>
        </div>
      </div>
    </div>
  );
}

function ModelHealthCard({
  health,
}: {
  health: SimulationStatus["model_health"];
}) {
  const confidenceColor =
    health.confidence_level >= 0.7
      ? "#2D6A4F"
      : health.confidence_level >= 0.4
        ? "#D97706"
        : "#B44C3F";

  return (
    <div className="rounded-2xl bg-white p-5">
      <SectionHeader title="Model Health" />
      <div className="mt-4 space-y-4">
        {/* Brier scores */}
        <div className="grid grid-cols-2 gap-4">
          <div>
            <p className="text-xs text-[#9B9B9B]">Brier Score (30m)</p>
            <p
              className={cn(
                "mt-1 text-lg font-semibold",
                health.brier_score_30m < 0.15
                  ? "text-[#2D6A4F]"
                  : health.brier_score_30m < 0.25
                    ? "text-[#D97706]"
                    : "text-[#B44C3F]"
              )}
              style={MONO}
            >
              {health.brier_score_30m.toFixed(4)}
            </p>
          </div>
          <div>
            <p className="text-xs text-[#9B9B9B]">Brier Score (24h)</p>
            <p
              className={cn(
                "mt-1 text-lg font-semibold",
                health.brier_score_24h < 0.15
                  ? "text-[#2D6A4F]"
                  : health.brier_score_24h < 0.25
                    ? "text-[#D97706]"
                    : "text-[#B44C3F]"
              )}
              style={MONO}
            >
              {health.brier_score_24h.toFixed(4)}
            </p>
          </div>
        </div>

        {/* Confidence bar */}
        <div>
          <div className="flex items-center justify-between">
            <p className="text-xs text-[#9B9B9B]">Confidence Level</p>
            <p className="text-xs text-[#6B6B6B]" style={MONO}>
              {(health.confidence_level * 100).toFixed(0)}%
            </p>
          </div>
          <div className="mt-1.5 h-2 w-full rounded-full bg-[#F0EEEA]">
            <div
              className="h-2 rounded-full transition-all duration-500"
              style={{
                width: `${health.confidence_level * 100}%`,
                backgroundColor: confidenceColor,
              }}
            />
          </div>
        </div>

        {/* Drift badge */}
        <div className="flex items-center gap-2">
          <p className="text-xs text-[#9B9B9B]">Drift Detected</p>
          {health.drift_detected ? (
            <Badge className="bg-[#F5E0DD] text-[#B44C3F] text-[10px]">
              <AlertTriangle className="mr-1 h-3 w-3" />
              Drift
            </Badge>
          ) : (
            <Badge className="bg-[#DAE9E0] text-[#2D6A4F] text-[10px]">
              Stable
            </Badge>
          )}
        </div>
      </div>
    </div>
  );
}

function VarSummaryCard({
  varSummary,
}: {
  varSummary: SimulationStatus["var_summary"];
}) {
  return (
    <div className="rounded-2xl bg-white p-5">
      <SectionHeader title="VaR / CVaR Summary" />
      <div className="mt-4 grid grid-cols-3 gap-4">
        <div>
          <p className="text-xs text-[#9B9B9B]">VaR 95%</p>
          <p className="mt-1 text-lg font-semibold text-[#B44C3F]" style={MONO}>
            {varSummary.var_95}
          </p>
        </div>
        <div>
          <p className="text-xs text-[#9B9B9B]">VaR 99%</p>
          <p className="mt-1 text-lg font-semibold text-[#B44C3F]" style={MONO}>
            {varSummary.var_99}
          </p>
        </div>
        <div>
          <p className="text-xs text-[#9B9B9B]">CVaR 95%</p>
          <p className="mt-1 text-lg font-semibold text-[#B44C3F]" style={MONO}>
            {varSummary.cvar_95}
          </p>
        </div>
      </div>
      <div className="mt-3">
        <Badge className="bg-[#F0EEEA] text-[#6B6B6B] text-[10px]">
          {varSummary.method}
        </Badge>
      </div>
    </div>
  );
}

// ---------------------------------------------------------------------------
// Main Component
// ---------------------------------------------------------------------------

export function SimulationStatusPanel() {
  const [status, setStatus] = useState<SimulationStatus | null>(null);
  const [loading, setLoading] = useState(true);
  const [error, setError] = useState<string | null>(null);

  const loadStatus = useCallback(async () => {
    setLoading(true);
    setError(null);
    try {
      const data = await fetchSimulationStatus();
      setStatus(data);
    } catch (err) {
      setError(err instanceof Error ? err.message : "Failed to load simulation status");
    } finally {
      setLoading(false);
    }
  }, []);

  useEffect(() => {
    loadStatus();
    const interval = setInterval(loadStatus, 30_000); // refresh every 30s
    return () => clearInterval(interval);
  }, [loadStatus]);

  if (loading && !status) {
    return (
      <div className="space-y-4">
        <div className="flex items-center justify-between">
          <div>
            <h2 className="text-xl font-bold text-[#1A1A19]">
              Simulation Engine
            </h2>
            <p className="mt-1 text-sm text-[#6B6B6B]">
              Model estimates, convergence, and risk metrics
            </p>
          </div>
        </div>
        <div className="space-y-4">
          <div className="h-48 animate-pulse rounded-2xl bg-white" />
          <div className="grid gap-4 lg:grid-cols-3">
            <div className="h-48 animate-pulse rounded-2xl bg-white" />
            <div className="h-48 animate-pulse rounded-2xl bg-white" />
            <div className="h-48 animate-pulse rounded-2xl bg-white" />
          </div>
        </div>
      </div>
    );
  }

  if (error && !status) {
    return (
      <div className="rounded-lg border border-[#B44C3F]/30 bg-[#F5E0DD] p-5">
        <div className="flex items-start gap-3">
          <XCircle className="mt-0.5 h-5 w-5 shrink-0 text-[#B44C3F]" />
          <div>
            <h3 className="text-sm font-medium text-[#B44C3F]">
              Simulation Status Unavailable
            </h3>
            <p className="mt-1 text-sm text-[#B44C3F]/80">{error}</p>
          </div>
        </div>
      </div>
    );
  }

  if (!status) return null;

  return (
    <div className="space-y-4">
      {/* Header */}
      <div className="flex items-center justify-between">
        <div className="flex items-center gap-3">
          <Activity className="h-5 w-5 text-[#2D6A4F]" />
          <div>
            <h2 className="text-xl font-bold text-[#1A1A19]">
              Simulation Engine
            </h2>
            <p className="mt-0.5 text-sm text-[#6B6B6B]">
              Model estimates, convergence, and risk metrics
            </p>
          </div>
        </div>
        <Button
          variant="ghost"
          size="sm"
          onClick={loadStatus}
          disabled={loading}
          className="text-[#6B6B6B] hover:text-[#1A1A19]"
        >
          {loading ? (
            <Loader2 className="h-4 w-4 animate-spin" />
          ) : (
            <RefreshCw className="h-4 w-4" />
          )}
          Refresh
        </Button>
      </div>

      {/* Probability Divergence Table */}
      <ProbabilityDivergenceTable estimates={status.estimates} />

      {/* Convergence + Model Health + VaR cards */}
      <div className="grid gap-4 lg:grid-cols-3">
        <ConvergenceDiagnosticsCard convergence={status.convergence} />
        <ModelHealthCard health={status.model_health} />
        <VarSummaryCard varSummary={status.var_summary} />
      </div>
    </div>
  );
}
