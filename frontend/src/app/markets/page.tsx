"use client";

import { useState, useMemo, useCallback } from "react";
import { useRouter } from "next/navigation";
import { Search } from "lucide-react";
import { useDashboardStore } from "@/store";
import { DataTable, type Column } from "@/components/data-table";
import { Badge } from "@/components/ui/badge";
import { Input } from "@/components/ui/input";
import { Switch } from "@/components/ui/switch";
import { Label } from "@/components/ui/label";
import { formatUsd, formatDecimal, cn } from "@/lib/utils";
import type { MarketState } from "@/lib/types";

function truncateQuestion(question: string, max = 60): string {
  if (question.length <= max) return question;
  return question.slice(0, max) + "...";
}

function formatPrices(outcomes: string[], prices: string[]): string {
  return outcomes
    .map((name, i) => {
      const price = prices[i];
      const formatted = price ? parseFloat(price).toFixed(2) : "--";
      return `${name}: ${formatted}`;
    })
    .join(" / ");
}

const MARKET_COLUMNS: Column<MarketState>[] = [
  {
    key: "question",
    header: "Question",
    sortable: false,
    render: (row) => (
      <span className="text-zinc-200" title={row.question}>
        {truncateQuestion(row.question)}
      </span>
    ),
  },
  {
    key: "outcomes",
    header: "Outcomes",
    sortable: false,
    render: (row) => (
      <span className="text-zinc-400 text-xs">
        {row.outcomes.join(", ")}
      </span>
    ),
  },
  {
    key: "prices",
    header: "Prices",
    sortable: false,
    mono: true,
    render: (row) => (
      <span className="text-zinc-300 text-xs">
        {formatPrices(row.outcomes, row.outcome_prices)}
      </span>
    ),
  },
  {
    key: "volume_24hr",
    header: "Volume 24h",
    sortable: true,
    mono: true,
    render: (row) => (
      <span className="text-zinc-300">{formatUsd(row.volume_24hr)}</span>
    ),
    getValue: (row) =>
      row.volume_24hr ? parseFloat(row.volume_24hr) : 0,
  },
  {
    key: "liquidity",
    header: "Liquidity",
    sortable: true,
    mono: true,
    render: (row) => (
      <span className="text-zinc-300">{formatUsd(row.liquidity)}</span>
    ),
    getValue: (row) =>
      row.liquidity ? parseFloat(row.liquidity) : 0,
  },
  {
    key: "active",
    header: "Status",
    sortable: false,
    render: (row) =>
      row.active ? (
        <Badge className="bg-emerald-500/10 text-emerald-500 text-xs">
          Active
        </Badge>
      ) : (
        <Badge className="bg-zinc-500/10 text-zinc-500 text-xs">
          Inactive
        </Badge>
      ),
  },
];

export default function MarketsPage() {
  const markets = useDashboardStore((s) => s.markets);
  const router = useRouter();

  const [search, setSearch] = useState("");
  const [activeOnly, setActiveOnly] = useState(true);

  const filteredMarkets = useMemo(() => {
    let result = markets;

    if (activeOnly) {
      result = result.filter((m) => m.active);
    }

    if (search.trim()) {
      const query = search.trim().toLowerCase();
      result = result.filter((m) =>
        m.question.toLowerCase().includes(query)
      );
    }

    return result;
  }, [markets, search, activeOnly]);

  const handleRowClick = useCallback(
    (market: MarketState) => {
      router.push(`/markets/${market.condition_id}`);
    },
    [router]
  );

  return (
    <div className="space-y-6">
      {/* Header */}
      <div>
        <h1 className="text-2xl font-bold text-white">Markets</h1>
        <p className="mt-1 text-sm text-zinc-400">
          Browse and inspect Polymarket prediction markets
        </p>
      </div>

      {/* Filters row */}
      <div className="flex flex-col gap-4 sm:flex-row sm:items-center sm:justify-between">
        <div className="relative max-w-sm flex-1">
          <Search className="absolute left-3 top-1/2 h-4 w-4 -translate-y-1/2 text-zinc-500" />
          <Input
            placeholder="Search markets..."
            value={search}
            onChange={(e) => setSearch(e.target.value)}
            className="border-zinc-800 bg-zinc-900 pl-9 text-zinc-200 placeholder:text-zinc-600"
          />
        </div>
        <div className="flex items-center gap-4">
          <div className="flex items-center gap-2">
            <Switch
              id="active-only"
              checked={activeOnly}
              onCheckedChange={setActiveOnly}
            />
            <Label
              htmlFor="active-only"
              className="text-sm text-zinc-400 cursor-pointer"
            >
              Active only
            </Label>
          </div>
          <span className="text-xs text-zinc-500" style={{ fontFamily: "var(--font-mono)" }}>
            {filteredMarkets.length} of {markets.length} markets
          </span>
        </div>
      </div>

      {/* Markets table */}
      <div className="rounded-lg border border-zinc-800 bg-zinc-900">
        {markets.length === 0 ? (
          <div className="flex h-[300px] items-center justify-center text-sm text-zinc-600">
            No markets loaded
          </div>
        ) : (
          <DataTable
            columns={MARKET_COLUMNS}
            data={filteredMarkets}
            pageSize={20}
            onRowClick={handleRowClick}
          />
        )}
      </div>
    </div>
  );
}
