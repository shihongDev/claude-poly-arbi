"use client";

import { useState } from "react";
import { Badge } from "@/components/ui/badge";
import { cn } from "@/lib/utils";
import { formatBps, formatUsd, timeAgo } from "@/lib/utils";
import { ChevronDown, ChevronRight } from "lucide-react";
import type { Opportunity, ArbType } from "@/lib/types";

const arbTypeConfig: Record<ArbType, { label: string; className: string }> = {
  IntraMarket: {
    label: "Intra",
    className: "bg-blue-50 text-blue-600",
  },
  CrossMarket: {
    label: "Cross",
    className: "bg-purple-50 text-purple-600",
  },
  MultiOutcome: {
    label: "Multi",
    className: "bg-amber-50 text-amber-600",
  },
};

interface OpportunityRowProps {
  opportunity: Opportunity;
}

export function OpportunityRow({ opportunity }: OpportunityRowProps) {
  const [expanded, setExpanded] = useState(false);
  const config = arbTypeConfig[opportunity.arb_type];
  const netEdge = parseFloat(opportunity.net_edge);
  const confidence = opportunity.confidence;

  return (
    <>
      <tr
        className="cursor-pointer transition-colors hover:bg-[#F8F7F4]"
        onClick={() => setExpanded(!expanded)}
      >
        <td className="px-3 py-3">
          <div className="flex items-center gap-2">
            {expanded ? (
              <ChevronDown className="h-3.5 w-3.5 text-[#9B9B9B]" />
            ) : (
              <ChevronRight className="h-3.5 w-3.5 text-[#9B9B9B]" />
            )}
            <Badge className={cn("text-xs", config.className)}>
              {config.label}
            </Badge>
          </div>
        </td>
        <td className="px-3 py-3 text-sm text-[#1A1A19]">
          <span className="max-w-[200px] truncate inline-block">
            {opportunity.markets.join(", ")}
          </span>
        </td>
        <td className="px-3 py-3">
          <span
            className={cn(
              "text-sm font-bold",
              netEdge > 0 ? "text-[#2D6A4F]" : "text-[#B44C3F]"
            )}
            style={{ fontFamily: "var(--font-jetbrains-mono)" }}
          >
            {formatBps(opportunity.net_edge)}
          </span>
        </td>
        <td className="px-3 py-3">
          <div className="flex items-center gap-2">
            <div className="h-1.5 w-16 overflow-hidden rounded-full bg-[#E6E4DF]">
              <div
                className={cn(
                  "h-full rounded-full transition-all",
                  confidence > 0.8
                    ? "bg-[#2D6A4F]"
                    : confidence > 0.5
                      ? "bg-amber-500"
                      : "bg-[#B44C3F]"
                )}
                style={{ width: `${confidence * 100}%` }}
              />
            </div>
            <span
              className="text-xs text-[#6B6B6B]"
              style={{ fontFamily: "var(--font-jetbrains-mono)" }}
            >
              {(confidence * 100).toFixed(0)}%
            </span>
          </div>
        </td>
        <td
          className="px-3 py-3 text-sm text-[#1A1A19]"
          style={{ fontFamily: "var(--font-jetbrains-mono)" }}
        >
          {formatUsd(opportunity.size_available)}
        </td>
        <td className="px-3 py-3 text-xs text-[#9B9B9B]">
          {timeAgo(opportunity.detected_at)}
        </td>
      </tr>

      {expanded && (
        <tr className="bg-[#F8F7F4]">
          <td colSpan={6} className="px-6 py-4">
            <div className="space-y-3">
              <p className="text-[11px] font-medium uppercase tracking-wider text-[#9B9B9B]">
                Trade Legs
              </p>
              <div className="grid gap-2">
                {opportunity.legs.map((leg, i) => (
                  <div
                    key={i}
                    className="flex items-center gap-4 rounded-[10px] border border-[#E6E4DF] bg-white px-4 py-2.5"
                  >
                    <Badge
                      className={cn(
                        "text-xs",
                        leg.side === "Buy"
                          ? "bg-[#DAE9E0] text-[#2D6A4F]"
                          : "bg-[#F5E0DD] text-[#B44C3F]"
                      )}
                    >
                      {leg.side}
                    </Badge>
                    <span className="text-xs text-[#9B9B9B]" style={{ fontFamily: "var(--font-jetbrains-mono)" }}>
                      {leg.token_id.slice(0, 10)}...
                    </span>
                    <div className="flex gap-4 text-sm" style={{ fontFamily: "var(--font-jetbrains-mono)" }}>
                      <span className="text-[#6B6B6B]">
                        Price: <span className="text-[#1A1A19]">{parseFloat(leg.target_price).toFixed(4)}</span>
                      </span>
                      <span className="text-[#6B6B6B]">
                        Size: <span className="text-[#1A1A19]">{formatUsd(leg.target_size)}</span>
                      </span>
                      <span className="text-[#6B6B6B]">
                        VWAP: <span className="text-[#1A1A19]">{parseFloat(leg.vwap_estimate).toFixed(4)}</span>
                      </span>
                    </div>
                  </div>
                ))}
              </div>
              <div className="flex gap-6 text-sm" style={{ fontFamily: "var(--font-jetbrains-mono)" }}>
                <span className="text-[#6B6B6B]">
                  Gross Edge: <span className="text-[#1A1A19]">{formatBps(opportunity.gross_edge)}</span>
                </span>
                <span className="text-[#6B6B6B]">
                  ID: <span className="text-[#9B9B9B]">{opportunity.id.slice(0, 8)}</span>
                </span>
              </div>
            </div>
          </td>
        </tr>
      )}
    </>
  );
}
