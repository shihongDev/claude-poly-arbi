use arb_core::config::ArbConfig;
use tracing::info;

use crate::engine::ArbEngine;

/// Start the arbitrage daemon (paper or live mode).
pub async fn execute(live: bool) -> anyhow::Result<()> {
    let mut config = ArbConfig::load();

    if live {
        config.general.trading_mode = "live".into();
        println!("⚠  LIVE TRADING MODE ⚠");
        println!("Real orders will be placed on Polymarket.");
        println!("Press Ctrl+C to stop.\n");
    } else {
        config.general.trading_mode = "paper".into();
        println!("Paper trading mode (simulated execution).");
        println!("Press Ctrl+C to stop.\n");
    }

    // Initialize logging
    let _guard = arb_monitor::logger::init_logging(&config.general)?;

    info!(
        mode = config.general.trading_mode,
        min_edge_bps = config.strategy.min_edge_bps,
        intra = config.strategy.intra_market_enabled,
        cross = config.strategy.cross_market_enabled,
        multi = config.strategy.multi_outcome_enabled,
        "Starting arb daemon"
    );

    let mut engine = ArbEngine::new(config).await?;
    engine.run().await?;

    Ok(())
}
