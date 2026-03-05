"use client";

import { useState, useEffect, useCallback, useMemo } from "react";
import Link from "next/link";
import { Star, X } from "lucide-react";
import { Badge } from "@/components/ui/badge";
import { cn, formatSpreadBps, formatPriceChange, formatCents, MONO_STYLE } from "@/lib/utils";
import type { MarketState } from "@/lib/types";
const STORAGE_KEY = "poly-arb-watchlist";

// ── Watchlist Hook ─────────────────────────────────────────────

let globalWatchlist: Set<string> = new Set();
let listeners: Array<() => void> = [];

function notifyListeners() {
  for (const fn of listeners) fn();
}

function persistWatchlist(watchlist: Set<string>) {
  try {
    localStorage.setItem(STORAGE_KEY, JSON.stringify([...watchlist]));
  } catch {
    // localStorage not available
  }
}

function loadWatchlist(): Set<string> {
  try {
    const raw = localStorage.getItem(STORAGE_KEY);
    if (raw) {
      const parsed = JSON.parse(raw);
      if (Array.isArray(parsed)) {
        return new Set(parsed.filter((v): v is string => typeof v === "string"));
      }
    }
  } catch {
    // localStorage not available or corrupt
  }
  return new Set();
}

export function useWatchlist() {
  const [, setTick] = useState(0);

  useEffect(() => {
    // Initialize from localStorage on first mount across hooks
    if (globalWatchlist.size === 0) {
      const stored = loadWatchlist();
      if (stored.size > 0) {
        globalWatchlist = stored;
        notifyListeners();
      }
    }

    const listener = () => setTick((t) => t + 1);
    listeners.push(listener);
    return () => {
      listeners = listeners.filter((l) => l !== listener);
    };
  }, []);

  const toggle = useCallback((conditionId: string) => {
    const next = new Set(globalWatchlist);
    if (next.has(conditionId)) {
      next.delete(conditionId);
    } else {
      next.add(conditionId);
    }
    globalWatchlist = next;
    persistWatchlist(next);
    notifyListeners();
  }, []);

  const isWatched = useCallback((conditionId: string) => {
    return globalWatchlist.has(conditionId);
  }, []);

  return { watchlist: globalWatchlist, toggle, isWatched };
}

// ── Star Icon Button ───────────────────────────────────────────

interface WatchlistStarProps {
  conditionId: string;
}

export function WatchlistStar({ conditionId }: WatchlistStarProps) {
  const { isWatched, toggle } = useWatchlist();
  const watched = isWatched(conditionId);

  return (
    <button
      onClick={(e) => {
        e.preventDefault();
        e.stopPropagation();
        toggle(conditionId);
      }}
      className={cn(
        "inline-flex items-center justify-center transition-transform hover:scale-110 active:scale-95",
        watched ? "text-[#D97706]" : "text-[#D5D3CE] hover:text-[#9B9B9B]"
      )}
      aria-label={watched ? "Remove from watchlist" : "Add to watchlist"}
    >
      <Star
        className="h-3.5 w-3.5"
        fill={watched ? "currentColor" : "none"}
        strokeWidth={watched ? 0 : 1.5}
      />
    </button>
  );
}

// ── Watchlist Section ──────────────────────────────────────────

interface WatchlistSectionProps {
  markets: MarketState[];
}

function WatchlistCard({
  market,
  onRemove,
}: {
  market: MarketState;
  onRemove: () => void;
}) {
  const priceChange = formatPriceChange(market.one_day_price_change);
  const primaryPrice = market.outcome_prices[0] ?? null;

  return (
    <Link
      href={`/markets/${market.condition_id}`}
      className="group relative block w-[200px] shrink-0 rounded-xl bg-[#F8F7F4] p-3.5 transition-colors hover:bg-[#F0EEEA]"
      style={{ scrollSnapAlign: "start" }}
    >
      {/* Remove button */}
      <button
        onClick={(e) => {
          e.preventDefault();
          e.stopPropagation();
          onRemove();
        }}
        className="absolute right-2 top-2 flex h-5 w-5 items-center justify-center rounded-full text-[#D5D3CE] opacity-0 transition-opacity hover:bg-white hover:text-[#6B6B6B] group-hover:opacity-100"
        aria-label="Unpin from watchlist"
      >
        <X className="h-3 w-3" />
      </button>

      {/* Question */}
      <p className="line-clamp-1 pr-5 text-[12px] font-medium leading-snug text-[#1A1A19]">
        {market.question}
      </p>

      {/* Price + 24h change */}
      <div className="mt-2 flex items-baseline gap-1.5">
        <span
          className="text-lg font-bold text-[#1A1A19]"
          style={MONO_STYLE}
        >
          {formatCents(primaryPrice)}
        </span>
        {priceChange.positive !== null && (
          <span
            className={cn(
              "text-[10px] font-medium",
              priceChange.positive ? "text-[#2D6A4F]" : "text-[#B44C3F]"
            )}
            style={MONO_STYLE}
          >
            {priceChange.text}
          </span>
        )}
      </div>

      {/* Spread badge */}
      {market.spread && (
        <div className="mt-2">
          <Badge className="bg-white text-[#6B6B6B] text-[10px]">
            {formatSpreadBps(market.spread)}
          </Badge>
        </div>
      )}
    </Link>
  );
}

export function WatchlistSection({ markets }: WatchlistSectionProps) {
  const { watchlist, toggle } = useWatchlist();

  const watchedMarkets = useMemo(() => {
    if (watchlist.size === 0) return [];
    return markets.filter((m) => watchlist.has(m.condition_id));
  }, [markets, watchlist]);

  if (watchedMarkets.length === 0) return null;

  return (
    <div className="rounded-2xl bg-white p-5">
      {/* Header */}
      <div className="flex items-center gap-2">
        <Star className="h-3.5 w-3.5 text-[#D97706]" fill="currentColor" strokeWidth={0} />
        <h3 className="text-[11px] font-medium uppercase tracking-wider text-[#9B9B9B]">
          Watchlist
        </h3>
        <Badge className="bg-[#F0EEEA] text-[#6B6B6B] text-[10px]">
          {watchedMarkets.length}
        </Badge>
      </div>

      {/* Horizontal scroll row */}
      <div
        className="mt-3 flex gap-2.5 overflow-x-auto pb-1"
        style={{
          scrollSnapType: "x mandatory",
          scrollbarWidth: "none",
          msOverflowStyle: "none",
        }}
      >
        {watchedMarkets.map((market) => (
          <WatchlistCard
            key={market.condition_id}
            market={market}
            onRemove={() => toggle(market.condition_id)}
          />
        ))}
      </div>
    </div>
  );
}
