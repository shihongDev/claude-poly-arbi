"use client";

import { useMemo, useState } from "react";
import dynamic from "next/dynamic";
import { OUTCOME_COLORS, MONO_FONT } from "@/lib/utils";
import { Tabs, TabsList, TabsTrigger, TabsContent } from "@/components/ui/tabs";
import type { MarketState, OrderbookLevel } from "@/lib/types";

const ReactECharts = dynamic(() => import("echarts-for-react"), { ssr: false });

interface LiquidityDepthProfileProps {
  market: MarketState;
}

function cumulateDepth(
  levels: OrderbookLevel[],
  ascending: boolean
): { price: number; cumSize: number }[] {
  const sorted = [...levels].sort((a, b) =>
    ascending
      ? parseFloat(a.price) - parseFloat(b.price)
      : parseFloat(b.price) - parseFloat(a.price)
  );

  let cumSize = 0;
  const result = sorted.map((level) => {
    cumSize += parseFloat(level.size);
    return { price: parseFloat(level.price), cumSize };
  });

  if (!ascending) result.reverse();
  return result;
}

function buildMultiSeriesOption(
  market: MarketState,
  tokenFilter?: string
) {
  const series: Array<{
    name: string;
    type: "line";
    data: [number, number][];
    lineStyle: { color: string; width: number; type?: string };
    areaStyle?: { color: string };
    symbol: string;
    smooth: boolean;
  }> = [];

  const tokensToShow = tokenFilter
    ? [tokenFilter]
    : market.token_ids;

  for (const tokenId of tokensToShow) {
    const idx = market.token_ids.indexOf(tokenId);
    const ob = market.orderbooks.find((o) => o.token_id === tokenId);
    if (!ob) continue;

    const color = OUTCOME_COLORS[idx % OUTCOME_COLORS.length];
    const name = market.outcomes[idx] ?? `Token ${idx}`;

    const bidData = cumulateDepth(ob.bids, false);
    const askData = cumulateDepth(ob.asks, true);

    series.push({
      name: `${name} Bids`,
      type: "line" as const,
      data: bidData.map((d) => [d.price, d.cumSize]),
      lineStyle: { color, width: 1.5 },
      areaStyle: { color: `${color}14` },
      symbol: "none",
      smooth: true,
    });

    series.push({
      name: `${name} Asks`,
      type: "line" as const,
      data: askData.map((d) => [d.price, d.cumSize]),
      lineStyle: { color, width: 1.5, type: "dashed" },
      areaStyle: { color: `${color}0A` },
      symbol: "none",
      smooth: true,
    });
  }

  return {
    backgroundColor: "transparent",
    grid: { top: 16, right: 16, bottom: 40, left: 48 },
    xAxis: {
      type: "value" as const,
      axisLabel: {
        color: "#6B6B6B",
        fontSize: 10,
        fontFamily: MONO_FONT,
        formatter: (v: number) => v.toFixed(2),
      },
      axisLine: { lineStyle: { color: "#E6E4DF" } },
      splitLine: { show: false },
    },
    yAxis: {
      type: "value" as const,
      axisLabel: {
        color: "#6B6B6B",
        fontSize: 10,
        fontFamily: MONO_FONT,
      },
      axisLine: { lineStyle: { color: "#E6E4DF" } },
      splitLine: {
        lineStyle: { color: "#F0EEEA", type: "dashed" as const },
      },
    },
    tooltip: {
      trigger: "axis" as const,
      backgroundColor: "#FFFFFF",
      borderColor: "#E6E4DF",
      textStyle: { color: "#1A1A19", fontSize: 11, fontFamily: MONO_FONT },
    },
    legend: {
      bottom: 0,
      textStyle: { color: "#6B6B6B", fontSize: 10 },
      itemWidth: 12,
      itemHeight: 8,
    },
    series,
  };
}

export function LiquidityDepthProfile({
  market,
}: LiquidityDepthProfileProps) {
  const hasMultipleTokens = market.token_ids.length > 2;
  const [activeTab, setActiveTab] = useState("all");

  const allOption = useMemo(
    () => buildMultiSeriesOption(market),
    [market]
  );

  const perTokenOptions = useMemo(() => {
    const map = new Map<string, ReturnType<typeof buildMultiSeriesOption>>();
    for (const tokenId of market.token_ids) {
      map.set(tokenId, buildMultiSeriesOption(market, tokenId));
    }
    return map;
  }, [market]);

  if (market.orderbooks.length === 0) {
    return (
      <div className="rounded-2xl bg-white p-5">
        <h2 className="text-[11px] font-medium uppercase tracking-wider text-[#9B9B9B]">
          Liquidity Depth Profile
        </h2>
        <div className="flex h-[280px] items-center justify-center text-sm text-[#9B9B9B]">
          No orderbook data available
        </div>
      </div>
    );
  }

  return (
    <div className="rounded-2xl bg-white p-5">
      <h2 className="mb-3 text-[11px] font-medium uppercase tracking-wider text-[#9B9B9B]">
        Liquidity Depth Profile
      </h2>

      {hasMultipleTokens ? (
        <Tabs value={activeTab} onValueChange={setActiveTab}>
          <TabsList className="border-[#E6E4DF] bg-[#F0EEEA]">
            <TabsTrigger
              value="all"
              className="text-xs data-[state=active]:bg-white data-[state=active]:text-[#1A1A19]"
            >
              All
            </TabsTrigger>
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
          <TabsContent value="all">
            <div className="h-[280px]">
              <ReactECharts
                option={allOption}
                style={{ height: "100%", width: "100%" }}
                opts={{ renderer: "canvas" }}
              />
            </div>
          </TabsContent>
          {market.token_ids.map((tokenId) => {
            const opt = perTokenOptions.get(tokenId);
            return (
              <TabsContent key={tokenId} value={tokenId}>
                <div className="h-[280px]">
                  {opt && (
                    <ReactECharts
                      option={opt}
                      style={{ height: "100%", width: "100%" }}
                      opts={{ renderer: "canvas" }}
                    />
                  )}
                </div>
              </TabsContent>
            );
          })}
        </Tabs>
      ) : (
        <div className="h-[280px]">
          <ReactECharts
            option={allOption}
            style={{ height: "100%", width: "100%" }}
            opts={{ renderer: "canvas" }}
          />
        </div>
      )}
    </div>
  );
}
