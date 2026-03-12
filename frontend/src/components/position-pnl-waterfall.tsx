"use client";

import { useMemo } from "react";
import dynamic from "next/dynamic";
import { MONO_FONT } from "@/lib/utils";
import type { Position } from "@/lib/types";

const ReactECharts = dynamic(() => import("echarts-for-react"), { ssr: false });

interface PositionPnlWaterfallProps {
  positions: Position[];
}

function truncateTokenId(id: string): string {
  return id.length > 16 ? id.slice(0, 8) + "..." : id;
}

interface ProcessedPosition {
  tokenId: string;
  entryValue: number;
  currentValue: number;
  pnl: number;
}

export function PositionPnlWaterfall({ positions }: PositionPnlWaterfallProps) {
  const processed = useMemo<ProcessedPosition[]>(() => {
    return positions
      .filter((p) => parseFloat(p.size) > 0)
      .map((p) => {
        const size = parseFloat(p.size);
        const avgEntry = parseFloat(p.avg_entry_price);
        const current = parseFloat(p.current_price);
        const pnl = parseFloat(p.unrealized_pnl);
        return {
          tokenId: p.token_id,
          entryValue: size * avgEntry,
          currentValue: size * current,
          pnl: isNaN(pnl) ? 0 : pnl,
        };
      })
      .sort((a, b) => Math.abs(b.pnl) - Math.abs(a.pnl));
  }, [positions]);

  const option = useMemo(() => {
    if (processed.length === 0) return null;

    const labels = [
      ...processed.map((p) => truncateTokenId(p.tokenId)),
      "Total",
    ];

    const totalPnl = processed.reduce((sum, p) => sum + p.pnl, 0);

    // Build waterfall data: placeholder (transparent base) + value (colored bar)
    const placeholderData: (number | string)[] = [];
    const valueData: {
      value: number;
      itemStyle: { color: string };
    }[] = [];

    let running = 0;

    for (const p of processed) {
      if (p.pnl >= 0) {
        // Positive bar: base = running total, bar grows upward
        placeholderData.push(running);
        valueData.push({
          value: p.pnl,
          itemStyle: { color: "#2D6A4F" },
        });
      } else {
        // Negative bar: base = running total + pnl (lower point), bar grows upward to running
        placeholderData.push(running + p.pnl);
        valueData.push({
          value: Math.abs(p.pnl),
          itemStyle: { color: "#B44C3F" },
        });
      }
      running += p.pnl;
    }

    // Total bar
    if (totalPnl >= 0) {
      placeholderData.push(0);
      valueData.push({
        value: totalPnl,
        itemStyle: { color: "#2D6A4F" },
      });
    } else {
      placeholderData.push(totalPnl);
      valueData.push({
        value: Math.abs(totalPnl),
        itemStyle: { color: "#B44C3F" },
      });
    }

    // Build a lookup for tooltip (index -> processed position data)
    const positionLookup = processed.map((p) => ({
      tokenId: p.tokenId,
      entryValue: p.entryValue,
      currentValue: p.currentValue,
      pnl: p.pnl,
    }));

    return {
      backgroundColor: "transparent",
      grid: {
        top: 10,
        right: 30,
        bottom: 30,
        left: 100,
      },
      xAxis: {
        type: "value" as const,
        axisLabel: {
          color: "#9B9B9B",
          fontSize: 10,
          fontFamily: MONO_FONT,
          formatter: (v: number) => `$${v.toFixed(2)}`,
        },
        axisLine: { lineStyle: { color: "#F0EEEA" } },
        splitLine: { lineStyle: { color: "#F0EEEA" } },
      },
      yAxis: {
        type: "category" as const,
        data: labels,
        inverse: true,
        axisLabel: {
          color: "#1A1A19",
          fontSize: 10,
          fontFamily: MONO_FONT,
        },
        axisLine: { lineStyle: { color: "#F0EEEA" } },
        axisTick: { show: false },
      },
      tooltip: {
        trigger: "axis" as const,
        backgroundColor: "#FFFFFF",
        borderColor: "#E6E4DF",
        textStyle: {
          color: "#1A1A19",
          fontFamily: MONO_FONT,
        },
        formatter: (params: { dataIndex: number }[]) => {
          if (!params || params.length === 0) return "";
          const idx = params[0].dataIndex;

          if (idx < positionLookup.length) {
            const pos = positionLookup[idx];
            const sign = pos.pnl >= 0 ? "+" : "";
            const pnlColor = pos.pnl >= 0 ? "#2D6A4F" : "#B44C3F";
            return [
              `<strong>${pos.tokenId}</strong>`,
              `Entry Value: $${pos.entryValue.toFixed(2)}`,
              `Current Value: $${pos.currentValue.toFixed(2)}`,
              `Unrealized P&L: <span style="color:${pnlColor};font-weight:600">${sign}$${pos.pnl.toFixed(2)}</span>`,
            ].join("<br/>");
          }

          // Total bar
          const sign = totalPnl >= 0 ? "+" : "";
          const pnlColor = totalPnl >= 0 ? "#2D6A4F" : "#B44C3F";
          return `<strong>Total</strong><br/>Cumulative P&L: <span style="color:${pnlColor};font-weight:600">${sign}$${totalPnl.toFixed(2)}</span>`;
        },
      },
      series: [
        {
          name: "placeholder",
          type: "bar" as const,
          stack: "waterfall",
          silent: true,
          itemStyle: {
            color: "transparent",
            borderColor: "transparent",
          },
          emphasis: {
            itemStyle: {
              color: "transparent",
              borderColor: "transparent",
            },
          },
          data: placeholderData,
        },
        {
          name: "value",
          type: "bar" as const,
          stack: "waterfall",
          data: valueData,
          barWidth: "50%",
          itemStyle: {
            borderRadius: [0, 3, 3, 0],
          },
          label: {
            show: true,
            position: "right" as const,
            color: "#9B9B9B",
            fontSize: 9,
            fontFamily: MONO_FONT,
            formatter: (params: { dataIndex: number }) => {
              const idx = params.dataIndex;
              const pnl =
                idx < positionLookup.length
                  ? positionLookup[idx].pnl
                  : totalPnl;
              const sign = pnl >= 0 ? "+" : "";
              return `${sign}$${pnl.toFixed(2)}`;
            },
          },
        },
      ],
    };
  }, [processed]);

  if (processed.length === 0) {
    return (
      <div className="rounded-2xl bg-white p-5">
        <h2 className="text-[11px] font-medium uppercase tracking-wider text-[#9B9B9B]">
          P&L Waterfall
        </h2>
        <div className="flex h-[200px] items-center justify-center">
          <p className="text-sm text-[#9B9B9B]">Awaiting data...</p>
        </div>
      </div>
    );
  }

  const chartHeight = Math.max(200, processed.length * 30 + 60);

  return (
    <div className="rounded-2xl bg-white p-5">
      <h2 className="text-[11px] font-medium uppercase tracking-wider text-[#9B9B9B]">
        P&L Waterfall
      </h2>
      <div className="mt-2" style={{ height: chartHeight }}>
        <ReactECharts
          option={option}
          style={{ height: "100%", width: "100%" }}
          opts={{ renderer: "canvas" }}
        />
      </div>
    </div>
  );
}
