"use client";

import { useState, useMemo, useCallback } from "react";
import { useParams, useRouter } from "next/navigation";
import {
  ArrowLeft,
  Copy,
  Check,
  ExternalLink,
  Calendar,
  AlertTriangle,
  ArrowUpRight,
  ArrowDownRight,
} from "lucide-react";
import { useDashboardStore } from "@/store";
import { OrderbookDepth } from "@/components/orderbook-depth";
import { OrderbookLadder } from "@/components/orderbook-ladder";
import { OrderForm } from "@/components/order-form";
import { OpportunityRow } from "@/components/opportunity-row";
import { Badge } from "@/components/ui/badge";
import { Button } from "@/components/ui/button";
import { Tabs, TabsList, TabsTrigger, TabsContent } from "@/components/ui/tabs";
import {
  formatUsd,
  formatSpreadBps,
  formatPriceChange,
  formatEndDate,
  probSumDeviation,
  cn,
} from "@/lib/utils";
import type { MarketState } from "@/lib/types";

function truncateId(id: string, chars = 10): string {
  if (id.length <= chars * 2 + 3) return id;
  return `${id.slice(0, chars)}...${id.slice(-chars)}`;
}

function CopyButton({ text }: { text: string }) {
  const [copied, setCopied] = useState(false);

  const handleCopy = useCallback(async () => {
    try {
      await navigator.clipboard.writeText(text);
      setCopied(true);
      setTimeout(() => setCopied(false), 2000);
    } catch {
      // Clipboard API not available
    }
  }, [text]);

  return (
    <Button
      variant="ghost"
      size="icon-xs"
      onClick={handleCopy}
      className="text-[#9B9B9B] hover:text-[#1A1A19]"
      title="Copy to clipboard"
    >
      {copied ? (
        <Check className="h-3 w-3 text-[#2D6A4F]" />
      ) : (
        <Copy className="h-3 w-3" />
      )}
    </Button>
  );
}

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

