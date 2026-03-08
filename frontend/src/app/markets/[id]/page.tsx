"use client";

import { useMemo } from "react";
import { useParams, useRouter } from "next/navigation";
import {
  ArrowLeft,
  ArrowUpRight,
  ArrowDownRight,
  Calendar,
  ExternalLink,
} from "lucide-react";
import { useDashboardStore } from "@/store";
import { OrderbookDepth } from "@/components/orderbook-depth";
import { OrderbookLadder } from "@/components/orderbook-ladder";
import { OrderForm } from "@/components/order-form";
import { OpportunityRow } from "@/components/opportunity-row";
import { OutcomeProbabilityPanel } from "@/components/outcome-probability-panel";
import { MarketMicrostructurePanel } from "@/components/market-microstructure-panel";
import { LiquidityDepthProfile } from "@/components/liquidity-depth-profile";
import { SimulationQuickView } from "@/components/simulation-quick-view";
import { OrderImbalanceChart } from "@/components/order-imbalance-chart";
import { MarketMetadataCard } from "@/components/market-metadata-card";
import { Badge } from "@/components/ui/badge";
import { Tabs, TabsList, TabsTrigger, TabsContent } from "@/components/ui/tabs";
import {
  formatUsd,
  formatSpreadBps,
  formatPriceChange,
  formatEndDate,
  cn,
  MONO_STYLE,
} from "@/lib/utils";
import type { MarketState } from "@/lib/types";
import { useState } from "react";

/* ------------------------------------------------------------------ */
/*  Helpers                                                            */
/* ------------------------------------------------------------------ */

function InfoItem({
  label,
  children,
  className,
}: {
  label: string;
  children: React.ReactNode;
  className?: string;
}) {
  return (
    <div
      className={cn(
        "rounded-[10px] border border-[#E6E4DF] bg-[#F8F7F4] px-4 py-3",
        className
      )}
    >
      <p className="text-[11px] font-medium uppercase tracking-wider text-[#9B9B9B]">
        {label}
      </p>
      <div className="mt-1">{children}</div>
    </div>
  );
}

function MarketNotFound() {
  const router = useRouter();
  return (
    <div className="space-y-6">
      <button
        onClick={() => router.push("/")}
        className="flex items-center gap-2 text-sm text-[#6B6B6B] transition-colors hover:text-[#1A1A19]"
      >
        <ArrowLeft className="h-4 w-4" />
        Back to Markets
      </button>
      <div className="flex h-[400px] items-center justify-center rounded-2xl bg-white">
        <p className="text-sm text-[#9B9B9B]">Market not found</p>
      </div>
    </div>
  );
}

/* ------------------------------------------------------------------ */
/*  Positions section (extracted for the right column)                 */
/* ------------------------------------------------------------------ */

function PositionsSection({ market }: { market: MarketState }) {
  const positions = useDashboardStore((s) => s.positions);
  const marketPositions = useMemo(
    () => positions.filter((p) => p.condition_id === market.condition_id),
    [positions, market.condition_id]
  );

  if (marketPositions.length === 0) return null;

  return (
    <div className="rounded-2xl bg-white p-5">
      <h2 className="text-[11px] font-medium uppercase tracking-wider text-[#9B9B9B]">
        Your Positions ({marketPositions.length})
      </h2>
      <div className="mt-4 space-y-2">
        {marketPositions.map((pos) => {
          const outcomeName =
            market.outcomes[market.token_ids.indexOf(pos.token_id)] ??
            "Unknown";
          const size = parseFloat(pos.size);
          const entry = parseFloat(pos.avg_entry_price);
          const current = parseFloat(pos.current_price);
          const pnl = parseFloat(pos.unrealized_pnl);
          return (
            <div
              key={pos.token_id}
              className="rounded-[10px] border border-[#E6E4DF] bg-[#F8F7F4] px-4 py-3"
            >
              <div className="flex items-center justify-between">
                <span className="text-sm font-medium text-[#1A1A19]">
                  {outcomeName}
                </span>
                <span
                  className={cn(
                    "text-sm font-bold",
                    pnl >= 0 ? "text-[#2D6A4F]" : "text-[#B44C3F]"
                  )}
                  style={MONO_STYLE}
                >
                  {pnl >= 0 ? "+" : ""}
                  {formatUsd(pos.unrealized_pnl)}
                </span>
              </div>
              <div
                className="mt-2 flex gap-4 text-xs text-[#6B6B6B]"
                style={MONO_STYLE}
              >
                <span>
                  Size: <span className="text-[#1A1A19]">{size.toFixed(2)}</span>
                </span>
                <span>
                  Entry: <span className="text-[#1A1A19]">{entry.toFixed(4)}</span>
                </span>
                <span>
                  Current: <span className="text-[#1A1A19]">{current.toFixed(4)}</span>
                </span>
              </div>
            </div>
          );
        })}
      </div>
    </div>
  );
}

