"use client";

import { useState, useMemo, useCallback } from "react";
import { ChevronRight } from "lucide-react";
import { Badge } from "@/components/ui/badge";
import {
  cn,
  formatUsd,
  formatSpreadBps,
  formatPriceChange,
  formatCents,
  deriveGroupTitle,
  MONO_STYLE,
} from "@/lib/utils";
import type { MarketState } from "@/lib/types";

// ── Types ──────────────────────────────────────────────────────

interface EventGroup {
  id: string;
  title: string;
  markets: MarketState[];
  aggregateVolume: number;
  averageSpread: number;
  totalOpps: number;
  probSum: number;
  isStandalone: boolean;
}

interface EventGroupTableProps {
  markets: MarketState[];
  opportunityCounts: Map<string, number>;
  onRowClick: (conditionId: string) => void;
}

// ── Component ──────────────────────────────────────────────────

export function EventGroupTable({
  markets,
  opportunityCounts,
  onRowClick,
}: EventGroupTableProps) {
  const [expandedGroups, setExpandedGroups] = useState<Set<string>>(new Set());

  const toggleGroup = useCallback((groupId: string) => {
    setExpandedGroups((prev) => {
      const next = new Set(prev);
      if (next.has(groupId)) {
        next.delete(groupId);
      } else {
        next.add(groupId);
      }
      return next;
    });
  }, []);

  const groups = useMemo(() => {
    // Group markets by event_id
    const eventMap = new Map<string, MarketState[]>();
    const standaloneMarkets: MarketState[] = [];

    for (const market of markets) {
      const eventId = market.event_id;
      if (!eventId) {
        standaloneMarkets.push(market);
        continue;
      }

      const existing = eventMap.get(eventId);
      if (existing) {
        existing.push(market);
      } else {
        eventMap.set(eventId, [market]);
      }
    }

    const result: EventGroup[] = [];

    // Process multi-market event groups
    for (const [eventId, eventMarkets] of eventMap.entries()) {
      if (eventMarkets.length === 1) {
        // Single market in event group => treat as standalone
        result.push({
          id: `standalone-${eventMarkets[0].condition_id}`,
          title: eventMarkets[0].question,
          markets: eventMarkets,
          aggregateVolume: parseFloat(eventMarkets[0].volume_24hr ?? "0"),
          averageSpread: parseFloat(eventMarkets[0].spread ?? "0"),
          totalOpps: opportunityCounts.get(eventMarkets[0].condition_id) ?? 0,
          probSum: parseFloat(eventMarkets[0].outcome_prices[0] ?? "0"),
          isStandalone: true,
        });
        continue;
      }

      const questions = eventMarkets.map((m) => m.question);
      const title = deriveGroupTitle(questions);

      let totalVolume = 0;
      let spreadSum = 0;
      let spreadCount = 0;
      let totalOpps = 0;
      let probSum = 0;

      for (const m of eventMarkets) {
        totalVolume += parseFloat(m.volume_24hr ?? "0");
        const sp = parseFloat(m.spread ?? "");
        if (!isNaN(sp)) {
          spreadSum += sp;
          spreadCount++;
        }
        totalOpps += opportunityCounts.get(m.condition_id) ?? 0;
        probSum += parseFloat(m.outcome_prices[0] ?? "0");
      }

      result.push({
        id: eventId,
        title,
        markets: eventMarkets,
        aggregateVolume: totalVolume,
        averageSpread: spreadCount > 0 ? spreadSum / spreadCount : 0,
        totalOpps,
        probSum,
        isStandalone: false,
      });
    }

    // Process standalone markets (no event_id)
    for (const market of standaloneMarkets) {
      result.push({
        id: `standalone-${market.condition_id}`,
        title: market.question,
        markets: [market],
        aggregateVolume: parseFloat(market.volume_24hr ?? "0"),
        averageSpread: parseFloat(market.spread ?? "0"),
        totalOpps: opportunityCounts.get(market.condition_id) ?? 0,
        probSum: parseFloat(market.outcome_prices[0] ?? "0"),
        isStandalone: true,
      });
    }

    // Sort by aggregate volume descending
    result.sort((a, b) => b.aggregateVolume - a.aggregateVolume);

    return result;
  }, [markets, opportunityCounts]);

  if (markets.length === 0) {
    return (
      <div className="rounded-2xl bg-white p-5">
        <div className="flex h-[200px] items-center justify-center text-sm text-[#9B9B9B]">
          No markets to display
        </div>
      </div>
    );
  }

  return (
    <div className="rounded-2xl bg-white overflow-hidden">
      {/* Header row */}
      <div
        className="sticky top-0 z-10 grid bg-white border-b border-[#E6E4DF] px-4 py-2.5"
        style={{
          gridTemplateColumns: "minmax(0, 1fr) 100px 100px 90px 100px 80px",
        }}
      >
        <span className="text-xs font-medium uppercase tracking-wider text-[#9B9B9B]">
          Market
        </span>
        <span className="text-right text-xs font-medium uppercase tracking-wider text-[#9B9B9B]">
          Price
        </span>
        <span className="text-right text-xs font-medium uppercase tracking-wider text-[#9B9B9B]">
          24h
        </span>
        <span className="text-right text-xs font-medium uppercase tracking-wider text-[#9B9B9B]">
          Spread
        </span>
        <span className="text-right text-xs font-medium uppercase tracking-wider text-[#9B9B9B]">
          Volume
        </span>
        <span className="text-right text-xs font-medium uppercase tracking-wider text-[#9B9B9B]">
          Opps
        </span>
      </div>

      {/* Body rows */}
      <div>
        {groups.map((group) => {
          if (group.isStandalone) {
            return (
              <StandaloneRow
                key={group.id}
                market={group.markets[0]}
                oppCount={group.totalOpps}
                onClick={() => onRowClick(group.markets[0].condition_id)}
              />
            );
          }

          const isExpanded = expandedGroups.has(group.id);

          return (
            <div key={group.id}>
              {/* Parent row */}
              <ParentRow
                group={group}
                isExpanded={isExpanded}
                onToggle={() => toggleGroup(group.id)}
              />

              {/* Child rows */}
              <div
                className={cn(
                  "overflow-hidden transition-all duration-200 ease-in-out",
                  isExpanded ? "max-h-[2000px] opacity-100" : "max-h-0 opacity-0"
                )}
              >
                {group.markets.map((market) => (
                  <ChildRow
                    key={market.condition_id}
                    market={market}
                    oppCount={opportunityCounts.get(market.condition_id) ?? 0}
                    onClick={() => onRowClick(market.condition_id)}
                  />
                ))}
              </div>
            </div>
          );
        })}
      </div>
    </div>
  );
}

