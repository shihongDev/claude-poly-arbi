//! Pre-flight checks for live trading.
//!
//! Verifies wallet conditions (gas balance, etc.) before starting the live
//! trading engine. Should be called once at startup when `--live` is passed.

use alloy::primitives::{Address, U256};
use alloy::providers::{Provider, ProviderBuilder};
use tracing::{error, info};

/// Polygon RPC endpoint (same one used by the CLI's auth module).
const POLYGON_RPC: &str = "https://polygon.drpc.org";

/// Minimum POL balance in wei: 0.1 POL (~$0.04, enough for ~100 transactions).
const MIN_POL_WEI: u128 = 100_000_000_000_000_000;

/// Result of the pre-flight checks.
#[derive(Debug, Clone)]
pub struct PreflightResult {
    /// Whether the POL (native gas token) balance is sufficient.
    pub pol_balance_sufficient: bool,
    /// USDC.e balance on Polygon (placeholder — requires ERC-20 ABI call).
    pub usdc_balance: U256,
    /// Human-readable warnings collected during checks.
    pub warnings: Vec<String>,
}

impl PreflightResult {
    /// Returns `true` if all checks passed with no warnings.
    pub fn is_ok(&self) -> bool {
        self.pol_balance_sufficient && self.warnings.is_empty()
    }
}

/// Run pre-flight checks for the given EOA address.
///
/// Currently checks:
/// - Native POL balance (must be >= 0.1 POL for gas)
///
/// Placeholder (TODO):
/// - USDC.e ERC-20 balance (requires `sol!` macro or manual ABI encoding)
pub async fn run_preflight_checks(address: Address) -> anyhow::Result<PreflightResult> {
    let provider = ProviderBuilder::new().connect_http(POLYGON_RPC.parse()?);
    let mut warnings = Vec::new();

    // Check native token (POL) balance for gas
    let pol_balance = provider.get_balance(address).await?;
    let pol_sufficient = pol_balance >= U256::from(MIN_POL_WEI);

    if !pol_sufficient {
        let msg = format!("POL balance too low for gas: {} wei", pol_balance);
        error!("{}", msg);
        warnings.push(msg);
    } else {
        info!(balance = %pol_balance, "POL balance sufficient for gas");
    }

    // TODO: USDC.e balance check via ERC-20 balanceOf call.
    // Requires either the sol! macro or manual ABI encoding for:
    //   USDC.e on Polygon: 0x2791Bca1f2de4661ED88A30C99A7a9449Aa84174
    let usdc_balance = U256::ZERO;
    info!(usdc = %usdc_balance, "USDC.e balance check (placeholder)");

    Ok(PreflightResult {
        pol_balance_sufficient: pol_sufficient,
        usdc_balance,
        warnings,
    })
}
