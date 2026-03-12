use chrono::{DateTime, Utc};
use rust_decimal::Decimal;
use serde::{Deserialize, Serialize};
use uuid::Uuid;

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct OrderbookLevel {
    pub price: Decimal,
    pub size: Decimal,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct OrderbookSnapshot {
    pub token_id: String,
    pub bids: Vec<OrderbookLevel>,
    pub asks: Vec<OrderbookLevel>,
    pub timestamp: DateTime<Utc>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct MarketState {
    pub condition_id: String,
    pub question: String,
    pub outcomes: Vec<String>,
    pub token_ids: Vec<String>,
    pub outcome_prices: Vec<Decimal>,
    pub orderbooks: Vec<OrderbookSnapshot>,
    pub volume_24hr: Option<Decimal>,
    pub liquidity: Option<Decimal>,
    pub active: bool,
    pub neg_risk: bool,
    pub best_bid: Option<Decimal>,
    pub best_ask: Option<Decimal>,
    pub spread: Option<Decimal>,
    pub last_trade_price: Option<Decimal>,
    pub description: Option<String>,
    pub end_date_iso: Option<String>,
    pub slug: Option<String>,
    pub one_day_price_change: Option<Decimal>,
    /// The Polymarket event ID this market belongs to.
    /// Multi-outcome events have multiple markets sharing the same event_id.
    /// Used by `MultiOutcomeDetector` to group related markets for cross-market
    /// probability-sum arbitrage detection.
    #[serde(default)]
    pub event_id: Option<String>,
    /// Cache generation when this market was last updated.
    /// Used for dirty-tracking: detectors only scan markets where
    /// `last_updated_gen > last_scan_gen`.
    #[serde(default)]
    pub last_updated_gen: u64,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub enum ArbType {
    IntraMarket,
    CrossMarket,
    MultiOutcome,
}

impl std::fmt::Display for ArbType {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::IntraMarket => write!(f, "intra_market"),
            Self::CrossMarket => write!(f, "cross_market"),
            Self::MultiOutcome => write!(f, "multi_outcome"),
        }
    }
}

/// Broader strategy categorization covering both structural arb and directional strategies.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub enum StrategyType {
    #[default]
    IntraMarketArb,
    CrossMarketArb,
    MultiOutcomeArb,
    ResolutionSniping,
    LiquiditySniping,
    MarketMaking,
    ProbabilityModel,
    StaleMarket,
    VolumeSpike,
}

impl From<ArbType> for StrategyType {
    fn from(arb: ArbType) -> Self {
        match arb {
            ArbType::IntraMarket => Self::IntraMarketArb,
            ArbType::CrossMarket => Self::CrossMarketArb,
            ArbType::MultiOutcome => Self::MultiOutcomeArb,
        }
    }
}


impl std::fmt::Display for StrategyType {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::IntraMarketArb => write!(f, "intra_market_arb"),
            Self::CrossMarketArb => write!(f, "cross_market_arb"),
            Self::MultiOutcomeArb => write!(f, "multi_outcome_arb"),
            Self::ResolutionSniping => write!(f, "resolution_sniping"),
            Self::LiquiditySniping => write!(f, "liquidity_sniping"),
            Self::MarketMaking => write!(f, "market_making"),
            Self::ProbabilityModel => write!(f, "probability_model"),
            Self::StaleMarket => write!(f, "stale_market"),
            Self::VolumeSpike => write!(f, "volume_spike"),
        }
    }
}

