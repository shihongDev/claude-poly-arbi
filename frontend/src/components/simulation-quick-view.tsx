"use client";

import { useState, useCallback } from "react";
import { Loader2, Play } from "lucide-react";
import { Button } from "@/components/ui/button";
import { MONO_STYLE, cn } from "@/lib/utils";
import { fetchApi } from "@/lib/api";
import type { SimulationEstimate } from "@/lib/types";

interface SimulationQuickViewProps {
  conditionId: string;
  marketPrice: number;
}

interface SimResult {
  estimates: SimulationEstimate[];
}

function divergenceColor(absDivPp: number): string {
  if (absDivPp < 2) return "text-[#2D6A4F]";
  if (absDivPp < 5) return "text-[#D97706]";
  return "text-[#B44C3F]";
}

function divergenceBg(absDivPp: number): string {
  if (absDivPp < 2) return "bg-[#DAE9E0]";
  if (absDivPp < 5) return "bg-[#FEF3CD]";
  return "bg-[#FDECEA]";
}

export function SimulationQuickView({
  conditionId,
  marketPrice,
}: SimulationQuickViewProps) {
  const [result, setResult] = useState<SimResult | null>(null);
  const [loading, setLoading] = useState(false);
  const [error, setError] = useState<string | null>(null);

  const runSimulation = useCallback(async () => {
    setLoading(true);
    setError(null);
    try {
      const data = await fetchApi<SimResult>(
        `/api/simulate/${conditionId}`,
        { method: "POST", body: JSON.stringify({}) }
      );
      setResult(data);
    } catch (err) {
      setError(err instanceof Error ? err.message : "Simulation failed");
    } finally {
      setLoading(false);
    }
  }, [conditionId]);

  return (
    <div className="rounded-2xl bg-white p-5">
      <div className="flex items-center justify-between">
        <h2 className="text-[11px] font-medium uppercase tracking-wider text-[#9B9B9B]">
          Simulation Quick View
        </h2>
        <Button
          variant="ghost"
          size="sm"
          onClick={runSimulation}
          disabled={loading}
          className="h-7 gap-1 text-xs text-[#2D6A4F] hover:text-[#2D6A4F]"
        >
          {loading ? (
            <Loader2 className="h-3 w-3 animate-spin" />
          ) : (
            <Play className="h-3 w-3" />
          )}
          {loading ? "Running..." : "Run"}
        </Button>
      </div>

      {error && (
        <p className="mt-3 text-xs text-[#B44C3F]">{error}</p>
      )}

      {!result && !loading && !error && (
        <p className="mt-3 text-sm text-[#9B9B9B]">
          Click Run to compare model estimates against market prices.
        </p>
      )}

      {result && result.estimates.length > 0 && (
        <div className="mt-4 space-y-3">
          {result.estimates.map((est) => {
            const absDivPp = Math.abs(est.divergence) * 100;
            const divSign = est.divergence >= 0 ? "+" : "";

            return (
              <div
                key={`${est.condition_id}-${est.method}`}
                className="grid grid-cols-2 gap-4"
              >
                {/* Left: divergence indicator */}
                <div className="space-y-2">
                  <div className="flex items-center justify-between text-xs text-[#9B9B9B]">
                    <span>Market</span>
                    <span>Model</span>
                  </div>
                  {/* Bar comparison */}
                  <div className="relative space-y-1">
                    <div className="flex items-center gap-2">
                      <div className="h-4 rounded-full bg-[#E6E4DF]" style={{ width: `${Math.max(est.market_price * 100, 2)}%` }} />
                      <span className="text-[10px] text-[#1A1A19]" style={MONO_STYLE}>
                        {(est.market_price * 100).toFixed(1)}%
                      </span>
                    </div>
                    <div className="flex items-center gap-2">
                      <div className="h-4 rounded-full bg-[#2D6A4F]" style={{ width: `${Math.max(est.model_estimate * 100, 2)}%`, opacity: 0.3 }} />
                      <span className="text-[10px] text-[#1A1A19]" style={MONO_STYLE}>
                        {(est.model_estimate * 100).toFixed(1)}%
                      </span>
                    </div>
                  </div>
                  <span
                    className={cn(
                      "inline-block rounded-[9999px] px-2 py-0.5 text-[10px] font-medium",
                      divergenceBg(absDivPp),
                      divergenceColor(absDivPp)
                    )}
                    style={MONO_STYLE}
                  >
                    {divSign}{(est.divergence * 100).toFixed(1)}pp
                  </span>
                </div>

                {/* Right: method + CI */}
                <div className="space-y-1.5">
                  <p className="text-[10px] text-[#9B9B9B]">{est.method}</p>
                  <div className="rounded-[10px] border border-[#E6E4DF] bg-[#F8F7F4] px-3 py-2">
                    <p className="text-[10px] text-[#9B9B9B]">95% CI</p>
                    <p className="text-sm font-medium text-[#1A1A19]" style={MONO_STYLE}>
                      [{(est.confidence_interval[0] * 100).toFixed(1)}%, {(est.confidence_interval[1] * 100).toFixed(1)}%]
                    </p>
                  </div>
                </div>
              </div>
            );
          })}
        </div>
      )}

      {result && result.estimates.length === 0 && (
        <p className="mt-3 text-sm text-[#9B9B9B]">
          No simulation estimates available for this market.
        </p>
      )}
    </div>
  );
}