/* ------------------------------------------------------------------ */
/*  Orderbook section (tabbed depth + ladder)                          */
/* ------------------------------------------------------------------ */

function OrderbookSection({ market }: { market: MarketState }) {
  const hasMultipleTokens = market.token_ids.length > 1;
  const defaultToken = market.token_ids[0] ?? "";

  if (market.orderbooks.length === 0) {
    return (
      <div className="rounded-2xl bg-white p-5">
        <h2 className="mb-4 text-[11px] font-medium uppercase tracking-wider text-[#9B9B9B]">
          Orderbook
        </h2>
        <div className="flex h-[300px] items-center justify-center text-sm text-[#9B9B9B]">
          No orderbook data available
        </div>
      </div>
    );
  }

  return (
    <div className="rounded-2xl bg-white p-5">
      <h2 className="mb-4 text-[11px] font-medium uppercase tracking-wider text-[#9B9B9B]">
        Orderbook
      </h2>
      {hasMultipleTokens ? (
        <Tabs defaultValue={defaultToken}>
          <TabsList className="border-[#E6E4DF] bg-[#F0EEEA]">
            {market.token_ids.map((tokenId, i) => (
              <TabsTrigger
                key={tokenId}
                value={tokenId}
                className="text-xs data-[state=active]:bg-white data-[state=active]:text-[#1A1A19]"
              >
                {market.outcomes[i] ?? `Token ${i}`}
              </TabsTrigger>
            ))}
          </TabsList>
          {market.token_ids.map((tokenId) => {
            const ob = market.orderbooks.find((o) => o.token_id === tokenId);
            return (
              <TabsContent key={tokenId} value={tokenId}>
                {ob ? (
                  <div className="grid gap-4 lg:grid-cols-2">
                    <div className="h-[300px]">
                      <OrderbookDepth bids={ob.bids} asks={ob.asks} />
                    </div>
                    <div className="h-[300px] overflow-hidden rounded-[10px] border border-[#E6E4DF]">
                      <OrderbookLadder bids={ob.bids} asks={ob.asks} />
                    </div>
                  </div>
                ) : (
                  <div className="flex h-[300px] items-center justify-center text-sm text-[#9B9B9B]">
                    No orderbook for this token
                  </div>
                )}
              </TabsContent>
            );
          })}
        </Tabs>
      ) : (
        <>
          {market.orderbooks[0] ? (
            <div className="grid gap-4 lg:grid-cols-2">
              <div className="h-[300px]">
                <OrderbookDepth
                  bids={market.orderbooks[0].bids}
                  asks={market.orderbooks[0].asks}
                />
              </div>
              <div className="h-[300px] overflow-hidden rounded-[10px] border border-[#E6E4DF]">
                <OrderbookLadder
                  bids={market.orderbooks[0].bids}
                  asks={market.orderbooks[0].asks}
                />
              </div>
            </div>
          ) : (
            <div className="flex h-[300px] items-center justify-center text-sm text-[#9B9B9B]">
              No orderbook data available
            </div>
          )}
        </>
      )}
    </div>
  );
}

/* ------------------------------------------------------------------ */
/*  Related opportunities section                                      */
/* ------------------------------------------------------------------ */

