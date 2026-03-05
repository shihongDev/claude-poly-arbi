"use client";

import { cn, formatEndDate, formatPriceChange, formatCents, formatUsdCompact, spreadColorClass, MONO_STYLE, OUTCOME_COLORS } from "@/lib/utils";
import { Badge } from "@/components/ui/badge";
import { ArrowUp, ArrowDown, BookOpen } from "lucide-react";
import type { MarketState } from "@/lib/types";

interface MarketCardGridProps {
  markets: MarketState[];
  opportunityCounts: Map<string, number>;
  onMarketClick: (conditionId: string) => void;
}

// ── Helpers ────────────────────────────────────────────────────

function spreadBps(spread: string | null): number | null {
  if (!spread) return null;
  const num = parseFloat(spread);
  if (isNaN(num)) return null;
  return Math.round(num * 10000);
}

// ── Outcome bar ────────────────────────────────────────────────

function OutcomeBar({ prices }: { prices: string[] }) {
  const nums = prices.map((p) => parseFloat(p) || 0);
  const total = nums.reduce((a, b) => a + b, 0) || 1;

  return (
    <div className="flex h-1 w-full overflow-hidden rounded-full bg-[#E6E4DF]">
      {nums.map((n, i) => (
        <div
          key={i}
          className="h-full transition-all duration-300"
          style={{
            width: `${(n / total) * 100}%`,
            backgroundColor: OUTCOME_COLORS[i % OUTCOME_COLORS.length],
          }}
        />
      ))}
    </div>
  );
}

// ── Stat cell ──────────────────────────────────────────────────

function StatCell({
  label,
  value,
  className,
}: {
  label: string;
  value: string;
  className?: string;
}) {
  return (
    <div>
      <p className="text-[10px] font-medium uppercase tracking-wider text-[#9B9B9B]">
        {label}
      </p>
      <p
        className={cn("mt-0.5 text-sm text-[#1A1A19]", className)}
        style={MONO_STYLE}
      >
        {value}
      </p>
    </div>
  );
}

// ── Card ───────────────────────────────────────────────────────

function MarketCard({
  market,
  arbCount,
  onClick,
}: {
  market: MarketState;
  arbCount: number;
  onClick: () => void;
}) {
  const price = market.outcome_prices[0] ?? "0";
  const change = formatPriceChange(market.one_day_price_change);
  const bps = spreadBps(market.spread);
  const hasOrderbook = market.orderbooks.length > 0;

  return (
    <div
      role="button"
      tabIndex={0}
      onClick={onClick}
      onKeyDown={(e) => {
        if (e.key === "Enter" || e.key === " ") onClick();
      }}
      className="cursor-pointer rounded-2xl bg-white p-4 transition-shadow hover:shadow-md focus-visible:outline-none focus-visible:ring-2 focus-visible:ring-[#2D6A4F]/30"
    >
      {/* Header: question */}
      <p className="line-clamp-2 text-sm font-medium leading-snug text-[#1A1A19]">
        {market.question}
      </p>

      {/* Price + change */}
      <div className="mt-3 flex items-baseline gap-2">
        <span
          className="text-2xl font-bold text-[#1A1A19]"
          style={MONO_STYLE}
        >
          {formatCents(price)}
        </span>
        {change.positive !== null && (
          <Badge
            className={cn(
              "rounded-[9999px] px-1.5 py-0 text-[11px] font-medium",
              change.positive
                ? "bg-[#DAE9E0] text-[#2D6A4F]"
                : "bg-[#F5E0DD] text-[#B44C3F]"
            )}
          >
            {change.positive ? (
              <ArrowUp className="mr-0.5 inline h-3 w-3" />
            ) : (
              <ArrowDown className="mr-0.5 inline h-3 w-3" />
            )}
            {change.text}
          </Badge>
        )}
      </div>

      {/* Outcome bar */}
      <div className="mt-3">
        <OutcomeBar prices={market.outcome_prices} />
      </div>

      {/* Stats grid 2x2 */}
      <div className="mt-3 grid grid-cols-2 gap-x-4 gap-y-2">
        <StatCell
          label="Spread"
          value={bps !== null ? `${bps} bps` : "\u2014"}
          className={spreadColorClass(bps)}
        />
        <StatCell label="Volume" value={formatUsdCompact(market.volume_24hr)} />
        <StatCell label="Liquidity" value={formatUsdCompact(market.liquidity)} />
        <div>
          <p className="text-[10px] font-medium uppercase tracking-wider text-[#9B9B9B]">
            Orderbook
          </p>
          <div className="mt-0.5">
            {hasOrderbook ? (
              <Badge className="rounded-[9999px] bg-[#DAE9E0] px-1.5 py-0 text-[11px] text-[#2D6A4F]">
                <BookOpen className="mr-0.5 inline h-3 w-3" />
                Yes
              </Badge>
            ) : (
              <Badge className="rounded-[9999px] bg-[#F0EEEA] px-1.5 py-0 text-[11px] text-[#9B9B9B]">
                No
              </Badge>
            )}
          </div>
        </div>
      </div>

      {/* Footer: end date + arb count */}
      <div className="mt-3 flex items-center justify-between border-t border-[#E6E4DF] pt-2">
        <span className="text-xs text-[#9B9B9B]">
          {formatEndDate(market.end_date_iso)}
        </span>
        {arbCount > 0 && (
          <Badge className="rounded-[9999px] bg-[#2D6A4F] px-2 py-0.5 text-[11px] text-white">
            {arbCount} arb{arbCount !== 1 ? "s" : ""}
          </Badge>
        )}
      </div>
    </div>
  );
}

// ── Grid ───────────────────────────────────────────────────────

export function MarketCardGrid({
  markets,
  opportunityCounts,
  onMarketClick,
}: MarketCardGridProps) {
  if (markets.length === 0) {
    return (
      <div className="flex min-h-[240px] items-center justify-center">
        <p className="text-sm text-[#9B9B9B]">No markets match your filters</p>
      </div>
    );
  }

  return (
    <div className="grid grid-cols-1 gap-4 sm:grid-cols-2 lg:grid-cols-3 xl:grid-cols-4">
      {markets.map((market) => (
        <MarketCard
          key={market.condition_id}
          market={market}
          arbCount={opportunityCounts.get(market.condition_id) ?? 0}
          onClick={() => onMarketClick(market.condition_id)}
        />
      ))}
    </div>
  );
}
