"use client";

import { useMemo } from "react";
import dynamic from "next/dynamic";
import { MONO_FONT } from "@/lib/utils";
import type { Opportunity } from "@/lib/types";

const ReactECharts = dynamic(() => import("echarts-for-react"), { ssr: false });

const BIN_WIDTH = 5;
const BIN_COUNT = 20; // 0-5, 5-10, ..., 95-100

interface EdgeDistributionHistogramProps {
  opportunities: Opportunity[];
}

function buildBins(values: number[]): number[] {
  const counts = new Array(BIN_COUNT).fill(0) as number[];
  for (const v of values) {
    if (v < 0 || isNaN(v)) continue;
    const idx = Math.min(Math.floor(v / BIN_WIDTH), BIN_COUNT - 1);
    counts[idx]++;
  }
  return counts;
}

function trimBins(
  grossCounts: number[],
  netCounts: number[],
  labels: string[]
): { gross: number[]; net: number[]; labels: string[] } {
  let start = 0;
  let end = grossCounts.length - 1;

  while (start <= end && grossCounts[start] === 0 && netCounts[start] === 0) {
    start++;
  }
  while (end >= start && grossCounts[end] === 0 && netCounts[end] === 0) {
    end--;
  }

  if (start > end) {
    return { gross: [], net: [], labels: [] };
  }

  return {
    gross: grossCounts.slice(start, end + 1),
    net: netCounts.slice(start, end + 1),
    labels: labels.slice(start, end + 1),
  };
}

export function EdgeDistributionHistogram({
  opportunities,
}: EdgeDistributionHistogramProps) {
  const binLabels = useMemo(
    () =>
      Array.from({ length: BIN_COUNT }, (_, i) => {
        const lo = i * BIN_WIDTH;
        const hi = lo + BIN_WIDTH;
        return `${lo}-${hi}`;
      }),
    []
  );

  const { gross, net, labels } = useMemo(() => {
    const grossValues = opportunities.map(
      (o) => parseFloat(o.gross_edge) * 10000
    );
    const netValues = opportunities.map(
      (o) => parseFloat(o.net_edge) * 10000
    );

    const grossCounts = buildBins(grossValues);
    const netCounts = buildBins(netValues);

    return trimBins(grossCounts, netCounts, binLabels);
  }, [opportunities, binLabels]);

  const option = useMemo(
    () => ({
      backgroundColor: "transparent",
      grid: {
        top: 30,
        right: 20,
        bottom: 50,
        left: 40,
      },
      legend: {
        show: true,
        top: 0,
        right: 0,
        textStyle: {
          color: "#1A1A19",
          fontSize: 11,
          fontFamily: MONO_FONT,
        },
        itemWidth: 14,
        itemHeight: 10,
      },
      tooltip: {
        trigger: "axis" as const,
        backgroundColor: "#FFFFFF",
        borderColor: "#E6E4DF",
        textStyle: {
          color: "#1A1A19",
          fontFamily: MONO_FONT,
          fontSize: 12,
        },
        formatter: (
          params: Array<{
            axisValueLabel: string;
            seriesName: string;
            value: number;
            marker: string;
          }>
        ) => {
          if (!params.length) return "";
          const bin = params[0].axisValueLabel;
          let html = `<strong>${bin} bps</strong>`;
          for (const p of params) {
            html += `<br/>${p.marker} ${p.seriesName}: ${p.value}`;
          }
          return html;
        },
      },
      xAxis: {
        type: "category" as const,
        data: labels,
        axisLabel: {
          color: "#9B9B9B",
          fontSize: 10,
          fontFamily: MONO_FONT,
          rotate: labels.length > 10 ? 45 : 0,
        },
        axisLine: { lineStyle: { color: "#F0EEEA" } },
        axisTick: { show: false },
      },
      yAxis: {
        type: "value" as const,
        axisLabel: {
          color: "#9B9B9B",
          fontSize: 10,
          fontFamily: MONO_FONT,
        },
        axisLine: { show: false },
        splitLine: { lineStyle: { color: "#F0EEEA" } },
      },
      series: [
        {
          name: "Gross Edge",
          type: "bar" as const,
          data: gross,
          barWidth: "80%",
          barGap: "-100%",
          itemStyle: {
            color: "rgba(0,0,0,0)",
            borderColor: "#9B9B9B",
            borderWidth: 1.5,
            borderRadius: [2, 2, 0, 0],
          },
          emphasis: {
            itemStyle: {
              color: "rgba(0,0,0,0.03)",
              borderColor: "#9B9B9B",
              borderWidth: 2,
            },
          },
        },
        {
          name: "Net Edge",
          type: "bar" as const,
          data: net,
          barWidth: "80%",
          itemStyle: {
            color: "rgba(45,106,79,0.7)",
            borderRadius: [2, 2, 0, 0],
          },
          emphasis: {
            itemStyle: {
              color: "rgba(45,106,79,0.85)",
            },
          },
        },
      ],
    }),
    [gross, net, labels]
  );

  const isEmpty = opportunities.length === 0;

  return (
    <div className="rounded-2xl bg-white">
      <div className="border-b border-[#E6E4DF] px-5 py-4">
        <h3 className="text-[11px] font-medium uppercase tracking-wider text-[#9B9B9B]">
          Edge Distribution
        </h3>
      </div>
      <div className="p-4">
        {isEmpty ? (
          <div
            className="flex items-center justify-center text-sm text-[#9B9B9B]"
            style={{ height: 280 }}
          >
            Awaiting data...
          </div>
        ) : (
          <ReactECharts
            option={option}
            style={{ height: 280, width: "100%" }}
            opts={{ renderer: "canvas" }}
          />
        )}
      </div>
    </div>
  );
}
