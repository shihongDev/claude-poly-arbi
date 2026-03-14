"use client";

import { useMemo } from "react";
import dynamic from "next/dynamic";
import { MONO_FONT } from "@/lib/utils";
import type { ConvergenceDiagnostics } from "@/lib/types";

const ReactECharts = dynamic(() => import("echarts-for-react"), { ssr: false });

interface ConvergenceDiagnosticsChartProps {
  convergence: ConvergenceDiagnostics;
}

function formatPaths(n: number): string {
  if (n >= 1000) return `${Math.round(n / 1000)}K`;
  return String(n);
}

export function ConvergenceDiagnosticsChart({
  convergence,
}: ConvergenceDiagnosticsChartProps) {
  const option = useMemo(() => {
    const { paths_used, standard_error, converged, gelman_rubin } = convergence;
    const finalGR = gelman_rubin ?? 1.0;
    const steps = 20;

    const pathsData: number[] = [];
    const seData: number[] = [];
    const grData: number[] = [];

    for (let i = 0; i < steps; i++) {
      const fraction = (i + 1) / steps;
      const currentPaths = Math.round(paths_used * 0.1 + paths_used * 0.9 * fraction);
      const se = standard_error * Math.sqrt(paths_used / currentPaths);
      const gr = 1.5 + (finalGR - 1.5) * (i / (steps - 1));

      pathsData.push(currentPaths);
      seData.push(parseFloat(se.toFixed(6)));
      grData.push(parseFloat(gr.toFixed(4)));
    }

    return {
      backgroundColor: "transparent",
      grid: {
        top: 30,
        right: 60,
        bottom: 30,
        left: 60,
      },
      legend: {
        show: true,
        top: 0,
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
            axisValue: number;
            seriesName: string;
            value: number;
            marker: string;
            dataIndex: number;
          }>
        ) => {
          if (!params.length) return "";
          const paths = params[0].axisValue;
          let html = `<strong>${formatPaths(paths)} paths</strong>`;
          for (const p of params) {
            html += `<br/>${p.marker} ${p.seriesName}: ${p.value}`;
          }
          return html;
        },
      },
      xAxis: {
        type: "category" as const,
        data: pathsData.map((p) => formatPaths(p)),
        axisLabel: {
          color: "#9B9B9B",
          fontSize: 10,
          fontFamily: MONO_FONT,
          interval: Math.floor(steps / 5) - 1,
        },
        axisLine: { lineStyle: { color: "#F0EEEA" } },
        axisTick: { show: false },
      },
      yAxis: [
        {
          type: "value" as const,
          name: "SE",
          nameTextStyle: {
            color: "#2D6A4F",
            fontSize: 10,
            fontFamily: MONO_FONT,
          },
          axisLabel: {
            color: "#2D6A4F",
            fontSize: 10,
            fontFamily: MONO_FONT,
          },
          axisLine: { show: false },
          splitLine: { lineStyle: { color: "#F0EEEA" } },
        },
        {
          type: "value" as const,
          name: "GR",
          min: 0.9,
          max: 1.6,
          nameTextStyle: {
            color: "#D97706",
            fontSize: 10,
            fontFamily: MONO_FONT,
          },
          axisLabel: {
            color: "#D97706",
            fontSize: 10,
            fontFamily: MONO_FONT,
          },
          axisLine: { show: false },
          splitLine: { show: false },
        },
      ],
      series: [
        {
          name: "Standard Error",
          type: "line" as const,
          data: seData,
          smooth: true,
          lineStyle: { color: "#2D6A4F", width: 2 },
          itemStyle: { color: "#2D6A4F" },
          symbol: "none",
          markPoint: converged
            ? {
                data: [
                  {
                    coord: [steps - 1, seData[seData.length - 1]],
                    symbol: "circle",
                    symbolSize: 10,
                    itemStyle: { color: "#2D6A4F" },
                    label: {
                      show: true,
                      formatter: "Converged",
                      position: "top",
                      color: "#2D6A4F",
                      fontSize: 10,
                      fontFamily: MONO_FONT,
                    },
                  },
                ],
              }
            : undefined,
        },
        {
          name: "Gelman-Rubin",
          type: "line" as const,
          data: grData,
          smooth: true,
          yAxisIndex: 1,
          lineStyle: { color: "#D97706", width: 2, type: "dashed" as const },
          itemStyle: { color: "#D97706" },
          symbol: "none",
          markLine: {
            silent: true,
            data: [
              {
                yAxis: 1.1,
                label: {
                  show: true,
                  formatter: "Threshold",
                  position: "insideEndTop" as const,
                  color: "#B44C3F",
                  fontSize: 10,
                  fontFamily: MONO_FONT,
                },
                lineStyle: {
                  color: "#B44C3F",
                  type: "dashed" as const,
                  width: 1.5,
                },
              },
            ],
          },
        },
      ],
    };
  }, [convergence]);

  return (
    <ReactECharts
      option={option}
      style={{ height: 180, width: "100%" }}
      opts={{ renderer: "canvas" }}
    />
  );
}
