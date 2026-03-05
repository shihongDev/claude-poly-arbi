"use client";

import { cn } from "@/lib/utils";

interface ArbOpportunityBadgeProps {
  count: number;
  bestEdge?: string;
}

export function ArbOpportunityBadge({ count, bestEdge }: ArbOpportunityBadgeProps) {
  if (count === 0) return null;

  const edgeBps = bestEdge
    ? `${(parseFloat(bestEdge) * 10000).toFixed(0)}bps`
    : null;

  const tooltip = edgeBps
    ? `${count} active opportunit${count === 1 ? "y" : "ies"}, best edge: ${edgeBps}`
    : `${count} active opportunit${count === 1 ? "y" : "ies"}`;

  return (
    <span
      title={tooltip}
      className={cn(
        "relative inline-flex items-center gap-1 rounded-full px-2 py-0.5",
        "bg-[#DAE9E0] text-[#2D6A4F]",
        "text-[10px] font-medium"
      )}
      style={{ fontFamily: "var(--font-jetbrains-mono)" }}
    >
      {/* Pulse glow behind the badge */}
      <span
        className="absolute inset-0 rounded-full bg-[#2D6A4F]/20 animate-pulse"
        aria-hidden="true"
      />
      <span className="relative inline-flex items-center gap-1">
        <span
          className="inline-block h-1.5 w-1.5 rounded-full bg-[#2D6A4F]"
          aria-hidden="true"
        />
        {count} opp{count !== 1 ? "s" : ""}
      </span>
    </span>
  );
}
