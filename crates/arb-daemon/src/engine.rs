use std::sync::Arc;
use std::time::Duration;

use futures_util::FutureExt;

use arb_core::{
    RiskDecision,
    config::ArbConfig,
    error::Result as ArbResult,
    traits::{ArbDetector, MarketDataSource, RiskManager, SlippageEstimator, TradeExecutor},
};
use arb_data::correlation::CorrelationGraph;
use arb_data::market_cache::MarketCache;
use arb_data::orderbook::OrderbookProcessor;
use arb_data::poller::{MarketPoller, SdkMarketDataSource};
use arb_execution::paper_trade::PaperTradeExecutor;
use arb_monitor::alerts::AlertManager;
use arb_risk::limits::RiskLimits;
use arb_strategy::cross_market::CrossMarketDetector;
use arb_strategy::edge::EdgeCalculator;
use arb_strategy::intra_market::IntraMarketDetector;
use arb_strategy::liquidity_sniping::LiquiditySnipingDetector;
use arb_strategy::market_making::MarketMakingDetector;
use arb_strategy::multi_outcome::MultiOutcomeDetector;
use arb_strategy::resolution_sniping::ResolutionSnipingDetector;
use arb_strategy::stale_market::StaleMarketDetector;
use arb_strategy::volume_spike::VolumeSpikeDetector;
use rust_decimal::Decimal;
use tracing::{debug, error, info, warn};

/// The main arbitrage engine: orchestrates the detect→risk→execute pipeline.
pub struct ArbEngine {
    config: ArbConfig,
    data_source: SdkMarketDataSource,
    poller: MarketPoller,
    cache: Arc<MarketCache>,
    detectors: Vec<Box<dyn ArbDetector>>,
    edge_calculator: EdgeCalculator,
    executor: Box<dyn TradeExecutor>,
    risk_manager: RiskLimits,
    monitor: AlertManager,
}

impl ArbEngine {
    pub async fn new(config: ArbConfig) -> anyhow::Result<Self> {
        let data_source = SdkMarketDataSource::new();
        let poller = MarketPoller::new(config.polling.clone());
        let cache = Arc::new(MarketCache::new());

        let slippage_estimator: Arc<dyn SlippageEstimator> =
            Arc::new(OrderbookProcessor::new(config.slippage.clone()));

        // Build detectors based on config
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

        if config.strategy.cross_market_enabled {
            let correlation_graph = if let Some(ref file) =
                config.strategy.cross_market.correlation_file
            {
                let path = ArbConfig::config_dir().join(file);
                if path.exists() {
                    CorrelationGraph::load(&path).unwrap_or_else(|e| {
                        warn!("Failed to load correlation file: {e}. Cross-market disabled.");
                        CorrelationGraph::empty()
                    })
                } else {
                    info!(
                        "No correlation file found at {}. Cross-market detection will find no pairs.",
                        path.display()
                    );
                    CorrelationGraph::empty()
                }
            } else {
                CorrelationGraph::empty()
            };

            detectors.push(Box::new(CrossMarketDetector::new(
                config.strategy.cross_market.clone(),
                config.strategy.clone(),
                Arc::new(correlation_graph),
                cache.clone(),
                slippage_estimator.clone(),
            )));
        }

        if config.strategy.resolution_sniping_enabled {
            detectors.push(Box::new(ResolutionSnipingDetector::new(
                config.strategy.resolution_sniping.clone(),
                config.strategy.clone(),
                slippage_estimator.clone(),
            )));
        }

        if config.strategy.stale_market_enabled {
            detectors.push(Box::new(StaleMarketDetector::new(
                config.strategy.stale_market.clone(),
                config.strategy.clone(),
                slippage_estimator.clone(),
            )));
        }

        if config.strategy.volume_spike_enabled {
            detectors.push(Box::new(VolumeSpikeDetector::new(
                config.strategy.volume_spike.clone(),
                config.strategy.clone(),
                slippage_estimator.clone(),
            )));
        }

        if config.strategy.liquidity_sniping_enabled {
            detectors.push(Box::new(LiquiditySnipingDetector::new(
                config.strategy.liquidity_sniping.clone(),
                config.strategy.clone(),
                slippage_estimator.clone(),
            )));
        }

        if config.strategy.market_making_enabled {
            detectors.push(Box::new(MarketMakingDetector::new(
                config.strategy.market_making.clone(),
                config.strategy.clone(),
                slippage_estimator.clone(),
            )));
        }

        let edge_calculator = EdgeCalculator::default_with_estimator(slippage_estimator.clone());

        // Executor: paper by default, live requires --live flag
        let executor: Box<dyn TradeExecutor> = Box::new(PaperTradeExecutor::default_pessimism());

        let risk_manager = RiskLimits::new(config.risk.clone(), config.general.starting_equity);
        let monitor = AlertManager::new(config.alerts.clone());

        Ok(Self {
            config,
            data_source,
            poller,
            cache,
            detectors,
            edge_calculator,
            executor,
            risk_manager,
            monitor,
        })
    }

