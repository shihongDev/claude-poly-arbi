"use client";

import { useMemo } from "react";
import { cn } from "@/lib/utils";
import type { OrderbookLevel } from "@/lib/types";

interface OrderbookLadderProps {
  bids: OrderbookLevel[];
  asks: OrderbookLevel[];
  maxLevels?: number;
}

export function OrderbookLadder({
  bids,
  asks,
  maxLevels = 10,
}: OrderbookLadderProps) {
  const { topBids, topAsks, maxSize } = useMemo(() => {
    const sortedBids = [...bids]
      .sort((a, b) => parseFloat(b.price) - parseFloat(a.price))
      .slice(0, maxLevels);
    const sortedAsks = [...asks]
      .sort((a, b) => parseFloat(a.price) - parseFloat(b.price))
      .slice(0, maxLevels);

    const allSizes = [
      ...sortedBids.map((l) => parseFloat(l.size)),
      ...sortedAsks.map((l) => parseFloat(l.size)),
    ];
    const max = allSizes.length > 0 ? Math.max(...allSizes) : 1;

    return { topBids: sortedBids, topAsks: sortedAsks, maxSize: max };
  }, [bids, asks, maxLevels]);

  if (topBids.length === 0 && topAsks.length === 0) {
    return (
      <div className="flex h-full items-center justify-center text-sm text-[#9B9B9B]">
        No orderbook data
      </div>
    );
  }

  const formatSize = (size: string) => {
    const num = parseFloat(size);
    if (num >= 1000) return `${(num / 1000).toFixed(1)}K`;
    return num.toFixed(0);
  };

  return (
    <div className="flex h-full flex-col text-xs" style={{ fontFamily: "var(--font-jetbrains-mono)" }}>
      {/* Header */}
      <div className="flex items-center border-b border-[#E6E4DF] px-3 py-2">
        <span className="w-20 text-[#9B9B9B]">Price</span>
        <span className="flex-1 text-right text-[#9B9B9B]">Size</span>
      </div>

      {/* Asks (reversed so lowest ask is closest to spread) */}
      <div className="flex flex-col-reverse">
        {topAsks.map((level, i) => {
          const size = parseFloat(level.size);
          const widthPct = maxSize > 0 ? (size / maxSize) * 100 : 0;
          return (
            <div
              key={`ask-${i}`}
              className="relative flex items-center px-3 py-1 hover:bg-[#FDF5F4]"
            >
              <div
                className="absolute inset-y-0 right-0 bg-[#B44C3F]/8"
                style={{ width: `${widthPct}%` }}
              />
              <span className="relative w-20 text-[#B44C3F]">
                {parseFloat(level.price).toFixed(4)}
              </span>
              <span className="relative flex-1 text-right text-[#6B6B6B]">
                {formatSize(level.size)}
              </span>
            </div>
          );
        })}
      </div>

      {/* Spread divider */}
      {topBids.length > 0 && topAsks.length > 0 && (
        <div className="flex items-center justify-center border-y border-[#E6E4DF] bg-[#F8F7F4] px-3 py-1.5">
          <span className="text-[10px] text-[#9B9B9B]">
            Spread:{" "}
            {(
              (parseFloat(topAsks[0].price) - parseFloat(topBids[0].price)) *
              10000
            ).toFixed(0)}{" "}
            bps
          </span>
        </div>
      )}

      {/* Bids */}
      <div className="flex-1">
        {topBids.map((level, i) => {
          const size = parseFloat(level.size);
          const widthPct = maxSize > 0 ? (size / maxSize) * 100 : 0;
          return (
            <div
              key={`bid-${i}`}
              className="relative flex items-center px-3 py-1 hover:bg-[#F5FAF7]"
            >
              <div
                className="absolute inset-y-0 right-0 bg-[#2D6A4F]/8"
                style={{ width: `${widthPct}%` }}
              />
              <span className="relative w-20 text-[#2D6A4F]">
                {parseFloat(level.price).toFixed(4)}
              </span>
              <span className="relative flex-1 text-right text-[#6B6B6B]">
                {formatSize(level.size)}
              </span>
            </div>
          );
        })}
      </div>
    </div>
  );
}
