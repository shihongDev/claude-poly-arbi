"use client";

import { memo, useEffect, useRef } from "react";
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

export const PnlChart = memo(function PnlChart({ data }: PnlChartProps) {
  const containerRef = useRef<HTMLDivElement>(null);
  const chartRef = useRef<IChartApi | null>(null);
  const seriesRef = useRef<ISeriesApi<"Area"> | null>(null);

  useEffect(() => {
    if (!containerRef.current) return;

    const chart = createChart(containerRef.current, {
      layout: {
        background: { type: ColorType.Solid, color: "transparent" },
        textColor: "#6B6B6B",
        fontFamily: "var(--font-jetbrains-mono), monospace",
        fontSize: 11,
      },
      grid: {
        vertLines: { color: "#F0EEEA" },
        horzLines: { color: "#F0EEEA" },
      },
      crosshair: {
        mode: CrosshairMode.Normal,
        vertLine: { color: "#E6E4DF", labelBackgroundColor: "#FFFFFF" },
        horzLine: { color: "#E6E4DF", labelBackgroundColor: "#FFFFFF" },
      },
      rightPriceScale: {
        borderColor: "#E6E4DF",
      },
      timeScale: {
        borderColor: "#E6E4DF",
        timeVisible: true,
      },
      handleScale: true,
      handleScroll: true,
    });

    chartRef.current = chart;

    const series = chart.addSeries(AreaSeries, {
      lineColor: "#2D6A4F",
      lineWidth: 2,
      lineType: LineType.Curved,
      topColor: "rgba(45, 106, 79, 0.15)",
      bottomColor: "rgba(45, 106, 79, 0.01)",
      crosshairMarkerBackgroundColor: "#2D6A4F",
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

    // Deduplicate by time (keep last value per timestamp) — chart requires strictly ascending times
    const byTime = new Map<string, number>();
    for (const d of data) {
      byTime.set(d.time, d.value);
    }
    const chartData: AreaData<Time>[] = Array.from(byTime, ([time, value]) => ({
      time: time as Time,
      value,
    }));

    seriesRef.current.setData(chartData);

    const lastValue = data[data.length - 1]?.value ?? 0;
    const isPositive = lastValue >= 0;

    seriesRef.current.applyOptions({
      lineColor: isPositive ? "#2D6A4F" : "#B44C3F",
      topColor: isPositive
        ? "rgba(45, 106, 79, 0.15)"
        : "rgba(180, 76, 63, 0.15)",
      bottomColor: isPositive
        ? "rgba(45, 106, 79, 0.01)"
        : "rgba(180, 76, 63, 0.01)",
      crosshairMarkerBackgroundColor: isPositive ? "#2D6A4F" : "#B44C3F",
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
});