// ── Row Subcomponents ──────────────────────────────────────────

function StandaloneRow({
  market,
  oppCount,
  onClick,
}: {
  market: MarketState;
  oppCount: number;
  onClick: () => void;
}) {
  const priceChange = formatPriceChange(market.one_day_price_change);
  const primaryPrice = market.outcome_prices[0] ?? null;

  return (
    <div
      onClick={onClick}
      className="grid cursor-pointer items-center border-b border-[#E6E4DF] px-4 py-3 transition-colors hover:bg-[#F8F7F4]/50"
      style={{
        gridTemplateColumns: "minmax(0, 1fr) 100px 100px 90px 100px 80px",
      }}
    >
      <div className="min-w-0">
        <p className="truncate text-sm text-[#1A1A19]">{market.question}</p>
      </div>
      <span className="text-right text-sm text-[#1A1A19]" style={MONO_STYLE}>
        {formatCents(primaryPrice)}
      </span>
      <span
        className={cn(
          "text-right text-sm",
          priceChange.positive === null
            ? "text-[#9B9B9B]"
            : priceChange.positive
              ? "text-[#2D6A4F]"
              : "text-[#B44C3F]"
        )}
        style={MONO_STYLE}
      >
        {priceChange.text}
      </span>
      <span className="text-right text-sm text-[#6B6B6B]" style={MONO_STYLE}>
        {formatSpreadBps(market.spread)}
      </span>
      <span className="text-right text-sm text-[#6B6B6B]" style={MONO_STYLE}>
        {formatUsd(market.volume_24hr)}
      </span>
      <div className="text-right">
        {oppCount > 0 ? (
          <Badge className="bg-[#DAE9E0] text-[#2D6A4F] text-[10px]">
            {oppCount}
          </Badge>
        ) : (
          <span className="text-sm text-[#D5D3CE]" style={MONO_STYLE}>0</span>
        )}
      </div>
    </div>
  );
}

