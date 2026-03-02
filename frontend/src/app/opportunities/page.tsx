"use client";

import { useState, useMemo } from "react";
import { useDashboardStore } from "@/store";
import { DataTable, type Column } from "@/components/data-table";
import { Badge } from "@/components/ui/badge";
import { Input } from "@/components/ui/input";
import { Label } from "@/components/ui/label";
import {
  Select,
  SelectContent,
  SelectItem,
  SelectTrigger,
  SelectValue,
} from "@/components/ui/select";
import {
  Sheet,
  SheetContent,
  SheetHeader,
  SheetTitle,
  SheetDescription,
} from "@/components/ui/sheet";
import { ScrollArea } from "@/components/ui/scroll-area";
import { cn, formatBps, formatUsd, formatDecimal, timeAgo } from "@/lib/utils";
import { SearchX } from "lucide-react";
import type { Opportunity, ArbType } from "@/lib/types";

const arbTypeConfig: Record<ArbType, { label: string; className: string }> = {
  IntraMarket: {
    label: "Intra-Market",
    className: "bg-blue-500/10 text-blue-500 border-blue-500/20",
  },
  CrossMarket: {
    label: "Cross-Market",
    className: "bg-purple-500/10 text-purple-500 border-purple-500/20",
  },
  MultiOutcome: {
    label: "Multi-Outcome",
    className: "bg-amber-500/10 text-amber-500 border-amber-500/20",
  },
};

type ArbTypeFilter = "All" | ArbType;

