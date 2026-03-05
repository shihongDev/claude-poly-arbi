"use client";

import { useRef, useState, useMemo, useCallback } from "react";
import Link from "next/link";
import { ChevronLeft, ChevronRight, Flame } from "lucide-react";
import { Badge } from "@/components/ui/badge";
import { cn, formatUsd, formatSpreadBps, formatPriceChange, formatCents, MONO_STYLE } from "@/lib/utils";
import type { MarketState } from "@/lib/types";

interface HotMarketsCarouselProps {
  markets: MarketState[];
  opportunityCounts: Map<string, number>;
}

function computeHotnessScores(
  markets: MarketState[],
  opportunityCounts: Map<string, number>
): { market: MarketState; score: number }[] {
  if (markets.length === 0) return [];

  // Compute raw values
  const entries = markets.map((m) => ({
    market: m,
    volume: parseFloat(m.volume_24hr ?? "0"),
    absChange: Math.abs(parseFloat(m.one_day_price_change ?? "0")),
    oppCount: opportunityCounts.get(m.condition_id) ?? 0,
  }));

  // Rank by volume (higher = better rank)
  const sortedByVolume = [...entries].sort((a, b) => b.volume - a.volume);
  const volumeRank = new Map<string, number>();
  sortedByVolume.forEach((e, i) => {
    volumeRank.set(e.market.condition_id, entries.length - i);
  });

  // Rank by abs change
  const sortedByChange = [...entries].sort(
    (a, b) => b.absChange - a.absChange
  );
  const changeRank = new Map<string, number>();
  sortedByChange.forEach((e, i) => {
    changeRank.set(e.market.condition_id, entries.length - i);
  });

  return entries.map((e) => ({
    market: e.market,
    score:
      (volumeRank.get(e.market.condition_id) ?? 0) * 0.4 +
      (changeRank.get(e.market.condition_id) ?? 0) * 0.3 +
      e.oppCount * 0.3,
  }));
}

function HotMarketCard({
  market,
  oppCount,
}: {
  market: MarketState;
  oppCount: number;
}) {
  const priceChange = formatPriceChange(market.one_day_price_change);
  const primaryPrice = market.outcome_prices[0] ?? null;

  return (
    <Link
      href={`/markets/${market.condition_id}`}
      className="block w-[240px] shrink-0 scroll-snap-align-start rounded-2xl bg-white p-4 transition-shadow hover:shadow-md"
      style={{ scrollSnapAlign: "start" }}
    >
      {/* Question text */}
      <p className="line-clamp-2 text-[13px] font-medium leading-snug text-[#1A1A19]">
        {market.question}
      </p>

      {/* Price + 24h change */}
      <div className="mt-3 flex items-baseline gap-2">
        <span
          className="text-2xl font-bold text-[#1A1A19]"
          style={MONO_STYLE}
        >
          {formatCents(primaryPrice)}
        </span>
        {priceChange.positive !== null && (
          <Badge
            className={cn(
              "text-[10px]",
              priceChange.positive
                ? "bg-[#DAE9E0] text-[#2D6A4F]"
                : "bg-[#F5E0DD] text-[#B44C3F]"
            )}
          >
            {priceChange.text}
          </Badge>
        )}
      </div>

      {/* Bottom row: volume, spread, arb count */}
      <div className="mt-3 flex flex-wrap items-center gap-1.5">
        <Badge className="bg-[#F0EEEA] text-[#6B6B6B] text-[10px]">
          Vol {formatUsd(market.volume_24hr)}
        </Badge>
        <Badge className="bg-[#F0EEEA] text-[#6B6B6B] text-[10px]">
          {formatSpreadBps(market.spread)}
        </Badge>
        {oppCount > 0 && (
          <Badge className="bg-[#DAE9E0] text-[#2D6A4F] text-[10px]">
            {oppCount} arb{oppCount !== 1 ? "s" : ""}
          </Badge>
        )}
      </div>
    </Link>
  );
}

export function HotMarketsCarousel({
  markets,
  opportunityCounts,
}: HotMarketsCarouselProps) {
  const scrollRef = useRef<HTMLDivElement>(null);
  const [canScrollLeft, setCanScrollLeft] = useState(false);
  const [canScrollRight, setCanScrollRight] = useState(true);

  const hotMarkets = useMemo(() => {
    const activeMarkets = markets.filter((m) => m.active);
    const scored = computeHotnessScores(activeMarkets, opportunityCounts);
    scored.sort((a, b) => b.score - a.score);
    return scored.slice(0, 8);
  }, [markets, opportunityCounts]);

  const updateScrollState = useCallback(() => {
    const el = scrollRef.current;
    if (!el) return;
    setCanScrollLeft(el.scrollLeft > 0);
    setCanScrollRight(el.scrollLeft + el.clientWidth < el.scrollWidth - 1);
  }, []);

  const scroll = useCallback(
    (direction: "left" | "right") => {
      const el = scrollRef.current;
      if (!el) return;
      const scrollAmount = 252; // card width + gap
      el.scrollBy({
        left: direction === "left" ? -scrollAmount : scrollAmount,
        behavior: "smooth",
      });
      // Delay check to allow smooth scroll to finish
      setTimeout(updateScrollState, 350);
    },
    [updateScrollState]
  );

  if (hotMarkets.length === 0) return null;

  return (
    <div className="space-y-3">
      {/* Header with scroll arrows */}
      <div className="flex items-center justify-between">
        <div className="flex items-center gap-2">
          <Flame className="h-3.5 w-3.5 text-[#D97706]" />
          <h3 className="text-[11px] font-medium uppercase tracking-wider text-[#9B9B9B]">
            Trending Markets
          </h3>
        </div>
        <div className="flex items-center gap-1">
          <button
            onClick={() => scroll("left")}
            disabled={!canScrollLeft}
            className={cn(
              "flex h-7 w-7 items-center justify-center rounded-[10px] transition-colors",
              canScrollLeft
                ? "text-[#6B6B6B] hover:bg-[#F0EEEA] hover:text-[#1A1A19]"
                : "text-[#E6E4DF] cursor-default"
            )}
            aria-label="Scroll left"
          >
            <ChevronLeft className="h-4 w-4" />
          </button>
          <button
            onClick={() => scroll("right")}
            disabled={!canScrollRight}
            className={cn(
              "flex h-7 w-7 items-center justify-center rounded-[10px] transition-colors",
              canScrollRight
                ? "text-[#6B6B6B] hover:bg-[#F0EEEA] hover:text-[#1A1A19]"
                : "text-[#E6E4DF] cursor-default"
            )}
            aria-label="Scroll right"
          >
            <ChevronRight className="h-4 w-4" />
          </button>
        </div>
      </div>

      {/* Scroll container */}
      <div
        ref={scrollRef}
        className="flex gap-3 overflow-x-auto scroll-smooth pb-2"
        style={{
          scrollSnapType: "x mandatory",
          scrollbarWidth: "none",
          msOverflowStyle: "none",
        }}
        onScroll={updateScrollState}
      >
        {hotMarkets.map(({ market }) => (
          <HotMarketCard
            key={market.condition_id}
            market={market}
            oppCount={opportunityCounts.get(market.condition_id) ?? 0}
          />
        ))}
      </div>
    </div>
  );
}
