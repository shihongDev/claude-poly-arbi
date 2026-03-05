use arb_core::Position;
use rust_decimal::Decimal;
use rust_decimal::prelude::*;
use serde::{Deserialize, Serialize};

/// Pre-defined stress scenarios for portfolio risk analysis.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub enum StressScenario {
    /// Sudden reduction in market depth — positions become harder to exit
    LiquidityShock {
        /// Percentage of depth removed (0.0 to 1.0)
        depth_reduction_pct: f64,
    },
    /// All positions become correlated — diversification benefit vanishes
    CorrelationSpike {
        /// Target correlation between all positions (0.0 to 1.0)
        target_correlation: f64,
    },
    /// Sudden adverse price move across all positions
    FlashCrash {
        /// Percentage adverse move (0.0 to 1.0, e.g. 0.20 = 20%)
        adverse_move_pct: f64,
    },
    /// Kill switch doesn't trigger immediately — positions keep losing
    KillSwitchDelay {
        /// How long the system continues trading before kill switch activates
        delay_secs: u64,
    },
}

/// Result of running a stress test scenario against a portfolio.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct StressTestResult {
    /// Human-readable scenario name
    pub scenario: String,
    /// Estimated PnL impact on the portfolio
    pub portfolio_impact: Decimal,
    /// Worst-case loss under this scenario
    pub max_loss: Decimal,
    /// Number of positions affected by this scenario
    pub positions_at_risk: usize,
    /// VaR before the stress event
    pub var_before: Decimal,
    /// VaR after the stress event (typically worse)
    pub var_after: Decimal,
}

/// Run a stress test scenario against a set of positions.
///
/// Each scenario models a different risk event and estimates the portfolio impact.
/// The `current_var` parameter is used as the baseline for VaR comparison.
pub fn run_stress_test(
    scenario: &StressScenario,
    positions: &[Position],
    current_var: Decimal,
) -> StressTestResult {
    match scenario {
        StressScenario::LiquidityShock {
            depth_reduction_pct,
        } => stress_liquidity_shock(positions, *depth_reduction_pct, current_var),
        StressScenario::CorrelationSpike {
            target_correlation,
        } => stress_correlation_spike(positions, *target_correlation, current_var),
        StressScenario::FlashCrash { adverse_move_pct } => {
            stress_flash_crash(positions, *adverse_move_pct, current_var)
        }
        StressScenario::KillSwitchDelay { delay_secs } => {
            stress_kill_switch_delay(positions, *delay_secs, current_var)
        }
    }
}

/// Liquidity shock: depth reduction means wider spreads and slippage.
///
/// Model: each position's exit cost increases proportionally to depth reduction.
/// If 50% of depth disappears, slippage roughly doubles.
/// Impact = sum(position_notional * slippage_increase_factor)
/// where slippage_increase_factor = depth_reduction / (1 - depth_reduction)
fn stress_liquidity_shock(
    positions: &[Position],
    depth_reduction_pct: f64,
    current_var: Decimal,
) -> StressTestResult {
    if positions.is_empty() {
        return empty_result("LiquidityShock", current_var);
    }

    let reduction = depth_reduction_pct.clamp(0.0, 0.99);
    // Slippage multiplier: if 50% depth removed, remaining depth is 50%,
    // so slippage increases by factor of 1/(1-reduction) - 1
    let slippage_factor = if reduction < 1.0 {
        reduction / (1.0 - reduction)
    } else {
        100.0 // near-total loss
    };

    let slippage_dec = Decimal::from_f64(slippage_factor).unwrap_or(Decimal::ONE);

    // Base slippage assumption: 1% of notional per position
    let base_slippage_rate = Decimal::from_f64(0.01).unwrap();

    let mut total_impact = Decimal::ZERO;
    let mut max_loss = Decimal::ZERO;
    let mut at_risk = 0usize;

    for pos in positions {
        if pos.size > Decimal::ZERO {
            let notional = pos.size * pos.current_price;
            let additional_slippage = notional * base_slippage_rate * slippage_dec;
            total_impact += additional_slippage;
            if additional_slippage > max_loss {
                max_loss = additional_slippage;
            }
            at_risk += 1;
        }
    }

    // VaR worsens by the total impact amount
    let var_after = current_var + total_impact;

    StressTestResult {
        scenario: format!("LiquidityShock({}%)", (reduction * 100.0) as u32),
        portfolio_impact: -total_impact, // negative = loss
        max_loss,
        positions_at_risk: at_risk,
        var_before: current_var,
        var_after,
    }
}