/// Actions that strategies can emit back to the engine.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub enum StrategyAction {
    Execute(Opportunity),
    PlaceQuote {
        market_id: String,
        token_id: String,
        side: Side,
        price: Decimal,
        size: Decimal,
    },
    CancelQuote {
        order_id: String,
    },
    CancelAllQuotes {
        market_id: String,
    },
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub enum Side {
    Buy,
    Sell,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum TradingMode {
    Paper,
    Live,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Opportunity {
    pub id: Uuid,
    pub arb_type: ArbType,
    #[serde(default)]
    pub strategy_type: StrategyType,
    pub markets: Vec<String>,
    pub legs: Vec<TradeLeg>,
    pub gross_edge: Decimal,
    pub net_edge: Decimal,
    pub estimated_vwap: Vec<Decimal>,
    pub confidence: f64,
    pub size_available: Decimal,
    pub detected_at: DateTime<Utc>,
}

impl Opportunity {
    /// Net edge in basis points.
    pub fn net_edge_bps(&self) -> Decimal {
        self.net_edge * Decimal::from(10_000)
    }

    /// Return a copy with size capped to `max_size`.
    pub fn with_max_size(&self, max_size: Decimal) -> Self {
        let mut opp = self.clone();
        if opp.size_available > max_size {
            opp.size_available = max_size;
            for leg in &mut opp.legs {
                if leg.target_size > max_size {
                    leg.target_size = max_size;
                }
            }
        }
        opp
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct TradeLeg {
    pub token_id: String,
    pub side: Side,
    pub target_price: Decimal,
    pub target_size: Decimal,
    pub vwap_estimate: Decimal,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ExecutionReport {
    pub opportunity_id: Uuid,
    pub legs: Vec<LegReport>,
    pub realized_edge: Decimal,
    pub slippage: Decimal,
    pub total_fees: Decimal,
    pub timestamp: DateTime<Utc>,
    pub mode: TradingMode,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct LegReport {
    pub order_id: String,
    pub token_id: String,
    pub condition_id: String,
    pub side: Side,
    pub expected_vwap: Decimal,
    pub actual_fill_price: Decimal,
    pub filled_size: Decimal,
    pub status: FillStatus,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum FillStatus {
    FullyFilled,
    PartiallyFilled,
    Rejected,
    Cancelled,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct VwapEstimate {
    pub vwap: Decimal,
    pub total_size: Decimal,
    pub levels_consumed: usize,
    pub max_available: Decimal,
    pub slippage_bps: Decimal,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct OrderChunk {
    pub size: Decimal,
    pub limit_price: Decimal,
    pub delay_ms: u64,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ProbEstimate {
    pub probabilities: Vec<f64>,
    pub confidence_interval: Vec<(f64, f64)>,
    pub method: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub enum RiskDecision {
    Approve { max_size: Decimal },
    Reject { reason: String },
    ReduceSize { new_size: Decimal, reason: String },
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Position {
    pub token_id: String,
    pub condition_id: String,
    pub size: Decimal,
    pub avg_entry_price: Decimal,
    pub current_price: Decimal,
    pub unrealized_pnl: Decimal,
}

/// Defines a correlation relationship between two markets.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct MarketCorrelation {
    pub condition_id_a: String,
    pub condition_id_b: String,
    pub relationship: CorrelationRelationship,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub enum CorrelationRelationship {
    /// P(A) <= P(B) — A implies B
    ImpliedBy,
    /// P(A) + P(B) <= 1.0 — mutually exclusive
    MutuallyExclusive,
    /// P(A) + P(B) >= 1.0 — at least one must be true
    Exhaustive,
    /// Custom constraint with a bound
    Custom { constraint: String, bound: Decimal },
}

/// Flat override struct for sandbox/playground requests.
/// All fields are optional — `None` means "use current live config value".
#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct SandboxConfigOverrides {
    // ── Strategy toggles ─────────────────────────────────────
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub min_edge_bps: Option<u64>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub intra_market_enabled: Option<bool>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub cross_market_enabled: Option<bool>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub multi_outcome_enabled: Option<bool>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub resolution_sniping_enabled: Option<bool>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub stale_market_enabled: Option<bool>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub volume_spike_enabled: Option<bool>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub prob_model_enabled: Option<bool>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub liquidity_sniping_enabled: Option<bool>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub market_making_enabled: Option<bool>,

    // ── Per-strategy config ──────────────────────────────────
    // Intra-market
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub intra_min_deviation: Option<Decimal>,
    // Cross-market
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub cross_min_implied_edge: Option<Decimal>,
    // Multi-outcome
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub multi_min_deviation: Option<Decimal>,
    // Resolution sniping
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub res_min_price: Option<Decimal>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub res_max_price: Option<Decimal>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub res_max_hours: Option<u64>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub res_min_volume: Option<Decimal>,
    // Stale market
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub stale_max_hours: Option<u64>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub stale_min_divergence_bps: Option<u64>,
    // Volume spike
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub vol_spike_multiplier: Option<f64>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub vol_min_absolute_volume: Option<Decimal>,
    // Prob model
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub prob_min_deviation_bps: Option<u64>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub prob_min_confidence: Option<f64>,
    // Liquidity sniping
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub liq_min_depth_change_pct: Option<f64>,
    // Market making
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub mm_target_spread_bps: Option<u64>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub mm_max_inventory: Option<Decimal>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub mm_min_volume: Option<Decimal>,

    // ── Slippage ─────────────────────────────────────────────
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub max_slippage_bps: Option<u64>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub vwap_depth_levels: Option<usize>,

    // ── Risk ─────────────────────────────────────────────────
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub max_position_per_market: Option<Decimal>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub max_total_exposure: Option<Decimal>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub daily_loss_limit: Option<Decimal>,

    // ── Fee override ─────────────────────────────────────────
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub fee_rate_override: Option<Decimal>,
}
