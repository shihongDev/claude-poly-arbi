use thiserror::Error;

#[derive(Error, Debug)]
pub enum ArbError {
    #[error("Market data error: {0}")]
    MarketData(String),

    #[error("Orderbook error: {0}")]
    Orderbook(String),

    #[error("Insufficient liquidity: need {needed}, available {available}")]
    InsufficientLiquidity {
        needed: rust_decimal::Decimal,
        available: rust_decimal::Decimal,
    },

    #[error("Slippage too high: {actual_bps}bps exceeds {max_bps}bps limit")]
    SlippageTooHigh {
        actual_bps: rust_decimal::Decimal,
        max_bps: rust_decimal::Decimal,
    },

    #[error("Risk limit exceeded: {0}")]
    RiskLimit(String),

    #[error("Kill switch active: {0}")]
    KillSwitch(String),

    #[error("Execution error: {0}")]
    Execution(String),

    #[error("Configuration error: {0}")]
    Config(String),

    #[error("Simulation error: {0}")]
    Simulation(String),

    #[error("IO error: {0}")]
    Io(#[from] std::io::Error),

    #[error("JSON error: {0}")]
    Json(#[from] serde_json::Error),

    #[error("TOML parse error: {0}")]
    TomlParse(#[from] toml::de::Error),

    #[error("SDK error: {0}")]
    Sdk(String),
}

pub type Result<T> = std::result::Result<T, ArbError>;
