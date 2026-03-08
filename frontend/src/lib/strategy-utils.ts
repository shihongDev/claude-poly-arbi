import type { Opportunity, StrategyType, ArbType } from "./types";

export const strategyTypeConfig: Record<
  StrategyType,
  { label: string; className: string }
> = {
  IntraMarketArb: { label: "Intra-Market", className: "bg-blue-50 text-blue-600" },
  CrossMarketArb: { label: "Cross-Market", className: "bg-purple-50 text-purple-600" },
  MultiOutcomeArb: { label: "Multi-Outcome", className: "bg-amber-50 text-amber-600" },
  ResolutionSniping: { label: "Resolution", className: "bg-emerald-50 text-emerald-600" },
  LiquiditySniping: { label: "Liq. Snipe", className: "bg-cyan-50 text-cyan-600" },
  MarketMaking: { label: "MM", className: "bg-indigo-50 text-indigo-600" },
  ProbabilityModel: { label: "Prob. Model", className: "bg-rose-50 text-rose-600" },
  StaleMarket: { label: "Stale", className: "bg-orange-50 text-orange-600" },
  VolumeSpike: { label: "Vol. Spike", className: "bg-teal-50 text-teal-600" },
};

const arbToStrategy: Record<ArbType, StrategyType> = {
  IntraMarket: "IntraMarketArb",
  CrossMarket: "CrossMarketArb",
  MultiOutcome: "MultiOutcomeArb",
};

/** Resolve the display StrategyType from an Opportunity, falling back to arb_type. */
export function getStrategyDisplayType(opp: Opportunity): StrategyType {
  return opp.strategy_type ?? arbToStrategy[opp.arb_type];
}

/** Human-readable label for a strategy/arb type key (used in P&L charts). */
export const strategyLabels: Record<string, string> = {
  // Derive from strategyTypeConfig
  ...Object.fromEntries(
    Object.entries(strategyTypeConfig).map(([k, v]) => [k, v.label])
  ),
  // Legacy ArbType keys (backend may send these for historical data)
  IntraMarket: "Intra-Market",
  CrossMarket: "Cross-Market",
  MultiOutcome: "Multi-Outcome",
};
