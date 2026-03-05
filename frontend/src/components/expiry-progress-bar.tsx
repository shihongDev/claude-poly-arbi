"use client";

import { cn } from "@/lib/utils";
import type { MarketState } from "@/lib/types";

interface ExpiryProgressBarProps {
  market: MarketState;
}

interface ExpiryInfo {
  label: string;
  pct: number;
  color: string;
}

function computeExpiry(endDateIso: string | null): ExpiryInfo | null {
  if (!endDateIso) return null;

  const now = Date.now();
  const end = new Date(endDateIso).getTime();
  const msRemaining = end - now;

  if (msRemaining <= 0) {
    return { label: "Ended", pct: 100, color: "bg-[#B44C3F]" };
  }

  const hoursRemaining = msRemaining / (1000 * 60 * 60);
  const daysRemaining = hoursRemaining / 24;

  if (daysRemaining > 30) {
    return { label: "30d+", pct: 15, color: "bg-[#2D6A4F]" };
  }
  if (daysRemaining >= 7) {
    return {
      label: `${Math.floor(daysRemaining)}d`,
      pct: 25 + ((30 - daysRemaining) / 23) * 25,
      color: "bg-[#2D6A4F]",
    };
  }
  if (daysRemaining >= 1) {
    return {
      label: `${Math.floor(daysRemaining)}d`,
      pct: 55 + ((7 - daysRemaining) / 6) * 25,
      color: "bg-[#D97706]",
    };
  }
  // Less than 24 hours
  return {
    label: `${Math.max(1, Math.floor(hoursRemaining))}h`,
    pct: 85 + ((24 - hoursRemaining) / 24) * 15,
    color: "bg-[#B44C3F]",
  };
}

export function ExpiryProgressBar({ market }: ExpiryProgressBarProps) {
  const expiry = computeExpiry(market.end_date_iso);

  if (!expiry) {
    return (
      <span
        className="text-xs text-[#9B9B9B]"
        style={{ fontFamily: "var(--font-jetbrains-mono)" }}
      >
        &infin;
      </span>
    );
  }

  const tooltip = market.end_date_iso
    ? `Expires: ${new Date(market.end_date_iso).toLocaleString("en-US", {
        month: "short",
        day: "numeric",
        year: "numeric",
        hour: "numeric",
        minute: "2-digit",
      })}`
    : "No expiry";

  return (
    <div title={tooltip} className="inline-flex items-center gap-1.5">
      <div
        className="h-1 overflow-hidden rounded-full bg-[#E6E4DF]"
        style={{ width: 80 }}
      >
        <div
          className={cn("h-full rounded-full transition-all", expiry.color)}
          style={{ width: `${Math.min(100, expiry.pct)}%` }}
        />
      </div>
      <span
        className={cn(
          "text-[10px] tabular-nums",
          expiry.pct > 80 ? "text-[#B44C3F]" : "text-[#6B6B6B]"
        )}
        style={{ fontFamily: "var(--font-jetbrains-mono)" }}
      >
        {expiry.label}
      </span>
    </div>
  );
}
