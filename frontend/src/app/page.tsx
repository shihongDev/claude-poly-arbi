"use client";

import { useState, useMemo, useCallback } from "react";
import { useRouter } from "next/navigation";
import {
  Search,
  ArrowUpRight,
  ArrowDownRight,
  BarChart3,
  Activity,
  Target,
  TrendingUp,
} from "lucide-react";
import { useDashboardStore } from "@/store";
import { DataTable, type Column } from "@/components/data-table";
import { Badge } from "@/components/ui/badge";
import { Input } from "@/components/ui/input";
import { Switch } from "@/components/ui/switch";
import { Label } from "@/components/ui/label";
import {
  Select,
  SelectContent,
  SelectItem,
  SelectTrigger,
  SelectValue,
} from "@/components/ui/select";
import {
  formatUsd,
  formatSpreadBps,
  formatPriceChange,
  formatEndDate,
  cn,
} from "@/lib/utils";
import type { MarketState } from "@/lib/types";

function truncateQuestion(question: string, max = 55): string {
  if (question.length <= max) return question;
  return question.slice(0, max) + "\u2026";
}

type SortField = "volume" | "liquidity" | "spread" | "change";

const MIN_VOLUME_OPTIONS = [
  { value: "0", label: "Any" },
  { value: "1000", label: "$1K+" },
  { value: "10000", label: "$10K+" },
  { value: "100000", label: "$100K+" },
  { value: "1000000", label: "$1M+" },
];

const SORT_OPTIONS: { value: SortField; label: string }[] = [
  { value: "volume", label: "Volume" },
  { value: "liquidity", label: "Liquidity" },
  { value: "spread", label: "Spread" },
  { value: "change", label: "24h Change" },
];

interface EnrichedMarket extends MarketState {
  oppCount: number;
  positionSize: number;
}

function StatCard({
  label,
  value,
  icon: Icon,
}: {
  label: string;
  value: string | number;
  icon: React.ComponentType<{ className?: string }>;
}) {
  return (
    <div className="rounded-[10px] border border-[#E6E4DF] bg-[#F8F7F4] px-4 py-3">
      <div className="flex items-center gap-2">
        <Icon className="h-3.5 w-3.5 text-[#9B9B9B]" />
        <p className="text-[11px] font-medium uppercase tracking-wider text-[#9B9B9B]">
          {label}
        </p>
      </div>
      <p
        className="mt-1 text-lg font-semibold text-[#1A1A19]"
        style={{ fontFamily: "var(--font-jetbrains-mono)" }}
      >
        {value}
      </p>
    </div>
  );
}

const MARKET_COLUMNS: Column<EnrichedMarket>[] = [
  {
    key: "question",
    header: "Question",
    sortable: false,
    render: (row) => (
      <div className="flex items-center gap-2">
        {row.oppCount > 0 && (
          <span className="h-1.5 w-1.5 shrink-0 rounded-full bg-[#2D6A4F]" />
        )}
        <span className="text-[#1A1A19]" title={row.question}>
          {truncateQuestion(row.question)}
        </span>
      </div>
    ),
  },
  {
    key: "price",
    header: "Price",
    sortable: true,
    mono: true,
    render: (row) => {
      const price = row.outcome_prices[0]
        ? parseFloat(row.outcome_prices[0])
        : null;
      if (price === null) return <span className="text-[#9B9B9B]">&mdash;</span>;
      return (
        <span className="text-[#1A1A19]">
          {(price * 100).toFixed(0)}&cent;
        </span>
      );
    },
    getValue: (row) =>
      row.outcome_prices[0] ? parseFloat(row.outcome_prices[0]) : 0,
  },
  {
    key: "change",
    header: "24h",
    sortable: true,
    mono: true,
    render: (row) => {
      const { text, positive } = formatPriceChange(row.one_day_price_change);
      if (positive === null)
        return <span className="text-[#9B9B9B]">{text}</span>;
      return (
        <span
          className={cn(
            "inline-flex items-center gap-0.5",
            positive ? "text-[#2D6A4F]" : "text-[#B44C3F]"
          )}
        >
          {positive ? (
            <ArrowUpRight className="h-3 w-3" />
          ) : (
            <ArrowDownRight className="h-3 w-3" />
          )}
          {text}
        </span>
      );
    },
    getValue: (row) =>
      row.one_day_price_change ? parseFloat(row.one_day_price_change) : 0,
  },
  {
    key: "spread",
    header: "Spread",
    sortable: true,
    mono: true,
    render: (row) => {
      const spreadStr = formatSpreadBps(row.spread);
      if (!row.spread)
        return <span className="text-[#9B9B9B]">{spreadStr}</span>;
      const bps = parseFloat(row.spread) * 10000;
      const color =
        bps < 30
          ? "text-[#2D6A4F]"
          : bps < 100
            ? "text-[#B8860B]"
            : "text-[#B44C3F]";
      return <span className={color}>{spreadStr}</span>;
    },
    getValue: (row) => (row.spread ? parseFloat(row.spread) : 999),
  },
  {
    key: "volume_24hr",
    header: "Volume 24h",
    sortable: true,
    mono: true,
    render: (row) => (
      <span className="text-[#1A1A19]">{formatUsd(row.volume_24hr)}</span>
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
      <span className="text-[#1A1A19]">{formatUsd(row.liquidity)}</span>
    ),
    getValue: (row) =>
      row.liquidity ? parseFloat(row.liquidity) : 0,
  },
  {
    key: "end_date",
    header: "End Date",
    sortable: true,
    render: (row) => (
      <span className="text-[#6B6B6B] text-xs">
        {formatEndDate(row.end_date_iso)}
      </span>
    ),
    getValue: (row) =>
      row.end_date_iso ? new Date(row.end_date_iso).getTime() : 0,
  },
  {
    key: "book",
    header: "Book",
    sortable: false,
    render: (row) =>
      row.orderbooks.length > 0 ? (
        <Badge className="bg-[#DAE9E0] text-[#2D6A4F] text-[10px]">Yes</Badge>
      ) : (
        <Badge className="bg-[#F0EEEA] text-[#9B9B9B] text-[10px]">No</Badge>
      ),
  },
];

