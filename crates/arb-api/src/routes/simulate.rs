use arb_simulation::monte_carlo::{MonteCarloParams, run_monte_carlo};
use arb_simulation::particle_filter::ParticleFilter;
use axum::{
    Json,
    extract::{Path, State},
    http::StatusCode,
    response::IntoResponse,
};
use serde::Deserialize;

use crate::state::AppState;

#[derive(Deserialize, Default)]
pub struct SimulateRequest {
    pub num_paths: Option<usize>,
    pub volatility: Option<f64>,
    pub drift: Option<f64>,
    pub time_horizon: Option<f64>,
    pub strike: Option<f64>,
    pub particle_count: Option<usize>,
    pub process_noise: Option<f64>,
    pub observation_noise: Option<f64>,
}

pub async fn run_simulation(
    Path(condition_id): Path<String>,
    State(state): State<AppState>,
    body: Option<Json<SimulateRequest>>,
) -> impl IntoResponse {
    let req = body.map(|b| b.0).unwrap_or_default();

    let market = match state.market_cache.get(&condition_id) {
        Some(m) => m,
        None => {
            let json = serde_json::json!({"error": "market not found"});
            return (StatusCode::NOT_FOUND, Json(json)).into_response();
        }
    };

    let initial_price = market
        .outcome_prices
        .first()
        .map(|p| rust_decimal::prelude::ToPrimitive::to_f64(p).unwrap_or(0.5))
        .unwrap_or(0.5);

    let config = state.config.read().unwrap();
    let n_paths = req.num_paths.unwrap_or(config.simulation.monte_carlo_paths);
    let n_particles = req
        .particle_count
        .unwrap_or(config.simulation.particle_count);
    drop(config);

    let mc_params = MonteCarloParams {
        initial_price,
        drift: req.drift.unwrap_or(0.0),
        volatility: req.volatility.unwrap_or(0.3),
        time_horizon: req.time_horizon.unwrap_or(1.0),
        strike: req.strike.unwrap_or(0.5),
        n_paths,
    };
    let mc_result = run_monte_carlo(&mc_params);

    let process_noise = req.process_noise.unwrap_or(0.03);
    let observation_noise = req.observation_noise.unwrap_or(0.02);
    let mut pf = ParticleFilter::new(n_particles, initial_price, process_noise, observation_noise);
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
