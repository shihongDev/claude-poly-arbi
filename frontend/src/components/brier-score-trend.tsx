"use client";

import { useMemo } from "react";
import dynamic from "next/dynamic";
import { AlertTriangle } from "lucide-react";
import { Badge } from "@/components/ui/badge";
import { cn, MONO_FONT } from "@/lib/utils";
import type { ModelHealth } from "@/lib/types";

const ReactECharts = dynamic(() => import("echarts-for-react"), { ssr: false });

interface BrierScoreTrendProps {
  health: ModelHealth;
}

function brierColor(score: number): string {
  if (score < 0.15) return "#2D6A4F";
  if (score < 0.25) return "#D97706";
  return "#B44C3F";
}

export function BrierScoreTrend({ health }: BrierScoreTrendProps) {
  const option = useMemo(() => {
    const categories = ["24h Window", "30m Window"];
    const values = [health.brier_score_24h, health.brier_score_30m];

    return {
      backgroundColor: "transparent",
      grid: {
        top: 10,
        right: 40,
        bottom: 10,
        left: 80,
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
        formatter: (
          params: Array<{
            name: string;
            seriesName: string;
            value: number;
            marker: string;
          }>
        ) => {
          if (!params.length) return "";
          const p = params[0];
          return `<strong>${p.name}</strong><br/>${p.marker} Brier Score: ${p.value.toFixed(4)}`;
        },
      },
      xAxis: {
        type: "value" as const,
        min: 0,
        max: 0.5,
        axisLabel: {
          color: "#9B9B9B",
          fontSize: 10,
          fontFamily: MONO_FONT,
        },
        axisLine: { lineStyle: { color: "#F0EEEA" } },
        splitLine: { show: false },
        axisTick: { show: false },
        markArea: {
          silent: true,
          data: [
            [
              {
                xAxis: 0,
                itemStyle: { color: "rgba(218, 233, 224, 0.3)" },
              },
              { xAxis: 0.15 },
            ],
            [
              {
                xAxis: 0.15,
                itemStyle: { color: "rgba(254, 243, 199, 0.3)" },
              },
              { xAxis: 0.25 },
            ],
            [
              {
                xAxis: 0.25,
                itemStyle: { color: "rgba(245, 224, 221, 0.3)" },
              },
              { xAxis: 0.5 },
            ],
          ],
        },
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
      series: [
        {
          type: "bar" as const,
          data: values.map((v) => ({
            value: v,
            itemStyle: {
              color: brierColor(v),
              borderRadius: [0, 4, 4, 0],
            },
          })),
          barWidth: 16,
          markLine: {
            silent: true,
            symbol: "none",
            data: [
              {
                xAxis: 0.25,
                label: {
                  show: true,
                  formatter: "Random\nBaseline",
                  position: "insideEndTop" as const,
                  color: "#9B9B9B",
                  fontSize: 9,
                  fontFamily: MONO_FONT,
                },
                lineStyle: {
                  color: "#9B9B9B",
                  type: "dashed" as const,
                  width: 1,
                },
              },
            ],
          },
        },
      ],
    };
  }, [health.brier_score_30m, health.brier_score_24h]);

  const confidenceColor =
    health.confidence_level >= 0.7
      ? "#2D6A4F"
      : health.confidence_level >= 0.4
        ? "#D97706"
        : "#B44C3F";

  return (
    <div className="rounded-2xl bg-white p-5">
      <h3 className="text-[11px] font-medium uppercase tracking-wider text-[#9B9B9B]">
        Model Health
      </h3>

      {/* Bullet chart */}
      <div className="mt-3">
        <ReactECharts
          option={option}
          style={{ height: 120, width: "100%" }}
          opts={{ renderer: "canvas" }}
        />
      </div>

      {/* Confidence progress bar */}
      <div className="mt-4">
        <div className="flex items-center justify-between">
          <p className="text-xs text-[#9B9B9B]">Confidence Level</p>
          <p
            className="text-xs text-[#6B6B6B]"
            style={{ fontFamily: "var(--font-jetbrains-mono)" }}
          >
            {(health.confidence_level * 100).toFixed(0)}%
          </p>
        </div>
        <div className="mt-1.5 h-2 w-full rounded-full bg-[#F0EEEA]">
          <div
            className="h-2 rounded-full transition-all duration-500"
            style={{
              width: `${health.confidence_level * 100}%`,
              backgroundColor: confidenceColor,
            }}
          />
        </div>
      </div>

      {/* Drift badge */}
      <div className="mt-4 flex items-center gap-2">
        {health.drift_detected ? (
          <Badge
            className={cn(
              "bg-[#F5E0DD] text-[#B44C3F] text-[10px] animate-pulse"
            )}
          >
            <AlertTriangle className="mr-1 h-3 w-3" />
            Drift Detected
          </Badge>
        ) : (
          <Badge className="bg-[#DAE9E0] text-[#2D6A4F] text-[10px]">
            Stable
          </Badge>
        )}
      </div>
    </div>
  );
}
