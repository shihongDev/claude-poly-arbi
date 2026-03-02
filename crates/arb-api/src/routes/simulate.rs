use axum::{
    Json,
    extract::{Path, State},
    http::StatusCode,
    response::IntoResponse,
};
use arb_simulation::monte_carlo::{MonteCarloParams, run_monte_carlo};
use arb_simulation::particle_filter::ParticleFilter;

use crate::state::AppState;

pub async fn run_simulation(
    Path(condition_id): Path<String>,
    State(state): State<AppState>,
) -> impl IntoResponse {
    let market = match state.market_cache.get(&condition_id) {
        Some(m) => m,
        None => {
            let json = serde_json::json!({"error": "market not found"});
            return (StatusCode::NOT_FOUND, Json(json)).into_response();
        }
    };

    // Use the first outcome price as the initial price for simulation
    let initial_price = market
        .outcome_prices
        .first()
        .map(|p| p.to_string().parse::<f64>().unwrap_or(0.5))
        .unwrap_or(0.5);

    let config = state.config.read().unwrap();
    let n_paths = config.simulation.monte_carlo_paths;
    let n_particles = config.simulation.particle_count;
    drop(config);

    // Run Monte Carlo simulation
    let mc_params = MonteCarloParams {
        initial_price,
        drift: 0.0,
        volatility: 0.3,
        time_horizon: 1.0,
        strike: 0.5,
        n_paths,
    };
    let mc_result = run_monte_carlo(&mc_params);

    // Run particle filter estimate
    let mut pf = ParticleFilter::new(n_particles, initial_price, 0.03, 0.02);
    // Feed current price as observation
    pf.update(initial_price);
    let pf_estimate = pf.estimate();

    let result = serde_json::json!({
        "condition_id": condition_id,
        "initial_price": initial_price,
        "monte_carlo": {
            "probability": mc_result.probability,
            "standard_error": mc_result.standard_error,
            "confidence_interval": mc_result.confidence_interval,
            "n_paths": mc_result.n_paths,
        },
        "particle_filter": {
            "probability": pf_estimate.probabilities,
            "confidence_interval": pf_estimate.confidence_interval,
            "method": pf_estimate.method,
        },
    });

    (StatusCode::OK, Json(result)).into_response()
}
