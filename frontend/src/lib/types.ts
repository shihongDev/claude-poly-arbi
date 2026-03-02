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
  | "opportunity_detected"
  | "trade_executed"
  | "position_update"
  | "metrics_update"
  | "kill_switch_change"
  | "market_update"
  | "markets_loaded"
  | "market_count_update"
  | "alert";

export interface WsEvent {
  type: WsEventType;
  data: unknown;
}
