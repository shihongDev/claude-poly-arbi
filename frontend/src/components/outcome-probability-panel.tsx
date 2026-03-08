"use client";

import { useMemo } from "react";
import dynamic from "next/dynamic";
import { AlertTriangle } from "lucide-react";
import { Badge } from "@/components/ui/badge";
import {
  OUTCOME_COLORS,
  MONO_FONT,
  MONO_STYLE,
  probSumDeviation,
} from "@/lib/utils";
import type { MarketState } from "@/lib/types";

const ReactECharts = dynamic(() => import("echarts-for-react"), { ssr: false });

interface OutcomeProbabilityPanelProps {
  market: MarketState;
}

export function OutcomeProbabilityPanel({
  market,
}: OutcomeProbabilityPanelProps) {
  const deviation = probSumDeviation(market.outcome_prices);
  const hasDeviation = deviation > 0.5;

  const primaryIdx = useMemo(() => {
    let maxIdx = 0;
    let maxVal = 0;
    market.outcome_prices.forEach((p, i) => {
      const v = parseFloat(p || "0");
      if (v > maxVal) {
        maxVal = v;
        maxIdx = i;
      }
    });
    return maxIdx;
  }, [market.outcome_prices]);

  const primaryPct = Math.round(
    parseFloat(market.outcome_prices[primaryIdx] || "0") * 100
  );

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

  const donutOption = useMemo(() => {
    const seriesData = market.outcomes.map((name, i) => ({
      name,
      value: Math.round(parseFloat(market.outcome_prices[i] || "0") * 10000) / 100,
      itemStyle: { color: OUTCOME_COLORS[i % OUTCOME_COLORS.length] },
    }));

    return {
      backgroundColor: "transparent",
      tooltip: {
        trigger: "item" as const,
        backgroundColor: "#FFFFFF",
        borderColor: "#E6E4DF",
        textStyle: { color: "#1A1A19", fontFamily: MONO_FONT, fontSize: 12 },
        formatter: (params: { name: string; value: number; percent: number }) =>
          `${params.name}<br/>${params.value.toFixed(1)}% (${params.percent.toFixed(1)}%)`,
      },
      legend: { show: false },
      graphic: [
        {
          type: "text" as const,
          left: "center",
          top: "center",
          style: {
            text: `${primaryPct}%`,
            fontSize: 28,
            fontWeight: "bold" as const,
            fontFamily: MONO_FONT,
            fill: "#1A1A19",
            textAlign: "center" as const,
          },
        },
      ],
      series: [
        {
          type: "pie" as const,
          radius: ["40%", "65%"],
          center: ["50%", "50%"],
          avoidLabelOverlap: false,
          padAngle: 2,
          itemStyle: { borderRadius: 4 },
          label: { show: false },
          emphasis: {
            scale: true,
            scaleSize: 4,
          },
          data: seriesData,
        },
      ],
    };
  }, [market.outcomes, market.outcome_prices, primaryPct]);

  return (
    <div className="rounded-2xl bg-white p-5">
      <div className="flex items-center gap-3">
        <h2 className="text-[11px] font-medium uppercase tracking-wider text-[#9B9B9B]">
          Outcome Probabilities
        </h2>
        {hasDeviation && (
          <Badge className="bg-[#FEF3CD] text-[#856404] text-[10px] inline-flex items-center gap-1">
            <AlertTriangle className="h-2.5 w-2.5" />
            Sum deviates by {deviation.toFixed(1)}pp
          </Badge>
        )}
      </div>

      <div className="mt-4 grid grid-cols-2 gap-4">
        {/* Donut chart */}
        <div className="h-[200px]">
          <ReactECharts
            option={donutOption}
            style={{ height: "100%", width: "100%" }}
            opts={{ renderer: "canvas" }}
          />
        </div>

        {/* Outcome detail list */}
        <div className="flex flex-col justify-center space-y-2">
          {market.outcomes.map((name, i) => {
            const price = parseFloat(market.outcome_prices[i] || "0");
            const pct = price * 100;
            const tokenId = market.token_ids[i];
            const ba = tokenId ? tokenBidAsk.get(tokenId) : undefined;
            const color = OUTCOME_COLORS[i % OUTCOME_COLORS.length];

            return (
              <div key={name} className="space-y-1">
                <div className="flex items-center justify-between">
                  <div className="flex items-center gap-2">
                    <div
                      className="h-2.5 w-2.5 rounded-full"
                      style={{ backgroundColor: color }}
                    />
                    <span className="text-sm text-[#1A1A19]">{name}</span>
                  </div>
                  <span
                    className="text-sm font-medium text-[#1A1A19]"
                    style={MONO_STYLE}
                  >
                    {pct.toFixed(1)}%
                  </span>
                </div>
                {/* Mini price bar */}
                <div className="h-1.5 w-full overflow-hidden rounded-full bg-[#F0EEEA]">
                  <div
                    className="h-full rounded-full transition-all"
                    style={{
                      width: `${Math.max(pct, 1)}%`,
                      backgroundColor: color,
                    }}
                  />
                </div>
                {ba && (ba.bid !== null || ba.ask !== null) && (
                  <div
                    className="flex gap-3 text-[9px] text-[#9B9B9B]"
                    style={MONO_STYLE}
                  >
                    <span>
                      Bid:{" "}
                      <span className="text-[#2D6A4F]">
                        {ba.bid !== null ? ba.bid.toFixed(4) : "\u2014"}
                      </span>
                    </span>
                    <span>
                      Ask:{" "}
                      <span className="text-[#B44C3F]">
                        {ba.ask !== null ? ba.ask.toFixed(4) : "\u2014"}
                      </span>
                    </span>
                  </div>
                )}
              </div>
            );
          })}
        </div>
      </div>
    </div>
  );
}
