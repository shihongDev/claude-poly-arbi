"use client";

import { useMemo } from "react";
import dynamic from "next/dynamic";
import type { ExecutionReport, Opportunity } from "@/lib/types";
import { strategyLabels, getStrategyDisplayType } from "@/lib/strategy-utils";

const ReactECharts = dynamic(() => import("echarts-for-react"), { ssr: false });

import { MONO_FONT } from "@/lib/utils";

interface WinRateByStrategyProps {
  history: ExecutionReport[];
  opportunities: Opportunity[];
}

interface StrategyStats {
  wins: number;
  losses: number;
  winAmounts: number[];
  lossAmounts: number[];
}

export function WinRateByStrategy({
  history,
  opportunities,
}: WinRateByStrategyProps) {
  const strategyData = useMemo(() => {
    if (history.length === 0) return null;

    const oppMap = new Map<string, Opportunity>();
    for (const opp of opportunities) {
      oppMap.set(opp.id, opp);
    }

    const statsMap = new Map<string, StrategyStats>();

    for (const exec of history) {
      const opp = oppMap.get(exec.opportunity_id);
      const strategyKey = opp ? getStrategyDisplayType(opp) : "Unknown";

      if (!statsMap.has(strategyKey)) {
        statsMap.set(strategyKey, {
          wins: 0,
          losses: 0,
          winAmounts: [],
          lossAmounts: [],
        });
      }

      const stats = statsMap.get(strategyKey)!;
      const realizedEdge = parseFloat(exec.realized_edge) || 0;
      const totalFees = parseFloat(exec.total_fees) || 0;
      const netPnl = realizedEdge - totalFees;

      if (netPnl >= 0) {
        stats.wins++;
        stats.winAmounts.push(netPnl);
      } else {
        stats.losses++;
        stats.lossAmounts.push(netPnl);
      }
    }

    const strategies = Array.from(statsMap.entries())
      .map(([key, stats]) => {
        const total = stats.wins + stats.losses;
        const winRate = total > 0 ? (stats.wins / total) * 100 : 0;
        const avgWin =
          stats.winAmounts.length > 0
            ? stats.winAmounts.reduce((a, b) => a + b, 0) /
              stats.winAmounts.length
            : 0;
        const avgLoss =
          stats.lossAmounts.length > 0
            ? stats.lossAmounts.reduce((a, b) => a + b, 0) /
              stats.lossAmounts.length
            : 0;

        return {
          key,
          label: strategyLabels[key] ?? key,
          wins: stats.wins,
          losses: stats.losses,
          winRate,
          avgWin,
          avgLoss,
        };
      })
      .sort((a, b) => b.winRate - a.winRate);

    return strategies;
  }, [history, opportunities]);

  const chartHeight = useMemo(() => {
    if (!strategyData) return 200;
    return Math.max(200, strategyData.length * 35 + 60);
  }, [strategyData]);

  const option = useMemo(() => {
    if (!strategyData || strategyData.length === 0) return null;

    const categories = strategyData.map((s) => s.label);
    const winRates = strategyData.map((s) => s.winRate);
    const lossRates = strategyData.map((s) => 100 - s.winRate);

    return {
      backgroundColor: "transparent",
      grid: {
        top: 10,
        right: 30,
        bottom: 30,
        left: 110,
        containLabel: false,
      },
      xAxis: {
        type: "value" as const,
        min: 0,
        max: 100,
        axisLabel: {
          color: "#6B6B6B",
          fontSize: 10,
          fontFamily: MONO_FONT,
          formatter: "{value}%",
        },
        axisLine: { lineStyle: { color: "#E6E4DF" } },
        axisTick: { show: false },
        splitLine: { lineStyle: { color: "#F0EEEA" } },
      },
      yAxis: {
        type: "category" as const,
        data: categories,
        axisLabel: {
          color: "#1A1A19",
          fontSize: 11,
          fontFamily: MONO_FONT,
        },
        axisLine: { show: false },
        axisTick: { show: false },
      },
      tooltip: {
        trigger: "axis" as const,
        axisPointer: { type: "shadow" as const },
        backgroundColor: "#FFFFFF",
        borderColor: "#E6E4DF",
        textStyle: {
          color: "#1A1A19",
          fontFamily: MONO_FONT,
          fontSize: 12,
        },
        formatter: (params: Array<{ dataIndex: number }>) => {
          if (!params.length || !strategyData) return "";
          const idx = params[0].dataIndex;
          const s = strategyData[idx];
          return [
            `<strong>${s.label}</strong>`,
            `Wins: ${s.wins} / Losses: ${s.losses}`,
            `Win rate: ${s.winRate.toFixed(1)}%`,
            `Avg win: $${s.avgWin.toFixed(4)}`,
            `Avg loss: $${Math.abs(s.avgLoss).toFixed(4)}`,
          ].join("<br/>");
        },
      },
      series: [
        {
          name: "Wins",
          type: "bar" as const,
          stack: "total",
          barWidth: 20,
          data: winRates,
          itemStyle: { color: "#2D6A4F" },
          label: {
            show: true,
            position: "inside" as const,
            color: "#FFFFFF",
            fontSize: 11,
            fontFamily: MONO_FONT,
            fontWeight: 600,
            formatter: (params: { value: number }) => {
              return params.value > 0 ? `${Math.round(params.value)}%` : "";
            },
          },
        },
        {
          name: "Losses",
          type: "bar" as const,
          stack: "total",
          barWidth: 20,
          data: lossRates,
          itemStyle: { color: "#B44C3F" },
          label: {
            show: false,
          },
        },
      ],
    };
  }, [strategyData]);

  const hasData = strategyData && strategyData.length > 0 && option;

  return (
    <div className="rounded-2xl bg-white p-5">
      <h2 className="text-[11px] font-medium uppercase tracking-wider text-[#9B9B9B]">
        Win Rate by Strategy
      </h2>
      {hasData ? (
        <div className="mt-2" style={{ height: chartHeight }}>
          <ReactECharts
            option={option}
            style={{ height: "100%", width: "100%" }}
            opts={{ renderer: "canvas" }}
          />
        </div>
      ) : (
        <div
          className="flex items-center justify-center text-[13px] text-[#9B9B9B]"
          style={{ height: 200, fontFamily: MONO_FONT }}
        >
          Awaiting data...
        </div>
      )}
    </div>
  );
}
