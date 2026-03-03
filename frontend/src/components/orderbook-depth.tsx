"use client";

import { memo, useMemo } from "react";
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

  if (!ascending) result.reverse();
  return result;
}

export const OrderbookDepth = memo(function OrderbookDepth({
  bids,
  asks,
}: OrderbookDepthProps) {
  const option = useMemo(() => {
    const bidData = cumulateDepth(bids, false);
    const askData = cumulateDepth(asks, true);

    return {
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
          color: "#6B6B6B",
          fontSize: 10,
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
        textStyle: { color: "#1A1A19", fontSize: 12 },
      },
      series: [
        {
          name: "Bids",
          type: "line" as const,
          data: bidData.map((d) => [d.price, d.cumSize]),
          lineStyle: { color: "#2D6A4F", width: 1.5 },
          areaStyle: { color: "rgba(45, 106, 79, 0.08)" },
          symbol: "none",
          smooth: true,
        },
        {
          name: "Asks",
          type: "line" as const,
          data: askData.map((d) => [d.price, d.cumSize]),
          lineStyle: { color: "#B44C3F", width: 1.5 },
          areaStyle: { color: "rgba(180, 76, 63, 0.08)" },
          symbol: "none",
          smooth: true,
        },
      ],
    };
  }, [bids, asks]);

  return (
    <ReactECharts
      option={option}
      style={{ height: "100%", width: "100%" }}
      opts={{ renderer: "canvas" }}
    />
  );
});
