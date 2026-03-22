"use client";

import { useState, useMemo, useCallback } from "react";
import { useRouter } from "next/navigation";
import dynamic from "next/dynamic";
import {
  Search,
  ArrowUpRight,
  ArrowDownRight,
  BarChart3,
  Activity,
  Target,
  TrendingUp,
  SlidersHorizontal,
} from "lucide-react";
import { useDashboardStore } from "@/store";
import { DataTable, type Column } from "@/components/data-table";
import { Badge } from "@/components/ui/badge";
import { Input } from "@/components/ui/input";
import { Switch } from "@/components/ui/switch";
import { Label } from "@/components/ui/label";
import { Button } from "@/components/ui/button";
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
  formatCents,
  truncate,
  spreadColorClass,
  cn,
  MONO_STYLE,
} from "@/lib/utils";
import type { MarketState } from "@/lib/types";

// New components
import { MarketHealthBadge } from "@/components/market-health-badge";
import { LiquidityDepthBar } from "@/components/liquidity-depth-bar";
import { OrderImbalanceArrow } from "@/components/order-imbalance-arrow";
import { ExpiryProgressBar } from "@/components/expiry-progress-bar";
import { ProbSumBadge } from "@/components/prob-sum-badge";
import { ArbOpportunityBadge } from "@/components/arb-opportunity-badge";
import { HotMarketsCarousel } from "@/components/hot-markets-carousel";
import { SpreadHistogram } from "@/components/spread-histogram";
import { ProbabilityStackedBar } from "@/components/probability-stacked-bar";
import {
  useWatchlist,
  WatchlistStar,
  WatchlistSection,
} from "@/components/market-watchlist";
import { EventGroupTable } from "@/components/event-group-table";
import { ViewModeToggle } from "@/components/view-mode-toggle";
import { MarketCardGrid } from "@/components/market-card-grid";
import {
  AdvancedFilterPanel,
  DEFAULT_FILTERS,
  type MarketFilters,
} from "@/components/advanced-filter-panel";

const MarketTreemap = dynamic(
  () =>
    import("@/components/market-treemap").then((mod) => ({
      default: mod.MarketTreemap,
    })),
  { ssr: false }
);
const SpreadVolumeScatter = dynamic(
  () =>
    import("@/components/spread-volume-scatter").then((mod) => ({
      default: mod.SpreadVolumeScatter,
    })),
  { ssr: false }
);

// ---------------------------------------------------------------------------
// Helpers
// ---------------------------------------------------------------------------

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

// ---------------------------------------------------------------------------
// Stat Card
// ---------------------------------------------------------------------------

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
        style={MONO_STYLE}
      >
        {value}
      </p>
    </div>
  );
}

// ---------------------------------------------------------------------------
// Enhanced Table Columns
// ---------------------------------------------------------------------------