function ParentRow({
  group,
  isExpanded,
  onToggle,
}: {
  group: EventGroup;
  isExpanded: boolean;
  onToggle: () => void;
}) {
  const probDeviation = Math.abs(group.probSum - 1) * 10000; // in bps

  return (
    <div
      onClick={onToggle}
      className={cn(
        "grid cursor-pointer items-center border-b border-[#E6E4DF] px-4 py-3 transition-colors",
        isExpanded ? "bg-[#FAFAF8]" : "hover:bg-[#F8F7F4]/50"
      )}
      style={{
        gridTemplateColumns: "minmax(0, 1fr) 100px 100px 90px 100px 80px",
      }}
    >
      <div className="flex min-w-0 items-center gap-2">
        <ChevronRight
          className={cn(
            "h-4 w-4 shrink-0 text-[#9B9B9B] transition-transform duration-200",
            isExpanded && "rotate-90"
          )}
        />
        <div className="min-w-0">
          <p className="truncate text-sm font-medium text-[#1A1A19]">
            {group.title}
          </p>
          <div className="mt-0.5 flex items-center gap-1.5">
            <Badge className="bg-[#F0EEEA] text-[#6B6B6B] text-[10px]">
              {group.markets.length} markets
            </Badge>
            {probDeviation > 100 && (
              <Badge className="bg-[#F5E0DD] text-[#B44C3F] text-[10px]">
                {"\u03A3"}P {(group.probSum * 100).toFixed(1)}%
              </Badge>
            )}
          </div>
        </div>
      </div>
      {/* Aggregate: no single price for a group */}
      <span className="text-right text-xs text-[#9B9B9B]" style={MONO_STYLE}>
        \u2014
      </span>
      {/* No 24h for group level */}
      <span className="text-right text-xs text-[#9B9B9B]" style={MONO_STYLE}>
        \u2014
      </span>
      <span className="text-right text-sm text-[#6B6B6B]" style={MONO_STYLE}>
        {formatSpreadBps(group.averageSpread.toString())}
      </span>
      <span className="text-right text-sm text-[#6B6B6B]" style={MONO_STYLE}>
        {formatUsd(group.aggregateVolume.toString())}
      </span>
      <div className="text-right">
        {group.totalOpps > 0 ? (
          <Badge className="bg-[#DAE9E0] text-[#2D6A4F] text-[10px]">
            {group.totalOpps}
          </Badge>
        ) : (
          <span className="text-sm text-[#D5D3CE]" style={MONO_STYLE}>0</span>
        )}
      </div>
    </div>
  );
}

function ChildRow({
  market,
  oppCount,
  onClick,
}: {
  market: MarketState;
  oppCount: number;
  onClick: () => void;
}) {
  const priceChange = formatPriceChange(market.one_day_price_change);
  const primaryPrice = market.outcome_prices[0] ?? null;

  return (
    <div
      onClick={onClick}
      className="grid cursor-pointer items-center border-b border-[#E6E4DF]/60 py-2.5 pr-4 transition-colors hover:bg-[#F8F7F4]/50"
      style={{
        gridTemplateColumns: "minmax(0, 1fr) 100px 100px 90px 100px 80px",
        paddingLeft: "calc(1rem + 24px)", // px-4 + indent
      }}
    >
      <div className="min-w-0">
        <p className="truncate text-[13px] text-[#6B6B6B]">{market.question}</p>
      </div>
      <span className="text-right text-[13px] text-[#1A1A19]" style={MONO_STYLE}>
        {formatCents(primaryPrice)}
      </span>
      <span
        className={cn(
          "text-right text-[13px]",
          priceChange.positive === null
            ? "text-[#9B9B9B]"
            : priceChange.positive
              ? "text-[#2D6A4F]"
              : "text-[#B44C3F]"
        )}
        style={MONO_STYLE}
      >
        {priceChange.text}
      </span>
      <span className="text-right text-[13px] text-[#9B9B9B]" style={MONO_STYLE}>
        {formatSpreadBps(market.spread)}
      </span>
      <span className="text-right text-[13px] text-[#9B9B9B]" style={MONO_STYLE}>
        {formatUsd(market.volume_24hr)}
      </span>
      <div className="text-right">
        {oppCount > 0 ? (
          <Badge className="bg-[#DAE9E0] text-[#2D6A4F] text-[10px]">
            {oppCount}
          </Badge>
        ) : (
          <span className="text-[13px] text-[#D5D3CE]" style={MONO_STYLE}>0</span>
        )}
      </div>
    </div>
  );
}
