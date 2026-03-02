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
    className: "bg-blue-500/10 text-blue-500 border-blue-500/20",
  },
  CrossMarket: {
    label: "Cross",
    className: "bg-purple-500/10 text-purple-500 border-purple-500/20",
  },
  MultiOutcome: {
    label: "Multi",
    className: "bg-amber-500/10 text-amber-500 border-amber-500/20",
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
        className="cursor-pointer border-b border-zinc-800 bg-zinc-950 transition-colors hover:bg-zinc-800/50"
        onClick={() => setExpanded(!expanded)}
      >
        <td className="px-3 py-3">
          <div className="flex items-center gap-2">
            {expanded ? (
              <ChevronDown className="h-3.5 w-3.5 text-zinc-500" />
            ) : (
              <ChevronRight className="h-3.5 w-3.5 text-zinc-500" />
            )}
            <Badge className={cn("text-xs", config.className)}>
              {config.label}
            </Badge>
          </div>
        </td>
        <td className="px-3 py-3 text-sm text-zinc-300">
          <span className="max-w-[200px] truncate inline-block">
            {opportunity.markets.join(", ")}
          </span>
        </td>
        <td className="px-3 py-3">
          <span
            className={cn(
              "text-sm font-bold",
              netEdge > 0 ? "text-emerald-500" : "text-red-500"
            )}
            style={{ fontFamily: "var(--font-mono)" }}
          >
            {formatBps(opportunity.net_edge)}
          </span>
        </td>
        <td className="px-3 py-3">
          <div className="flex items-center gap-2">
            <div className="h-1.5 w-16 overflow-hidden rounded-full bg-zinc-800">
              <div
                className={cn(
                  "h-full rounded-full transition-all",
                  confidence > 0.8
                    ? "bg-emerald-500"
                    : confidence > 0.5
                      ? "bg-amber-500"
                      : "bg-red-500"
                )}
                style={{ width: `${confidence * 100}%` }}
              />
            </div>
            <span
              className="text-xs text-zinc-400"
              style={{ fontFamily: "var(--font-mono)" }}
            >
              {(confidence * 100).toFixed(0)}%
            </span>
          </div>
        </td>
        <td
          className="px-3 py-3 text-sm text-zinc-300"
          style={{ fontFamily: "var(--font-mono)" }}
        >
          {formatUsd(opportunity.size_available)}
        </td>
        <td className="px-3 py-3 text-xs text-zinc-500">
          {timeAgo(opportunity.detected_at)}
        </td>
      </tr>

      {expanded && (
        <tr className="border-b border-zinc-800 bg-zinc-900/50">
          <td colSpan={6} className="px-6 py-4">
            <div className="space-y-3">
              <p className="text-xs font-medium uppercase tracking-wider text-zinc-400">
                Trade Legs
              </p>
              <div className="grid gap-2">
                {opportunity.legs.map((leg, i) => (
                  <div
                    key={i}
                    className="flex items-center gap-4 rounded-lg border border-zinc-800 bg-zinc-950 px-4 py-2.5"
                  >
                    <Badge
                      className={cn(
                        "text-xs",
                        leg.side === "Buy"
                          ? "bg-emerald-500/10 text-emerald-500"
                          : "bg-red-500/10 text-red-500"
                      )}
                    >
                      {leg.side}
                    </Badge>
                    <span className="text-xs text-zinc-500 font-mono" style={{ fontFamily: "var(--font-mono)" }}>
                      {leg.token_id.slice(0, 10)}...
                    </span>
                    <div className="flex gap-4 text-sm" style={{ fontFamily: "var(--font-mono)" }}>
                      <span className="text-zinc-400">
                        Price: <span className="text-white">{parseFloat(leg.target_price).toFixed(4)}</span>
                      </span>
                      <span className="text-zinc-400">
                        Size: <span className="text-white">{formatUsd(leg.target_size)}</span>
                      </span>
                      <span className="text-zinc-400">
                        VWAP: <span className="text-white">{parseFloat(leg.vwap_estimate).toFixed(4)}</span>
                      </span>
                    </div>
                  </div>
                ))}
              </div>
              <div className="flex gap-6 text-sm" style={{ fontFamily: "var(--font-mono)" }}>
                <span className="text-zinc-400">
                  Gross Edge: <span className="text-white">{formatBps(opportunity.gross_edge)}</span>
                </span>
                <span className="text-zinc-400">
                  ID: <span className="text-zinc-500">{opportunity.id.slice(0, 8)}</span>
                </span>
              </div>
            </div>
          </td>
        </tr>
      )}
    </>
  );
}
