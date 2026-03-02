use arb_core::config::ArbConfig;
use arb_core::traits::{ArbDetector, MarketDataSource, SlippageEstimator};
use arb_data::market_cache::MarketCache;
use arb_data::orderbook::OrderbookProcessor;
use arb_data::poller::SdkMarketDataSource;
use arb_strategy::edge::EdgeCalculator;
use arb_strategy::intra_market::IntraMarketDetector;
use arb_strategy::multi_outcome::MultiOutcomeDetector;
use rust_decimal::Decimal;
use std::sync::Arc;
use tracing::info;

/// One-shot scan: fetch all markets, run detectors, print opportunities.
pub async fn execute() -> anyhow::Result<()> {
    let config = ArbConfig::load();

    // Initialize logging (minimal for one-shot)
    let _guard = arb_monitor::logger::init_logging(&config.general).ok();

    println!("Scanning Polymarket for arbitrage opportunities...\n");

    // Fetch markets
    let data_source = SdkMarketDataSource::new();
    let markets = data_source.fetch_markets().await?;
    println!("Fetched {} active markets", markets.len());

    // Fetch orderbooks for all markets
    let cache = MarketCache::new();
    let mut markets_with_books = Vec::new();

    for market in &markets {
        let mut m = market.clone();
        let books = data_source.fetch_orderbooks(&m.token_ids).await?;
        m.orderbooks = books;
        cache.update_one(m.clone());
        markets_with_books.push(m);
    }

    println!("Fetched orderbooks for {} markets\n", markets_with_books.len());

    // Set up detectors
    let slippage_estimator: Arc<dyn SlippageEstimator> =
        Arc::new(OrderbookProcessor::new(config.slippage.clone()));

    let mut detectors: Vec<Box<dyn ArbDetector>> = Vec::new();

    if config.strategy.intra_market_enabled {
        detectors.push(Box::new(IntraMarketDetector::new(
            config.strategy.intra_market.clone(),
            config.strategy.clone(),
            slippage_estimator.clone(),
        )));
    }

    if config.strategy.multi_outcome_enabled {
        detectors.push(Box::new(MultiOutcomeDetector::new(
            config.strategy.multi_outcome.clone(),
            config.strategy.clone(),
            slippage_estimator.clone(),
        )));
    }

    // Run all detectors
    let mut all_opportunities = Vec::new();
    for detector in &detectors {
        let opps = detector.scan(&markets_with_books).await?;
        info!(
            detector = %detector.arb_type(),
            count = opps.len(),
            "Detector scan complete"
        );
        all_opportunities.extend(opps);
    }

    // Refine with VWAP
    let edge_calc = EdgeCalculator::default_with_config(config.slippage.clone());
    for opp in &mut all_opportunities {
        let _ = edge_calc.refine_with_vwap(opp, &cache);
    }

    // Filter by minimum edge
    let min_edge = Decimal::from(config.strategy.min_edge_bps);
    all_opportunities.retain(|o| o.net_edge_bps() >= min_edge);

    // Sort by net edge (best first)
    all_opportunities.sort_by(|a, b| b.net_edge.cmp(&a.net_edge));

    // Print results
    if all_opportunities.is_empty() {
        println!("No arbitrage opportunities found above {}bps minimum edge.", config.strategy.min_edge_bps);
    } else {
        println!(
            "Found {} opportunities:\n",
            all_opportunities.len()
        );
        println!(
            "{:<8} {:<15} {:<12} {:<12} {:<10} {:<8} {:<36}",
            "Type", "Market", "Gross Edge", "Net Edge", "Edge BPS", "Size", "ID"
        );
        println!("{}", "-".repeat(100));

        for opp in &all_opportunities {
            let market_short = opp
                .markets
                .first()
                .map(|m| if m.len() > 12 { &m[..12] } else { m })
                .unwrap_or("?");

            println!(
                "{:<8} {:<15} {:<12} {:<12} {:<10} {:<8} {:<36}",
                opp.arb_type,
                market_short,
                format!("${:.4}", opp.gross_edge),
                format!("${:.4}", opp.net_edge),
                format!("{:.0}", opp.net_edge_bps()),
                format!("${:.0}", opp.size_available),
                opp.id,
            );
        }
    }

    Ok(())
}
