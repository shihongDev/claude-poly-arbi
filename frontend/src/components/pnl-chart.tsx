"use client";

import { useEffect, useRef } from "react";
import {
  createChart,
  AreaSeries,
  type IChartApi,
  type ISeriesApi,
  type AreaData,
  type Time,
  ColorType,
  LineType,
  CrosshairMode,
} from "lightweight-charts";

interface PnlChartProps {
  data: { time: string; value: number }[];
}

export function PnlChart({ data }: PnlChartProps) {
  const containerRef = useRef<HTMLDivElement>(null);
  const chartRef = useRef<IChartApi | null>(null);
  const seriesRef = useRef<ISeriesApi<"Area"> | null>(null);

  useEffect(() => {
    if (!containerRef.current) return;

    const chart = createChart(containerRef.current, {
      layout: {
        background: { type: ColorType.Solid, color: "transparent" },
        textColor: "#a1a1aa",
        fontFamily: "var(--font-mono), monospace",
        fontSize: 11,
      },
      grid: {
        vertLines: { color: "#27272a" },
        horzLines: { color: "#27272a" },
      },
      crosshair: {
        mode: CrosshairMode.Normal,
        vertLine: { color: "#3f3f46", labelBackgroundColor: "#18181b" },
        horzLine: { color: "#3f3f46", labelBackgroundColor: "#18181b" },
      },
      rightPriceScale: {
        borderColor: "#3f3f46",
      },
      timeScale: {
        borderColor: "#3f3f46",
        timeVisible: true,
      },
      handleScale: true,
      handleScroll: true,
    });

    chartRef.current = chart;

    const series = chart.addSeries(AreaSeries, {
      lineColor: "#10b981",
      lineWidth: 2,
      lineType: LineType.Curved,
      topColor: "rgba(16, 185, 129, 0.3)",
      bottomColor: "rgba(16, 185, 129, 0.02)",
      crosshairMarkerBackgroundColor: "#10b981",
      priceFormat: {
        type: "price",
        precision: 2,
        minMove: 0.01,
      },
    });

    seriesRef.current = series;

    const resizeObserver = new ResizeObserver((entries) => {
      for (const entry of entries) {
        const { width, height } = entry.contentRect;
        chart.resize(width, height);
      }
    });

    resizeObserver.observe(containerRef.current);

    return () => {
      resizeObserver.disconnect();
      chart.remove();
    };
  }, []);

  useEffect(() => {
    if (!seriesRef.current || data.length === 0) return;

    const chartData: AreaData<Time>[] = data.map((d) => ({
      time: d.time as Time,
      value: d.value,
    }));

    seriesRef.current.setData(chartData);

    // Color based on whether final value is positive or negative
    const lastValue = data[data.length - 1]?.value ?? 0;
    const isPositive = lastValue >= 0;

    seriesRef.current.applyOptions({
      lineColor: isPositive ? "#10b981" : "#ef4444",
      topColor: isPositive
        ? "rgba(16, 185, 129, 0.3)"
        : "rgba(239, 68, 68, 0.3)",
      bottomColor: isPositive
        ? "rgba(16, 185, 129, 0.02)"
        : "rgba(239, 68, 68, 0.02)",
      crosshairMarkerBackgroundColor: isPositive ? "#10b981" : "#ef4444",
    });

    chartRef.current?.timeScale().fitContent();
  }, [data]);

  return (
    <div
      ref={containerRef}
      className="h-full w-full"
      style={{ minHeight: 200 }}
    />
  );
}
