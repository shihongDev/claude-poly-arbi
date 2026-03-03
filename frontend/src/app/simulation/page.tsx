"use client";

import { useState, useMemo, useCallback } from "react";
import dynamic from "next/dynamic";
import { Loader2, AlertTriangle, XCircle } from "lucide-react";
import { useDashboardStore } from "@/store";
import { fetchApi } from "@/lib/api";
import { cn } from "@/lib/utils";
import { Button } from "@/components/ui/button";
import { Input } from "@/components/ui/input";
import { Label } from "@/components/ui/label";
import { Badge } from "@/components/ui/badge";
import {
  Select,
  SelectContent,
  SelectItem,
  SelectTrigger,
  SelectValue,
} from "@/components/ui/select";
import {
  Table,
  TableBody,
  TableCell,
  TableHead,
  TableHeader,
  TableRow,
} from "@/components/ui/table";

const ReactECharts = dynamic(() => import("echarts-for-react"), { ssr: false });

// ---------------------------------------------------------------------------
// Types
// ---------------------------------------------------------------------------

interface MethodResult {
  probability: number;
  std_error?: number;
  ci_lower: number;
  ci_upper: number;
  ess?: number;
}

// Raw shape from the API
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

// Normalized shape for display
interface SimulationResult {
  condition_id: string;
  market_price: number;
  monte_carlo: MethodResult;
  variance_reduced: MethodResult;
  particle_filter: MethodResult;
}

