"use client";

import { cn } from "@/lib/utils";
import type { MarketState } from "@/lib/types";

interface MarketHealthBadgeProps {
  market: MarketState;
}

type Grade = "A" | "B" | "C" | "D" | "F";

const gradeColors: Record<Grade, string> = {
  A: "bg-[#DAE9E0] text-[#2D6A4F]",
  B: "bg-[#E8EFE6] text-[#3D7A5F]",
  C: "bg-[#FEF3C7] text-[#D97706]",
  D: "bg-[#FDEDD3] text-[#C2570A]",
  F: "bg-[#F5E0DD] text-[#B44C3F]",
};

function scoreSpread(market: MarketState): number {
  const spread = market.spread ? parseFloat(market.spread) : null;
  if (spread === null || isNaN(spread)) return 0;
  const bps = spread * 10000;
  if (bps < 30) return 3;
  if (bps <= 100) return 2;
  return 0;
}

function scoreVolume(market: MarketState): number {
  const vol = market.volume_24hr ? parseFloat(market.volume_24hr) : 0;
  if (isNaN(vol)) return 0;
  if (vol > 100000) return 3;
  if (vol >= 10000) return 2;
  return 0;
}

function scoreOrderbookDepth(market: MarketState): number {
  if (!market.orderbooks || market.orderbooks.length === 0) return 0;

  let totalDepth = 0;
  for (const ob of market.orderbooks) {
    const topBids = ob.bids.slice(0, 5);
    const topAsks = ob.asks.slice(0, 5);
    for (const level of [...topBids, ...topAsks]) {
      totalDepth += parseFloat(level.price) * parseFloat(level.size);
    }
  }

  if (totalDepth > 5000) return 3;
  if (totalDepth >= 1000) return 2;
  return 0;
}

function computeGrade(market: MarketState): { grade: Grade; score: number; tooltip: string } {
  const spreadScore = scoreSpread(market);
  const volumeScore = scoreVolume(market);
  const depthScore = scoreOrderbookDepth(market);
  const total = spreadScore + volumeScore + depthScore;

  let grade: Grade;
  if (total >= 8) grade = "A";
  else if (total >= 6) grade = "B";
  else if (total >= 4) grade = "C";
  else if (total >= 2) grade = "D";
  else grade = "F";

  const tooltip = `Health: ${grade} (${total}/9) - Spread: ${spreadScore}/3, Volume: ${volumeScore}/3, Depth: ${depthScore}/3`;
  return { grade, score: total, tooltip };
}

export function MarketHealthBadge({ market }: MarketHealthBadgeProps) {
  const { grade, tooltip } = computeGrade(market);

  return (
    <span
      title={tooltip}
      className={cn(
        "inline-flex items-center justify-center rounded-full px-2 py-0.5 text-xs font-semibold",
        gradeColors[grade]
      )}
      style={{ fontFamily: "var(--font-jetbrains-mono)" }}
    >
      {grade}
    </span>
  );
}
