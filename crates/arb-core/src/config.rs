use rust_decimal::Decimal;
use serde::{Deserialize, Serialize};
use std::path::{Path, PathBuf};

use crate::error::{ArbError, Result};

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ArbConfig {
    pub general: GeneralConfig,
    pub polling: PollingConfig,
    pub strategy: StrategyConfig,
    pub slippage: SlippageConfig,
    pub risk: RiskConfig,
    pub simulation: SimulationConfig,
    pub alerts: AlertsConfig,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct GeneralConfig {
    #[serde(default = "default_trading_mode")]
    pub trading_mode: String,
    #[serde(default = "default_log_level")]
    pub log_level: String,
    #[serde(default = "default_log_format")]
    pub log_format: String,
    #[serde(default)]
    pub log_file: Option<String>,
    #[serde(default)]
    pub state_file: Option<String>,
    #[serde(default = "default_starting_equity")]
    pub starting_equity: Decimal,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct PollingConfig {
    #[serde(default = "default_hot_interval")]
    pub hot_interval_secs: u64,
    #[serde(default = "default_warm_interval")]
    pub warm_interval_secs: u64,
    #[serde(default = "default_cold_interval")]
    pub cold_interval_secs: u64,
    #[serde(default = "default_hot_volume_threshold")]
    pub hot_volume_threshold: u64,
    #[serde(default = "default_warm_volume_threshold")]
    pub warm_volume_threshold: u64,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct StrategyConfig {
    #[serde(default = "default_min_edge_bps")]
    pub min_edge_bps: u64,
    #[serde(default = "default_true")]
    pub intra_market_enabled: bool,
    #[serde(default)]
    pub cross_market_enabled: bool,
    #[serde(default)]
    pub multi_outcome_enabled: bool,
    #[serde(default)]
    pub intra_market: IntraMarketConfig,
    #[serde(default)]
    pub cross_market: CrossMarketConfig,
    #[serde(default)]
    pub multi_outcome: MultiOutcomeConfig,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct IntraMarketConfig {
    #[serde(default = "default_intra_min_deviation")]
    pub min_deviation: Decimal,
}

impl Default for IntraMarketConfig {
    fn default() -> Self {
        Self {
            min_deviation: default_intra_min_deviation(),
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CrossMarketConfig {
    #[serde(default)]
    pub correlation_file: Option<String>,
    #[serde(default = "default_cross_min_edge")]
    pub min_implied_edge: Decimal,
}

impl Default for CrossMarketConfig {
    fn default() -> Self {
        Self {
            correlation_file: None,
            min_implied_edge: default_cross_min_edge(),
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct MultiOutcomeConfig {
    #[serde(default = "default_multi_min_deviation")]
    pub min_deviation: Decimal,
}

impl Default for MultiOutcomeConfig {
    fn default() -> Self {
        Self {
            min_deviation: default_multi_min_deviation(),
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SlippageConfig {
    #[serde(default = "default_max_slippage_bps")]
    pub max_slippage_bps: u64,
    #[serde(default = "default_order_split_threshold")]
    pub order_split_threshold: u64,
    #[serde(default = "default_true")]
    pub prefer_post_only: bool,
    #[serde(default = "default_vwap_depth_levels")]
    pub vwap_depth_levels: usize,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct RiskConfig {
    #[serde(default = "default_max_position_per_market")]
    pub max_position_per_market: Decimal,
    #[serde(default = "default_max_total_exposure")]
    pub max_total_exposure: Decimal,
    #[serde(default = "default_daily_loss_limit")]
    pub daily_loss_limit: Decimal,
    #[serde(default = "default_max_open_orders")]
    pub max_open_orders: usize,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SimulationConfig {
    #[serde(default = "default_mc_paths")]
    pub monte_carlo_paths: usize,
    #[serde(default)]
    pub importance_sampling_enabled: bool,
    #[serde(default = "default_particle_count")]
    pub particle_count: usize,
    #[serde(default)]
    pub variance_reduction: Vec<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct AlertsConfig {
    #[serde(default = "default_drawdown_warning")]
    pub drawdown_warning_pct: f64,
    #[serde(default = "default_drawdown_critical")]
    pub drawdown_critical_pct: f64,
    #[serde(default = "default_calibration_interval")]
    pub calibration_check_interval_mins: u64,
    #[serde(default)]
    pub discord_webhook_url: Option<String>,
    #[serde(default)]
    pub telegram_bot_token: Option<String>,
    #[serde(default)]
    pub telegram_chat_id: Option<String>,
}

// ─── Defaults ────────────────────────────────────────────

fn default_trading_mode() -> String {
    "paper".into()
}
fn default_log_level() -> String {
    "info".into()
}
fn default_log_format() -> String {
    "json".into()
}
fn default_hot_interval() -> u64 {
    5
}
fn default_warm_interval() -> u64 {
    15
}
fn default_cold_interval() -> u64 {
    60
}
fn default_hot_volume_threshold() -> u64 {
    100_000
}
fn default_warm_volume_threshold() -> u64 {
    10_000
}
fn default_min_edge_bps() -> u64 {
    50
}
fn default_true() -> bool {
    true
}
fn default_intra_min_deviation() -> Decimal {
    Decimal::new(5, 3) // 0.005
}
fn default_cross_min_edge() -> Decimal {
    Decimal::new(2, 2) // 0.02
}
fn default_multi_min_deviation() -> Decimal {
    Decimal::new(1, 2) // 0.01
}
fn default_max_slippage_bps() -> u64 {
    100
}
fn default_order_split_threshold() -> u64 {
    500
}
fn default_vwap_depth_levels() -> usize {
    10
}
fn default_max_position_per_market() -> Decimal {
    Decimal::from(1000)
}
fn default_max_total_exposure() -> Decimal {
    Decimal::from(5000)
}
fn default_daily_loss_limit() -> Decimal {
    Decimal::from(200)
}
fn default_max_open_orders() -> usize {
    20
}
fn default_mc_paths() -> usize {
    10_000
}
fn default_particle_count() -> usize {
    500
}
fn default_drawdown_warning() -> f64 {
    5.0
}
fn default_drawdown_critical() -> f64 {
    10.0
}
fn default_calibration_interval() -> u64 {
    60
}
fn default_starting_equity() -> Decimal {
    Decimal::from(10_000)
}

// ─── Load/Save ───────────────────────────────────────────

impl ArbConfig {
    /// Default config directory: ~/.config/polymarket/
    pub fn config_dir() -> PathBuf {
        dirs::config_dir()
            .unwrap_or_else(|| PathBuf::from("."))
            .join("polymarket")
    }

    /// Default config file path.
    pub fn default_path() -> PathBuf {
        Self::config_dir().join("arb-config.toml")
    }

    /// Load config from a specific path.
    pub fn load_from(path: &Path) -> Result<Self> {
        let content = std::fs::read_to_string(path).map_err(|e| {
            ArbError::Config(format!("Cannot read config at {}: {e}", path.display()))
        })?;
        toml::from_str(&content).map_err(|e| ArbError::Config(format!("Invalid config: {e}")))
    }

    /// Load config from the default path, falling back to defaults.
    pub fn load() -> Self {
        let path = Self::default_path();
        if path.exists() {
            Self::load_from(&path).unwrap_or_else(|e| {
                tracing::warn!("Failed to load config from {}: {e}. Using defaults.", path.display());
                Self::default()
            })
        } else {
            Self::default()
        }
    }

    /// Save config to the default path.
    pub fn save(&self) -> Result<()> {
        let dir = Self::config_dir();
        std::fs::create_dir_all(&dir)?;
        let content = toml::to_string_pretty(self)
            .map_err(|e| ArbError::Config(format!("Serialize error: {e}")))?;
        std::fs::write(Self::default_path(), content)?;
        Ok(())
    }

    /// Resolve the state file path (expanding ~).
    pub fn state_file_path(&self) -> PathBuf {
        self.general
            .state_file
            .as_ref()
            .map(|p| {
                if p.starts_with("~/") {
                    dirs::home_dir()
                        .unwrap_or_default()
                        .join(p.strip_prefix("~/").unwrap())
                } else {
                    PathBuf::from(p)
                }
            })
            .unwrap_or_else(|| Self::config_dir().join("arb-state.json"))
    }

    pub fn is_live(&self) -> bool {
        self.general.trading_mode == "live"
    }

    /// Clone this config and apply sandbox overrides on top.
    pub fn with_overrides(&self, ov: &crate::types::SandboxConfigOverrides) -> Self {
        let mut c = self.clone();
        if let Some(v) = ov.min_edge_bps { c.strategy.min_edge_bps = v; }
        if let Some(v) = ov.intra_market_enabled { c.strategy.intra_market_enabled = v; }
        if let Some(v) = ov.cross_market_enabled { c.strategy.cross_market_enabled = v; }
        if let Some(v) = ov.multi_outcome_enabled { c.strategy.multi_outcome_enabled = v; }
        if let Some(v) = ov.intra_min_deviation { c.strategy.intra_market.min_deviation = v; }
        if let Some(v) = ov.cross_min_implied_edge { c.strategy.cross_market.min_implied_edge = v; }
        if let Some(v) = ov.multi_min_deviation { c.strategy.multi_outcome.min_deviation = v; }
        if let Some(v) = ov.max_slippage_bps { c.slippage.max_slippage_bps = v; }
        if let Some(v) = ov.vwap_depth_levels { c.slippage.vwap_depth_levels = v; }
        if let Some(v) = ov.max_position_per_market { c.risk.max_position_per_market = v; }
        if let Some(v) = ov.max_total_exposure { c.risk.max_total_exposure = v; }
        if let Some(v) = ov.daily_loss_limit { c.risk.daily_loss_limit = v; }
        c
    }

    /// Validate config values are within sane bounds.
    /// Returns a list of human-readable error messages, or empty vec if valid.
    pub fn validate(&self) -> Vec<String> {
        let mut errors = Vec::new();

        if self.risk.max_total_exposure <= Decimal::ZERO {
            errors.push("risk.max_total_exposure must be positive".into());
        }
        if self.risk.max_position_per_market <= Decimal::ZERO {
            errors.push("risk.max_position_per_market must be positive".into());
        }
        if self.risk.daily_loss_limit <= Decimal::ZERO {
            errors.push("risk.daily_loss_limit must be positive".into());
        }
        if self.risk.max_open_orders == 0 {
            errors.push("risk.max_open_orders must be > 0".into());
        }
        if self.risk.max_position_per_market > self.risk.max_total_exposure {
            errors.push("risk.max_position_per_market must not exceed max_total_exposure".into());
        }
        if self.polling.hot_interval_secs == 0 {
            errors.push("polling.hot_interval_secs must be > 0".into());
        }
        if self.polling.warm_interval_secs == 0 {
            errors.push("polling.warm_interval_secs must be > 0".into());
        }
        if self.polling.cold_interval_secs == 0 {
            errors.push("polling.cold_interval_secs must be > 0".into());
        }
        if self.slippage.vwap_depth_levels == 0 {
            errors.push("slippage.vwap_depth_levels must be > 0".into());
        }

        errors
    }
}

impl Default for ArbConfig {
    fn default() -> Self {
        Self {
            general: GeneralConfig {
                trading_mode: default_trading_mode(),
                log_level: default_log_level(),
                log_format: default_log_format(),
                log_file: None,
                state_file: None,
                starting_equity: default_starting_equity(),
            },
            polling: PollingConfig {
                hot_interval_secs: default_hot_interval(),
                warm_interval_secs: default_warm_interval(),
                cold_interval_secs: default_cold_interval(),
                hot_volume_threshold: default_hot_volume_threshold(),
                warm_volume_threshold: default_warm_volume_threshold(),
            },
            strategy: StrategyConfig {
                min_edge_bps: default_min_edge_bps(),
                intra_market_enabled: true,
                cross_market_enabled: true,
                multi_outcome_enabled: true,
                intra_market: IntraMarketConfig::default(),
                cross_market: CrossMarketConfig::default(),
                multi_outcome: MultiOutcomeConfig::default(),
            },
            slippage: SlippageConfig {
                max_slippage_bps: default_max_slippage_bps(),
                order_split_threshold: default_order_split_threshold(),
                prefer_post_only: true,
                vwap_depth_levels: default_vwap_depth_levels(),
            },
            risk: RiskConfig {
                max_position_per_market: default_max_position_per_market(),
                max_total_exposure: default_max_total_exposure(),
                daily_loss_limit: default_daily_loss_limit(),
                max_open_orders: default_max_open_orders(),
            },
            simulation: SimulationConfig {
                monte_carlo_paths: default_mc_paths(),
                importance_sampling_enabled: false,
                particle_count: default_particle_count(),
                variance_reduction: vec!["antithetic".into()],
            },
            alerts: AlertsConfig {
                drawdown_warning_pct: default_drawdown_warning(),
                drawdown_critical_pct: default_drawdown_critical(),
                calibration_check_interval_mins: default_calibration_interval(),
                discord_webhook_url: None,
                telegram_bot_token: None,
                telegram_chat_id: None,
            },
        }
    }
}
