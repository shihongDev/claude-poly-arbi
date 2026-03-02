"use client";

import ReactECharts from "echarts-for-react";

interface RiskGaugeProps {
  value: number;
  max: number;
  label: string;
  warningThreshold?: number;
  criticalThreshold?: number;
}

export function RiskGauge({
  value,
  max,
  label,
  warningThreshold = 0.6,
  criticalThreshold = 0.8,
}: RiskGaugeProps) {
  const pct = max > 0 ? value / max : 0;

  const getColor = () => {
    if (pct >= criticalThreshold) return "#ef4444"; // red-500
    if (pct >= warningThreshold) return "#f59e0b"; // amber-500
    return "#10b981"; // emerald-500
  };

  const option = {
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
          itemStyle: {
            color: getColor(),
          },
        },
        pointer: { show: false },
        axisLine: {
          lineStyle: {
            width: 12,
            color: [[1, "#27272a"]], // zinc-800
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
          color: "#a1a1aa", // zinc-400
          fontFamily: "Inter, sans-serif",
        },
        detail: {
          offsetCenter: [0, "20%"],
          fontSize: 24,
          fontWeight: "bold" as const,
          fontFamily: "var(--font-mono), JetBrains Mono, monospace",
          color: "#fafafa",
          formatter: (v: number) => v.toFixed(1),
        },
        data: [{ value, name: label }],
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
