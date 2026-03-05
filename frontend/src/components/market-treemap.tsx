"use client";

import { memo, useMemo, useCallback } from "react";
import dynamic from "next/dynamic";
import { cn, truncate, deriveGroupTitle, MONO_FONT } from "@/lib/utils";
import type { MarketState } from "@/lib/types";

const ReactECharts = dynamic(() => import("echarts-for-react"), { ssr: false });

interface MarketTreemapProps {
  markets: MarketState[];
  onMarketClick?: (conditionId: string) => void;
}

function changeColor(change: number): string {
  if (change > 5) return "#166534";
  if (change > 2) return "#2D6A4F";
  if (change > 0.5) return "#86EFAC";
  if (change >= -0.5) return "#D4D4D4";
  if (change >= -2) return "#FCA5A5";
  if (change >= -5) return "#B44C3F";
  return "#7F1D1D";
}

function deriveGroupName(questions: string[], eventId: string): string {
  if (questions.length === 0) return "Event " + eventId.slice(0, 4);
  const title = deriveGroupTitle(questions, 10);
  return truncate(title, 50);
}

export const MarketTreemap = memo(function MarketTreemap({
  markets,
  onMarketClick,
}: MarketTreemapProps) {
  const option = useMemo(() => {
    if (markets.length < 3) return null;

    // Group markets by event_id
    const groups = new Map<string, MarketState[]>();
    for (const m of markets) {
      const key = m.event_id || "__other__";
      if (!groups.has(key)) groups.set(key, []);
      groups.get(key)!.push(m);
    }

    const treeData = Array.from(groups.entries()).map(([eventId, members]) => {
      const groupName =
        eventId === "__other__"
          ? "Other"
          : deriveGroupName(
              members.map((m) => m.question),
              eventId
            );

      const children = members.map((m) => {
        const volume = parseFloat(m.volume_24hr || "0") || 1;
        const changePct = m.one_day_price_change
          ? parseFloat(m.one_day_price_change) * 100
          : 0;

        return {
          name: truncate(m.question, 40),
          value: volume,
          conditionId: m.condition_id,
          question: m.question,
          price: m.outcome_prices?.[0] ?? null,
          changePct,
          volume: m.volume_24hr,
          spread: m.spread,
          itemStyle: {
            color: changeColor(changePct),
          },
        };
      });

      return {
        name: groupName,
        children,
      };
    });

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
        formatter: (params: {
          data?: {
            question?: string;
            price?: string | null;
            changePct?: number;
            volume?: string | null;
            spread?: string | null;
          };
        }) => {
          const d = params.data;
          if (!d || !d.question) return "";
          const price = d.price ? parseFloat(d.price).toFixed(3) : "\u2014";
          const change =
            d.changePct !== undefined
              ? `${d.changePct >= 0 ? "+" : ""}${d.changePct.toFixed(1)}%`
              : "\u2014";
          const vol = d.volume
            ? `$${parseFloat(d.volume).toLocaleString(undefined, { maximumFractionDigits: 0 })}`
            : "\u2014";
          const spr = d.spread
            ? `${(parseFloat(d.spread) * 10000).toFixed(0)} bps`
            : "\u2014";
          return [
            `<div style="max-width:300px;white-space:normal;font-size:12px">`,
            `<strong>${d.question}</strong>`,
            `<br/>Price: ${price}`,
            `<br/>24h Change: ${change}`,
            `<br/>Volume: ${vol}`,
            `<br/>Spread: ${spr}`,
            `</div>`,
          ].join("");
        },
      },
      series: [
        {
          type: "treemap",
          roam: false,
          nodeClick: false,
          breadcrumb: { show: false },
          width: "100%",
          height: "100%",
          top: 0,
          left: 0,
          right: 0,
          bottom: 0,
          levels: [
            {
              // Level 0: parent (event groups)
              itemStyle: {
                borderColor: "#E6E4DF",
                borderWidth: 2,
                gapWidth: 2,
              },
              upperLabel: {
                show: true,
                height: 20,
                fontSize: 11,
                color: "#6B6B6B",
                fontFamily: "Space Grotesk, sans-serif",
                padding: [2, 6, 0, 6],
              },
            },
            {
              // Level 1: leaves (individual markets)
              itemStyle: {
                borderColor: "#F8F7F4",
                borderWidth: 1,
              },
              label: {
                show: true,
                fontSize: 10,
                color: "#1A1A19",
                fontFamily: MONO_FONT,
                formatter: (params: { name: string }) => truncate(params.name, 40),
              },
            },
          ],
          data: treeData,
        },
      ],
    };
  }, [markets]);

  const onEvents = useMemo((): Record<string, Function> | undefined => {
    if (!onMarketClick) return undefined;
    return {
      click: (params: { data?: { conditionId?: string; children?: unknown } }) => {
        const conditionId = params.data?.conditionId;
        // Only fire for leaf nodes (no children)
        if (conditionId && !params.data?.children) {
          onMarketClick(conditionId);
        }
      },
    };
  }, [onMarketClick]);

  if (markets.length < 3) {
    return (
      <div className="rounded-2xl bg-white overflow-hidden" style={{ width: "100%", height: 500 }}>
        <div className="px-5 pt-5 pb-3">
          <h2 className="text-[11px] font-medium uppercase tracking-wider text-[#9B9B9B]">
            Market Landscape
          </h2>
        </div>
        <div className="flex h-[420px] items-center justify-center text-sm text-[#9B9B9B]">
          Not enough markets for treemap view
        </div>
      </div>
    );
  }

  return (
    <div className="rounded-2xl bg-white overflow-hidden" style={{ width: "100%", height: 500 }}>
      <div className="flex items-center justify-between px-5 pt-5 pb-3">
        <h2 className="text-[11px] font-medium uppercase tracking-wider text-[#9B9B9B]">
          Market Landscape
        </h2>
        <div className="flex items-center gap-3">
          <div className="flex items-center gap-1.5">
            <span
              className="inline-block h-2.5 w-2.5 rounded-sm"
              style={{ backgroundColor: "#2D6A4F" }}
            />
            <span className="text-[10px] text-[#9B9B9B]" style={{ fontFamily: MONO_FONT }}>
              Up
            </span>
          </div>
          <div className="flex items-center gap-1.5">
            <span
              className="inline-block h-2.5 w-2.5 rounded-sm"
              style={{ backgroundColor: "#B44C3F" }}
            />
            <span className="text-[10px] text-[#9B9B9B]" style={{ fontFamily: MONO_FONT }}>
              Down
            </span>
          </div>
          <div className="flex items-center gap-1.5">
            <span
              className="inline-block h-2.5 w-2.5 rounded-sm"
              style={{ backgroundColor: "#D4D4D4" }}
            />
            <span className="text-[10px] text-[#9B9B9B]" style={{ fontFamily: MONO_FONT }}>
              Flat
            </span>
          </div>
        </div>
      </div>
      <div style={{ height: 450 }}>
        <ReactECharts
          option={option!}
          style={{ height: "100%", width: "100%" }}
          opts={{ renderer: "canvas" }}
          onEvents={onEvents}
        />
      </div>
    </div>
  );
});