/// Correlation spike: all positions move together.
///
/// Model: portfolio variance under perfect correlation is (sum of individual volatilities)^2,
/// vs. sum of individual variances when uncorrelated.
/// Impact is the increase in portfolio standard deviation.
fn stress_correlation_spike(
    positions: &[Position],
    target_correlation: f64,
    current_var: Decimal,
) -> StressTestResult {
    if positions.is_empty() {
        return empty_result("CorrelationSpike", current_var);
    }

    let corr = target_correlation.clamp(0.0, 1.0);

    // Assume each position has volatility proportional to 10% of notional
    let base_vol_rate = 0.10;
    let individual_vols: Vec<f64> = positions
        .iter()
        .filter(|p| p.size > Decimal::ZERO)
        .map(|p| {
            let notional = (p.size * p.current_price).to_f64().unwrap_or(0.0);
            notional * base_vol_rate
        })
        .collect();

    let at_risk = individual_vols.len();

    if at_risk == 0 {
        return empty_result("CorrelationSpike", current_var);
    }

    // Uncorrelated portfolio variance: sum(vol_i^2)
    let uncorr_var: f64 = individual_vols.iter().map(|v| v * v).sum();

    // Correlated portfolio variance:
    // Var(P) = sum(vol_i^2) + 2 * corr * sum_{i<j}(vol_i * vol_j)
    let sum_vols: f64 = individual_vols.iter().sum();
    let sum_sq: f64 = individual_vols.iter().map(|v| v * v).sum();
    let cross_term = (sum_vols * sum_vols - sum_sq) * corr;
    let corr_var = sum_sq + cross_term;

    // Increase in portfolio std dev
    let uncorr_std = uncorr_var.sqrt();
    let corr_std = corr_var.sqrt();
    let std_increase = corr_std - uncorr_std;

    // Impact: 1.645 * std_increase (95% VaR impact)
    let impact = std_increase * 1.645;
    let impact_dec = Decimal::from_f64(impact).unwrap_or(Decimal::ZERO);

    let max_individual_loss = individual_vols
        .iter()
        .map(|v| v * 1.645)
        .fold(0.0_f64, f64::max);
    let max_loss_dec = Decimal::from_f64(max_individual_loss).unwrap_or(Decimal::ZERO);

    let var_after = current_var + impact_dec;

    StressTestResult {
        scenario: format!("CorrelationSpike({}%)", (corr * 100.0) as u32),
        portfolio_impact: -impact_dec,
        max_loss: max_loss_dec,
        positions_at_risk: at_risk,
        var_before: current_var,
        var_after,
    }
}