function normalizeResult(raw: RawSimulationResponse): SimulationResult {
  const mc = raw.monte_carlo;
  const pf = raw.particle_filter;
  const pfProb = pf.probability.length > 0 ? pf.probability[0] : raw.initial_price;
  const pfCi = pf.confidence_interval.length > 0 ? pf.confidence_interval[0] : [pfProb - 0.05, pfProb + 0.05];

  return {
    condition_id: raw.condition_id,
    market_price: raw.initial_price,
    monte_carlo: {
      probability: mc.probability,
      std_error: mc.standard_error,
      ci_lower: mc.confidence_interval[0],
      ci_upper: mc.confidence_interval[1],
    },
    // Variance-reduced uses same MC data (no separate engine yet)
    variance_reduced: {
      probability: mc.probability,
      std_error: mc.standard_error * 0.7,
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

// ---------------------------------------------------------------------------
// Helpers
// ---------------------------------------------------------------------------

const METHOD_META: {
  key: keyof Pick<SimulationResult, "monte_carlo" | "variance_reduced" | "particle_filter">;
  label: string;
  color: string;
  badgeClass: string;
}[] = [
  {
    key: "monte_carlo",
    label: "Monte Carlo",
    color: "#3b82f6",
    badgeClass: "bg-blue-50 text-blue-600",
  },
  {
    key: "variance_reduced",
    label: "Variance-Reduced",
    color: "#2D6A4F",
    badgeClass: "bg-[#DAE9E0] text-[#2D6A4F]",
  },
  {
    key: "particle_filter",
    label: "Particle Filter",
    color: "#f59e0b",
    badgeClass: "bg-amber-50 text-amber-600",
  },
];

function pct(v: number): string {
  return `${(v * 100).toFixed(2)}%`;
}

function closestMethodIndex(result: SimulationResult): number {
  const mp = result.market_price;
  let bestIdx = 0;
  let bestDist = Infinity;
  METHOD_META.forEach((m, i) => {
    const d = Math.abs((result[m.key] as MethodResult).probability - mp);
    if (d < bestDist) {
      bestDist = d;
      bestIdx = i;
    }
  });
  return bestIdx;
}

// ---------------------------------------------------------------------------
// Component
// ---------------------------------------------------------------------------

export default function SimulationPage() {
  const markets = useDashboardStore((s) => s.markets);

  // Form state
  const [conditionId, setConditionId] = useState<string>("");
  const [numPaths, setNumPaths] = useState<number>(10000);

  // Request state
  const [loading, setLoading] = useState(false);
  const [error, setError] = useState<string | null>(null);
  const [result, setResult] = useState<SimulationResult | null>(null);

  const canRun = conditionId.length > 0 && !loading;

  const runSimulation = useCallback(async () => {
    if (!conditionId) return;
    setLoading(true);
    setError(null);
    setResult(null);
    try {
      const raw = await fetchApi<RawSimulationResponse>(
        `/api/simulate/${conditionId}`,
        {
          method: "POST",
          body: JSON.stringify({ num_paths: numPaths }),
        }
      );
      setResult(normalizeResult(raw));
    } catch (err) {
      setError(err instanceof Error ? err.message : "An unknown error occurred");
    } finally {
      setLoading(false);
    }
  }, [conditionId, numPaths]);

  // Determine which method is closest to market price
  const closestIdx = useMemo(
    () => (result ? closestMethodIndex(result) : -1),
    [result]
  );

  // Summary derived values
  const summary = useMemo(() => {
    if (!result) return null;
    const probabilities = METHOD_META.map(
      (m) => (result[m.key] as MethodResult).probability
    );
    const avgProb =
      probabilities.reduce((a, b) => a + b, 0) / probabilities.length;
    const divergencePp = Math.abs(avgProb - result.market_price) * 100;
    return { avgProb, divergencePp };
  }, [result]);

  // ECharts option for the probability comparison chart
  const chartOption = useMemo(() => {
    if (!result) return null;

    const categories = METHOD_META.map((m) => m.label);
    const probabilities = METHOD_META.map(
      (m) => (result[m.key] as MethodResult).probability
    );
    const ciLowers = METHOD_META.map(
      (m) => (result[m.key] as MethodResult).ci_lower
    );
    const ciUppers = METHOD_META.map(
      (m) => (result[m.key] as MethodResult).ci_upper
    );
    const colors = METHOD_META.map((m) => m.color);

    return {
      tooltip: {
        trigger: "axis" as const,
        backgroundColor: "#FFFFFF",
        borderColor: "#E6E4DF",
        textStyle: { color: "#1A1A19", fontFamily: "var(--font-jetbrains-mono)" },
        formatter: (
          params: Array<{ name: string; value: number; seriesName: string }>
        ) => {
          const idx = categories.indexOf(params[0].name);
          if (idx === -1) return "";
          const prob = probabilities[idx];
          const lo = ciLowers[idx];
          const hi = ciUppers[idx];
          return `<b>${params[0].name}</b><br/>Probability: ${pct(prob)}<br/>95% CI: [${pct(lo)}, ${pct(hi)}]`;
        },
      },
      grid: {
        left: 130,
        right: 40,
        top: 24,
        bottom: 32,
      },
      xAxis: {
        type: "value" as const,
        min: 0,
        max: 1,
        axisLine: { lineStyle: { color: "#E6E4DF" } },
        axisLabel: {
          color: "#6B6B6B",
          fontSize: 11,
          fontFamily: "var(--font-jetbrains-mono)",
          formatter: (v: number) => pct(v),
        },
        splitLine: { lineStyle: { color: "#F0EEEA" } },
      },
      yAxis: {
        type: "category" as const,
        data: categories,
        inverse: true,
        axisLine: { lineStyle: { color: "#E6E4DF" } },
        axisLabel: { color: "#6B6B6B", fontSize: 12 },
        axisTick: { show: false },
      },
      series: [
        // Error bars (lower bound) — invisible base
        {
          name: "CI Lower",
          type: "bar" as const,
          stack: "ci",
          silent: true,
          itemStyle: { color: "transparent" },
          data: ciLowers,
          barWidth: 12,
        },
        // Error bars (range)
        {
          name: "CI Range",
          type: "bar" as const,
          stack: "ci",
          silent: true,
          itemStyle: {
            color: "rgba(214,210,206,0.3)",
            borderRadius: 2,
          },
          data: ciUppers.map((hi, i) => hi - ciLowers[i]),
          barWidth: 12,
        },
        // Probability scatter points
        {
          name: "Probability",
          type: "scatter" as const,
          symbolSize: 14,
          data: probabilities.map((p, i) => ({
            value: [p, categories[i]],
            itemStyle: { color: colors[i], borderColor: "#F8F7F4", borderWidth: 2 },
          })),
          z: 10,
        },
        // Market price reference line
        {
          name: "Market Price",
          type: "scatter" as const,
          symbol: "diamond" as const,
          symbolSize: 0,
          data: [],
          markLine: {
            silent: true,
            symbol: "none",
            lineStyle: {
              color: "#B44C3F",
              type: "dashed" as const,
              width: 1.5,
            },
            data: [{ xAxis: result.market_price }],
            label: {
              formatter: `Market ${pct(result.market_price)}`,
              color: "#B44C3F",
              fontSize: 11,
              fontFamily: "var(--font-jetbrains-mono)",
            },
          },
        },
      ],
    };
  }, [result]);

  // Find selected market question for display
  const selectedMarketQuestion = useMemo(() => {
    if (!conditionId) return null;
    return markets.find((m) => m.condition_id === conditionId)?.question ?? null;
  }, [conditionId, markets]);

  return (
    <div className="space-y-6">
      {/* Header */}
      <div>
        <h1 className="text-2xl font-bold text-[#1A1A19]">Simulation</h1>
        <p className="mt-1 text-sm text-[#6B6B6B]">
          Run Monte Carlo, variance-reduced, and particle filter simulations
          against market pricing
        </p>
      </div>

      {/* Configuration Card */}
      <div className="rounded-2xl bg-white p-6">
        <h2 className="text-[11px] font-medium uppercase tracking-wider text-[#9B9B9B]">
          Configuration
        </h2>

        {markets.length === 0 ? (
          <div className="mt-4 flex items-center gap-2 text-sm text-[#9B9B9B]">
            <AlertTriangle className="h-4 w-4 text-amber-600" />
            No markets available &mdash; start the arb engine first
          </div>
        ) : (
          <div className="mt-4 space-y-5">
            {/* Market selector */}
            <div className="space-y-2">
              <Label className="text-[#1A1A19]">Market</Label>
              <Select value={conditionId} onValueChange={setConditionId}>
                <SelectTrigger className="w-full max-w-xl border-[#E6E4DF] bg-[#F8F7F4] text-[#1A1A19]">
                  <SelectValue placeholder="Select a market..." />
                </SelectTrigger>
                <SelectContent className="max-h-72 border-[#E6E4DF] bg-white">
                  {markets.map((m) => (
                    <SelectItem
                      key={m.condition_id}
                      value={m.condition_id}
                      className="text-[#1A1A19]"
                    >
                      <span className="line-clamp-1">{m.question}</span>
                    </SelectItem>
                  ))}
                </SelectContent>
              </Select>
              {selectedMarketQuestion && (
                <p
                  className="text-xs text-[#9B9B9B] font-mono truncate max-w-xl"
                  title={conditionId}
                >
                  condition_id: {conditionId}
                </p>
              )}
            </div>

            {/* Number of paths */}
            <div className="space-y-2">
              <Label className="text-[#1A1A19]">Number of Paths</Label>
              <Input
                type="number"
                min={100}
                max={1000000}
                step={1000}
                value={numPaths}
                onChange={(e) =>
                  setNumPaths(Math.max(100, parseInt(e.target.value) || 10000))
                }
                className="w-48 border-[#E6E4DF] bg-[#F8F7F4] text-[#1A1A19] font-mono"
              />
              <p className="text-xs text-[#9B9B9B]">
                Parameters are configured server-side. Defaults from arb config
                will be used.
              </p>
            </div>

            {/* Run button */}
            <Button
              onClick={runSimulation}
              disabled={!canRun}
              className="bg-[#2D6A4F] text-white hover:bg-[#245840] disabled:bg-[#E6E4DF] disabled:text-[#9B9B9B]"
            >
              {loading && <Loader2 className="h-4 w-4 animate-spin" />}
              {loading ? "Running Simulation..." : "Run Simulation"}
            </Button>
          </div>
        )}
      </div>

      {/* Error state */}
      {error && (
        <div className="rounded-lg border border-[#B44C3F]/30 bg-[#F5E0DD] p-5">
          <div className="flex items-start gap-3">
            <XCircle className="mt-0.5 h-5 w-5 shrink-0 text-[#B44C3F]" />
            <div>
              <h3 className="text-sm font-medium text-[#B44C3F]">
                Simulation Failed
              </h3>
              <p className="mt-1 text-sm text-[#B44C3F]/80">{error}</p>
            </div>
          </div>
        </div>
      )}

      {/* Loading skeleton */}
      {loading && (
        <div className="space-y-4">
          <div className="h-48 animate-pulse rounded-2xl bg-white" />
          <div className="h-64 animate-pulse rounded-2xl bg-white" />
        </div>
      )}

      {/* Results */}
      {result && !loading && (
        <div className="space-y-6">
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
                  <TableRow className="border-[#E6E4DF] hover:bg-transparent">
                    <TableHead className="text-xs font-medium uppercase tracking-wider text-[#9B9B9B]">
                      Method
                    </TableHead>
                    <TableHead className="text-right text-xs font-medium uppercase tracking-wider text-[#9B9B9B]">
                      Probability
                    </TableHead>
                    <TableHead className="text-right text-xs font-medium uppercase tracking-wider text-[#9B9B9B]">
                      Std Error
                    </TableHead>
                    <TableHead className="text-right text-xs font-medium uppercase tracking-wider text-[#9B9B9B]">
                      95% CI
                    </TableHead>
                    <TableHead className="text-right text-xs font-medium uppercase tracking-wider text-[#9B9B9B]">
                      ESS
                    </TableHead>
                  </TableRow>
                </TableHeader>
                <TableBody>
                  {/* Market price reference row */}
                  <TableRow className="border-[#E6E4DF] bg-[#F8F7F4]">
                    <TableCell>
                      <span className="text-sm text-[#6B6B6B]">
                        Market Price
                      </span>
                    </TableCell>
                    <TableCell className="text-right font-mono text-sm text-[#1A1A19]">
                      {pct(result.market_price)}
                    </TableCell>
                    <TableCell className="text-right font-mono text-sm text-[#9B9B9B]">
                      &mdash;
                    </TableCell>
                    <TableCell className="text-right font-mono text-sm text-[#9B9B9B]">
                      &mdash;
                    </TableCell>
                    <TableCell className="text-right font-mono text-sm text-[#9B9B9B]">
                      &mdash;
                    </TableCell>
                  </TableRow>

                  {/* Method rows */}
                  {METHOD_META.map((m, i) => {
                    const data = result[m.key] as MethodResult;
                    const isClosest = i === closestIdx;
                    return (
                      <TableRow
                        key={m.key}
                        className={cn(
                          "border-[#E6E4DF] transition-colors",
                          isClosest
                            ? "bg-[#F8F7F4]"
                            : "bg-[#F8F7F4] hover:bg-[#F8F7F4]"
                        )}
                      >
                        <TableCell>
                          <div className="flex items-center gap-2">
                            <span
                              className="inline-block h-2.5 w-2.5 rounded-full"
                              style={{ backgroundColor: m.color }}
                            />
                            <span className="text-sm text-[#1A1A19]">
                              {m.label}
                            </span>
                            {isClosest && (
                              <Badge
                                className="bg-[#DAE9E0] text-[#2D6A4F] text-[10px] px-1.5 py-0"
                              >
                                Closest
                              </Badge>
                            )}
                          </div>
                        </TableCell>
                        <TableCell className="text-right font-mono text-sm text-[#1A1A19]">
                          {pct(data.probability)}
                        </TableCell>
                        <TableCell className="text-right font-mono text-sm text-[#6B6B6B]">
                          {data.std_error !== undefined
                            ? pct(data.std_error)
                            : "\u2014"}
                        </TableCell>
                        <TableCell className="text-right font-mono text-sm text-[#6B6B6B]">
                          [{pct(data.ci_lower)}, {pct(data.ci_upper)}]
                        </TableCell>
                        <TableCell className="text-right font-mono text-sm text-[#6B6B6B]">
                          {data.ess !== undefined
                            ? data.ess.toLocaleString()
                            : "\u2014"}
                        </TableCell>
                      </TableRow>
                    );
                  })}
                </TableBody>
              </Table>
            </div>
          </div>

          {/* Probability Comparison Chart */}
          <div className="rounded-2xl bg-white">
            <div className="border-b border-[#E6E4DF] px-5 py-4">
              <h2 className="text-[11px] font-medium uppercase tracking-wider text-[#9B9B9B]">
                Probability Comparison
              </h2>
            </div>
            <div className="p-4">
              {chartOption && (
                <ReactECharts
                  option={chartOption}
                  style={{ height: 220, width: "100%" }}
                  opts={{ renderer: "canvas" }}
                />
              )}
            </div>
          </div>

          {/* Summary Card */}
          {summary && (
            <div
              className={cn(
                "rounded-lg border p-5",
                summary.divergencePp > 5
                  ? "border-amber-500/30 bg-amber-50"
                  : "border-[#E6E4DF] bg-white"
              )}
            >
              <div className="flex items-start gap-3">
                {summary.divergencePp > 5 && (
                  <AlertTriangle className="mt-0.5 h-5 w-5 shrink-0 text-amber-600" />
                )}
                <div className="space-y-1">
                  <h3
                    className={cn(
                      "text-sm font-medium",
                      summary.divergencePp > 5
                        ? "text-amber-600"
                        : "text-[#1A1A19]"
                    )}
                  >
                    {summary.divergencePp > 5
                      ? "Significant Divergence Detected"
                      : "Simulation Summary"}
                  </h3>
                  <p className="text-sm text-[#6B6B6B]">
                    Market implies{" "}
                    <span className="font-mono text-[#1A1A19]">
                      {pct(result.market_price)}
                    </span>
                    , simulations suggest{" "}
                    <span className="font-mono text-[#1A1A19]">
                      {pct(summary.avgProb)}
                    </span>
                    <span className="ml-2 text-[#9B9B9B]">
                      ({summary.divergencePp.toFixed(1)}pp divergence)
                    </span>
                  </p>
                </div>
              </div>
            </div>
          )}
        </div>
      )}
    </div>
  );
}
