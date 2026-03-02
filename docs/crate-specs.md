# Crate Specifications — Detailed API & Module Design

## arb-core

### types.rs — Full Type Inventory

```rust
// ─── Orderbook ───
OrderbookLevel { price: Decimal, size: Decimal }
OrderbookSnapshot { token_id: String, bids: Vec<OrderbookLevel>, asks: Vec<OrderbookLevel>, timestamp: DateTime<Utc> }

// ─── Market State ───
MarketState { condition_id, question, outcomes: Vec<String>, token_ids: Vec<String>, outcome_prices: Vec<Decimal>, orderbooks: Vec<OrderbookSnapshot>, volume_24hr: Option<Decimal>, liquidity: Option<Decimal>, active: bool, neg_risk: bool }

// ─── Enums ───
ArbType { IntraMarket, CrossMarket, MultiOutcome }
Side { Buy, Sell }
TradingMode { Paper, Live }
FillStatus { FullyFilled, PartiallyFilled, Rejected, Cancelled }

// ─── Opportunity ───
Opportunity { id: Uuid, arb_type, markets: Vec<String>, legs: Vec<TradeLeg>, gross_edge, net_edge, estimated_vwap: Vec<Decimal>, confidence: f64, size_available: Decimal, detected_at }
  Methods: net_edge_bps() -> Decimal, with_max_size(Decimal) -> Self

TradeLeg { token_id, side, target_price, target_size, vwap_estimate: Decimal }

// ─── Execution ───
ExecutionReport { opportunity_id: Uuid, legs: Vec<LegReport>, realized_edge, slippage, total_fees, timestamp, mode: TradingMode }
LegReport { order_id, token_id, side, expected_vwap, actual_fill_price, filled_size, status: FillStatus }

// ─── VWAP ───
VwapEstimate { vwap, total_size, levels_consumed: usize, max_available, slippage_bps }
OrderChunk { size, limit_price, delay_ms: u64 }

// ─── Simulation ───
ProbEstimate { probabilities: Vec<f64>, confidence_interval: Vec<(f64, f64)>, method: String }

// ─── Risk ───
RiskDecision { Approve { max_size }, Reject { reason }, ReduceSize { new_size, reason } }
Position { token_id, condition_id, size, avg_entry_price, current_price, unrealized_pnl }

// ─── Correlation ───
MarketCorrelation { condition_id_a, condition_id_b, relationship: CorrelationRelationship }
CorrelationRelationship { ImpliedBy, MutuallyExclusive, Exhaustive, Custom { constraint, bound } }
```

### traits.rs — Trait Contracts

```rust
#[async_trait]
trait MarketDataSource: Send + Sync {
    async fn fetch_markets(&self) -> Result<Vec<MarketState>>;
    async fn fetch_orderbook(&self, token_id: &str) -> Result<OrderbookSnapshot>;
    async fn fetch_orderbooks(&self, token_ids: &[String]) -> Result<Vec<OrderbookSnapshot>>;
}

#[async_trait]
trait ArbDetector: Send + Sync {
    fn arb_type(&self) -> ArbType;
    async fn scan(&self, markets: &[MarketState]) -> Result<Vec<Opportunity>>;
}

trait SlippageEstimator: Send + Sync {
    fn estimate_vwap(&self, book: &OrderbookSnapshot, side: Side, size: Decimal) -> Result<VwapEstimate>;
    fn split_order(&self, book: &OrderbookSnapshot, side: Side, total_size: Decimal, max_slippage_bps: Decimal) -> Result<Vec<OrderChunk>>;
}

#[async_trait]
trait TradeExecutor: Send + Sync {
    async fn execute_opportunity(&self, opp: &Opportunity) -> Result<ExecutionReport>;
    async fn cancel_all(&self) -> Result<()>;
    fn mode(&self) -> TradingMode;
}

trait RiskManager: Send + Sync {
    fn check_opportunity(&self, opp: &Opportunity) -> Result<RiskDecision>;
    fn record_execution(&mut self, report: &ExecutionReport);
    fn is_kill_switch_active(&self) -> bool;
    fn activate_kill_switch(&mut self, reason: &str);
    fn daily_pnl(&self) -> Decimal;
    fn current_exposure(&self) -> Decimal;
}

trait ProbabilityEstimator: Send + Sync {
    fn estimate(&self, market: &MarketState) -> Result<ProbEstimate>;
    fn update(&mut self, market: &MarketState, new_data: &MarketState);
}
```