function PriceBar({
  name,
  price,
  bidAsk,
}: {
  name: string;
  price: number;
  bidAsk?: { bid: number | null; ask: number | null };
}) {
  const widthPct = Math.max(price * 100, 1);
  return (
    <div className="space-y-1">
      <div className="flex items-center justify-between text-sm">
        <span className="text-[#1A1A19]">{name}</span>
        <span
          className="text-[#1A1A19]"
          style={{ fontFamily: "var(--font-jetbrains-mono)" }}
        >
          {price.toFixed(4)}
        </span>
      </div>
      <div className="h-2 w-full overflow-hidden rounded-full bg-[#F0EEEA]">
        <div
          className="h-full rounded-full bg-[#2D6A4F] transition-all"
          style={{ width: `${widthPct}%` }}
        />
      </div>
      {bidAsk && (bidAsk.bid !== null || bidAsk.ask !== null) && (
        <div
          className="flex gap-4 text-[10px] text-[#9B9B9B]"
          style={{ fontFamily: "var(--font-jetbrains-mono)" }}
        >
          <span>
            Bid:{" "}
            <span className="text-[#2D6A4F]">
              {bidAsk.bid !== null ? bidAsk.bid.toFixed(4) : "\u2014"}
            </span>
          </span>
          <span>
            Ask:{" "}
            <span className="text-[#B44C3F]">
              {bidAsk.ask !== null ? bidAsk.ask.toFixed(4) : "\u2014"}
            </span>
          </span>
        </div>
      )}
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

function MarketDetail({ market }: { market: MarketState }) {
  const router = useRouter();
  const opportunities = useDashboardStore((s) => s.opportunities);
  const positions = useDashboardStore((s) => s.positions);
  const hasMultipleTokens = market.token_ids.length > 1;
  const defaultToken = market.token_ids[0] ?? "";
  const [descExpanded, setDescExpanded] = useState(false);

  // Probability sum deviation
  const deviation = probSumDeviation(market.outcome_prices);
  const hasDeviation = deviation > 0.5; // > 0.5 percentage points

  // Per-token bid/ask from orderbooks
  const tokenBidAsk = useMemo(() => {
    const map = new Map<
      string,
      { bid: number | null; ask: number | null }
    >();
    for (const ob of market.orderbooks) {
      const bid = ob.bids[0] ? parseFloat(ob.bids[0].price) : null;
      const ask = ob.asks[0] ? parseFloat(ob.asks[0].price) : null;
      map.set(ob.token_id, { bid, ask });
    }
    return map;
  }, [market.orderbooks]);

  // Related opportunities
  const relatedOpps = useMemo(
    () =>
      opportunities.filter((o) =>
        o.markets.includes(market.condition_id)
      ),
    [opportunities, market.condition_id]
  );

  // Market positions
  const marketPositions = useMemo(
    () => positions.filter((p) => p.condition_id === market.condition_id),
    [positions, market.condition_id]
  );

  // 24h change display
  const priceChange = formatPriceChange(market.one_day_price_change);

  return (
    <div className="space-y-6">
      {/* Back link */}
      <button
        onClick={() => router.push("/")}
        className="flex items-center gap-2 text-sm text-[#6B6B6B] transition-colors hover:text-[#1A1A19]"
      >
        <ArrowLeft className="h-4 w-4" />
        Back to Markets
      </button>

      {/* Header section */}
      <div className="space-y-3">
        <h1 className="text-2xl font-bold text-[#1A1A19]">
          {market.question}
        </h1>

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

        {/* Badge row */}
        <div className="flex flex-wrap items-center gap-2">
          {market.neg_risk && (
            <Badge className="bg-[#F5E6D3] text-[#8B6914] text-[10px]">
              Neg Risk
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
          {market.active ? (
            <Badge className="bg-[#DAE9E0] text-[#2D6A4F] text-[10px]">
              Active
            </Badge>
          ) : (
            <Badge className="bg-[#F0EEEA] text-[#6B6B6B] text-[10px]">
              Inactive
            </Badge>
          )}
        </div>
      </div>

      {/* Trading stats grid */}
      <div className="grid gap-3 sm:grid-cols-2 lg:grid-cols-4 xl:grid-cols-7">
        <InfoItem label="Best Bid">
          <span
            className="text-sm font-medium text-[#2D6A4F]"
            style={{ fontFamily: "var(--font-jetbrains-mono)" }}
          >
            {market.best_bid
              ? parseFloat(market.best_bid).toFixed(4)
              : "\u2014"}
          </span>
        </InfoItem>

        <InfoItem label="Best Ask">
          <span
            className="text-sm font-medium text-[#B44C3F]"
            style={{ fontFamily: "var(--font-jetbrains-mono)" }}
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
            style={{ fontFamily: "var(--font-jetbrains-mono)" }}
          >
            {formatSpreadBps(market.spread)}
          </span>
        </InfoItem>

        <InfoItem label="Last Trade">
          <span
            className="text-sm text-[#1A1A19]"
            style={{ fontFamily: "var(--font-jetbrains-mono)" }}
          >
            {market.last_trade_price
              ? parseFloat(market.last_trade_price).toFixed(4)
              : "\u2014"}
          </span>
        </InfoItem>

        <InfoItem label="Volume 24h">
          <span
            className="text-sm text-[#1A1A19]"
            style={{ fontFamily: "var(--font-jetbrains-mono)" }}
          >
            {formatUsd(market.volume_24hr)}
          </span>
        </InfoItem>

        <InfoItem label="Liquidity">
          <span
            className="text-sm text-[#1A1A19]"
            style={{ fontFamily: "var(--font-jetbrains-mono)" }}
          >
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
              style={{ fontFamily: "var(--font-jetbrains-mono)" }}
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

      {/* Secondary info row */}
      <div className="grid gap-3 sm:grid-cols-3">
        <InfoItem label="Condition ID">
          <div className="flex items-center gap-1.5">
            <span
              className="text-sm text-[#1A1A19]"
              style={{ fontFamily: "var(--font-jetbrains-mono)" }}
              title={market.condition_id}
            >
              {truncateId(market.condition_id)}
            </span>
            <CopyButton text={market.condition_id} />
          </div>
        </InfoItem>

        <InfoItem label="Status">
          {market.active ? (
            <Badge className="bg-[#DAE9E0] text-[#2D6A4F] text-xs">
              Active
            </Badge>
          ) : (
            <Badge className="bg-[#F0EEEA] text-[#6B6B6B] text-xs">
              Inactive
            </Badge>
          )}
        </InfoItem>

        <InfoItem label="Neg Risk">
          <span className="text-sm text-[#1A1A19]">
            {market.neg_risk ? "Yes" : "No"}
          </span>
        </InfoItem>
      </div>

      {/* Outcomes section */}
      <div className="rounded-2xl bg-white p-5">
        <div className="flex items-center gap-3">
          <h2 className="text-[11px] font-medium uppercase tracking-wider text-[#9B9B9B]">
            Outcomes
          </h2>
          {hasDeviation && (
            <Badge className="bg-[#FEF3CD] text-[#856404] text-[10px] inline-flex items-center gap-1">
              <AlertTriangle className="h-2.5 w-2.5" />
              Sum deviates by {deviation.toFixed(1)}pp
            </Badge>
          )}
        </div>
        <div className="mt-4 space-y-3">
          {market.outcomes.map((name, i) => {
            const price = market.outcome_prices[i]
              ? parseFloat(market.outcome_prices[i])
              : 0;
            const tokenId = market.token_ids[i];
            const ba = tokenId ? tokenBidAsk.get(tokenId) : undefined;
            return (
              <PriceBar key={name} name={name} price={price} bidAsk={ba} />
            );
          })}
        </div>
      </div>

      {/* Place Order */}
      <OrderForm market={market} />

      {/* Orderbook section — depth chart + price ladder side by side */}
      <div className="rounded-2xl bg-white p-5">
        <h2 className="mb-4 text-[11px] font-medium uppercase tracking-wider text-[#9B9B9B]">
          Orderbook
        </h2>
        {market.orderbooks.length === 0 ? (
          <div className="flex h-[300px] items-center justify-center text-sm text-[#9B9B9B]">
            No orderbook data available
          </div>
        ) : hasMultipleTokens ? (
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
              const ob = market.orderbooks.find(
                (o) => o.token_id === tokenId
              );
              return (
                <TabsContent key={tokenId} value={tokenId}>
                  {ob ? (
                    <div className="grid gap-4 lg:grid-cols-2">
                      <div className="h-[300px]">
                        <OrderbookDepth bids={ob.bids} asks={ob.asks} />
                      </div>
                      <div className="h-[300px] rounded-[10px] border border-[#E6E4DF] overflow-hidden">
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
                <div className="h-[300px] rounded-[10px] border border-[#E6E4DF] overflow-hidden">
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

      {/* Related Opportunities */}
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

      {/* Your Positions */}
      {marketPositions.length > 0 && (
        <div className="rounded-2xl bg-white p-5">
          <h2 className="text-[11px] font-medium uppercase tracking-wider text-[#9B9B9B]">
            Your Positions ({marketPositions.length})
          </h2>
          <div className="mt-4 grid gap-2 sm:grid-cols-2">
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
                      style={{ fontFamily: "var(--font-jetbrains-mono)" }}
                    >
                      {pnl >= 0 ? "+" : ""}
                      {formatUsd(pos.unrealized_pnl)}
                    </span>
                  </div>
                  <div
                    className="mt-2 flex gap-4 text-xs text-[#6B6B6B]"
                    style={{ fontFamily: "var(--font-jetbrains-mono)" }}
                  >
                    <span>
                      Size:{" "}
                      <span className="text-[#1A1A19]">
                        {size.toFixed(2)}
                      </span>
                    </span>
                    <span>
                      Entry:{" "}
                      <span className="text-[#1A1A19]">
                        {entry.toFixed(4)}
                      </span>
                    </span>
                    <span>
                      Current:{" "}
                      <span className="text-[#1A1A19]">
                        {current.toFixed(4)}
                      </span>
                    </span>
                  </div>
                </div>
              );
            })}
          </div>
        </div>
      )}

      {/* Token IDs */}
      <div className="rounded-2xl bg-white p-5">
        <h2 className="text-[11px] font-medium uppercase tracking-wider text-[#9B9B9B]">
          Token IDs
        </h2>
        <div className="mt-3 space-y-2">
          {market.token_ids.map((tokenId, i) => (
            <div
              key={tokenId}
              className="flex items-center justify-between rounded-[10px] border border-[#E6E4DF] bg-[#F8F7F4] px-3 py-2"
            >
              <div className="flex items-center gap-3">
                <span className="text-xs text-[#9B9B9B]">
                  {market.outcomes[i] ?? `Token ${i}`}
                </span>
                <span
                  className="text-xs text-[#6B6B6B] break-all"
                  style={{ fontFamily: "var(--font-jetbrains-mono)" }}
                >
                  {tokenId}
                </span>
              </div>
              <CopyButton text={tokenId} />
            </div>
          ))}
        </div>
      </div>
    </div>
  );
}

export default function MarketDetailPage() {
  const params = useParams<{ id: string }>();
  const markets = useDashboardStore((s) => s.markets);

  const market = markets.find((m) => m.condition_id === params.id);

  if (!market) {
    return <MarketNotFound />;
  }

  return <MarketDetail market={market} />;
}
