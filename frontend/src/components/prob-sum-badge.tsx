"use client";

import { cn } from "@/lib/utils";
import type { MarketState } from "@/lib/types";

interface ProbSumBadgeProps {
  market: MarketState;
}

function computeProbSum(market: MarketState): {
  sum: number;
  deviationBps: number;
} | null {
  if (!market.outcomes || market.outcomes.length <= 2) return null;
  if (!market.outcome_prices || market.outcome_prices.length === 0) return null;

  const sum = market.outcome_prices.reduce(
    (acc, p) => acc + parseFloat(p || "0"),
    0
  );
  const deviationBps = Math.round(Math.abs(sum - 1) * 10000);

  return { sum, deviationBps };
}

export function ProbSumBadge({ market }: ProbSumBadgeProps) {
  const result = computeProbSum(market);

  if (!result) return null;

  const { sum, deviationBps } = result;
  const pctLabel = (sum * 100).toFixed(1);

  let colorClasses: string;
  if (deviationBps > 200) {
    colorClasses = "bg-[#F5E0DD] text-[#B44C3F]";
  } else if (deviationBps >= 50) {
    colorClasses = "bg-[#FEF3C7] text-[#D97706]";
  } else {
    colorClasses = "bg-[#DAE9E0] text-[#2D6A4F]";
  }

  const sign = sum >= 1 ? "+" : "-";
  const tooltip = `Probability sum deviation: ${sign}${deviationBps}bps${
    deviationBps > 200 ? " \u2014 potential arbitrage" : ""
  }`;

  return (
    <span
      title={tooltip}
      className={cn(
        "inline-flex items-center rounded-full px-2 py-0.5 text-[10px] font-medium",
        colorClasses
      )}
      style={{ fontFamily: "var(--font-jetbrains-mono)" }}
    >
      &Sigma; {pctLabel}%
    </span>
  );
}