function RelatedOpportunities({ market }: { market: MarketState }) {
  const opportunities = useDashboardStore((s) => s.opportunities);
  const relatedOpps = useMemo(
    () => opportunities.filter((o) => o.markets.includes(market.condition_id)),
    [opportunities, market.condition_id]
  );

  return (
    <div className="rounded-2xl bg-white p-5">
      <h2 className="text-[11px] font-medium uppercase tracking-wider text-[#9B9B9B]">
        Active Opportunities
        {relatedOpps.length > 0 && (
          <span className="ml-2 text-[#2D6A4F]">({relatedOpps.length})</span>
        )}
      </h2>
      {relatedOpps.length > 0 ? (
        <div className="mt-4 overflow-x-auto">
          <table className="w-full">
            <thead>
              <tr className="text-left text-[11px] font-medium uppercase tracking-wider text-[#9B9B9B]">
                <th className="px-3 py-2">Type</th>
                <th className="px-3 py-2">Markets</th>
                <th className="px-3 py-2">Net Edge</th>
                <th className="px-3 py-2">Confidence</th>
                <th className="px-3 py-2">Size</th>
                <th className="px-3 py-2">Detected</th>
              </tr>
            </thead>
            <tbody>
              {relatedOpps.map((opp) => (
                <OpportunityRow key={opp.id} opportunity={opp} />
              ))}
            </tbody>
          </table>
        </div>
      ) : (
        <p className="mt-3 text-sm text-[#9B9B9B]">
          No active arbitrage opportunities for this market.
        </p>
      )}
    </div>
  );
}

/* ------------------------------------------------------------------ */
/*  Main market detail component                                       */
/* ------------------------------------------------------------------ */

