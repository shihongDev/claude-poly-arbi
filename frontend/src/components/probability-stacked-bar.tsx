"use client";

import { useState, useMemo } from "react";
import { cn } from "@/lib/utils";

const PALETTE = [
  "#2D6A4F",
  "#3b82f6",
  "#D97706",
  "#8b5cf6",
  "#ec4899",
  "#06b6d4",
  "#f97316",
  "#14b8a6",
];

const BINARY_COLORS = ["#2D6A4F", "#B44C3F"];

interface ProbabilityStackedBarProps {
  outcomes: string[];
  prices: string[];
}

interface Segment {
  name: string;
  value: number;
  color: string;
  widthPct: number;
}

export function ProbabilityStackedBar({
  outcomes,
  prices,
}: ProbabilityStackedBarProps) {
  const [hovering, setHovering] = useState(false);

  const { segments, total, hasGap, gapPct } = useMemo(() => {
    const parsed = prices.map((p) => parseFloat(p || "0"));
    const sum = parsed.reduce((acc, v) => acc + v, 0);
    const isBinary = outcomes.length === 2;
    const colors = isBinary ? BINARY_COLORS : PALETTE;

    // Normalize widths proportional to price/total (or price/1 if sum < 1)
    const denominator = Math.max(sum, 1);
    const segs: Segment[] = parsed.map((val, i) => ({
      name: outcomes[i] ?? `Outcome ${i + 1}`,
      value: val,
      color: colors[i % colors.length],
      widthPct: (val / denominator) * 100,
    }));

    const gap = Math.abs(sum - 1);
    const hasGapOrExcess = gap > 0.005; // > 0.5% threshold
    const gapWidthPct = sum < 1 ? ((1 - sum) / 1) * 100 : 0;

    return {
      segments: segs,
      total: sum,
      hasGap: hasGapOrExcess && sum < 1,
      gapPct: gapWidthPct,
    };
  }, [outcomes, prices]);

  return (
    <div
      className="relative inline-block"
      onMouseEnter={() => setHovering(true)}
      onMouseLeave={() => setHovering(false)}
    >
      {/* Bar */}
      <div
        className="flex h-3.5 overflow-hidden rounded-full"
        style={{ width: 100 }}
      >
        {segments.map((seg, i) => (
          <div
            key={i}
            className="h-full shrink-0 transition-all"
            style={{
              width: `${seg.widthPct}%`,
              backgroundColor: seg.color,
              minWidth: seg.value > 0 ? 2 : 0,
            }}
          />
        ))}
        {hasGap && (
          <div
            className="h-full shrink-0"
            style={{
              width: `${gapPct}%`,
              background:
                "repeating-linear-gradient(45deg, #E6E4DF, #E6E4DF 2px, transparent 2px, transparent 4px)",
              minWidth: gapPct > 0 ? 2 : 0,
            }}
          />
        )}
        {total > 1 && (
          <div
            className="absolute right-0 top-0 h-full"
            style={{
              width: 3,
              background:
                "repeating-linear-gradient(45deg, #B44C3F, #B44C3F 1px, transparent 1px, transparent 2px)",
            }}
          />
        )}
      </div>

      {/* Tooltip */}
      {hovering && (
        <div
          className="absolute bottom-full left-1/2 z-50 mb-2 -translate-x-1/2 rounded-[10px] border border-[#E6E4DF] bg-white px-3 py-2 shadow-md"
          style={{ minWidth: 140, whiteSpace: "nowrap" }}
        >
          <div className="space-y-1">
            {segments.map((seg, i) => (
              <div key={i} className="flex items-center gap-2 text-xs">
                <div
                  className="h-2 w-2 shrink-0 rounded-full"
                  style={{ backgroundColor: seg.color }}
                />
                <span className="text-[#6B6B6B]">{seg.name}</span>
                <span
                  className="ml-auto text-[#1A1A19]"
                  style={{ fontFamily: "var(--font-jetbrains-mono)" }}
                >
                  {(seg.value * 100).toFixed(1)}%
                </span>
              </div>
            ))}
            {Math.abs(total - 1) > 0.005 && (
              <div className="border-t border-[#E6E4DF] pt-1">
                <div className="flex items-center justify-between text-[10px]">
                  <span className="text-[#9B9B9B]">Sum</span>
                  <span
                    className={cn(
                      total > 1 ? "text-[#B44C3F]" : "text-[#D97706]"
                    )}
                    style={{ fontFamily: "var(--font-jetbrains-mono)" }}
                  >
                    {(total * 100).toFixed(1)}%
                  </span>
                </div>
              </div>
            )}
          </div>
          {/* Tooltip arrow */}
          <div className="absolute left-1/2 top-full -translate-x-1/2">
            <div className="h-0 w-0 border-x-[5px] border-t-[5px] border-x-transparent border-t-[#E6E4DF]" />
            <div className="absolute -top-px left-1/2 h-0 w-0 -translate-x-1/2 border-x-[4px] border-t-[4px] border-x-transparent border-t-white" />
          </div>
        </div>
      )}
    </div>
  );
}
