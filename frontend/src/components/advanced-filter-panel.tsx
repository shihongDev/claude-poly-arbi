"use client";

import { useEffect, useRef, useCallback } from "react";
import { cn } from "@/lib/utils";
import { Badge } from "@/components/ui/badge";
import { Button } from "@/components/ui/button";
import { Input } from "@/components/ui/input";
import { Switch } from "@/components/ui/switch";
import {
  Select,
  SelectContent,
  SelectItem,
  SelectTrigger,
  SelectValue,
} from "@/components/ui/select";
import { X, ArrowUp, ArrowDown, Search } from "lucide-react";

// ── Types & defaults ───────────────────────────────────────────

export interface MarketFilters {
  search: string;
  activeOnly: boolean;
  hasOrderbooks: boolean;
  minVolume: number;
  maxSpread: number;
  minPrice: number;
  maxPrice: number;
  outcomesFilter: "all" | "binary" | "multi";
  hasOpportunities: boolean;
  expiresWithin: number;
  sortBy: string;
  sortDir: "asc" | "desc";
}

export const DEFAULT_FILTERS: MarketFilters = {
  search: "",
  activeOnly: true,
  hasOrderbooks: false,
  minVolume: 0,
  maxSpread: 0,
  minPrice: 0,
  maxPrice: 100,
  outcomesFilter: "all",
  hasOpportunities: false,
  expiresWithin: 0,
  sortBy: "volume",
  sortDir: "desc",
};

// ── Props ──────────────────────────────────────────────────────

interface AdvancedFilterPanelProps {
  open: boolean;
  onClose: () => void;
  filters: MarketFilters;
  onFiltersChange: (filters: MarketFilters) => void;
  marketCount: number;
  totalCount: number;
}

// ── Pill button group helper ───────────────────────────────────

function PillGroup<T extends string | number>({
  options,
  value,
  onChange,
}: {
  options: { label: string; value: T }[];
  value: T;
  onChange: (v: T) => void;
}) {
  return (
    <div className="flex flex-wrap gap-1.5">
      {options.map((opt) => (
        <button
          key={String(opt.value)}
          type="button"
          onClick={() => onChange(opt.value)}
          className={cn(
            "rounded-[9999px] px-3 py-1 text-xs font-medium transition-all duration-150",
            value === opt.value
              ? "bg-[#1A1A19] text-white"
              : "bg-[#F0EEEA] text-[#6B6B6B] hover:bg-[#E6E4DF] hover:text-[#1A1A19]"
          )}
        >
          {opt.label}
        </button>
      ))}
    </div>
  );
}

// ── Section wrapper ────────────────────────────────────────────

function FilterSection({
  label,
  children,
  last = false,
}: {
  label: string;
  children: React.ReactNode;
  last?: boolean;
}) {
  return (
    <div className={cn("py-4", !last && "border-b border-[#E6E4DF]")}>
      <p className="mb-3 text-[11px] font-medium uppercase tracking-wider text-[#9B9B9B]">
        {label}
      </p>
      {children}
    </div>
  );
}

// ── Toggle row ─────────────────────────────────────────────────

function ToggleRow({
  label,
  checked,
  onCheckedChange,
}: {
  label: string;
  checked: boolean;
  onCheckedChange: (v: boolean) => void;
}) {
  return (
    <label className="flex cursor-pointer items-center justify-between py-1.5">
      <span className="text-sm text-[#1A1A19]">{label}</span>
      <Switch checked={checked} onCheckedChange={onCheckedChange} size="sm" />
    </label>
  );
}

// ── Panel component ────────────────────────────────────────────

