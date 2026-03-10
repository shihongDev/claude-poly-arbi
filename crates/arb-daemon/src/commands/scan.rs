use std::collections::HashMap;
use std::sync::Arc;
use std::time::Instant;

use arb_core::config::ArbConfig;
use arb_core::traits::{ArbDetector, MarketDataSource, SlippageEstimator};
use arb_core::{MarketState, Opportunity};
use arb_data::market_cache::MarketCache;
use arb_data::orderbook::OrderbookProcessor;
use arb_data::poller::{ConcurrentFetchConfig, SdkMarketDataSource, classify_markets};
use arb_strategy::deadline::DeadlineMonotonicityDetector;
use arb_strategy::edge::EdgeCalculator;
use arb_strategy::intra_market::IntraMarketDetector;
use arb_strategy::multi_outcome::MultiOutcomeDetector;
use rust_decimal::Decimal;
use rust_decimal::prelude::ToPrimitive;
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
    let mut markets_with_books: Vec<Arc<MarketState>> = Vec::new();

    for market in &markets {
        let mut m = market.clone();
        let books = data_source.fetch_orderbooks(&m.token_ids).await?;
        m.orderbooks = books;
        cache.update_one(m.clone());
        markets_with_books.push(Arc::new(m));
    }

    println!(
        "Fetched orderbooks for {} markets\n",
        markets_with_books.len()
    );

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
    let edge_calc = EdgeCalculator::default_with_estimator(slippage_estimator.clone());
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
        println!(
            "No arbitrage opportunities found above {}bps minimum edge.",
            config.strategy.min_edge_bps
        );
    } else {
        println!("Found {} opportunities:\n", all_opportunities.len());
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

/// Comprehensive scan: fetch ALL active markets via pagination, fetch orderbooks
/// concurrently, run all detectors (intra, multi, deadline), rank by edge,
/// display VWAP tiers, and optionally export to JSON/CSV.
#[allow(clippy::too_many_arguments)]
pub async fn execute_comprehensive(
    min_edge_bps: u64,
    size_tiers_str: &str,
    export_json_path: Option<String>,
    export_csv_path: Option<String>,
    max_concurrent: usize,
    timeout_secs: u64,
    min_volume: u64,
    verbose: bool,
) -> anyhow::Result<()> {
    use crate::export::{self, OpportunityRow, ScanReport};

    let config = ArbConfig::load();
    let _guard = arb_monitor::logger::init_logging(&config.general).ok();
    let scan_start = Instant::now();

    // Parse size tiers from comma-separated string
    let size_tiers: Vec<Decimal> = size_tiers_str
        .split(',')
        .filter_map(|s| s.trim().parse::<Decimal>().ok())
        .collect();
    if size_tiers.is_empty() {
        anyhow::bail!("No valid size tiers parsed from '{size_tiers_str}'");
    }

    // ── Phase 1: Fetch all markets ─────────────────────────────────────
    println!("=== Comprehensive Polymarket Scan ===\n");
    println!("[1/4] Fetching all active markets...");

    let data_source = SdkMarketDataSource::new();
    let all_markets = data_source.fetch_all_active_markets().await?;
    println!("  {} total active markets fetched", all_markets.len());

    // Filter by minimum 24h volume if specified
    let markets: Vec<MarketState> = if min_volume > 0 {
        let min_vol = Decimal::from(min_volume);
        let filtered: Vec<MarketState> = all_markets
            .into_iter()
            .filter(|m| m.volume_24hr.unwrap_or_default() >= min_vol)
            .collect();
        println!(
            "  {} markets after --min-volume {} filter",
            filtered.len(),
            min_volume,
        );
        filtered
    } else {
        all_markets
    };

    let arc_markets: Vec<Arc<MarketState>> = markets.iter().map(|m| Arc::new(m.clone())).collect();
    let classified = classify_markets(&arc_markets);
    println!(
        "  {} classified ({} binary, {} neg-risk)",
        classified.binary.len() + classified.neg_risk.len(),
        classified.binary.len(),
        classified.neg_risk.len(),
    );
    println!(
        "  {} unique token IDs to fetch orderbooks for",
        classified.all_token_ids.len(),
    );

    // ── Phase 2: Fetch all orderbooks concurrently ─────────────────────
    println!(
        "\n[2/4] Fetching orderbooks concurrently (max_concurrent={max_concurrent}, timeout={timeout_secs}s)..."
    );

    let fetch_config = ConcurrentFetchConfig {
        max_concurrent,
        delay_ms: 50,
        timeout_secs,
    };
    let orderbook_map = data_source
        .fetch_orderbooks_concurrent(&classified.all_token_ids, &fetch_config)
        .await?;

    let total_fetched = orderbook_map.len();
    let total_failed = classified.all_token_ids.len().saturating_sub(total_fetched);

    println!(
        "  {} orderbooks fetched, {} failed",
        total_fetched, total_failed,
    );

    // ── Phase 3: Enrich markets, run detectors ─────────────────────────
    println!("\n[3/4] Running arbitrage detectors...");

    // Attach orderbooks to markets and populate cache
    let cache = MarketCache::new();
    let mut enriched_markets: Vec<Arc<MarketState>> = Vec::with_capacity(markets.len());

    for market in &markets {
        let mut m = market.clone();
        let books: Vec<_> = m
            .token_ids
            .iter()
            .filter_map(|tid| orderbook_map.get(tid).cloned())
            .collect();
        m.orderbooks = books;
        cache.update_one(m.clone());
        enriched_markets.push(Arc::new(m));
    }

    // Set up detectors
    let slippage_estimator: Arc<dyn SlippageEstimator> =
        Arc::new(OrderbookProcessor::new(config.slippage.clone()));

    let mut all_opportunities: Vec<(Opportunity, String)> = Vec::new();

    // Intra-market detector
    if config.strategy.intra_market_enabled {
        let detector = IntraMarketDetector::new(
            config.strategy.intra_market.clone(),
            config.strategy.clone(),
            slippage_estimator.clone(),
        );
        let opps = detector.scan(&enriched_markets).await?;
        if verbose {
            eprintln!("  IntraMarket: {} raw detections", opps.len());
        }
        for opp in opps {
            let question = find_question_arc(&enriched_markets, &opp);
            all_opportunities.push((opp, question));
        }
    }

    // Multi-outcome detector
    if config.strategy.multi_outcome_enabled {
        let detector = MultiOutcomeDetector::new(
            config.strategy.multi_outcome.clone(),
            config.strategy.clone(),
            slippage_estimator.clone(),
        );
        let opps = detector.scan(&enriched_markets).await?;
        if verbose {
            eprintln!("  MultiOutcome: {} raw detections", opps.len());
        }
        for opp in opps {
            let question = find_question_arc(&enriched_markets, &opp);
            all_opportunities.push((opp, question));
        }
    }

    // Deadline monotonicity detector on non-neg-risk markets
    let deadline_detector = DeadlineMonotonicityDetector::new();
    let event_groups = group_by_event_prefix(&classified.binary);
    if verbose {
        eprintln!(
            "  Deadline groups: {} (from {} binary markets)",
            event_groups.len(),
            classified.binary.len(),
        );
    }
    for group in &event_groups {
        // Enrich group markets with orderbooks
        let enriched_group: Vec<MarketState> = group
            .iter()
            .map(|m| {
                cache
                    .get(&m.condition_id)
                    .map(|arc| (*arc).clone())
                    .unwrap_or_else(|| m.clone())
            })
            .collect();

        let opps = deadline_detector.check_event_group(&enriched_group);
        if verbose && !opps.is_empty() {
            let prefix = enriched_group
                .first()
                .map(|m| m.question.as_str())
                .unwrap_or("?");
            eprintln!(
                "  Deadline: {} inversions in group starting with '{}'",
                opps.len(),
                prefix
            );
        }
        for opp in opps {
            let question = find_question_arc(&enriched_markets, &opp);
            all_opportunities.push((opp, question));
        }
    }

    // Refine with VWAP
    let edge_calc = EdgeCalculator::default_with_estimator(slippage_estimator.clone());
    for (opp, _) in &mut all_opportunities {
        let _ = edge_calc.refine_with_vwap(opp, &cache);
    }

    // Sort by gross_edge descending
    all_opportunities.sort_by(|a, b| b.0.gross_edge.cmp(&a.0.gross_edge));

    // Filter by minimum edge
    let min_edge = Decimal::from(min_edge_bps);
    let above_threshold: Vec<&(Opportunity, String)> = all_opportunities
        .iter()
        .filter(|(opp, _)| opp.net_edge_bps() >= min_edge)
        .collect();

    // ── Phase 4: Display and export ────────────────────────────────────
    println!("\n[4/4] Results\n");

    // Compute VWAP tiers for each opportunity
    let processor = OrderbookProcessor::new(config.slippage.clone());
    let mut opportunity_rows: Vec<OpportunityRow> = Vec::new();

    let display_list: &[(Opportunity, String)] = if above_threshold.is_empty() {
        // Show "near-miss" list (all detections)
        if all_opportunities.is_empty() {
            println!(
                "No arbitrage opportunities detected across {} markets.",
                markets.len()
            );
        } else {
            println!(
                "No opportunities above {}bps threshold. Showing {} near-miss detections:\n",
                min_edge_bps,
                all_opportunities.len(),
            );
        }
        &all_opportunities
    } else {
        println!(
            "Found {} opportunities above {}bps threshold:\n",
            above_threshold.len(),
            min_edge_bps,
        );
        // We'll iterate over above_threshold directly below
        &all_opportunities
    };

    // Print table header
    if !display_list.is_empty() {
        println!(
            "{:<5} {:<14} {:>8} {:>8} {:>8} {:>8} {:>8} {:>6} {:<50}",
            "Rank", "Type", "Edge", "V100", "V500", "V1000", "V5000", "Mkts", "Question"
        );
        println!("{}", "-".repeat(125));
    }

    let items_to_display: Vec<&(Opportunity, String)> = if !above_threshold.is_empty() {
        above_threshold
    } else {
        all_opportunities.iter().collect()
    };

    for (rank, (opp, question)) in items_to_display.iter().enumerate() {
        let vwap_edges = compute_vwap_edge_tiers(opp, &cache, &processor, &size_tiers);

        let row = export::opportunity_to_row(rank + 1, opp, question, &vwap_edges);

        let q_short = if question.len() > 48 {
            format!("{}...", &question[..45])
        } else {
            question.clone()
        };

        println!(
            "{:<5} {:<14} {:>7.0} {:>7.1} {:>7.1} {:>7.1} {:>7.1} {:>6} {:<50}",
            row.rank,
            row.arb_type,
            row.edge_bps,
            row.vwap_edge_100,
            row.vwap_edge_500,
            row.vwap_edge_1000,
            row.vwap_edge_5000,
            row.num_markets,
            q_short,
        );

        opportunity_rows.push(row);
    }

    // Market structure summary
    println!("\n--- Market Structure Summary ---\n");
    let summary_rows = compute_market_summary(&enriched_markets, &orderbook_map, &processor);

    println!(
        "{:<20} {:>8} {:>12} {:>16} {:>12}",
        "Category", "Markets", "Avg Spread", "Volume 24h", "Avg Depth"
    );
    println!("{}", "-".repeat(72));
    for row in &summary_rows {
        println!(
            "{:<20} {:>8} {:>11.1} {:>15.0} {:>11.0}",
            row.category, row.market_count, row.avg_spread_bps, row.total_volume_24h, row.avg_depth,
        );
    }

    let scan_time = scan_start.elapsed().as_secs_f64();
    println!(
        "\nScan completed in {:.1}s | {} markets | {} orderbooks | {} opportunities",
        scan_time,
        markets.len(),
        total_fetched,
        items_to_display.len(),
    );

    // Export
    if export_json_path.is_some() || export_csv_path.is_some() {
        let report = ScanReport {
            scan_time_secs: scan_time,
            total_markets: markets.len(),
            total_orderbooks_fetched: total_fetched,
            total_orderbooks_failed: total_failed,
            opportunities: opportunity_rows.clone(),
            market_summary: summary_rows,
        };

        if let Some(ref path) = export_json_path {
            export::export_json(&report, path)?;
            println!("Exported JSON report to {path}");
        }

        if let Some(ref path) = export_csv_path {
            export::export_csv(&opportunity_rows, path)?;
            println!("Exported CSV opportunities to {path}");
        }
    }

    Ok(())
}

/// Find the question text for an opportunity by matching its first market condition_id.
fn find_question_arc(markets: &[Arc<MarketState>], opp: &Opportunity) -> String {
    opp.markets
        .first()
        .and_then(|cid| markets.iter().find(|m| m.condition_id == *cid))
        .map(|m| m.question.clone())
        .unwrap_or_else(|| "Unknown".to_string())
}

/// Group non-neg-risk markets by event prefix for deadline monotonicity detection.
///
/// Looks for " by ", " before ", or " in " (case-insensitive) in the question text,
/// takes the prefix, and groups markets sharing the same prefix. Only returns groups
/// with 2 or more markets.
fn group_by_event_prefix(markets: &[MarketState]) -> Vec<Vec<MarketState>> {
    let mut groups: HashMap<String, Vec<MarketState>> = HashMap::new();

    for market in markets {
        let q_lower = market.question.to_lowercase();
        let prefix = [" by ", " before ", " in "]
            .iter()
            .filter_map(|sep| {
                q_lower
                    .find(sep)
                    .map(|pos| q_lower[..pos].trim().to_string())
            })
            .next();

        if let Some(prefix) = prefix
            && !prefix.is_empty()
        {
            groups.entry(prefix).or_default().push(market.clone());
        }
    }

    groups.into_values().filter(|g| g.len() >= 2).collect()
}

/// Compute VWAP edge in basis points at each size tier for an opportunity.
///
/// For the first leg's token, walks the appropriate side of the orderbook at each
/// tier size. Returns a 4-element array (padded with 0.0 if fewer tiers configured).
fn compute_vwap_edge_tiers(
    opp: &Opportunity,
    cache: &MarketCache,
    processor: &OrderbookProcessor,
    size_tiers: &[Decimal],
) -> [f64; 4] {
    let mut result = [0.0_f64; 4];

    let first_leg = match opp.legs.first() {
        Some(leg) => leg,
        None => return result,
    };

    // Find the orderbook for this leg's token
    let book = opp
        .markets
        .iter()
        .find_map(|cid| cache.get(cid))
        .and_then(|m| {
            m.orderbooks
                .iter()
                .find(|ob| ob.token_id == first_leg.token_id)
                .cloned()
        });

    let book = match book {
        Some(b) => b,
        None => return result,
    };

    let estimates = processor.estimate_vwap_tiers(&book, first_leg.side, size_tiers);

    for (i, est) in estimates.iter().enumerate().take(4) {
        if est.slippage_bps > Decimal::ZERO {
            // Use slippage as the VWAP edge degradation
            let edge_bps = opp.net_edge_bps() - est.slippage_bps;
            result[i] = edge_bps.to_f64().unwrap_or(0.0);
        } else {
            result[i] = opp.net_edge_bps().to_f64().unwrap_or(0.0);
        }
    }

    result
}

/// Compute aggregate market structure summary by category (binary vs neg-risk).
fn compute_market_summary(
    markets: &[Arc<MarketState>],
    orderbook_map: &HashMap<String, arb_core::OrderbookSnapshot>,
    processor: &OrderbookProcessor,
) -> Vec<crate::export::MarketSummaryRow> {
    use crate::export::MarketSummaryRow;

    let mut rows = Vec::new();

    for (category, filter_fn) in [
        (
            "binary",
            (|m: &&Arc<MarketState>| !m.neg_risk && m.token_ids.len() == 2)
                as fn(&&Arc<MarketState>) -> bool,
        ),
        (
            "neg_risk",
            (|m: &&Arc<MarketState>| m.neg_risk) as fn(&&Arc<MarketState>) -> bool,
        ),
        (
            "all",
            (|_: &&Arc<MarketState>| true) as fn(&&Arc<MarketState>) -> bool,
        ),
    ] {
        let subset: Vec<&Arc<MarketState>> = markets.iter().filter(filter_fn).collect();
        if subset.is_empty() {
            continue;
        }

        let mut total_spread_bps = 0.0_f64;
        let mut total_depth = 0.0_f64;
        let mut total_volume = 0.0_f64;
        let mut profile_count = 0_usize;

        for m in &subset {
            total_volume += m.volume_24hr.and_then(|v| v.to_f64()).unwrap_or(0.0);

            for tid in &m.token_ids {
                if let Some(book) = orderbook_map.get(tid) {
                    let profile = processor.spread_depth_profile(book);
                    total_spread_bps += profile.spread_bps.to_f64().unwrap_or(0.0);
                    total_depth += profile.bid_depth_5.to_f64().unwrap_or(0.0)
                        + profile.ask_depth_5.to_f64().unwrap_or(0.0);
                    profile_count += 1;
                }
            }
        }

        let avg_spread = if profile_count > 0 {
            total_spread_bps / profile_count as f64
        } else {
            0.0
        };
        let avg_depth = if profile_count > 0 {
            total_depth / profile_count as f64
        } else {
            0.0
        };

        rows.push(MarketSummaryRow {
            category: category.to_string(),
            market_count: subset.len(),
            avg_spread_bps: avg_spread,
            total_volume_24h: total_volume,
            avg_depth,
        });
    }

    rows
}