function buildColumns(): Column<EnrichedMarket>[] {
  return [
    {
      key: "star",
      header: "",
      sortable: false,
      render: (row) => <WatchlistStar conditionId={row.condition_id} />,
    },
    {
      key: "question",
      header: "Question",
      sortable: false,
      render: (row) => (
        <div className="flex items-center gap-2">
          <ArbOpportunityBadge count={row.oppCount} />
          <ProbSumBadge market={row} />
          <span className="text-[#1A1A19]" title={row.question}>
            {truncate(row.question, 55)}
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
        const isMulti = row.outcomes.length > 2;
        return (
          <div className="flex flex-col gap-1">
            <span className="text-[#1A1A19]">
              {formatCents(row.outcome_prices[0] ?? null)}
            </span>
            {isMulti && (
              <ProbabilityStackedBar
                outcomes={row.outcomes}
                prices={row.outcome_prices}
              />
            )}
          </div>
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
        return (
          <div className="flex items-center gap-1.5">
            {positive === null ? (
              <span className="text-[#9B9B9B]">{text}</span>
            ) : (
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
            )}
            <OrderImbalanceArrow market={row} />
          </div>
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
        return <span className={spreadColorClass(bps)}>{spreadStr}</span>;
      },
      getValue: (row) => (row.spread ? parseFloat(row.spread) : 999),
    },
    {
      key: "depth",
      header: "Depth",
      sortable: false,
      render: (row) => <LiquidityDepthBar market={row} />,
    },
    {
      key: "volume_24hr",
      header: "Volume",
      sortable: true,
      mono: true,
      render: (row) => (
        <span className="text-[#1A1A19]">{formatUsd(row.volume_24hr)}</span>
      ),
      getValue: (row) =>
        row.volume_24hr ? parseFloat(row.volume_24hr) : 0,
    },
    {
      key: "health",
      header: "Health",
      sortable: false,
      render: (row) => <MarketHealthBadge market={row} />,
    },
    {
      key: "expiry",
      header: "Expiry",
      sortable: true,
      render: (row) => <ExpiryProgressBar market={row} />,
      getValue: (row) =>
        row.end_date_iso ? new Date(row.end_date_iso).getTime() : Infinity,
    },
  ];
}

// ---------------------------------------------------------------------------
// Page Component
// ---------------------------------------------------------------------------

export default function MarketsPage() {
  const markets = useDashboardStore((s) => s.markets);
  const opportunities = useDashboardStore((s) => s.opportunities);
  const positions = useDashboardStore((s) => s.positions);
  const wsStatus = useDashboardStore((s) => s.wsStatus);
  const marketsLoading = useDashboardStore((s) => s.marketsLoading);
  const router = useRouter();
  const { watchlist } = useWatchlist();

  const isLoading =
    markets.length === 0 && (marketsLoading || wsStatus === "connecting");

  // View & filter state
  const [viewMode, setViewMode] = useState<"table" | "cards" | "treemap">(
    "table"
  );
  const [showGrouped, setShowGrouped] = useState(false);
  const [filterOpen, setFilterOpen] = useState(false);
  const [search, setSearch] = useState("");
  const [activeOnly, setActiveOnly] = useState(true);
  const [hasOrderbooks, setHasOrderbooks] = useState(false);
  const [minVolume, setMinVolume] = useState("0");
  const [sortBy, setSortBy] = useState<SortField>("volume");
  const [advancedFilters, setAdvancedFilters] = useState<MarketFilters>({ ...DEFAULT_FILTERS });

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

    // Advanced filters
    if (advancedFilters.hasOpportunities) {
      result = result.filter((m) => (oppsByMarket.get(m.condition_id) ?? 0) > 0);
    }
    if (advancedFilters.maxSpread > 0) {
      result = result.filter((m) => {
        if (!m.spread) return false;
        const bps = parseFloat(m.spread) * 10000;
        return bps <= advancedFilters.maxSpread;
      });
    }
    if (advancedFilters.minPrice > 0 || advancedFilters.maxPrice < 100) {
      result = result.filter((m) => {
        const price = m.outcome_prices[0] ? parseFloat(m.outcome_prices[0]) * 100 : 0;
        return price >= advancedFilters.minPrice && price <= advancedFilters.maxPrice;
      });
    }
    if (advancedFilters.outcomesFilter === "binary") {
      result = result.filter((m) => m.outcomes.length === 2);
    } else if (advancedFilters.outcomesFilter === "multi") {
      result = result.filter((m) => m.outcomes.length > 2);
    }
    if (advancedFilters.expiresWithin > 0) {
      const cutoff = Date.now() + advancedFilters.expiresWithin * 24 * 60 * 60 * 1000;
      result = result.filter((m) => {
        if (!m.end_date_iso) return false;
        return new Date(m.end_date_iso).getTime() <= cutoff;
      });
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
  }, [enrichedMarkets, search, activeOnly, hasOrderbooks, minVolume, sortBy, advancedFilters, oppsByMarket]);

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

  const handleMarketClick = useCallback(
    (conditionId: string) => {
      router.push(`/markets/${conditionId}`);
    },
    [router]
  );

  const columns = useMemo(() => buildColumns(), []);

  return (
    <div className="space-y-5">
      {/* Header */}
      <div className="flex items-start justify-between">
        <div>
          <h1 className="text-2xl font-bold text-[#1A1A19]">Markets</h1>
          <p className="mt-1 text-sm text-[#6B6B6B]">
            Browse and inspect Polymarket prediction markets
          </p>
        </div>
        <ViewModeToggle mode={viewMode} onChange={setViewMode} />
      </div>

      {/* Watchlist */}
      {watchlist.size > 0 && <WatchlistSection markets={markets} />}

      {/* Hot Markets Carousel */}
      {!isLoading && markets.length > 0 && (
        <HotMarketsCarousel
          markets={markets}
          opportunityCounts={oppsByMarket}
        />
      )}

      {/* Summary stats + Spread Histogram */}
      <div className="grid gap-4 lg:grid-cols-3">
        <div className="grid grid-cols-2 gap-3 lg:col-span-2">
          <StatCard
            label="Total Markets"
            value={stats.total}
            icon={BarChart3}
          />
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
        {!isLoading && markets.length > 0 && (
          <SpreadHistogram markets={markets} />
        )}
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

          {viewMode === "table" && (
            <div className="flex items-center gap-2">
              <Switch
                id="group-events"
                checked={showGrouped}
                onCheckedChange={setShowGrouped}
              />
              <Label
                htmlFor="group-events"
                className="text-sm text-[#6B6B6B] cursor-pointer"
              >
                Group Events
              </Label>
            </div>
          )}

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

          <Button
            variant="ghost"
            size="sm"
            onClick={() => setFilterOpen(true)}
            className="text-[#6B6B6B] hover:text-[#1A1A19]"
          >
            <SlidersHorizontal className="mr-1 h-3.5 w-3.5" />
            Filters
          </Button>

          <span
            className="text-xs text-[#9B9B9B]"
            style={MONO_STYLE}
          >
            {filteredMarkets.length} of {markets.length}
          </span>
        </div>
      </div>

      {/* Main content — view-dependent */}
      {isLoading ? (
        <div className="rounded-2xl bg-white">
          <div className="space-y-3 p-6">
            {Array.from({ length: 8 }).map((_, i) => (
              <div key={i} className="flex items-center gap-4 animate-pulse">
                <div className="h-4 w-2/5 rounded bg-[#E6E4DF]" />
                <div className="h-4 w-1/6 rounded bg-[#E6E4DF]" />
                <div className="h-4 w-1/6 rounded bg-[#E6E4DF]" />
                <div className="h-4 w-1/6 rounded bg-[#E6E4DF]" />
              </div>
            ))}
            <p className="text-center text-sm text-[#9B9B9B] mt-4">
              Loading markets from Polymarket...
            </p>
          </div>
        </div>
      ) : markets.length === 0 ? (
        <div className="rounded-2xl bg-white">
          <div className="flex h-[300px] items-center justify-center text-sm text-[#9B9B9B]">
            No markets loaded
          </div>
        </div>
      ) : viewMode === "table" ? (
        showGrouped ? (
          <div className="rounded-2xl bg-white">
            <EventGroupTable
              markets={filteredMarkets}
              opportunityCounts={oppsByMarket}
              onRowClick={handleMarketClick}
            />
          </div>
        ) : (
          <div className="rounded-2xl bg-white">
            <DataTable
              columns={columns}
              data={filteredMarkets}
              pageSize={20}
              onRowClick={handleRowClick}
              keyExtractor={(row) => row.condition_id}
            />
          </div>
        )
      ) : viewMode === "cards" ? (
        <MarketCardGrid
          markets={filteredMarkets}
          opportunityCounts={oppsByMarket}
          onMarketClick={handleMarketClick}
        />
      ) : (
        <MarketTreemap
          markets={filteredMarkets}
          onMarketClick={handleMarketClick}
        />
      )}

      {/* Analytics section — Scatter plot */}
      {!isLoading && markets.length > 5 && (
        <SpreadVolumeScatter
          markets={filteredMarkets}
          onMarketClick={handleMarketClick}
        />
      )}

      {/* Advanced filter panel */}
      <AdvancedFilterPanel
        open={filterOpen}
        onClose={() => setFilterOpen(false)}
        filters={{ ...advancedFilters, activeOnly, search }}
        onFiltersChange={(newFilters) => {
          setAdvancedFilters(newFilters);
          // Sync shared filters back to page-level state
          setActiveOnly(newFilters.activeOnly);
          setSearch(newFilters.search);
          setHasOrderbooks(newFilters.hasOrderbooks);
          if (newFilters.minVolume > 0) {
            setMinVolume(String(newFilters.minVolume));
          }
        }}
        marketCount={filteredMarkets.length}
        totalCount={markets.length}
      />
    </div>
  );
}