/// Flash crash: all positions move adversely by the given percentage.
///
/// Model: every position loses `adverse_move_pct` of its notional value.
fn stress_flash_crash(
    positions: &[Position],
    adverse_move_pct: f64,
    current_var: Decimal,
) -> StressTestResult {
    if positions.is_empty() {
        return empty_result("FlashCrash", current_var);
    }

    let move_pct = adverse_move_pct.clamp(0.0, 1.0);
    let move_dec = Decimal::from_f64(move_pct).unwrap_or(Decimal::ZERO);

    let mut total_loss = Decimal::ZERO;
    let mut max_single_loss = Decimal::ZERO;
    let mut at_risk = 0usize;

    for pos in positions {
        if pos.size > Decimal::ZERO {
            let notional = pos.size * pos.current_price;
            let loss = notional * move_dec;
            total_loss += loss;
            if loss > max_single_loss {
                max_single_loss = loss;
            }
            at_risk += 1;
        }
    }

    // Flash crash VaR: assume the crash itself becomes the new VaR
    let var_after = current_var.max(total_loss);

    StressTestResult {
        scenario: format!("FlashCrash({}%)", (move_pct * 100.0) as u32),
        portfolio_impact: -total_loss,
        max_loss: max_single_loss,
        positions_at_risk: at_risk,
        var_before: current_var,
        var_after,
    }
}

/// Kill switch delay: positions continue losing for `delay_secs` at worst observed rate.
///
/// Model: assume each position loses at a rate of 5% per minute (extreme scenario)
/// during the delay window.
fn stress_kill_switch_delay(
    positions: &[Position],
    delay_secs: u64,
    current_var: Decimal,
) -> StressTestResult {
    if positions.is_empty() {
        return empty_result("KillSwitchDelay", current_var);
    }

    // Worst-case bleed rate: 5% of notional per minute
    let bleed_rate_per_sec = 0.05 / 60.0;
    let total_bleed_factor = bleed_rate_per_sec * delay_secs as f64;
    let bleed_dec = Decimal::from_f64(total_bleed_factor.min(1.0)).unwrap_or(Decimal::ZERO);

    let mut total_loss = Decimal::ZERO;
    let mut max_single_loss = Decimal::ZERO;
    let mut at_risk = 0usize;

    for pos in positions {
        if pos.size > Decimal::ZERO {
            let notional = pos.size * pos.current_price;
            let loss = notional * bleed_dec;
            total_loss += loss;
            if loss > max_single_loss {
                max_single_loss = loss;
            }
            at_risk += 1;
        }
    }

    let var_after = current_var + total_loss;

    StressTestResult {
        scenario: format!("KillSwitchDelay({}s)", delay_secs),
        portfolio_impact: -total_loss,
        max_loss: max_single_loss,
        positions_at_risk: at_risk,
        var_before: current_var,
        var_after,
    }
}