### config.rs — Config Structure

```toml
[general]
trading_mode = "paper"       # "paper" | "live"
log_level = "info"
log_format = "json"
log_file = "~/.config/polymarket/arb.log"
state_file = "~/.config/polymarket/arb-state.json"

[polling]
hot_interval_secs = 5        # markets with volume > 100k
warm_interval_secs = 15      # markets with volume > 10k
cold_interval_secs = 60      # all other markets
hot_volume_threshold = 100000
warm_volume_threshold = 10000

[strategy]
min_edge_bps = 50            # 50bps minimum net edge to trade
intra_market_enabled = true
cross_market_enabled = true
multi_outcome_enabled = true

[strategy.intra_market]
min_deviation = 0.005        # YES+NO must deviate >= 0.5c from $1.00

[strategy.cross_market]
correlation_file = "correlations.toml"
min_implied_edge = 0.02

[strategy.multi_outcome]
min_deviation = 0.01

[slippage]
max_slippage_bps = 100       # 1% max VWAP slippage
order_split_threshold = 500  # split orders above $500
prefer_post_only = true
vwap_depth_levels = 10

[risk]
max_position_per_market = 1000
max_total_exposure = 5000
daily_loss_limit = 200
max_open_orders = 20

[simulation]
monte_carlo_paths = 10000
importance_sampling_enabled = false
particle_count = 500
variance_reduction = ["antithetic"]

[alerts]
drawdown_warning_pct = 5.0
drawdown_critical_pct = 10.0
calibration_check_interval_mins = 60
```

### error.rs — Error Variants

```rust
ArbError {
    MarketData(String),
    Orderbook(String),
    InsufficientLiquidity { needed: Decimal, available: Decimal },
    SlippageTooHigh { actual_bps: Decimal, max_bps: Decimal },
    RiskLimit(String),
    KillSwitch(String),
    Execution(String),
    Config(String),
    Simulation(String),
    Io(std::io::Error),
    Json(serde_json::Error),
    TomlParse(toml::de::Error),
    Sdk(String),
}
```

---

## arb-data

### poller.rs — MarketPoller

```rust
pub struct MarketPoller {
    gamma_client: polymarket_client_sdk::gamma::Client,
    // NOTE: clob_client will be optional — read-only poller doesn't need auth
    last_poll: HashMap<String, Instant>,
    config: PollingConfig,
}

impl MarketPoller {
    pub fn new(config: PollingConfig) -> Self;
    pub fn polling_tier(&self, market: &MarketState) -> PollingTier;
    pub fn is_due(&self, condition_id: &str) -> bool;
    pub fn record_poll(&mut self, condition_id: &str);
}

#[async_trait]
impl MarketDataSource for MarketPoller {
    async fn fetch_markets(&self) -> Result<Vec<MarketState>>;
    async fn fetch_orderbook(&self, token_id: &str) -> Result<OrderbookSnapshot>;
    async fn fetch_orderbooks(&self, token_ids: &[String]) -> Result<Vec<OrderbookSnapshot>>;
}

enum PollingTier { Hot, Warm, Cold }
```

**SDK integration:**
- `gamma::Client::default()` for market list (no auth needed)
- `clob::Client` (unauthenticated) for orderbook data
- Convert SDK types to our `MarketState` / `OrderbookSnapshot`

### orderbook.rs — OrderbookProcessor

```rust
pub struct OrderbookProcessor {
    config: SlippageConfig,
}

impl OrderbookProcessor {
    pub fn new(config: SlippageConfig) -> Self;
    pub fn convert_sdk_orderbook(response: &OrderBookSummaryResponse, token_id: &str) -> OrderbookSnapshot;
}

impl SlippageEstimator for OrderbookProcessor {
    fn estimate_vwap(&self, book: &OrderbookSnapshot, side: Side, size: Decimal) -> Result<VwapEstimate>;
    fn split_order(&self, book: &OrderbookSnapshot, side: Side, total: Decimal, max_slip: Decimal) -> Result<Vec<OrderChunk>>;
}
```

### market_cache.rs — MarketCache

```rust
pub struct MarketCache {
    markets: DashMap<String, MarketState>,  // keyed by condition_id
}

impl MarketCache {
    pub fn new() -> Self;
    pub fn update(&self, markets: &[MarketState]);
    pub fn update_one(&self, market: MarketState);
    pub fn get(&self, condition_id: &str) -> Option<MarketState>;
    pub fn all_markets(&self) -> Vec<MarketState>;
    pub fn active_markets(&self) -> Vec<MarketState>;
    pub fn len(&self) -> usize;
    pub fn is_empty(&self) -> bool;
}
```

