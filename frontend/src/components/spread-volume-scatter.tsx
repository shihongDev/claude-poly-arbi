"use client";

import { memo, useMemo } from "react";
import dynamic from "next/dynamic";
import { cn, truncate, MONO_FONT } from "@/lib/utils";
import type { MarketState } from "@/lib/types";

const ReactECharts = dynamic(() => import("echarts-for-react"), { ssr: false });

interface SpreadVolumeScatterProps {
  markets: MarketState[];
  onMarketClick?: (conditionId: string) => void;
}

function dotColor(change: number | null): string {
  if (change === null || isNaN(change)) return "#9B9B9B";
  if (change > 0) return "#2D6A4F";
  if (change < 0) return "#B44C3F";
  return "#9B9B9B";
}

interface ScatterPoint {
  value: [number, number];
  conditionId: string;
  question: string;
  volume: number;
  spreadBps: number;
  liquidity: number;
  changePct: number | null;
  symbolSize: number;
  itemStyle: { color: string };
}

export const SpreadVolumeScatter = memo(function SpreadVolumeScatter({
  markets,
  onMarketClick,
}: SpreadVolumeScatterProps) {
  const points = useMemo<ScatterPoint[]>(() => {
    const valid: ScatterPoint[] = [];
    let maxLiquidity = 0;

    // First pass: collect valid points and find max liquidity
    const raw: {
      volume: number;
      spreadBps: number;
      liquidity: number;
      changePct: number | null;
      conditionId: string;
      question: string;
    }[] = [];

    for (const m of markets) {
      if (m.volume_24hr === null || m.spread === null) continue;
      const volume = parseFloat(m.volume_24hr);
      const spread = parseFloat(m.spread);
      if (isNaN(volume) || isNaN(spread) || volume <= 0 || spread <= 0) continue;

      const spreadBps = spread * 10000;
      const liquidity = m.liquidity ? parseFloat(m.liquidity) : 0;
      const changePct = m.one_day_price_change
        ? parseFloat(m.one_day_price_change) * 100
        : null;

      if (liquidity > maxLiquidity) maxLiquidity = liquidity;
      raw.push({
        volume,
        spreadBps,
        liquidity,
        changePct,
        conditionId: m.condition_id,
        question: m.question,
      });
    }

    // Second pass: compute symbol sizes
    const sqrtMax = Math.sqrt(maxLiquidity || 1);
    for (const r of raw) {
      const sqrtLiq = Math.sqrt(r.liquidity || 1);
      // Scale from 6 to 30px
      const size = sqrtMax > 0 ? 6 + (sqrtLiq / sqrtMax) * 24 : 10;

      valid.push({
        value: [r.volume, r.spreadBps],
        conditionId: r.conditionId,
        question: r.question,
        volume: r.volume,
        spreadBps: r.spreadBps,
        liquidity: r.liquidity,
        changePct: r.changePct,
        symbolSize: Math.max(6, Math.min(30, size)),
        itemStyle: { color: dotColor(r.changePct) },
      });
    }

    return valid;
  }, [markets]);

  const option = useMemo(() => {
    return {
      backgroundColor: "transparent",
      grid: {
        top: 16,
        right: 24,
        bottom: 40,
        left: 60,
      },
      tooltip: {
        trigger: "item" as const,
        backgroundColor: "#FFFFFF",
        borderColor: "#E6E4DF",
        borderWidth: 1,
        textStyle: {
          color: "#1A1A19",
          fontFamily: MONO_FONT,
          fontSize: 12,
        },
        formatter: (params: { data?: ScatterPoint }) => {
          const d = params.data;
          if (!d) return "";
          const change =
            d.changePct !== null
              ? `${d.changePct >= 0 ? "+" : ""}${d.changePct.toFixed(1)}%`
              : "\u2014";
          const vol = `$${d.volume.toLocaleString(undefined, { maximumFractionDigits: 0 })}`;
          const liq = `$${d.liquidity.toLocaleString(undefined, { maximumFractionDigits: 0 })}`;
          return [
            `<div style="max-width:280px;white-space:normal;font-size:12px">`,
            `<strong>${truncate(d.question, 60)}</strong>`,
            `<br/>Volume: ${vol}`,
            `<br/>Spread: ${d.spreadBps.toFixed(0)} bps`,
            `<br/>Liquidity: ${liq}`,
            `<br/>24h Change: ${change}`,
            `</div>`,
          ].join("");
        },
      },
      xAxis: {
        type: "log" as const,
        name: "Volume ($)",
        nameLocation: "center" as const,
        nameGap: 24,
        nameTextStyle: {
          color: "#9B9B9B",
          fontFamily: MONO_FONT,
          fontSize: 10,
        },
        axisLine: { lineStyle: { color: "#E6E4DF" } },
        axisTick: { show: false },
        splitLine: { lineStyle: { color: "#F0EEEA" } },
        axisLabel: {
          color: "#9B9B9B",
          fontFamily: MONO_FONT,
          fontSize: 10,
          formatter: (v: number) => {
            if (v >= 1_000_000) return `$${(v / 1_000_000).toFixed(0)}M`;
            if (v >= 1_000) return `$${(v / 1_000).toFixed(0)}K`;
            return `$${v}`;
          },
        },
      },
      yAxis: {
        type: "log" as const,
        name: "Spread (bps)",
        nameLocation: "center" as const,
        nameGap: 44,
        nameTextStyle: {
          color: "#9B9B9B",
          fontFamily: MONO_FONT,
          fontSize: 10,
        },
        axisLine: { lineStyle: { color: "#E6E4DF" } },
        axisTick: { show: false },
        splitLine: { lineStyle: { color: "#F0EEEA" } },
        axisLabel: {
          color: "#9B9B9B",
          fontFamily: MONO_FONT,
          fontSize: 10,
        },
      },
      series: [
        {
          type: "scatter" as const,
          data: points,
          markArea: {
            silent: true,
            data: [
              [
                {
                  xAxis: 10000,
                  yAxis: 0.1,
                  itemStyle: { color: "rgba(45,106,79,0.06)" },
                },
                { xAxis: 10000000, yAxis: 50 },
              ],
            ],
            label: {
              show: true,
              position: "insideTopRight" as const,
              formatter: "Sweet Spot",
              color: "#2D6A4F",
              fontFamily: MONO_FONT,
              fontSize: 10,
              opacity: 0.7,
            },
          },
        },
      ],
    };
  }, [points]);

  const onEvents = useMemo((): Record<string, Function> | undefined => {
    if (!onMarketClick) return undefined;
    return {
      click: (params: { data?: { conditionId?: string } }) => {
        const conditionId = params.data?.conditionId;
        if (conditionId) {
          onMarketClick(conditionId);
        }
      },
    };
  }, [onMarketClick]);

  if (points.length === 0) {
    return (
      <div className="rounded-2xl bg-white p-5" style={{ width: "100%", height: 400 }}>
        <h2 className="text-[11px] font-medium uppercase tracking-wider text-[#9B9B9B]">
          Volume vs Spread
        </h2>
        <div className="flex h-[340px] items-center justify-center text-sm text-[#9B9B9B]">
          No markets with volume and spread data
        </div>
      </div>
    );
  }

  return (
    <div className="rounded-2xl bg-white p-5" style={{ width: "100%", height: 400 }}>
      <h2 className="text-[11px] font-medium uppercase tracking-wider text-[#9B9B9B]">
        Volume vs Spread
      </h2>
      <div style={{ height: 350, marginTop: 8 }}>
        <ReactECharts
          option={option}
          style={{ height: "100%", width: "100%" }}
          opts={{ renderer: "canvas" }}
          onEvents={onEvents}
        />
      </div>
    </div>
  );
});
