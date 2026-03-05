"use client";

import { cn } from "@/lib/utils";
import type { MarketState } from "@/lib/types";

interface OrderImbalanceArrowProps {
  market: MarketState;
}

function computeImbalanceRatio(market: MarketState): number | null {
  if (!market.orderbooks || market.orderbooks.length === 0) return null;

  let totalBidSize = 0;
  let totalAskSize = 0;

  for (const ob of market.orderbooks) {
    const topBids = ob.bids.slice(0, 10);
    const topAsks = ob.asks.slice(0, 10);

    for (const level of topBids) {
      totalBidSize += parseFloat(level.size);
    }
    for (const level of topAsks) {
      totalAskSize += parseFloat(level.size);
    }
  }

  if (totalAskSize === 0) return totalBidSize > 0 ? Infinity : null;
  return totalBidSize / totalAskSize;
}

interface IndicatorConfig {
  symbol: string;
  color: string;
  fontWeight: string;
  tooltip: string;
}

function getIndicator(ratio: number | null): IndicatorConfig {
  if (ratio === null) {
    return {
      symbol: "\u2014",
      color: "text-[#9B9B9B]",
      fontWeight: "font-normal",
      tooltip: "No orderbook data",
    };
  }

  if (ratio > 1.5) {
    return {
      symbol: "\u2191",
      color: "text-[#2D6A4F]",
      fontWeight: "font-bold",
      tooltip: `Bullish \u2014 bid/ask ratio: ${ratio.toFixed(2)}`,
    };
  }
  if (ratio > 1.1) {
    return {
      symbol: "\u2191",
      color: "text-[#2D6A4F]/60",
      fontWeight: "font-medium",
      tooltip: `Slightly bullish \u2014 bid/ask ratio: ${ratio.toFixed(2)}`,
    };
  }
  if (ratio >= 0.9) {
    return {
      symbol: "\u2014",
      color: "text-[#9B9B9B]",
      fontWeight: "font-normal",
      tooltip: `Neutral \u2014 bid/ask ratio: ${ratio.toFixed(2)}`,
    };
  }
  if (ratio >= 0.67) {
    return {
      symbol: "\u2193",
      color: "text-[#B44C3F]/60",
      fontWeight: "font-medium",
      tooltip: `Slightly bearish \u2014 bid/ask ratio: ${ratio.toFixed(2)}`,
    };
  }
  return {
    symbol: "\u2193",
    color: "text-[#B44C3F]",
    fontWeight: "font-bold",
    tooltip: `Bearish \u2014 bid/ask ratio: ${ratio.toFixed(2)}`,
  };
}

export function OrderImbalanceArrow({ market }: OrderImbalanceArrowProps) {
  const ratio = computeImbalanceRatio(market);
  const { symbol, color, fontWeight, tooltip } = getIndicator(ratio);

  return (
    <span
      title={tooltip}
      className={cn(
        "inline-flex h-4 w-4 items-center justify-center text-sm leading-none",
        color,
        fontWeight
      )}
      style={{ fontFamily: "var(--font-jetbrains-mono)" }}
    >
      {symbol}
    </span>
  );
}