### correlation.rs — CorrelationGraph

```rust
pub struct CorrelationGraph {
    pairs: Vec<MarketCorrelation>,
}

impl CorrelationGraph {
    pub fn load(path: &Path) -> Result<Self>;
    pub fn pairs(&self) -> &[MarketCorrelation];
    pub fn pairs_for_market(&self, condition_id: &str) -> Vec<&MarketCorrelation>;
}
```

**Correlation file format:**
```toml
[[pairs]]
condition_id_a = "0xabc..."
condition_id_b = "0xdef..."
relationship = "implied_by"

[[pairs]]
condition_id_a = "0x123..."
condition_id_b = "0x456..."
relationship = "mutually_exclusive"
```

---

## arb-strategy

### intra_market.rs — IntraMarketDetector

```rust
pub struct IntraMarketDetector {
    config: IntraMarketConfig,
    strategy_config: StrategyConfig,
    slippage_estimator: Arc<dyn SlippageEstimator>,
}

#[async_trait]
impl ArbDetector for IntraMarketDetector {
    fn arb_type(&self) -> ArbType { ArbType::IntraMarket }
    async fn scan(&self, markets: &[MarketState]) -> Result<Vec<Opportunity>>;
}
```

### multi_outcome.rs — MultiOutcomeDetector

```rust
pub struct MultiOutcomeDetector {
    config: MultiOutcomeConfig,
    strategy_config: StrategyConfig,
    slippage_estimator: Arc<dyn SlippageEstimator>,
}

#[async_trait]
impl ArbDetector for MultiOutcomeDetector {
    fn arb_type(&self) -> ArbType { ArbType::MultiOutcome }
    async fn scan(&self, markets: &[MarketState]) -> Result<Vec<Opportunity>>;
}
```

### cross_market.rs — CrossMarketDetector

```rust
pub struct CrossMarketDetector {
    config: CrossMarketConfig,
    strategy_config: StrategyConfig,
    correlation_graph: Arc<CorrelationGraph>,
    slippage_estimator: Arc<dyn SlippageEstimator>,
}

#[async_trait]
impl ArbDetector for CrossMarketDetector {
    fn arb_type(&self) -> ArbType { ArbType::CrossMarket }
    async fn scan(&self, markets: &[MarketState]) -> Result<Vec<Opportunity>>;
}
```

### edge.rs — EdgeCalculator

```rust
pub struct EdgeCalculator {
    fee_rate: Decimal,  // Polymarket fee rate (currently 0.02 = 2%)
}

impl EdgeCalculator {
    pub fn new(fee_rate: Decimal) -> Self;
    pub fn refine_with_vwap(&self, opp: &mut Opportunity, cache: &MarketCache) -> Result<()>;
    pub fn calculate_fees(&self, legs: &[TradeLeg]) -> Decimal;
    pub fn structural_ev(&self, price_sum: Decimal, target: Decimal) -> Decimal;
}
```

---

## arb-simulation

### monte_carlo.rs

```rust
pub struct MonteCarloParams {
    pub initial_price: f64,
    pub drift: f64,
    pub volatility: f64,
    pub time_horizon: f64,
    pub strike: f64,
    pub n_paths: usize,
}

pub struct MonteCarloResult {
    pub probability: f64,
    pub standard_error: f64,
    pub confidence_interval: (f64, f64),
    pub n_paths: usize,
}

pub fn run_monte_carlo(params: &MonteCarloParams) -> MonteCarloResult;
```

### variance_reduction.rs

```rust
pub struct MonteCarloBuilder {
    params: MonteCarloParams,
    antithetic: bool,
    control_variate: Option<f64>,  // known analytical price
    n_strata: Option<usize>,
}

impl MonteCarloBuilder {
    pub fn new(params: MonteCarloParams) -> Self;
    pub fn with_antithetic(self) -> Self;
    pub fn with_control_variate(self, known_price: f64) -> Self;
    pub fn with_stratification(self, n_strata: usize) -> Self;
    pub fn build(self) -> VarianceReducedMC;
}

pub struct VarianceReducedMC { /* fields */ }
impl VarianceReducedMC {
    pub fn run(&self) -> MonteCarloResult;
}
```

### particle_filter.rs