export default function MarketsPage() {
  const markets = useDashboardStore((s) => s.markets);
  const opportunities = useDashboardStore((s) => s.opportunities);
  const positions = useDashboardStore((s) => s.positions);
  const router = useRouter();

  const [search, setSearch] = useState("");
  const [activeOnly, setActiveOnly] = useState(true);
  const [hasOrderbooks, setHasOrderbooks] = useState(false);
  const [minVolume, setMinVolume] = useState("0");
  const [sortBy, setSortBy] = useState<SortField>("volume");

  // Cross-reference maps
  const oppsByMarket = useMemo(() => {
    const map = new Map<string, number>();
    for (const opp of opportunities) {
      for (const condId of opp.markets) {
        map.set(condId, (map.get(condId) ?? 0) + 1);
      }
    }
    return map;
  }, [opportunities]);

  const positionsByMarket = useMemo(() => {
    const map = new Map<string, number>();
    for (const pos of positions) {
      const size = parseFloat(pos.size) || 0;
      map.set(pos.condition_id, (map.get(pos.condition_id) ?? 0) + size);
    }
    return map;
  }, [positions]);

  // Enrich markets
  const enrichedMarkets = useMemo(
    () =>
      markets.map(
        (m): EnrichedMarket => ({
          ...m,
          oppCount: oppsByMarket.get(m.condition_id) ?? 0,
          positionSize: positionsByMarket.get(m.condition_id) ?? 0,
        })
      ),
    [markets, oppsByMarket, positionsByMarket]
  );

  // Filter & sort
  const filteredMarkets = useMemo(() => {
    let result = enrichedMarkets;

    if (activeOnly) {
      result = result.filter((m) => m.active);
    }

    if (hasOrderbooks) {
      result = result.filter((m) => m.orderbooks.length > 0);
    }

    const minVol = parseFloat(minVolume);
    if (minVol > 0) {
      result = result.filter(
        (m) => m.volume_24hr && parseFloat(m.volume_24hr) >= minVol
      );
    }

    if (search.trim()) {
      const query = search.trim().toLowerCase();
      result = result.filter((m) =>
        m.question.toLowerCase().includes(query)
      );
    }

    result = [...result].sort((a, b) => {
      switch (sortBy) {
        case "volume":
          return (
            parseFloat(b.volume_24hr ?? "0") -
            parseFloat(a.volume_24hr ?? "0")
          );
        case "liquidity":
          return (
            parseFloat(b.liquidity ?? "0") - parseFloat(a.liquidity ?? "0")
          );
        case "spread": {
          const aSpread = a.spread ? parseFloat(a.spread) : 999;
          const bSpread = b.spread ? parseFloat(b.spread) : 999;
          return aSpread - bSpread;
        }
        case "change":
          return (
            Math.abs(parseFloat(b.one_day_price_change ?? "0")) -
            Math.abs(parseFloat(a.one_day_price_change ?? "0"))
          );
        default:
          return 0;
      }
    });

    return result;
  }, [enrichedMarkets, search, activeOnly, hasOrderbooks, minVolume, sortBy]);

  // Summary stats
  const stats = useMemo(() => {
    const active = enrichedMarkets.filter((m) => m.active);
    const withBooks = active.filter((m) => m.orderbooks.length > 0);
    const spreads = active
      .filter((m) => m.spread)
      .map((m) => parseFloat(m.spread!));
    const avgSpread =
      spreads.length > 0
        ? spreads.reduce((a, b) => a + b, 0) / spreads.length
        : null;
    const tightSpread = spreads.filter((s) => s < 0.005).length;

    return {
      total: active.length,
      withBooks: withBooks.length,
      avgSpread,
      tightSpread,
    };
  }, [enrichedMarkets]);

  const handleRowClick = useCallback(
    (market: EnrichedMarket) => {
      router.push(`/markets/${market.condition_id}`);
    },
    [router]
  );

  return (
    <div className="space-y-5">
      {/* Header */}
      <div>
        <h1 className="text-2xl font-bold text-[#1A1A19]">Markets</h1>
        <p className="mt-1 text-sm text-[#6B6B6B]">
          Browse and inspect Polymarket prediction markets
        </p>
      </div>

      {/* Summary stats */}
      <div className="grid grid-cols-2 gap-3 lg:grid-cols-4">
        <StatCard label="Total Markets" value={stats.total} icon={BarChart3} />
        <StatCard
          label="With Orderbooks"
          value={stats.withBooks}
          icon={Activity}
        />
        <StatCard
          label="Avg Spread"
          value={
            stats.avgSpread !== null
              ? formatSpreadBps(stats.avgSpread)
              : "\u2014"
          }
          icon={Target}
        />
        <StatCard
          label="Tight Spread (<50bps)"
          value={stats.tightSpread}
          icon={TrendingUp}
        />
      </div>

      {/* Filter bar */}
      <div className="flex flex-col gap-3 sm:flex-row sm:items-center sm:flex-wrap">
        <div className="relative max-w-sm flex-1">
          <Search className="absolute left-3 top-1/2 h-4 w-4 -translate-y-1/2 text-[#9B9B9B]" />
          <Input
            placeholder="Search markets..."
            value={search}
            onChange={(e) => setSearch(e.target.value)}
            className="border-[#E6E4DF] bg-white pl-9 text-[#1A1A19] placeholder:text-[#9B9B9B]"
          />
        </div>

        <div className="flex items-center gap-4 flex-wrap">
          <div className="flex items-center gap-2">
            <Switch
              id="active-only"
              checked={activeOnly}
              onCheckedChange={setActiveOnly}
            />
            <Label
              htmlFor="active-only"
              className="text-sm text-[#6B6B6B] cursor-pointer"
            >
              Active only
            </Label>
          </div>

          <div className="flex items-center gap-2">
            <Switch
              id="has-orderbooks"
              checked={hasOrderbooks}
              onCheckedChange={setHasOrderbooks}
            />
            <Label
              htmlFor="has-orderbooks"
              className="text-sm text-[#6B6B6B] cursor-pointer"
            >
              Has Orderbooks
            </Label>
          </div>

          <Select value={minVolume} onValueChange={setMinVolume}>
            <SelectTrigger size="sm" className="w-[100px]">
              <SelectValue placeholder="Min Vol" />
            </SelectTrigger>
            <SelectContent>
              {MIN_VOLUME_OPTIONS.map((opt) => (
                <SelectItem key={opt.value} value={opt.value}>
                  {opt.label}
                </SelectItem>
              ))}
            </SelectContent>
          </Select>

          <Select
            value={sortBy}
            onValueChange={(v) => setSortBy(v as SortField)}
          >
            <SelectTrigger size="sm" className="w-[110px]">
              <SelectValue placeholder="Sort by" />
            </SelectTrigger>
            <SelectContent>
              {SORT_OPTIONS.map((opt) => (
                <SelectItem key={opt.value} value={opt.value}>
                  {opt.label}
                </SelectItem>
              ))}
            </SelectContent>
          </Select>

          <span
            className="text-xs text-[#9B9B9B]"
            style={{ fontFamily: "var(--font-jetbrains-mono)" }}
          >
            {filteredMarkets.length} of {markets.length}
          </span>
        </div>
      </div>

      {/* Markets table */}
      <div className="rounded-2xl bg-white">
        {markets.length === 0 ? (
          <div className="flex h-[300px] items-center justify-center text-sm text-[#9B9B9B]">
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