function MarketDetail({ market }: { market: MarketState }) {
  const router = useRouter();
  const [descExpanded, setDescExpanded] = useState(false);

  // 24h change display
  const priceChange = formatPriceChange(market.one_day_price_change);

  // Primary outcome price for simulation
  const primaryPrice = parseFloat(market.outcome_prices[0] || "0");

  return (
    <div className="space-y-6">
      {/* ── Back link ───────────────────────────────────────────── */}
      <button
        onClick={() => router.push("/")}
        className="flex items-center gap-2 text-sm text-[#6B6B6B] transition-colors hover:text-[#1A1A19]"
      >
        <ArrowLeft className="h-4 w-4" />
        Back to Markets
      </button>

      {/* ── Header section (full width) ─────────────────────────── */}
      <div className="space-y-3">
        <div className="flex flex-wrap items-start justify-between gap-3">
          <h1 className="text-2xl font-bold text-[#1A1A19]">
            {market.question}
          </h1>
          <div className="flex flex-wrap items-center gap-2">
            {market.neg_risk && (
              <Badge className="bg-[#F5E6D3] text-[#8B6914] text-[10px]">
                Neg Risk
              </Badge>
            )}
            {market.active ? (
              <Badge className="bg-[#DAE9E0] text-[#2D6A4F] text-[10px]">
                Active
              </Badge>
            ) : (
              <Badge className="bg-[#F0EEEA] text-[#6B6B6B] text-[10px]">
                Inactive
              </Badge>
            )}
            {market.end_date_iso && (
              <Badge className="bg-[#F0EEEA] text-[#6B6B6B] text-[10px] inline-flex items-center gap-1">
                <Calendar className="h-2.5 w-2.5" />
                {formatEndDate(market.end_date_iso)}
              </Badge>
            )}
            {market.slug && (
              <a
                href={`https://polymarket.com/event/${market.slug}`}
                target="_blank"
                rel="noopener noreferrer"
                className="inline-flex items-center gap-1 rounded-[9999px] bg-[#F0EEEA] px-2.5 py-0.5 text-[10px] font-medium text-[#6B6B6B] transition-colors hover:bg-[#E6E4DF] hover:text-[#1A1A19]"
              >
                Polymarket
                <ExternalLink className="h-2.5 w-2.5" />
              </a>
            )}
          </div>
        </div>

        {/* Description */}
        {market.description && (
          <div className="relative">
            <p
              className={cn(
                "text-sm leading-relaxed text-[#6B6B6B]",
                !descExpanded && "line-clamp-3"
              )}
            >
              {market.description}
            </p>
            {market.description.length > 200 && (
              <button
                onClick={() => setDescExpanded(!descExpanded)}
                className="mt-1 text-xs text-[#2D6A4F] hover:underline"
              >
                {descExpanded ? "Show less" : "Show more"}
              </button>
            )}
          </div>
        )}
      </div>

      {/* ── Trading stats row (full width, 7 cols) ──────────────── */}
      <div className="grid gap-3 sm:grid-cols-2 lg:grid-cols-4 xl:grid-cols-7">
        <InfoItem label="Best Bid">
          <span
            className="text-sm font-medium text-[#2D6A4F]"
            style={MONO_STYLE}
          >
            {market.best_bid
              ? parseFloat(market.best_bid).toFixed(4)
              : "\u2014"}
          </span>
        </InfoItem>

        <InfoItem label="Best Ask">
          <span
            className="text-sm font-medium text-[#B44C3F]"
            style={MONO_STYLE}
          >
            {market.best_ask
              ? parseFloat(market.best_ask).toFixed(4)
              : "\u2014"}
          </span>
        </InfoItem>

        <InfoItem label="Spread">
          <span
            className={cn(
              "text-sm font-medium",
              market.spread
                ? parseFloat(market.spread) * 10000 < 30
                  ? "text-[#2D6A4F]"
                  : parseFloat(market.spread) * 10000 < 100
                    ? "text-[#B8860B]"
                    : "text-[#B44C3F]"
                : "text-[#9B9B9B]"
            )}
            style={MONO_STYLE}
          >
            {formatSpreadBps(market.spread)}
          </span>
        </InfoItem>

        <InfoItem label="Last Trade">
          <span className="text-sm text-[#1A1A19]" style={MONO_STYLE}>
            {market.last_trade_price
              ? parseFloat(market.last_trade_price).toFixed(4)
              : "\u2014"}
          </span>
        </InfoItem>

        <InfoItem label="Volume 24h">
          <span className="text-sm text-[#1A1A19]" style={MONO_STYLE}>
            {formatUsd(market.volume_24hr)}
          </span>
        </InfoItem>

        <InfoItem label="Liquidity">
          <span className="text-sm text-[#1A1A19]" style={MONO_STYLE}>
            {formatUsd(market.liquidity)}
          </span>
        </InfoItem>

        <InfoItem label="24h Change">
          {priceChange.positive === null ? (
            <span className="text-sm text-[#9B9B9B]">{priceChange.text}</span>
          ) : (
            <span
              className={cn(
                "inline-flex items-center gap-0.5 text-sm font-medium",
                priceChange.positive ? "text-[#2D6A4F]" : "text-[#B44C3F]"
              )}
              style={MONO_STYLE}
            >
              {priceChange.positive ? (
                <ArrowUpRight className="h-3.5 w-3.5" />
              ) : (
                <ArrowDownRight className="h-3.5 w-3.5" />
              )}
              {priceChange.text}
            </span>
          )}
        </InfoItem>
      </div>

      {/* ── Two-column grid ─────────────────────────────────────── */}
      <div className="grid gap-6 lg:grid-cols-5">
        {/* LEFT COLUMN (3/5): Market Intelligence */}
        <div className="space-y-6 lg:col-span-3">
          {/* A. Outcome Probability Panel */}
          <OutcomeProbabilityPanel market={market} />

          {/* B. Market Microstructure */}
          <MarketMicrostructurePanel market={market} />

          {/* C. Orderbook (depth + ladder, tabbed) */}
          <OrderbookSection market={market} />

          {/* D. Liquidity Depth Profile */}
          <LiquidityDepthProfile market={market} />

          {/* E. Simulation Quick View */}
          <SimulationQuickView
            conditionId={market.condition_id}
            marketPrice={primaryPrice}
          />

          {/* F. Related Opportunities */}
          <RelatedOpportunities market={market} />
        </div>

        {/* RIGHT COLUMN (2/5): Trading Actions */}
        <div className="space-y-6 lg:col-span-2">
          {/* F. Order Form (sticky) */}
          <div className="lg:sticky lg:top-6 lg:self-start space-y-6">
            <OrderForm market={market} />

            {/* G. Order Imbalance */}
            <OrderImbalanceChart market={market} />

            {/* H. Your Positions */}
            <PositionsSection market={market} />

            {/* I. Market Metadata */}
            <MarketMetadataCard market={market} />
          </div>
        </div>
      </div>
    </div>
  );
}

/* ------------------------------------------------------------------ */
/*  Page export                                                        */
/* ------------------------------------------------------------------ */

export default function MarketDetailPage() {
  const params = useParams<{ id: string }>();
  const markets = useDashboardStore((s) => s.markets);

  const market = markets.find((m) => m.condition_id === params.id);

  if (!market) {
    return <MarketNotFound />;
  }

  return <MarketDetail market={market} />;
}
