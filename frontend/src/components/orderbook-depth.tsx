"use client";

import ReactECharts from "echarts-for-react";
import type { OrderbookLevel } from "@/lib/types";

interface OrderbookDepthProps {
  bids: OrderbookLevel[];
  asks: OrderbookLevel[];
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

  // For bids, reverse so chart reads left to right (high to low price)
  if (!ascending) result.reverse();
  return result;
}

export function OrderbookDepth({ bids, asks }: OrderbookDepthProps) {
  const bidData = cumulateDepth(bids, false);
  const askData = cumulateDepth(asks, true);

  const option = {
    backgroundColor: "transparent",
    grid: {
      top: 16,
      right: 16,
      bottom: 32,
      left: 48,
    },
    xAxis: {
      type: "value" as const,
      axisLabel: {
        color: "#a1a1aa", // zinc-400
        fontSize: 10,
        formatter: (v: number) => v.toFixed(2),
      },
      axisLine: { lineStyle: { color: "#3f3f46" } }, // zinc-700
      splitLine: { show: false },
    },
    yAxis: {
      type: "value" as const,
      axisLabel: {
        color: "#a1a1aa",
        fontSize: 10,
      },
      axisLine: { lineStyle: { color: "#3f3f46" } },
      splitLine: { lineStyle: { color: "#27272a", type: "dashed" as const } }, // zinc-800
    },
    tooltip: {
      trigger: "axis" as const,
      backgroundColor: "#18181b", // zinc-900
      borderColor: "#3f3f46",
      textStyle: { color: "#fafafa", fontSize: 12 },
    },
    series: [
      {
        name: "Bids",
        type: "line" as const,
        data: bidData.map((d) => [d.price, d.cumSize]),
        lineStyle: { color: "#10b981", width: 1.5 }, // emerald-500
        areaStyle: { color: "rgba(16, 185, 129, 0.12)" },
        symbol: "none",
        smooth: true,
      },
      {
        name: "Asks",
        type: "line" as const,
        data: askData.map((d) => [d.price, d.cumSize]),
        lineStyle: { color: "#ef4444", width: 1.5 }, // red-500
        areaStyle: { color: "rgba(239, 68, 68, 0.12)" },
        symbol: "none",
        smooth: true,
      },
    ],
  };

  return (
    <ReactECharts
      option={option}
      style={{ height: "100%", width: "100%" }}
      opts={{ renderer: "canvas" }}
    />
  );
}
