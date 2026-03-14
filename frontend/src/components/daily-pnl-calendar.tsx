"use client";

import { useMemo } from "react";
import dynamic from "next/dynamic";
import { MONO_FONT } from "@/lib/utils";
import type { ExecutionReport } from "@/lib/types";

const ReactECharts = dynamic(() => import("echarts-for-react"), { ssr: false });

interface DailyPnlCalendarProps {
  history: ExecutionReport[];
}

interface DayEntry {
  pnl: number;
  trades: number;
}

export function DailyPnlCalendar({ history }: DailyPnlCalendarProps) {
  const dailyData = useMemo(() => {
    const byDate = new Map<string, DayEntry>();

    for (const entry of history) {
      const date = entry.timestamp.slice(0, 10);
      const pnl = parseFloat(entry.realized_edge) - parseFloat(entry.total_fees);
      if (isNaN(pnl)) continue;

      const existing = byDate.get(date);
      if (existing) {
        existing.pnl += pnl;
        existing.trades += 1;
      } else {
        byDate.set(date, { pnl, trades: 1 });
      }
    }

    return byDate;
  }, [history]);

  const { calendarData, dateRange, maxAbsPnl } = useMemo(() => {
    if (dailyData.size === 0) {
      return { calendarData: [], dateRange: null, maxAbsPnl: 0 };
    }

    const dates = Array.from(dailyData.keys()).sort();
    const startDate = dates[0];
    const endDate = dates[dates.length - 1];

    let maxAbs = 0;
    const data: [string, number][] = [];

    for (const [date, entry] of dailyData) {
      data.push([date, entry.pnl]);
      const abs = Math.abs(entry.pnl);
      if (abs > maxAbs) maxAbs = abs;
    }

    return {
      calendarData: data,
      dateRange: [startDate, endDate] as [string, string],
      maxAbsPnl: maxAbs || 1,
    };
  }, [dailyData]);

  const option = useMemo(() => {
    if (!dateRange) return null;

    return {
      backgroundColor: "transparent",
      tooltip: {
        backgroundColor: "#FFFFFF",
        borderColor: "#E6E4DF",
        borderWidth: 1,
        textStyle: {
          color: "#1A1A19",
          fontFamily: MONO_FONT,
          fontSize: 12,
        },
        formatter: (params: { value: [string, number] }) => {
          const [dateStr, pnl] = params.value;
          const d = new Date(dateStr + "T00:00:00");
          const formatted = d.toLocaleDateString("en-US", {
            weekday: "short",
            month: "short",
            day: "numeric",
            year: "numeric",
          });
          const entry = dailyData.get(dateStr);
          const trades = entry?.trades ?? 0;
          const sign = pnl >= 0 ? "+" : "-";
          const color = pnl >= 0 ? "#2D6A4F" : "#B44C3F";
          const absVal = Math.abs(pnl).toFixed(2);

          return [
            `<div style="font-family:${MONO_FONT}">`,
            `<div style="font-size:11px;color:#6B6B6B;margin-bottom:4px">${formatted}</div>`,
            `<div style="font-size:14px;font-weight:600;color:${color}">${sign}$${absVal}</div>`,
            `<div style="font-size:11px;color:#9B9B9B;margin-top:2px">${trades} trade${trades !== 1 ? "s" : ""}</div>`,
            `</div>`,
          ].join("");
        },
      },
      visualMap: {
        type: "continuous" as const,
        min: -maxAbsPnl,
        max: maxAbsPnl,
        inRange: {
          color: ["#B44C3F", "#F0EEEA", "#2D6A4F"],
        },
        orient: "horizontal" as const,
        left: "center",
        bottom: 0,
        text: ["Profit", "Loss"],
        textStyle: {
          color: "#6B6B6B",
          fontSize: 10,
          fontFamily: MONO_FONT,
        },
        itemWidth: 200,
        itemHeight: 10,
        show: true,
      },
      calendar: {
        range: dateRange,
        cellSize: ["auto", 16] as [string, number],
        top: 30,
        left: 40,
        right: 20,
        bottom: 40,
        itemStyle: {
          borderWidth: 2,
          borderColor: "#F8F7F4",
        },
        dayLabel: {
          nameMap: "en",
          fontSize: 10,
          color: "#9B9B9B",
          fontFamily: MONO_FONT,
        },
        monthLabel: {
          nameMap: "en",
          fontSize: 10,
          color: "#6B6B6B",
          fontFamily: MONO_FONT,
        },
        yearLabel: {
          show: false,
        },
        splitLine: {
          show: false,
        },
      },
      series: [
        {
          type: "heatmap" as const,
          coordinateSystem: "calendar" as const,
          data: calendarData,
        },
      ],
    };
  }, [dateRange, calendarData, maxAbsPnl, dailyData]);

  if (history.length === 0 || !option) {
    return (
      <div className="rounded-2xl bg-white p-5">
        <h2 className="text-[11px] font-medium uppercase tracking-wider text-[#9B9B9B]">
          Daily P&L Calendar
        </h2>
        <div
          className="flex items-center justify-center text-sm text-[#9B9B9B]"
          style={{ height: 200 }}
        >
          Awaiting data...
        </div>
      </div>
    );
  }

  return (
    <div className="rounded-2xl bg-white p-5">
      <h2 className="text-[11px] font-medium uppercase tracking-wider text-[#9B9B9B]">
        Daily P&L Calendar
      </h2>
      <div style={{ height: 200 }}>
        <ReactECharts
          option={option}
          style={{ height: "100%", width: "100%" }}
          opts={{ renderer: "canvas" }}
        />
      </div>
    </div>
  );
}
