"use client";

import { memo, useMemo } from "react";
import ReactECharts from "echarts-for-react";

interface RiskGaugeProps {
  value: number;
  max: number;
  label: string;
  warningThreshold?: number;
  criticalThreshold?: number;
}

export const RiskGauge = memo(function RiskGauge({
  value,
  max,
  label,
  warningThreshold = 0.6,
  criticalThreshold = 0.8,
}: RiskGaugeProps) {
  const option = useMemo(() => {
    const pct = max > 0 ? value / max : 0;
    const color =
      pct >= criticalThreshold
        ? "#B44C3F"
        : pct >= warningThreshold
          ? "#D97706"
          : "#2D6A4F";

    return {
      backgroundColor: "transparent",
      series: [
        {
          type: "gauge" as const,
          startAngle: 200,
          endAngle: -20,
          min: 0,
          max,
          radius: "90%",
          progress: {
            show: true,
            width: 12,
            roundCap: true,
            itemStyle: { color },
          },
          pointer: { show: false },
          axisLine: {
            lineStyle: {
              width: 12,
              color: [[1, "#E6E4DF"]],
            },
            roundCap: true,
          },
          axisTick: { show: false },
          splitLine: { show: false },
          axisLabel: { show: false },
          title: {
            show: true,
            offsetCenter: [0, "70%"],
            fontSize: 12,
            color: "#6B6B6B",
            fontFamily: "Space Grotesk, sans-serif",
          },
          detail: {
            offsetCenter: [0, "20%"],
            fontSize: 24,
            fontWeight: "bold" as const,
            fontFamily:
              "var(--font-jetbrains-mono), JetBrains Mono, monospace",
            color: "#1A1A19",
            formatter: (v: number) => v.toFixed(1),
          },
          data: [{ value, name: label }],
        },
      ],
    };
  }, [value, max, label, warningThreshold, criticalThreshold]);

  return (
    <ReactECharts
      option={option}
      style={{ height: "100%", width: "100%" }}
      opts={{ renderer: "canvas" }}
    />
  );
});
