"use client";

import { useMemo } from "react";
import { OUTCOME_COLORS, MONO_STYLE, formatUsdCompact } from "@/lib/utils";
import type { MarketState } from "@/lib/types";

interface OrderImbalanceChartProps {
  market: MarketState;
}

interface ImbalanceRow {
  name: string;
  bidSize: number;
  askSize: number;
  ratio: number;
  label: string;
  color: string;
}

export function OrderImbalanceChart({ market }: OrderImbalanceChartProps) {
  const rows = useMemo<ImbalanceRow[]>(() => {
    return market.outcomes.map((name, i) => {
      const tokenId = market.token_ids[i];
      const ob = market.orderbooks.find((o) => o.token_id === tokenId);

      let bidSize = 0;
      let askSize = 0;

      if (ob) {
        bidSize = ob.bids.reduce(
          (sum, lvl) => sum + parseFloat(lvl.price) * parseFloat(lvl.size),
          0
        );
        askSize = ob.asks.reduce(
          (sum, lvl) => sum + parseFloat(lvl.price) * parseFloat(lvl.size),
          0
        );
      }

      const total = bidSize + askSize;
      const ratio = askSize > 0 ? bidSize / askSize : 0;
      const label =
        ratio > 1.2 ? "Bullish" : ratio < 0.8 ? "Bearish" : "Balanced";

      return {
        name,
        bidSize,
        askSize,
        ratio,
        label,
        color: OUTCOME_COLORS[i % OUTCOME_COLORS.length],
      };
    });
  }, [market.outcomes, market.token_ids, market.orderbooks]);

  if (rows.length === 0) {
    return (
      <div className="rounded-2xl bg-white p-5">
        <h2 className="text-[11px] font-medium uppercase tracking-wider text-[#9B9B9B]">
          Order Imbalance
        </h2>
        <p className="mt-3 text-sm text-[#9B9B9B]">No orderbook data</p>
      </div>
    );
  }

  return (
    <div className="rounded-2xl bg-white p-5">
      <h2 className="text-[11px] font-medium uppercase tracking-wider text-[#9B9B9B]">
        Order Imbalance
      </h2>
      <div className="mt-4 space-y-4">
        {rows.map((row) => {
          const total = row.bidSize + row.askSize;
          const bidPct = total > 0 ? (row.bidSize / total) * 100 : 50;
          const askPct = total > 0 ? (row.askSize / total) * 100 : 50;

          return (
            <div key={row.name}>
              <div className="mb-1.5 flex items-center justify-between">
                <span className="text-sm text-[#1A1A19]">{row.name}</span>
                <span
                  className={`text-[10px] font-medium ${
                    row.label === "Bullish"
                      ? "text-[#2D6A4F]"
                      : row.label === "Bearish"
                        ? "text-[#B44C3F]"
                        : "text-[#9B9B9B]"
                  }`}
                >
                  {row.label}
                </span>
              </div>

              {/* Bar */}
              <div className="flex h-5 w-full overflow-hidden rounded-full">
                <div
                  className="flex items-center justify-end pr-1 transition-all"
                  style={{
                    width: `${bidPct}%`,
                    backgroundColor: "rgba(45, 106, 79, 0.15)",
                  }}
                >
                  {bidPct > 15 && (
                    <span
                      className="text-[9px] font-medium text-[#2D6A4F]"
                      style={MONO_STYLE}
                    >
                      {bidPct.toFixed(0)}%
                    </span>
                  )}
                </div>
                <div
                  className="flex items-center justify-start pl-1 transition-all"
                  style={{
                    width: `${askPct}%`,
                    backgroundColor: "rgba(180, 76, 63, 0.15)",
                  }}
                >
                  {askPct > 15 && (
                    <span
                      className="text-[9px] font-medium text-[#B44C3F]"
                      style={MONO_STYLE}
                    >
                      {askPct.toFixed(0)}%
                    </span>
                  )}
                </div>
              </div>

              {/* Dollar amounts */}
              <div
                className="mt-1 flex items-center justify-between text-[10px] text-[#9B9B9B]"
                style={MONO_STYLE}
              >
                <span>
                  Bid: <span className="text-[#2D6A4F]">{formatUsdCompact(row.bidSize.toFixed(2))}</span>
                </span>
                <span>
                  Ratio: <span className="text-[#1A1A19]">{row.ratio.toFixed(2)}</span>
                </span>
                <span>
                  Ask: <span className="text-[#B44C3F]">{formatUsdCompact(row.askSize.toFixed(2))}</span>
                </span>
              </div>
            </div>
          );
        })}
      </div>
    </div>
  );
}