    /// Main event loop. Runs until Ctrl+C or kill switch.
    pub async fn run(&mut self) -> anyhow::Result<()> {
        info!("Arb engine starting main loop");

        // Initial market fetch
        info!("Fetching initial market data...");
        match self.data_source.fetch_markets().await {
            Ok(markets) => {
                info!(count = markets.len(), "Initial market fetch complete");
                self.cache.update(&markets);
            }
            Err(e) => {
                error!(error = %e, "Failed initial market fetch");
            }
        }

        loop {
            // Check for Ctrl+C
            if tokio::signal::ctrl_c().now_or_never().is_some() {
                info!("Ctrl+C received, shutting down...");
                break;
            }

            // 1. Check kill switch
            if self.risk_manager.is_kill_switch_active() {
                debug!("Kill switch active, sleeping...");
                tokio::time::sleep(Duration::from_secs(5)).await;
                continue;
            }

            // 2. Poll markets due for refresh
            let all_markets = self.cache.active_markets();
            let due_markets = self.poller.filter_due(&all_markets);

            if !due_markets.is_empty() {
                debug!(count = due_markets.len(), "Refreshing due markets");

                for market in &due_markets {
                    // Fetch fresh orderbooks
                    match self.data_source.fetch_orderbooks(&market.token_ids).await {
                        Ok(books) => {
                            let mut updated = (**market).clone();
                            updated.orderbooks = books;
                            self.cache.update_one(updated);
                            self.poller.record_poll(&market.condition_id);
                        }
                        Err(e) => {
                            debug!(
                                market = %market.condition_id,
                                error = %e,
                                "Failed to refresh orderbook"
                            );
                        }
                    }
                }
            }

            // 3. Run all enabled detectors
            let markets_snapshot = self.cache.active_markets();
            let mut opportunities = Vec::new();

            for detector in &self.detectors {
                match detector.scan(&markets_snapshot).await {
                    Ok(opps) => {
                        if !opps.is_empty() {
                            debug!(
                                detector = %detector.arb_type(),
                                count = opps.len(),
                                "Opportunities found"
                            );
                        }
                        opportunities.extend(opps);
                    }
                    Err(e) => {
                        debug!(
                            detector = %detector.arb_type(),
                            error = %e,
                            "Detector error"
                        );
                    }
                }
            }

            // 4. Refine edge with VWAP
            for opp in &mut opportunities {
                let _ = self.edge_calculator.refine_with_vwap(opp, &self.cache);
            }

            // 5. Filter by minimum edge
            let min_edge = Decimal::from(self.config.strategy.min_edge_bps);
            opportunities.retain(|o| o.net_edge_bps() >= min_edge);

            // 6. Sort by net edge (best first)
            opportunities.sort_by(|a, b| b.net_edge.cmp(&a.net_edge));

            // 7. Risk check + execute
            for opp in &opportunities {
                self.monitor.log_opportunity(opp);

                match self.risk_manager.check_opportunity(opp) {
                    Ok(RiskDecision::Approve { max_size }) => {
                        let sized_opp = opp.with_max_size(max_size);
                        match self.executor.execute_opportunity(&sized_opp).await {
                            Ok(report) => {
                                self.risk_manager.record_execution(&report, opp.arb_type);
                                self.monitor.log_execution(&report);
                            }
                            Err(e) => {
                                error!(
                                    opportunity_id = %opp.id,
                                    error = %e,
                                    "Execution failed"
                                );
                            }
                        }
                    }
                    Ok(RiskDecision::ReduceSize { new_size, reason }) => {
                        info!(
                            opportunity_id = %opp.id,
                            new_size = %new_size,
                            reason = %reason,
                            "Size reduced by risk manager"
                        );
                        let sized_opp = opp.with_max_size(new_size);
                        match self.executor.execute_opportunity(&sized_opp).await {
                            Ok(report) => {
                                self.risk_manager.record_execution(&report, opp.arb_type);
                                self.monitor.log_execution(&report);
                            }
                            Err(e) => {
                                error!(
                                    opportunity_id = %opp.id,
                                    error = %e,
                                    "Execution failed"
                                );
                            }
                        }
                    }
                    Ok(RiskDecision::Reject { reason }) => {
                        self.monitor.log_rejected(opp, &reason);
                    }
                    Err(e) => {
                        error!(
                            opportunity_id = %opp.id,
                            error = %e,
                            "Risk check error"
                        );
                    }
                }
            }

            // 8. Check daily limits
            if self.risk_manager.daily_pnl() < -self.config.risk.daily_loss_limit {
                let reason = format!(
                    "Daily loss limit hit: {} < -{}",
                    self.risk_manager.daily_pnl(),
                    self.config.risk.daily_loss_limit
                );
                self.risk_manager.activate_kill_switch(&reason);
                self.monitor.log_kill_switch(&reason);
            }

            // 9. Check drawdown alerts
            self.monitor
                .check_drawdown(self.risk_manager.metrics().drawdown_pct());

            // Sleep before next tick
            tokio::time::sleep(Duration::from_millis(100)).await;
        }

        // Graceful shutdown: save state
        info!("Saving state...");
        let state_path = self.config.state_file_path();
        if let Ok(tracker) = self.risk_manager.positions().lock() {
            if let Err(e) = tracker.save(&state_path) {
                error!(error = %e, "Failed to save position state");
            } else {
                info!(path = %state_path.display(), "State saved");
            }
        }

        Ok(())
    }

    /// One-shot scan (used by `arb scan` command).
    #[allow(dead_code)]
    pub async fn scan_once(&mut self) -> ArbResult<Vec<arb_core::Opportunity>> {
        let markets = self.cache.active_markets();
        let mut opportunities = Vec::new();

        for detector in &self.detectors {
            let opps = detector.scan(&markets).await?;
            opportunities.extend(opps);
        }

        for opp in &mut opportunities {
            let _ = self.edge_calculator.refine_with_vwap(opp, &self.cache);
        }

        let min_edge = Decimal::from(self.config.strategy.min_edge_bps);
        opportunities.retain(|o| o.net_edge_bps() >= min_edge);
        opportunities.sort_by(|a, b| b.net_edge.cmp(&a.net_edge));

        Ok(opportunities)
    }
}