```rust
pub struct ParticleFilter {
    particles: Vec<f64>,      // logit(p) space
    weights: Vec<f64>,
    process_vol: f64,
    obs_noise: f64,
}

impl ParticleFilter {
    pub fn new(n_particles: usize, initial_price: f64, process_vol: f64, obs_noise: f64) -> Self;
    pub fn update(&mut self, observed_price: f64);
    pub fn estimate(&self) -> ProbEstimate;
    pub fn effective_sample_size(&self) -> f64;
}
```

### copula.rs

```rust
pub struct TCopula {
    correlation_matrix: nalgebra::DMatrix<f64>,
    degrees_of_freedom: f64,
    cholesky: nalgebra::DMatrix<f64>,
}

impl TCopula {
    pub fn new(correlation_matrix: nalgebra::DMatrix<f64>, df: f64) -> Result<Self>;
    pub fn sample(&self, n: usize) -> Vec<Vec<f64>>;
    pub fn joint_probability(&self, thresholds: &[f64]) -> f64;
}

pub struct ClaytonCopula {
    theta: f64,  // dependence parameter
}

impl ClaytonCopula {
    pub fn new(theta: f64) -> Self;
    pub fn sample_bivariate(&self, n: usize) -> Vec<(f64, f64)>;
    pub fn lower_tail_dependence(&self) -> f64;
}
```

### importance_sampling.rs

```rust
pub struct ImportanceSampler {
    params: MonteCarloParams,
    tilt: f64,
}

impl ImportanceSampler {
    pub fn new(params: MonteCarloParams) -> Self;
    pub fn optimal_tilt(&self) -> f64;
    pub fn run(&self) -> MonteCarloResult;
    pub fn effective_sample_size(&self) -> f64;
}
```

### agent_model.rs

```rust
pub struct AgentSimulation {
    informed_count: usize,
    noise_count: usize,
    mm_count: usize,
    true_value: f64,
    initial_price: f64,
    n_steps: usize,
}

pub struct SimulationTrace {
    pub prices: Vec<f64>,
    pub volumes: Vec<f64>,
    pub spreads: Vec<f64>,
    pub convergence_time: Option<usize>,
}

impl AgentSimulation {
    pub fn new(/* params */) -> Self;
    pub fn run(&self) -> SimulationTrace;
}
```

---

## arb-execution

### slippage.rs — VwapSlippageEstimator

```rust
pub struct VwapSlippageEstimator {
    config: SlippageConfig,
}

impl SlippageEstimator for VwapSlippageEstimator {
    fn estimate_vwap(&self, book: &OrderbookSnapshot, side: Side, size: Decimal) -> Result<VwapEstimate>;
    fn split_order(&self, book: &OrderbookSnapshot, side: Side, total: Decimal, max_slip: Decimal) -> Result<Vec<OrderChunk>>;
}
```

### paper_trade.rs — PaperTradeExecutor

```rust
pub struct PaperTradeExecutor {
    positions: HashMap<String, Position>,
    trade_log: Vec<ExecutionReport>,
    pessimism_factor: Decimal,  // multiply VWAP slippage by this (e.g. 1.2 = 20% worse fills)
    slippage_estimator: Arc<dyn SlippageEstimator>,
}

#[async_trait]
impl TradeExecutor for PaperTradeExecutor {
    async fn execute_opportunity(&self, opp: &Opportunity) -> Result<ExecutionReport>;
    async fn cancel_all(&self) -> Result<()>;
    fn mode(&self) -> TradingMode { TradingMode::Paper }
}
```

### executor.rs — LiveTradeExecutor

```rust
pub struct LiveTradeExecutor {
    clob_client: polymarket_client_sdk::clob::Client<Authenticated<Normal>>,
    slippage_estimator: Arc<dyn SlippageEstimator>,
    config: SlippageConfig,
}

#[async_trait]
impl TradeExecutor for LiveTradeExecutor {
    async fn execute_opportunity(&self, opp: &Opportunity) -> Result<ExecutionReport>;
    async fn cancel_all(&self) -> Result<()>;
    fn mode(&self) -> TradingMode { TradingMode::Live }
}
```

---

## arb-risk

### limits.rs — RiskLimits

