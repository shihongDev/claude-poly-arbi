"use client";

import { useState, useCallback } from "react";
import { useParams, useRouter } from "next/navigation";
import { ArrowLeft, Copy, Check } from "lucide-react";
import { useDashboardStore } from "@/store";
import { OrderbookDepth } from "@/components/orderbook-depth";
import { Badge } from "@/components/ui/badge";
import { Button } from "@/components/ui/button";
import { Tabs, TabsList, TabsTrigger, TabsContent } from "@/components/ui/tabs";
import { formatUsd, cn } from "@/lib/utils";
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
      className="text-zinc-500 hover:text-zinc-300"
      title="Copy to clipboard"
    >
      {copied ? (
        <Check className="h-3 w-3 text-emerald-500" />
      ) : (
        <Copy className="h-3 w-3" />
      )}
    </Button>
  );
}

function InfoItem({ label, children }: { label: string; children: React.ReactNode }) {
  return (
    <div className="rounded-lg border border-zinc-800 bg-zinc-950 px-4 py-3">
      <p className="text-xs font-medium uppercase tracking-wider text-zinc-500">
        {label}
      </p>
      <div className="mt-1">{children}</div>
    </div>
  );
}

function PriceBar({ name, price }: { name: string; price: number }) {
  const widthPct = Math.max(price * 100, 1);
  return (
    <div className="space-y-1">
      <div className="flex items-center justify-between text-sm">
        <span className="text-zinc-300">{name}</span>
        <span
          className="text-zinc-200"
          style={{ fontFamily: "var(--font-mono)" }}
        >
          {price.toFixed(4)}
        </span>
      </div>
      <div className="h-2 w-full overflow-hidden rounded-full bg-zinc-800">
        <div
          className="h-full rounded-full bg-emerald-500 transition-all"
          style={{ width: `${widthPct}%` }}
        />
      </div>
    </div>
  );
}

function MarketNotFound() {
  const router = useRouter();
  return (
    <div className="space-y-6">
      <button
        onClick={() => router.push("/markets")}
        className="flex items-center gap-2 text-sm text-zinc-400 transition-colors hover:text-white"
      >
        <ArrowLeft className="h-4 w-4" />
        Back to Markets
      </button>
      <div className="flex h-[400px] items-center justify-center rounded-lg border border-zinc-800 bg-zinc-900">
        <p className="text-sm text-zinc-600">Market not found</p>
      </div>
    </div>
  );
}

function MarketDetail({ market }: { market: MarketState }) {
  const router = useRouter();
  const hasMultipleTokens = market.token_ids.length > 1;
  const defaultToken = market.token_ids[0] ?? "";

  return (
    <div className="space-y-6">
      {/* Back link */}
      <button
        onClick={() => router.push("/markets")}
        className="flex items-center gap-2 text-sm text-zinc-400 transition-colors hover:text-white"
      >
        <ArrowLeft className="h-4 w-4" />
        Back to Markets
      </button>

      {/* Title */}
      <div>
        <h1 className="text-2xl font-bold text-white">{market.question}</h1>
      </div>

      {/* Info grid */}
      <div className="grid gap-3 sm:grid-cols-2 lg:grid-cols-5">
        <InfoItem label="Condition ID">
          <div className="flex items-center gap-1.5">
            <span
              className="text-sm text-zinc-300"
              style={{ fontFamily: "var(--font-mono)" }}
              title={market.condition_id}
            >
              {truncateId(market.condition_id)}
            </span>
            <CopyButton text={market.condition_id} />
          </div>
        </InfoItem>

        <InfoItem label="Status">
          {market.active ? (
            <Badge className="bg-emerald-500/10 text-emerald-500 text-xs">
              Active
            </Badge>
          ) : (
            <Badge className="bg-zinc-500/10 text-zinc-500 text-xs">
              Inactive
            </Badge>
          )}
        </InfoItem>

        <InfoItem label="Neg Risk">
          <span className="text-sm text-zinc-300">
            {market.neg_risk ? "Yes" : "No"}
          </span>
        </InfoItem>

        <InfoItem label="Volume 24h">
          <span
            className="text-sm text-zinc-200"
            style={{ fontFamily: "var(--font-mono)" }}
          >
            {formatUsd(market.volume_24hr)}
          </span>
        </InfoItem>

        <InfoItem label="Liquidity">
          <span
            className="text-sm text-zinc-200"
            style={{ fontFamily: "var(--font-mono)" }}
          >
            {formatUsd(market.liquidity)}
          </span>
        </InfoItem>
      </div>

      {/* Outcomes section */}
      <div className="rounded-lg border border-zinc-800 bg-zinc-900 p-5">
        <h2 className="text-sm font-medium uppercase tracking-wider text-zinc-400">
          Outcomes
        </h2>
        <div className="mt-4 space-y-3">
          {market.outcomes.map((name, i) => {
            const price = market.outcome_prices[i]
              ? parseFloat(market.outcome_prices[i])
              : 0;
            return <PriceBar key={name} name={name} price={price} />;
          })}
        </div>
      </div>

      {/* Orderbook section */}
      <div className="rounded-lg border border-zinc-800 bg-zinc-900 p-5">
        <h2 className="mb-4 text-sm font-medium uppercase tracking-wider text-zinc-400">
          Orderbook Depth
        </h2>
        {market.orderbooks.length === 0 ? (
          <div className="flex h-[300px] items-center justify-center text-sm text-zinc-600">
            No orderbook data available
          </div>
        ) : hasMultipleTokens ? (
          <Tabs defaultValue={defaultToken}>
            <TabsList className="border-zinc-800 bg-zinc-800/50">
              {market.token_ids.map((tokenId, i) => (
                <TabsTrigger
                  key={tokenId}
                  value={tokenId}
                  className="text-xs data-[state=active]:bg-zinc-700 data-[state=active]:text-white"
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
                  <div className="h-[300px]">
                    {ob ? (
                      <OrderbookDepth bids={ob.bids} asks={ob.asks} />
                    ) : (
                      <div className="flex h-full items-center justify-center text-sm text-zinc-600">
                        No orderbook for this token
                      </div>
                    )}
                  </div>
                </TabsContent>
              );
            })}
          </Tabs>
        ) : (
          <div className="h-[300px]">
            {market.orderbooks[0] ? (
              <OrderbookDepth
                bids={market.orderbooks[0].bids}
                asks={market.orderbooks[0].asks}
              />
            ) : (
              <div className="flex h-full items-center justify-center text-sm text-zinc-600">
                No orderbook data available
              </div>
            )}
          </div>
        )}
      </div>

      {/* Token IDs */}
      <div className="rounded-lg border border-zinc-800 bg-zinc-900 p-5">
        <h2 className="text-sm font-medium uppercase tracking-wider text-zinc-400">
          Token IDs
        </h2>
        <div className="mt-3 space-y-2">
          {market.token_ids.map((tokenId, i) => (
            <div
              key={tokenId}
              className="flex items-center justify-between rounded border border-zinc-800 bg-zinc-950 px-3 py-2"
            >
              <div className="flex items-center gap-3">
                <span className="text-xs text-zinc-500">
                  {market.outcomes[i] ?? `Token ${i}`}
                </span>
                <span
                  className="text-xs text-zinc-400 break-all"
                  style={{ fontFamily: "var(--font-mono)" }}
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