export function AdvancedFilterPanel({
  open,
  onClose,
  filters,
  onFiltersChange,
  marketCount,
  totalCount,
}: AdvancedFilterPanelProps) {
  const panelRef = useRef<HTMLDivElement>(null);

  const update = useCallback(
    (patch: Partial<MarketFilters>) => {
      onFiltersChange({ ...filters, ...patch });
    },
    [filters, onFiltersChange]
  );

  const handleReset = useCallback(() => {
    onFiltersChange({ ...DEFAULT_FILTERS });
  }, [onFiltersChange]);

  // Close on Escape
  useEffect(() => {
    if (!open) return;
    const handler = (e: KeyboardEvent) => {
      if (e.key === "Escape") onClose();
    };
    document.addEventListener("keydown", handler);
    return () => document.removeEventListener("keydown", handler);
  }, [open, onClose]);

  // Prevent body scroll when panel is open
  useEffect(() => {
    if (open) {
      document.body.style.overflow = "hidden";
    } else {
      document.body.style.overflow = "";
    }
    return () => {
      document.body.style.overflow = "";
    };
  }, [open]);

  return (
    <>
      {/* Backdrop */}
      <div
        className={cn(
          "fixed inset-0 z-40 bg-black/20 transition-opacity duration-200",
          open
            ? "pointer-events-auto opacity-100"
            : "pointer-events-none opacity-0"
        )}
        onClick={onClose}
        aria-hidden="true"
      />

      {/* Panel */}
      <div
        ref={panelRef}
        role="dialog"
        aria-label="Advanced filters"
        className={cn(
          "fixed right-0 top-0 z-50 flex h-full w-80 flex-col bg-white shadow-xl transition-transform duration-300 ease-out",
          open ? "translate-x-0" : "translate-x-full"
        )}
      >
        {/* Header */}
        <div className="flex items-center justify-between border-b border-[#E6E4DF] px-5 py-4">
          <div className="flex items-center gap-3">
            <h2 className="text-base font-semibold text-[#1A1A19]">Filters</h2>
            <Badge className="rounded-[9999px] bg-[#F0EEEA] px-2 py-0.5 text-[11px] font-medium text-[#6B6B6B]">
              {marketCount} of {totalCount}
            </Badge>
          </div>
          <div className="flex items-center gap-2">
            <button
              type="button"
              onClick={handleReset}
              className="text-xs font-medium text-[#9B9B9B] transition-colors hover:text-[#1A1A19]"
            >
              Reset
            </button>
            <Button
              variant="ghost"
              size="icon-xs"
              onClick={onClose}
              aria-label="Close filters"
            >
              <X className="h-4 w-4" />
            </Button>
          </div>
        </div>

        {/* Scrollable body */}
        <div className="flex-1 overflow-y-auto px-5">
          {/* Search */}
          <FilterSection label="Search">
            <div className="relative">
              <Search className="absolute left-3 top-1/2 h-3.5 w-3.5 -translate-y-1/2 text-[#9B9B9B]" />
              <Input
                placeholder="Search markets..."
                value={filters.search}
                onChange={(e) => update({ search: e.target.value })}
                className="pl-9"
              />
            </div>
          </FilterSection>

          {/* Market Status */}
          <FilterSection label="Market Status">
            <div className="space-y-1">
              <ToggleRow
                label="Active only"
                checked={filters.activeOnly}
                onCheckedChange={(v) => update({ activeOnly: v })}
              />
              <ToggleRow
                label="Has orderbooks"
                checked={filters.hasOrderbooks}
                onCheckedChange={(v) => update({ hasOrderbooks: v })}
              />
              <ToggleRow
                label="Has arb opportunities"
                checked={filters.hasOpportunities}
                onCheckedChange={(v) => update({ hasOpportunities: v })}
              />
            </div>
          </FilterSection>

          {/* Price Range */}
          <FilterSection label="Price Range">
            <div className="flex items-center gap-2">
              <div className="flex-1">
                <p className="mb-1 text-[10px] font-medium uppercase tracking-wider text-[#9B9B9B]">
                  Min
                </p>
                <div className="relative">
                  <Input
                    type="number"
                    min={0}
                    max={100}
                    value={filters.minPrice}
                    onChange={(e) =>
                      update({ minPrice: Math.max(0, Math.min(100, Number(e.target.value) || 0)) })
                    }
                    className="pr-6"
                  />
                  <span className="absolute right-3 top-1/2 -translate-y-1/2 text-xs text-[#9B9B9B]">
                    ¢
                  </span>
                </div>
              </div>
              <span className="mt-5 text-[#9B9B9B]">&ndash;</span>
              <div className="flex-1">
                <p className="mb-1 text-[10px] font-medium uppercase tracking-wider text-[#9B9B9B]">
                  Max
                </p>
                <div className="relative">
                  <Input
                    type="number"
                    min={0}
                    max={100}
                    value={filters.maxPrice}
                    onChange={(e) =>
                      update({ maxPrice: Math.max(0, Math.min(100, Number(e.target.value) || 0)) })
                    }
                    className="pr-6"
                  />
                  <span className="absolute right-3 top-1/2 -translate-y-1/2 text-xs text-[#9B9B9B]">
                    ¢
                  </span>
                </div>
              </div>
            </div>
          </FilterSection>

          {/* Volume */}
          <FilterSection label="Volume">
            <PillGroup
              options={[
                { label: "Any", value: 0 },
                { label: ">$1K", value: 1000 },
                { label: ">$10K", value: 10000 },
                { label: ">$100K", value: 100000 },
                { label: ">$1M", value: 1000000 },
              ]}
              value={filters.minVolume}
              onChange={(v) => update({ minVolume: v })}
            />
          </FilterSection>

          {/* Spread */}
          <FilterSection label="Spread">
            <PillGroup
              options={[
                { label: "Any", value: 0 },
                { label: "<30 bps", value: 30 },
                { label: "<50 bps", value: 50 },
                { label: "<100 bps", value: 100 },
              ]}
              value={filters.maxSpread}
              onChange={(v) => update({ maxSpread: v })}
            />
          </FilterSection>

          {/* Outcomes */}
          <FilterSection label="Outcomes">
            <PillGroup
              options={[
                { label: "All", value: "all" as const },
                { label: "Binary only", value: "binary" as const },
                { label: "Multi-outcome", value: "multi" as const },
              ]}
              value={filters.outcomesFilter}
              onChange={(v) => update({ outcomesFilter: v })}
            />
          </FilterSection>

          {/* Expiry */}
          <FilterSection label="Expiry">
            <PillGroup
              options={[
                { label: "Any", value: 0 },
                { label: "<24h", value: 1 },
                { label: "<7d", value: 7 },
                { label: "<30d", value: 30 },
                { label: "<90d", value: 90 },
              ]}
              value={filters.expiresWithin}
              onChange={(v) => update({ expiresWithin: v })}
            />
          </FilterSection>

          {/* Sort */}
          <FilterSection label="Sort" last>
            <div className="flex items-end gap-2">
              <div className="flex-1">
                <Select
                  value={filters.sortBy}
                  onValueChange={(v) => update({ sortBy: v })}
                >
                  <SelectTrigger className="w-full">
                    <SelectValue placeholder="Sort by..." />
                  </SelectTrigger>
                  <SelectContent position="popper">
                    <SelectItem value="volume">Volume</SelectItem>
                    <SelectItem value="spread">Spread</SelectItem>
                    <SelectItem value="price">Price</SelectItem>
                    <SelectItem value="liquidity">Liquidity</SelectItem>
                    <SelectItem value="change">24h Change</SelectItem>
                    <SelectItem value="end_date">End Date</SelectItem>
                    <SelectItem value="question">Name</SelectItem>
                  </SelectContent>
                </Select>
              </div>
              <Button
                variant="outline"
                size="icon"
                onClick={() =>
                  update({ sortDir: filters.sortDir === "asc" ? "desc" : "asc" })
                }
                aria-label={`Sort ${filters.sortDir === "asc" ? "descending" : "ascending"}`}
                className="shrink-0"
              >
                {filters.sortDir === "asc" ? (
                  <ArrowUp className="h-4 w-4" />
                ) : (
                  <ArrowDown className="h-4 w-4" />
                )}
              </Button>
            </div>
          </FilterSection>
        </div>
      </div>
    </>
  );
}