fn empty_result(scenario_name: &str, current_var: Decimal) -> StressTestResult {
    StressTestResult {
        scenario: scenario_name.to_string(),
        portfolio_impact: Decimal::ZERO,
        max_loss: Decimal::ZERO,
        positions_at_risk: 0,
        var_before: current_var,
        var_after: current_var,
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use rust_decimal_macros::dec;

    fn make_position(token: &str, size: Decimal, price: Decimal) -> Position {
        Position {
            token_id: token.to_string(),
            condition_id: format!("cond_{}", token),
            size,
            avg_entry_price: price,
            current_price: price,
            unrealized_pnl: Decimal::ZERO,
        }
    }

    fn sample_portfolio() -> Vec<Position> {
        vec![
            make_position("yes_a", dec!(100), dec!(0.60)),
            make_position("no_b", dec!(200), dec!(0.40)),
            make_position("yes_c", dec!(150), dec!(0.50)),
        ]
    }

    #[test]
    fn test_liquidity_shock() {
        let positions = sample_portfolio();
        let result = run_stress_test(
            &StressScenario::LiquidityShock {
                depth_reduction_pct: 0.50,
            },
            &positions,
            dec!(10),
        );

        assert_eq!(result.positions_at_risk, 3);
        assert!(
            result.portfolio_impact < Decimal::ZERO,
            "Liquidity shock should cause negative impact"
        );
        assert!(
            result.max_loss > Decimal::ZERO,
            "Should have max loss > 0"
        );
        assert!(
            result.var_after > result.var_before,
            "VaR should worsen after liquidity shock"
        );
        assert!(result.scenario.contains("LiquidityShock"));
    }

    #[test]
    fn test_correlation_spike() {
        let positions = sample_portfolio();
        let result = run_stress_test(
            &StressScenario::CorrelationSpike {
                target_correlation: 0.90,
            },
            &positions,
            dec!(10),
        );

        assert_eq!(result.positions_at_risk, 3);
        assert!(
            result.portfolio_impact < Decimal::ZERO,
            "Correlation spike should cause negative impact"
        );
        assert!(
            result.var_after >= result.var_before,
            "VaR should worsen or stay same after correlation spike"
        );
        assert!(result.scenario.contains("CorrelationSpike"));
    }

    #[test]
    fn test_flash_crash() {
        let positions = sample_portfolio();
        let result = run_stress_test(
            &StressScenario::FlashCrash {
                adverse_move_pct: 0.20,
            },
            &positions,
            dec!(10),
        );

        assert_eq!(result.positions_at_risk, 3);

        // Total notional: 100*0.60 + 200*0.40 + 150*0.50 = 60 + 80 + 75 = 215
        // 20% loss = 43
        let expected_total_loss = dec!(43);
        assert_eq!(
            result.portfolio_impact, -expected_total_loss,
            "Flash crash 20% on 215 notional = -43"
        );

        // Max single loss: largest notional is 200*0.40=80, 20% of 80 = 16
        assert_eq!(result.max_loss, dec!(16));
        assert!(result.scenario.contains("FlashCrash"));
    }

    #[test]
    fn test_kill_switch_delay() {
        let positions = sample_portfolio();
        let result = run_stress_test(
            &StressScenario::KillSwitchDelay { delay_secs: 120 },
            &positions,
            dec!(10),
        );

        assert_eq!(result.positions_at_risk, 3);
        assert!(
            result.portfolio_impact < Decimal::ZERO,
            "Kill switch delay should cause losses"
        );
        assert!(
            result.var_after > result.var_before,
            "VaR should worsen during delay"
        );
        assert!(result.scenario.contains("KillSwitchDelay"));
    }

    #[test]
    fn test_empty_positions() {
        let empty: Vec<Position> = vec![];

        let scenarios = vec![
            StressScenario::LiquidityShock {
                depth_reduction_pct: 0.50,
            },
            StressScenario::CorrelationSpike {
                target_correlation: 0.90,
            },
            StressScenario::FlashCrash {
                adverse_move_pct: 0.20,
            },
            StressScenario::KillSwitchDelay { delay_secs: 60 },
        ];

        for scenario in &scenarios {
            let result = run_stress_test(scenario, &empty, dec!(5));
            assert_eq!(result.positions_at_risk, 0);
            assert_eq!(result.portfolio_impact, Decimal::ZERO);
            assert_eq!(result.max_loss, Decimal::ZERO);
            assert_eq!(result.var_before, result.var_after);
        }
    }

    #[test]
    fn test_flash_crash_100_percent() {
        let positions = sample_portfolio();
        let result = run_stress_test(
            &StressScenario::FlashCrash {
                adverse_move_pct: 1.0,
            },
            &positions,
            dec!(10),
        );

        // Total notional: 215, 100% loss
        let total_notional = dec!(215);
        assert_eq!(
            result.portfolio_impact, -total_notional,
            "100% flash crash should wipe out all notional"
        );
    }

    #[test]
    fn test_zero_size_positions_ignored() {
        let positions = vec![
            make_position("active", dec!(100), dec!(0.50)),
            make_position("closed", Decimal::ZERO, dec!(0.50)),
        ];

        let result = run_stress_test(
            &StressScenario::FlashCrash {
                adverse_move_pct: 0.10,
            },
            &positions,
            dec!(5),
        );

        assert_eq!(result.positions_at_risk, 1, "Only active position at risk");
        // Notional: 100 * 0.50 = 50, 10% = 5
        assert_eq!(result.portfolio_impact, dec!(-5));
    }
}
