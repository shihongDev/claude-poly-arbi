"use client";

import { memo, useMemo } from "react";
import dynamic from "next/dynamic";
import { MONO_FONT, spreadSeverity } from "@/lib/utils";

const ReactECharts = dynamic(() => import("echarts-for-react"), { ssr: false });

interface SpreadSeverityGaugeProps {
  bps: number | null;
}

export const SpreadSeverityGauge = memo(function SpreadSeverityGauge({
  bps,
}: SpreadSeverityGaugeProps) {
  const option = useMemo(() => {
    const displayValue = bps ?? 0;
    const severity = spreadSeverity(bps);
    const color =
      severity === "good"
        ? "#2D6A4F"
        : severity === "warning"
          ? "#D97706"
          : severity === "danger"
            ? "#B44C3F"
            : "#9B9B9B";

    return {
      backgroundColor: "transparent",
      series: [
        {
          type: "gauge" as const,
          startAngle: 180,
          endAngle: 0,
          min: 0,
          max: 500,
          radius: "100%",
          center: ["50%", "75%"],
          progress: {
            show: true,
            width: 8,
            roundCap: true,
            itemStyle: { color },
          },
          pointer: { show: false },
          axisLine: {
            lineStyle: {
              width: 8,
              color: [[1, "#E6E4DF"]],
            },
            roundCap: true,
          },
          axisTick: { show: false },
          splitLine: { show: false },
          axisLabel: { show: false },
          title: { show: false },
          detail: {
            offsetCenter: [0, "-10%"],
            fontSize: 12,
            fontWeight: "bold" as const,
            fontFamily: MONO_FONT,
            color: "#1A1A19",
            formatter: (v: number) => `${Math.round(v)}`,
          },
          data: [{ value: Math.min(displayValue, 500) }],
        },
      ],
    };
  }, [bps]);

  return (
    <ReactECharts
      option={option}
      style={{ height: "60px", width: "100%" }}
      opts={{ renderer: "canvas" }}
    />
  );
});
