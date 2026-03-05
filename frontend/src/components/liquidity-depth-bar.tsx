"use client";

import { cn } from "@/lib/utils";
import type { MarketState } from "@/lib/types";

interface LiquidityDepthBarProps {
  market: MarketState;
}

function computeDepths(market: MarketState): { bidDepth: number; askDepth: number } | null {
  if (!market.orderbooks || market.orderbooks.length === 0) return null;

  let bidDepth = 0;
  let askDepth = 0;

  for (const ob of market.orderbooks) {
    const topBids = ob.bids.slice(0, 5);
    const topAsks = ob.asks.slice(0, 5);

    for (const level of topBids) {
      bidDepth += parseFloat(level.price) * parseFloat(level.size);
    }
    for (const level of topAsks) {
      askDepth += parseFloat(level.price) * parseFloat(level.size);
    }
  }

  return { bidDepth, askDepth };
}

function formatDollar(value: number): string {
  if (value >= 1000) return `$${(value / 1000).toFixed(1)}K`;
  return `$${value.toFixed(0)}`;
}

export function LiquidityDepthBar({ market }: LiquidityDepthBarProps) {
  const depths = computeDepths(market);

  if (!depths || (depths.bidDepth === 0 && depths.askDepth === 0)) {
    return (
      <span
        className="text-xs text-[#9B9B9B]"
        style={{ fontFamily: "var(--font-jetbrains-mono)" }}
      >
        &mdash;
      </span>
    );
  }

  const { bidDepth, askDepth } = depths;
  const total = bidDepth + askDepth;
  const bidPct = total > 0 ? (bidDepth / total) * 100 : 50;
  const tooltip = `Bid depth: ${formatDollar(bidDepth)} | Ask depth: ${formatDollar(askDepth)}`;

  return (
    <div title={tooltip} className="inline-flex items-center gap-0">
      <div
        className={cn(
          "relative h-3 overflow-hidden rounded-sm",
          "bg-[#E6E4DF]"
        )}
        style={{ width: 60 }}
      >
        {/* Bid side (green, left) */}
        <div
          className="absolute inset-y-0 left-0 rounded-l-sm bg-[#2D6A4F]/70"
          style={{ width: `${bidPct}%` }}
        />
        {/* Ask side (red, right) */}
        <div
          className="absolute inset-y-0 right-0 rounded-r-sm bg-[#B44C3F]/70"
          style={{ width: `${100 - bidPct}%` }}
        />
      </div>
    </div>
  );
}
