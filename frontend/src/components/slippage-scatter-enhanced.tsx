"use client";

import { useMemo } from "react";
import dynamic from "next/dynamic";
import { MONO_FONT } from "@/lib/utils";
import type { ExecutionReport } from "@/lib/types";

const ReactECharts = dynamic(() => import("echarts-for-react"), { ssr: false });

interface SlippageScatterEnhancedProps {
  history: ExecutionReport[];
}

interface SlippagePoint {
  value: [number, number];
  expected: number;
  actual: number;
  slippage: number;
  size: number;
  side: string;
  symbolSize: number;
}

function computeBubbleSize(size: number, minSize: number, maxSize: number): number {
  const sqrtVal = Math.sqrt(size);
  const sqrtMin = Math.sqrt(minSize || 0);
  const sqrtMax = Math.sqrt(maxSize || 1);
  const range = sqrtMax - sqrtMin;
  if (range <= 0) return 14;
  return 4 + ((sqrtVal - sqrtMin) / range) * 20;
}

export function SlippageScatterEnhanced({ history }: SlippageScatterEnhancedProps) {
  const { favorable, adverse } = useMemo(() => {
    const fav: SlippagePoint[] = [];
    const adv: SlippagePoint[] = [];

    // Flatten all legs
    const allLegs: { expected: number; actual: number; size: number; side: string }[] = [];
    for (const report of history) {
      for (const leg of report.legs) {
        const expected = parseFloat(leg.expected_vwap);
        const actual = parseFloat(leg.actual_fill_price);
        const size = parseFloat(leg.filled_size);
        if (isNaN(expected) || isNaN(actual) || isNaN(size)) continue;
        allLegs.push({ expected, actual, size, side: leg.side });
      }
    }

    if (allLegs.length === 0) return { favorable: fav, adverse: adv };

    // Find min/max size for bubble scaling
    let minSize = Infinity;
    let maxSize = -Infinity;
    for (const l of allLegs) {
      if (l.size < minSize) minSize = l.size;
      if (l.size > maxSize) maxSize = l.size;
    }

    for (const l of allLegs) {
      const slippage = l.actual - l.expected;
      const isBuy = l.side === "Buy";
      // Buy: favorable if actual < expected (got better price)
      // Sell: favorable if actual > expected (got better price)
      const isFavorable = isBuy ? l.actual < l.expected : l.actual > l.expected;
      const bubbleSize = computeBubbleSize(l.size, minSize, maxSize);

      const point: SlippagePoint = {
        value: [l.expected, l.actual],
        expected: l.expected,
        actual: l.actual,
        slippage,
        size: l.size,
        side: l.side,
        symbolSize: bubbleSize,
      };

      if (isFavorable) {
        fav.push(point);
      } else {
        adv.push(point);
      }
    }

    return { favorable: fav, adverse: adv };
  }, [history]);

  const option = useMemo(() => {
    const allPoints = [...favorable, ...adverse];
    if (allPoints.length === 0) return null;

    // Compute axis range for the diagonal reference line
    let minVal = Infinity;
    let maxVal = -Infinity;
    for (const p of allPoints) {
      const lo = Math.min(p.expected, p.actual);
      const hi = Math.max(p.expected, p.actual);
      if (lo < minVal) minVal = lo;
      if (hi > maxVal) maxVal = hi;
    }
    // Add a small margin
    const margin = (maxVal - minVal) * 0.05 || 0.01;
    const lineMin = Math.max(0, minVal - margin);
    const lineMax = maxVal + margin;

    const tooltipFormatter = (params: { data?: SlippagePoint }) => {
      const d = params.data;
      if (!d) return "";
      const sign = d.slippage >= 0 ? "+" : "";
      return [
        `<div style="font-size:12px">`,
        `Expected VWAP: ${d.expected.toFixed(4)}`,
        `<br/>Actual Fill: ${d.actual.toFixed(4)}`,
        `<br/>Slippage: ${sign}${d.slippage.toFixed(4)}`,
        `<br/>Size: ${d.size.toFixed(4)}`,
        `<br/>Side: ${d.side}`,
        `</div>`,
      ].join("");
    };

    return {
      backgroundColor: "transparent",
      grid: {
        left: 60,
        right: 24,
        top: 24,
        bottom: 48,
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
      },
      legend: {
        bottom: 0,
        textStyle: {
          color: "#6B6B6B",
          fontFamily: MONO_FONT,
          fontSize: 11,
        },
        itemWidth: 10,
        itemHeight: 10,
      },
      xAxis: {
        type: "value" as const,
        name: "Expected VWAP",
        nameLocation: "center" as const,
        nameGap: 28,
        nameTextStyle: {
          color: "#6B6B6B",
          fontFamily: MONO_FONT,
          fontSize: 10,
        },
        axisLine: { lineStyle: { color: "#E6E4DF" } },
        axisTick: { show: false },
        splitLine: { lineStyle: { color: "#F0EEEA" } },
        axisLabel: {
          color: "#6B6B6B",
          fontFamily: MONO_FONT,
          fontSize: 10,
        },
      },
      yAxis: {
        type: "value" as const,
        name: "Actual Fill Price",
        nameLocation: "center" as const,
        nameGap: 44,
        nameTextStyle: {
          color: "#6B6B6B",
          fontFamily: MONO_FONT,
          fontSize: 10,
        },
        axisLine: { lineStyle: { color: "#E6E4DF" } },
        axisTick: { show: false },
        splitLine: { lineStyle: { color: "#F0EEEA" } },
        axisLabel: {
          color: "#6B6B6B",
          fontFamily: MONO_FONT,
          fontSize: 10,
        },
      },
      series: [
        {
          name: "Favorable",
          type: "scatter" as const,
          data: favorable,
          itemStyle: {
            color: "#2D6A4F",
            opacity: 0.7,
          },
          tooltip: {
            formatter: tooltipFormatter,
          },
        },
        {
          name: "Adverse",
          type: "scatter" as const,
          data: adverse,
          itemStyle: {
            color: "#B44C3F",
            opacity: 0.7,
          },
          tooltip: {
            formatter: tooltipFormatter,
          },
        },
        {
          name: "x = y",
          type: "line" as const,
          data: [
            [lineMin, lineMin],
            [lineMax, lineMax],
          ],
          symbol: "none",
          lineStyle: {
            type: "dashed" as const,
            color: "#9B9B9B",
            width: 1,
          },
          tooltip: { show: false },
        },
      ],
    };
  }, [favorable, adverse]);

  const hasData = favorable.length > 0 || adverse.length > 0;

  if (!hasData || !option) {
    return (
      <div className="rounded-2xl bg-white">
        <div className="border-b px-5 py-4">
          <h2 className="text-[11px] font-medium uppercase tracking-wider text-[#9B9B9B]">
            Slippage Analysis
          </h2>
        </div>
        <div className="p-4">
          <div
            className="flex items-center justify-center text-sm text-[#9B9B9B]"
            style={{ height: 320 }}
          >
            No leg data to chart
          </div>
        </div>
      </div>
    );
  }

  return (
    <div className="rounded-2xl bg-white">
      <div className="border-b px-5 py-4">
        <h2 className="text-[11px] font-medium uppercase tracking-wider text-[#9B9B9B]">
          Slippage Analysis
        </h2>
      </div>
      <div className="p-4">
        <ReactECharts
          option={option}
          style={{ height: 320, width: "100%" }}
          opts={{ renderer: "canvas" }}
        />
      </div>
    </div>
  );
}
