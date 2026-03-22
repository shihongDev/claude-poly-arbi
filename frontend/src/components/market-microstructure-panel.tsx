"use client";

import { useMemo } from "react";
import { MONO_STYLE, formatUsdCompact, formatSpreadBps, spreadSeverity } from "@/lib/utils";
import { SpreadSeverityGauge } from "@/components/spread-severity-gauge";
import type { MarketState } from "@/lib/types";

interface MarketMicrostructurePanelProps {
  market: MarketState;
}

interface MicroMetric {
  label: string;
  value: string;
  colorClass?: string;
}

export function MarketMicrostructurePanel({
  market,
}: MarketMicrostructurePanelProps) {
  const metrics = useMemo(() => {
    // Aggregate across all outcome orderbooks
    let totalBidSize = 0;
    let totalAskSize = 0;
    let bidVwapNum = 0;
    let bidVwapDen = 0;
    let askVwapNum = 0;
    let askVwapDen = 0;

    for (const ob of market.orderbooks) {
      // Top 10 levels for imbalance
      const topBids = ob.bids.slice(0, 10);
      const topAsks = ob.asks.slice(0, 10);

      for (const lvl of topBids) {
        const p = parseFloat(lvl.price);
        const s = parseFloat(lvl.size);
        totalBidSize += p * s;
        bidVwapNum += p * s;
        bidVwapDen += s;
      }
      for (const lvl of topAsks) {
        const p = parseFloat(lvl.price);
        const s = parseFloat(lvl.size);
        totalAskSize += p * s;
        askVwapNum += p * s;
        askVwapDen += s;
      }
    }

    const imbalanceRatio =
      totalAskSize > 0 ? totalBidSize / totalAskSize : null;

    const spreadNum = market.spread ? parseFloat(market.spread) : null;
    const spreadBps = spreadNum !== null ? spreadNum * 10000 : null;

    const bidVwap = bidVwapDen > 0 ? bidVwapNum / bidVwapDen : null;
    const askVwap = askVwapDen > 0 ? askVwapNum / askVwapDen : null;

    // Depth-weighted midpoint
    const dwm =
      bidVwap !== null && askVwap !== null && totalBidSize + totalAskSize > 0
        ? (bidVwap * totalAskSize + askVwap * totalBidSize) /
          (totalBidSize + totalAskSize)
        : null;

    const severity = spreadSeverity(spreadBps);

    const result: MicroMetric[] = [
      {
        label: "Bid/Ask Imbalance",
        value: imbalanceRatio !== null ? imbalanceRatio.toFixed(3) : "\u2014",
        colorClass:
          imbalanceRatio !== null
            ? imbalanceRatio > 1.2
              ? "text-[#2D6A4F]"
              : imbalanceRatio < 0.8
                ? "text-[#B44C3F]"
                : "text-[#1A1A19]"
            : "text-[#9B9B9B]",
      },
      {
        label: "Effective Spread",
        value: formatSpreadBps(market.spread),
        colorClass:
          severity === "good"
            ? "text-[#2D6A4F]"
            : severity === "warning"
              ? "text-[#D97706]"
              : severity === "danger"
                ? "text-[#B44C3F]"
                : "text-[#9B9B9B]",
      },
      {
        label: "Depth-Weighted Mid",
        value: dwm !== null ? dwm.toFixed(4) : "\u2014",
      },
      {
        label: "Total Bid Depth",
        value: formatUsdCompact(totalBidSize.toFixed(2)),
        colorClass: "text-[#2D6A4F]",
      },
      {
        label: "Total Ask Depth",
        value: formatUsdCompact(totalAskSize.toFixed(2)),
        colorClass: "text-[#B44C3F]",
      },
    ];

    return { metrics: result, spreadBps };
  }, [market.orderbooks, market.spread]);

  return (
    <div className="rounded-2xl bg-white p-5">
      <h2 className="text-[11px] font-medium uppercase tracking-wider text-[#9B9B9B]">
        Market Microstructure
      </h2>
      <div className="mt-3 grid grid-cols-2 gap-3 sm:grid-cols-3">
        {metrics.metrics.map((m) => (
          <div
            key={m.label}
            className="rounded-[10px] border border-[#E6E4DF] bg-[#F8F7F4] px-3 py-2"
          >
            <p className="text-[10px] text-[#9B9B9B]">{m.label}</p>
            <p
              className={`mt-0.5 text-sm font-medium ${m.colorClass ?? "text-[#1A1A19]"}`}
              style={MONO_STYLE}
            >
              {m.value}
            </p>
          </div>
        ))}
        {/* Spread severity gauge in its own cell */}
        <div className="rounded-[10px] border border-[#E6E4DF] bg-[#F8F7F4] px-3 py-2">
          <p className="text-[10px] text-[#9B9B9B]">Spread Severity</p>
          <SpreadSeverityGauge bps={metrics.spreadBps} />
        </div>
      </div>
    </div>
  );
}