export default function OpportunitiesPage() {
  const opportunities = useDashboardStore((s) => s.opportunities);

  const [typeFilter, setTypeFilter] = useState<ArbTypeFilter>("All");
  const [minEdge, setMinEdge] = useState("");
  const [minConfidence, setMinConfidence] = useState(0);
  const [selectedOpp, setSelectedOpp] = useState<Opportunity | null>(null);

  const filtered = useMemo(() => {
    let result = opportunities;

    if (typeFilter !== "All") {
      result = result.filter((o) => o.arb_type === typeFilter);
    }

    const minEdgeNum = parseFloat(minEdge);
    if (!isNaN(minEdgeNum) && minEdgeNum > 0) {
      // minEdge is in bps, net_edge is a decimal (e.g. 0.005 = 50 bps)
      const minEdgeDecimal = minEdgeNum / 10000;
      result = result.filter(
        (o) => parseFloat(o.net_edge) >= minEdgeDecimal
      );
    }

    if (minConfidence > 0) {
      const threshold = minConfidence / 100;
      result = result.filter((o) => o.confidence >= threshold);
    }

    return result;
  }, [opportunities, typeFilter, minEdge, minConfidence]);

  const columns: Column<Opportunity>[] = useMemo(
    () => [
      {
        key: "time",
        header: "Time",
        sortable: true,
        render: (row) => (
          <span className="text-xs text-zinc-500">
            {timeAgo(row.detected_at)}
          </span>
        ),
        getValue: (row) => new Date(row.detected_at).getTime(),
      },
      {
        key: "type",
        header: "Type",
        render: (row) => {
          const config = arbTypeConfig[row.arb_type];
          return (
            <Badge className={cn("text-xs", config.className)}>
              {config.label}
            </Badge>
          );
        },
      },
      {
        key: "markets",
        header: "Markets",
        render: (row) => (
          <span
            className="inline-block max-w-[180px] truncate text-sm text-zinc-300"
            title={row.markets.join(", ")}
          >
            {row.markets
              .map((m) => (m.length > 10 ? `${m.slice(0, 6)}...${m.slice(-4)}` : m))
              .join(", ")}
          </span>
        ),
      },
      {
        key: "gross_edge",
        header: "Gross Edge",
        sortable: true,
        mono: true,
        render: (row) => (
          <span className="text-sm text-zinc-300">
            {formatDecimal(row.gross_edge, 4)}
          </span>
        ),
        getValue: (row) => parseFloat(row.gross_edge),
      },
      {
        key: "net_edge",
        header: "Net Edge (bps)",
        sortable: true,
        mono: true,
        render: (row) => {
          const val = parseFloat(row.net_edge);
          return (
            <span
              className={cn(
                "text-sm font-bold",
                val > 0 ? "text-emerald-500" : "text-red-500"
              )}
            >
              {formatBps(row.net_edge)}
            </span>
          );
        },
        getValue: (row) => parseFloat(row.net_edge),
      },
      {
        key: "confidence",
        header: "Confidence",
        sortable: true,
        render: (row) => (
          <div className="flex items-center gap-2">
            <div className="h-1.5 w-16 overflow-hidden rounded-full bg-zinc-800">
              <div
                className={cn(
                  "h-full rounded-full transition-all",
                  row.confidence > 0.8
                    ? "bg-emerald-500"
                    : row.confidence > 0.5
                      ? "bg-amber-500"
                      : "bg-red-500"
                )}
                style={{ width: `${row.confidence * 100}%` }}
              />
            </div>
            <span
              className="text-xs text-zinc-400"
              style={{ fontFamily: "var(--font-mono)" }}
            >
              {(row.confidence * 100).toFixed(0)}%
            </span>
          </div>
        ),
        getValue: (row) => row.confidence,
      },
      {
        key: "size",
        header: "Size Available",
        sortable: true,
        mono: true,
        render: (row) => (
          <span className="text-sm text-zinc-300">
            {formatUsd(row.size_available)}
          </span>
        ),
        getValue: (row) => parseFloat(row.size_available),
      },
      {
        key: "legs",
        header: "Legs",
        sortable: true,
        mono: true,
        render: (row) => (
          <span className="text-sm text-zinc-400">{row.legs.length}</span>
        ),
        getValue: (row) => row.legs.length,
      },
    ],
    []
  );

  return (
    <div className="space-y-6">
      {/* Header */}
      <div>
        <h1 className="text-2xl font-bold text-white">Opportunities</h1>
        <p className="mt-1 text-sm text-zinc-400">
          {filtered.length === opportunities.length
            ? `${opportunities.length} detected`
            : `${filtered.length} of ${opportunities.length} shown`}
        </p>
      </div>

      {/* Filter Bar */}
      <div className="flex flex-wrap items-end gap-4 rounded-lg border border-zinc-800 bg-zinc-900 p-4">
        {/* Arb Type */}
        <div className="space-y-1.5">
          <Label className="text-xs text-zinc-400">Type</Label>
          <Select
            value={typeFilter}
            onValueChange={(v) => setTypeFilter(v as ArbTypeFilter)}
          >
            <SelectTrigger className="w-[160px] border-zinc-700 bg-zinc-950 text-sm text-zinc-300">
              <SelectValue />
            </SelectTrigger>
            <SelectContent className="border-zinc-700 bg-zinc-900">
              <SelectItem value="All">All Types</SelectItem>
              <SelectItem value="IntraMarket">Intra-Market</SelectItem>
              <SelectItem value="CrossMarket">Cross-Market</SelectItem>
              <SelectItem value="MultiOutcome">Multi-Outcome</SelectItem>
            </SelectContent>
          </Select>
        </div>

        {/* Min Edge */}
        <div className="space-y-1.5">
          <Label className="text-xs text-zinc-400">Min Edge (bps)</Label>
          <Input
            type="number"
            placeholder="0"
            value={minEdge}
            onChange={(e) => setMinEdge(e.target.value)}
            className="w-[100px] border-zinc-700 bg-zinc-950 text-sm text-zinc-300"
            style={{ fontFamily: "var(--font-mono)" }}
            min={0}
          />
        </div>

        {/* Min Confidence */}
        <div className="space-y-1.5">
          <Label className="text-xs text-zinc-400">
            Min Confidence:{" "}
            <span
              className="text-zinc-300"
              style={{ fontFamily: "var(--font-mono)" }}
            >
              {minConfidence}%
            </span>
          </Label>
          <input
            type="range"
            min={0}
            max={100}
            step={5}
            value={minConfidence}
            onChange={(e) => setMinConfidence(parseInt(e.target.value))}
            className="h-9 w-[160px] cursor-pointer accent-emerald-500"
          />
        </div>
      </div>

      {/* Table */}
      <div className="overflow-hidden rounded-lg border border-zinc-800 bg-zinc-900">
        {opportunities.length === 0 ? (
          <div className="flex flex-col items-center justify-center py-20 text-zinc-500">
            <SearchX className="mb-3 h-10 w-10 text-zinc-700" />
            <p className="text-sm">No opportunities detected yet</p>
            <p className="mt-1 text-xs text-zinc-600">
              Opportunities will appear here in real-time
            </p>
          </div>
        ) : filtered.length === 0 ? (
          <div className="flex flex-col items-center justify-center py-20 text-zinc-500">
            <SearchX className="mb-3 h-10 w-10 text-zinc-700" />
            <p className="text-sm">No opportunities match filters</p>
            <p className="mt-1 text-xs text-zinc-600">
              Try adjusting your filter criteria
            </p>
          </div>
        ) : (
          <DataTable
            columns={columns}
            data={filtered}
            pageSize={25}
            onRowClick={(row) => setSelectedOpp(row)}
          />
        )}
      </div>

      {/* Detail Sheet */}
      <Sheet
        open={selectedOpp !== null}
        onOpenChange={(open) => {
          if (!open) setSelectedOpp(null);
        }}
      >
        <SheetContent
          side="right"
          className="w-full border-zinc-800 bg-zinc-950 sm:max-w-lg"
        >
          {selectedOpp && (
            <>
              <SheetHeader>
                <SheetTitle className="text-white">
                  Opportunity Detail
                </SheetTitle>
                <SheetDescription className="text-zinc-500">
                  {selectedOpp.id}
                </SheetDescription>
              </SheetHeader>

              <ScrollArea className="flex-1 px-4">
                <div className="space-y-6 pb-6">
                  {/* Summary Fields */}
                  <div className="grid grid-cols-2 gap-4">
                    <DetailField label="Type">
                      <Badge
                        className={cn(
                          "text-xs",
                          arbTypeConfig[selectedOpp.arb_type].className
                        )}
                      >
                        {arbTypeConfig[selectedOpp.arb_type].label}
                      </Badge>
                    </DetailField>
                    <DetailField label="Detected">
                      {timeAgo(selectedOpp.detected_at)}
                    </DetailField>
                    <DetailField label="Gross Edge" mono>
                      {formatDecimal(selectedOpp.gross_edge, 6)}
                    </DetailField>
                    <DetailField label="Net Edge" mono>
                      <span
                        className={cn(
                          "font-bold",
                          parseFloat(selectedOpp.net_edge) > 0
                            ? "text-emerald-500"
                            : "text-red-500"
                        )}
                      >
                        {formatBps(selectedOpp.net_edge)}
                      </span>
                    </DetailField>
                    <DetailField label="Confidence" mono>
                      {(selectedOpp.confidence * 100).toFixed(1)}%
                    </DetailField>
                    <DetailField label="Size Available" mono>
                      {formatUsd(selectedOpp.size_available)}
                    </DetailField>
                  </div>

                  {/* Markets */}
                  <div className="space-y-2">
                    <p className="text-xs font-medium uppercase tracking-wider text-zinc-400">
                      Markets
                    </p>
                    <div className="space-y-1">
                      {selectedOpp.markets.map((m, i) => (
                        <div
                          key={i}
                          className="rounded border border-zinc-800 bg-zinc-900 px-3 py-2 text-xs text-zinc-300"
                          style={{ fontFamily: "var(--font-mono)" }}
                        >
                          {m}
                        </div>
                      ))}
                    </div>
                  </div>

                  {/* Estimated VWAP */}
                  {selectedOpp.estimated_vwap.length > 0 && (
                    <div className="space-y-2">
                      <p className="text-xs font-medium uppercase tracking-wider text-zinc-400">
                        Estimated VWAP
                      </p>
                      <div className="flex flex-wrap gap-2">
                        {selectedOpp.estimated_vwap.map((v, i) => (
                          <span
                            key={i}
                            className="rounded border border-zinc-800 bg-zinc-900 px-2 py-1 text-xs text-zinc-300"
                            style={{ fontFamily: "var(--font-mono)" }}
                          >
                            {formatDecimal(v, 6)}
                          </span>
                        ))}
                      </div>
                    </div>
                  )}

                  {/* Legs Table */}
                  <div className="space-y-2">
                    <p className="text-xs font-medium uppercase tracking-wider text-zinc-400">
                      Trade Legs ({selectedOpp.legs.length})
                    </p>
                    <div className="space-y-2">
                      {selectedOpp.legs.map((leg, i) => (
                        <div
                          key={i}
                          className="rounded-lg border border-zinc-800 bg-zinc-900 p-3"
                        >
                          <div className="mb-2 flex items-center gap-3">
                            <Badge
                              className={cn(
                                "text-xs",
                                leg.side === "Buy"
                                  ? "bg-emerald-500/10 text-emerald-500 border-emerald-500/20"
                                  : "bg-red-500/10 text-red-500 border-red-500/20"
                              )}
                            >
                              {leg.side}
                            </Badge>
                            <span
                              className="text-xs text-zinc-500"
                              style={{ fontFamily: "var(--font-mono)" }}
                              title={leg.token_id}
                            >
                              {leg.token_id.length > 16
                                ? `${leg.token_id.slice(0, 8)}...${leg.token_id.slice(-6)}`
                                : leg.token_id}
                            </span>
                          </div>
                          <div
                            className="grid grid-cols-3 gap-3 text-xs"
                            style={{ fontFamily: "var(--font-mono)" }}
                          >
                            <div>
                              <span className="text-zinc-500">Price</span>
                              <p className="mt-0.5 text-zinc-200">
                                {parseFloat(leg.target_price).toFixed(4)}
                              </p>
                            </div>
                            <div>
                              <span className="text-zinc-500">Size</span>
                              <p className="mt-0.5 text-zinc-200">
                                {formatUsd(leg.target_size)}
                              </p>
                            </div>
                            <div>
                              <span className="text-zinc-500">VWAP Est.</span>
                              <p className="mt-0.5 text-zinc-200">
                                {parseFloat(leg.vwap_estimate).toFixed(4)}
                              </p>
                            </div>
                          </div>
                        </div>
                      ))}
                    </div>
                  </div>
                </div>
              </ScrollArea>
            </>
          )}
        </SheetContent>
      </Sheet>
    </div>
  );
}

function DetailField({
  label,
  mono,
  children,
}: {
  label: string;
  mono?: boolean;
  children: React.ReactNode;
}) {
  return (
    <div>
      <p className="text-xs text-zinc-500">{label}</p>
      <div
        className={cn("mt-0.5 text-sm text-zinc-200", mono && "font-mono")}
        style={mono ? { fontFamily: "var(--font-mono)" } : undefined}
      >
        {children}
      </div>
    </div>
  );
}
