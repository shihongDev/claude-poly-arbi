"use client";

import { useMemo } from "react";
import dynamic from "next/dynamic";
import { cn } from "@/lib/utils";
import type { MarketState } from "@/lib/types";

const ReactECharts = dynamic(() => import("echarts-for-react"), { ssr: false });

interface SpreadHistogramProps {
  markets: MarketState[];
}

interface Bucket {
  label: string;
  min: number;
  max: number;
  color: string;
}

const BUCKETS: Bucket[] = [
  { label: "0-10", min: 0, max: 10, color: "#2D6A4F" },
  { label: "10-30", min: 10, max: 30, color: "#2D6A4F" },
  { label: "30-50", min: 30, max: 50, color: "#D97706" },
  { label: "50-100", min: 50, max: 100, color: "#D97706" },
  { label: "100-200", min: 100, max: 200, color: "#B44C3F" },
  { label: "200+", min: 200, max: Infinity, color: "#B44C3F" },
];

export function SpreadHistogram({ markets }: SpreadHistogramProps) {
  const { counts, total } = useMemo(() => {
    const bucketCounts = new Array(BUCKETS.length).fill(0) as number[];
    let totalWithSpread = 0;

    for (const m of markets) {
      if (!m.active || m.spread === null) continue;
      const bps = parseFloat(m.spread) * 10000;
      if (isNaN(bps)) continue;
      totalWithSpread++;

      for (let i = 0; i < BUCKETS.length; i++) {
        if (bps >= BUCKETS[i].min && (bps < BUCKETS[i].max || BUCKETS[i].max === Infinity)) {
          bucketCounts[i]++;
          break;
        }
      }
    }

    return { counts: bucketCounts, total: totalWithSpread };
  }, [markets]);

  const option = useMemo(
    () => ({
      backgroundColor: "transparent",
      grid: {
        top: 20,
        right: 20,
        bottom: 30,
        left: 40,
      },
      xAxis: {
        type: "category" as const,
        data: BUCKETS.map((b) => b.label),
        axisLabel: {
          color: "#6B6B6B",
          fontSize: 10,
          fontFamily: "var(--font-jetbrains-mono), JetBrains Mono, monospace",
        },
        axisLine: { lineStyle: { color: "#E6E4DF" } },
        axisTick: { show: false },
      },
      yAxis: {
        type: "value" as const,
        axisLabel: {
          color: "#6B6B6B",
          fontSize: 10,
          fontFamily: "var(--font-jetbrains-mono), JetBrains Mono, monospace",
        },
        axisLine: { show: false },
        splitLine: { show: false },
      },
      tooltip: {
        trigger: "item" as const,
        backgroundColor: "#FFFFFF",
        borderColor: "#E6E4DF",
        textStyle: { color: "#1A1A19", fontSize: 12 },
        formatter: (params: { name: string; value: number }) => {
          const pct = total > 0 ? ((params.value / total) * 100).toFixed(1) : "0";
          return `<strong>${params.name} bps</strong><br/>${params.value} market${params.value !== 1 ? "s" : ""} (${pct}%)`;
        },
      },
      series: [
        {
          type: "bar" as const,
          data: counts.map((count, i) => ({
            value: count,
            itemStyle: { color: BUCKETS[i].color },
          })),
          barWidth: "60%",
          itemStyle: {
            borderRadius: [3, 3, 0, 0],
          },
        },
      ],
    }),
    [counts, total]
  );

  return (
    <div className="rounded-2xl bg-white p-5" style={{ height: 180 }}>
      <h3 className="text-[11px] font-medium uppercase tracking-wider text-[#9B9B9B]">
        Spread Distribution
      </h3>
      <div className="mt-1" style={{ height: 130 }}>
        <ReactECharts
          option={option}
          style={{ height: "100%", width: "100%" }}
          opts={{ renderer: "canvas" }}
        />
      </div>
    </div>
  );
}
