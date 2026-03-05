export interface OrderbookLevel {
  price: string;
  size: string;
}

export interface OrderbookSnapshot {
  token_id: string;
  bids: OrderbookLevel[];
  asks: OrderbookLevel[];
  timestamp: string;
}

export interface MarketState {
  condition_id: string;
  question: string;
  outcomes: string[];
  token_ids: string[];
  outcome_prices: string[];
  orderbooks: OrderbookSnapshot[];
  volume_24hr: string | null;
  liquidity: string | null;
  active: boolean;
  neg_risk: boolean;
  best_bid: string | null;
  best_ask: string | null;
  spread: string | null;
  last_trade_price: string | null;
  description: string | null;
  end_date_iso: string | null;
  slug: string | null;
  one_day_price_change: string | null;
  event_id?: string;
}

export type ArbType = "IntraMarket" | "CrossMarket" | "MultiOutcome";
export type Side = "Buy" | "Sell";
export type TradingMode = "Paper" | "Live";
export type FillStatus =
  | "FullyFilled"
  | "PartiallyFilled"
  | "Rejected"
  | "Cancelled";

export interface TradeLeg {
  token_id: string;
  side: Side;
  target_price: string;
  target_size: string;
  vwap_estimate: string;
}

export interface Opportunity {
  id: string;
  arb_type: ArbType;
  markets: string[];
  legs: TradeLeg[];
  gross_edge: string;
  net_edge: string;
  estimated_vwap: string[];
  confidence: number;
  size_available: string;
  detected_at: string;
}

export interface LegReport {
  order_id: string;
  token_id: string;
  condition_id: string;
  side: Side;
  expected_vwap: string;
  actual_fill_price: string;
  filled_size: string;
  status: FillStatus;
}

export interface ExecutionReport {
  opportunity_id: string;
  legs: LegReport[];
  realized_edge: string;
  slippage: string;
  total_fees: string;
  timestamp: string;
  mode: TradingMode;
}

export interface Position {
  token_id: string;
  condition_id: string;
  size: string;
  avg_entry_price: string;
  current_price: string;
  unrealized_pnl: string;
}

export interface MetricsSnapshot {
  brier_score: number;
  drawdown_pct: number;
  execution_quality: string;
  total_pnl: string;
  daily_pnl: string;
  trade_count: number;
  pnl_by_type: Record<string, string>;
  current_exposure: string;
  peak_equity: string;
  current_equity: string;
}

export interface StatusResponse {
  mode: TradingMode;
  kill_switch_active: boolean;
  kill_switch_reason: string | null;
  market_count: number;
  uptime_secs: number;
}

export type WsEventType =
  | "opportunities_batch"
  | "trade_executed"
  | "position_update"
  | "metrics_update"
  | "kill_switch_change"
  | "market_update"
  | "markets_loaded";

export interface WsEvent {
  type: WsEventType;
  data: unknown;
}

// ── Sandbox / Playground types ──────────────────────────────

export interface SandboxConfigOverrides {
  min_edge_bps?: number;
  intra_market_enabled?: boolean;
  cross_market_enabled?: boolean;
  multi_outcome_enabled?: boolean;
  intra_min_deviation?: string;
  cross_min_implied_edge?: string;
  multi_min_deviation?: string;
  max_slippage_bps?: number;
  vwap_depth_levels?: number;
  max_position_per_market?: string;
  max_total_exposure?: string;
  daily_loss_limit?: string;
}

export interface DetectResponse {
  opportunities: Opportunity[];
  detection_time_ms: number;
  markets_scanned: number;
  config_used: {
    min_edge_bps: number;
    intra_market_enabled: boolean;
    cross_market_enabled: boolean;
    multi_outcome_enabled: boolean;
  };
}

export interface BacktestTrade {
  opportunity_id: string;
  realized_edge: string;
  total_fees: string;
  net_pnl: string;
  timestamp: string;
  included: boolean;
  rejection_reason: string | null;
}

export interface BacktestDailyBreakdown {
  date: string;
  pnl: string;
  trade_count: number;
}

export interface BacktestResponse {
  total_trades_original: number;
  total_trades_filtered: number;
  trades_rejected: number;
  aggregate_pnl: string;
  aggregate_pnl_original: string;
  daily_breakdown: BacktestDailyBreakdown[];
  trades: BacktestTrade[];
}

export interface SimulateParams {
  num_paths?: number;
  volatility?: number;
  drift?: number;
  time_horizon?: number;
  strike?: number;
  particle_count?: number;
  process_noise?: number;
  observation_noise?: number;
}

// ── Simulation Status types ─────────────────────────────────

export interface SimulationEstimate {
  condition_id: string;
  market_price: number;
  model_estimate: number;
  divergence: number;
  confidence_interval: [number, number];
  method: string;
}

export interface ConvergenceDiagnostics {
  paths_used: number;
  standard_error: number;
  converged: boolean;
  gelman_rubin: number | null;
}

export interface ModelHealth {
  brier_score_30m: number;
  brier_score_24h: number;
  confidence_level: number;
  drift_detected: boolean;
}

export interface VarSummary {
  var_95: string;
  var_99: string;
  cvar_95: string;
  method: string;
}

export interface SimulationStatus {
  estimates: SimulationEstimate[];
  convergence: ConvergenceDiagnostics;
  model_health: ModelHealth;
  var_summary: VarSummary;
}

// ── Stress Test types ───────────────────────────────────────

export type StressScenarioType =
  | "liquidity_shock"
  | "correlation_spike"
  | "flash_crash"
  | "kill_switch_delay";

export interface StressScenario {
  scenario: StressScenarioType;
  params: Record<string, number>;
}

export interface StressTestResult {
  scenario: StressScenarioType;
  portfolio_impact: string;
  max_loss: string;
  positions_at_risk: number;
  var_before: string;
  var_after: string;
  details: string;
}
