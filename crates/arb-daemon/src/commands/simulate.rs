use arb_core::config::ArbConfig;
use arb_core::traits::MarketDataSource;
use arb_data::poller::SdkMarketDataSource;
use arb_simulation::monte_carlo::{MonteCarloParams, run_monte_carlo};
use arb_simulation::particle_filter::ParticleFilter;
use arb_simulation::variance_reduction::MonteCarloBuilder;

/// Run the simulation engine on a specific market.
pub async fn execute(condition_id: &str) -> anyhow::Result<()> {
    let config = ArbConfig::load();
    let _guard = arb_monitor::logger::init_logging(&config.general).ok();

    println!("=== Simulation: {condition_id} ===\n");

    // Fetch market data
    let data_source = SdkMarketDataSource::new();
    let markets = data_source.fetch_markets().await?;

    let market = markets
        .iter()
        .find(|m| m.condition_id == condition_id)
        .ok_or_else(|| anyhow::anyhow!("Market not found: {condition_id}"))?;

    println!("Market: {}", market.question);
    println!("Outcomes: {:?}", market.outcomes);
    println!("Current prices: {:?}", market.outcome_prices);
    println!();

    // Get the YES price as our starting probability
    let yes_price = market
        .outcome_prices
        .first()
        .copied()
        .unwrap_or(rust_decimal::Decimal::new(5, 1));
    let p: f64 = rust_decimal::prelude::ToPrimitive::to_f64(&yes_price).unwrap_or(0.5);

    // ─── Monte Carlo ───
    println!("--- Monte Carlo Simulation ---");
    let mc_params = MonteCarloParams {
        initial_price: p,
        drift: 0.0,
        volatility: 0.3,
        time_horizon: 1.0,
        strike: 0.5,
        n_paths: config.simulation.monte_carlo_paths,
    };

    let mc_result = run_monte_carlo(&mc_params);
    println!("  Paths:       {}", mc_result.n_paths);
    println!("  Probability: {:.4}", mc_result.probability);
    println!("  Std Error:   {:.6}", mc_result.standard_error);
    println!(
        "  95% CI:      [{:.4}, {:.4}]",
        mc_result.confidence_interval.0, mc_result.confidence_interval.1
    );

    // ─── Variance-reduced MC ───
    println!("\n--- Variance-Reduced MC (antithetic + stratified) ---");
    let vr_result = MonteCarloBuilder::new(mc_params.clone())
        .with_antithetic()
        .with_stratification(10)
        .build()
        .run();
    println!("  Probability: {:.4}", vr_result.probability);
    println!("  Std Error:   {:.6}", vr_result.standard_error);
    println!(
        "  95% CI:      [{:.4}, {:.4}]",
        vr_result.confidence_interval.0, vr_result.confidence_interval.1
    );

    // ─── Particle Filter ───
    println!("\n--- Particle Filter (sequential update) ---");
    let mut pf = ParticleFilter::new(
        config.simulation.particle_count,
        p,
        0.05,
        0.03,
    );

    // Simulate 10 observations at the current price
    for _ in 0..10 {
        pf.update(p);
    }

    let pf_est = pf.estimate();
    println!("  Probability: {:.4}", pf_est.probabilities[0]);
    if let Some(&(lo, hi)) = pf_est.confidence_interval.first() {
        println!("  95% CI:      [{:.4}, {:.4}]", lo, hi);
    }
    println!("  ESS:         {:.0}", pf.effective_sample_size());

    println!("\n--- Summary ---");
    println!("  MC estimate:     {:.4}", mc_result.probability);
    println!("  VR-MC estimate:  {:.4}", vr_result.probability);
    println!("  PF estimate:     {:.4}", pf_est.probabilities[0]);
    println!("  Market price:    {:.4}", p);

    Ok(())
}