```rust
pub struct RiskLimits {
    config: RiskConfig,
    positions: Arc<Mutex<PositionTracker>>,
    kill_switch: KillSwitch,
    daily_pnl: Decimal,
    daily_reset: DateTime<Utc>,
    trade_log: Vec<ExecutionReport>,
}

impl RiskManager for RiskLimits {
    fn check_opportunity(&self, opp: &Opportunity) -> Result<RiskDecision>;
    fn record_execution(&mut self, report: &ExecutionReport);
    fn is_kill_switch_active(&self) -> bool;
    fn activate_kill_switch(&mut self, reason: &str);
    fn daily_pnl(&self) -> Decimal;
    fn current_exposure(&self) -> Decimal;
}
```

### position_tracker.rs

```rust
pub struct PositionTracker {
    positions: HashMap<String, Position>,  // keyed by token_id
}

impl PositionTracker {
    pub fn new() -> Self;
    pub fn update(&mut self, report: &ExecutionReport);
    pub fn get(&self, token_id: &str) -> Option<&Position>;
    pub fn all_positions(&self) -> Vec<&Position>;
    pub fn total_exposure(&self) -> Decimal;
    pub fn market_exposure(&self, condition_id: &str) -> Decimal;
    pub fn save(&self, path: &Path) -> Result<()>;
    pub fn load(path: &Path) -> Result<Self>;
}
```

### kill_switch.rs

```rust
pub struct KillSwitch {
    flag_path: PathBuf,  // ~/.config/polymarket/KILL_SWITCH
    active: bool,
    reason: Option<String>,
    activated_at: Option<DateTime<Utc>>,
}

impl KillSwitch {
    pub fn new() -> Self;
    pub fn check(&mut self) -> bool;      // re-reads file
    pub fn activate(&mut self, reason: &str);
    pub fn deactivate(&mut self);
    pub fn is_active(&self) -> bool;
    pub fn reason(&self) -> Option<&str>;
}
```

### metrics.rs

```rust
pub struct PerformanceMetrics {
    predictions: Vec<(f64, bool)>,         // (predicted_prob, actual_outcome)
    pnl_by_type: HashMap<ArbType, Decimal>,
    peak_equity: Decimal,
    current_equity: Decimal,
    execution_reports: Vec<ExecutionReport>,
}

impl PerformanceMetrics {
    pub fn brier_score(&self) -> f64;
    pub fn drawdown_pct(&self) -> f64;
    pub fn execution_quality(&self) -> Decimal;  // avg(realized/expected)
    pub fn record_prediction(&mut self, predicted: f64, actual: bool);
    pub fn record_equity(&mut self, equity: Decimal);
}
```

---

## arb-monitor

### logger.rs

```rust
pub fn init_logging(config: &GeneralConfig) -> Result<tracing_appender::non_blocking::WorkerGuard>;
```

Sets up `tracing-subscriber` with:
- JSON format (structured events)
- Configurable level filter
- Stdout + optional file appender
- Non-blocking file writes

### alerts.rs

```rust
pub struct AlertManager {
    config: AlertsConfig,
    last_calibration_check: Instant,
}

impl AlertManager {
    pub fn new(config: AlertsConfig) -> Self;
    pub fn check_drawdown(&self, metrics: &PerformanceMetrics);
    pub fn check_calibration(&mut self, metrics: &PerformanceMetrics);
    pub fn log_opportunity(&self, opp: &Opportunity);
    pub fn log_execution(&self, report: &ExecutionReport);
    pub fn log_rejected(&self, opp: &Opportunity, reason: &str);
    pub fn log_kill_switch(&self, reason: &str);
}
```

---

## arb-daemon

### CLI structure

```rust
#[derive(Subcommand)]
enum ArbCommands {
    Scan { /* options */ },
    Run { #[arg(long)] live: bool },
    Status,
    Kill,
    Resume,
    History { #[arg(short, long, default_value = "20")] limit: usize },
    Config,
    Simulate { condition_id: String },
}
```

### engine.rs — Main Event Loop

```rust
pub struct ArbEngine {
    config: ArbConfig,
    poller: MarketPoller,
    cache: MarketCache,
    detectors: Vec<Box<dyn ArbDetector>>,
    edge_calculator: EdgeCalculator,
    executor: Box<dyn TradeExecutor>,
    risk_manager: Box<dyn RiskManager>,
    monitor: AlertManager,
    prob_engine: Option<ParticleFilter>,
}

impl ArbEngine {
    pub async fn new(config: ArbConfig) -> Result<Self>;
    pub async fn run(&mut self) -> Result<()>;      // main loop
    pub async fn scan_once(&mut self) -> Result<Vec<Opportunity>>;  // one-shot
}
```
