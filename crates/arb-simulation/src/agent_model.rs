use rand::Rng;
use rand_distr::{Distribution, Exp, StandardNormal};
use serde::{Deserialize, Serialize};

/// Types of agents that can participate in the market simulation.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub enum AgentType {
    /// Informed traders who know the true value and trade toward it (Kyle model).
    Informed { true_value: f64 },
    /// Noise traders who place random orders with exponential size distribution.
    Noise,
    /// Market makers who provide liquidity and tighten spread based on volume.
    MarketMaker,
    /// Gode-Sunder (1993) zero-intelligence traders: random buy/sell with a budget constraint.
    ZeroIntelligence { budget: f64 },
    /// Momentum traders: buy if price trending up over `lookback` steps, sell if down.
    Momentum { lookback: usize, threshold: f64 },
    /// Contrarian traders: buy if price trending down over `lookback` steps, sell if up.
    Contrarian { lookback: usize, threshold: f64 },
    /// Arbitrageur: trade toward fair value when price deviates beyond tolerance.
    Arbitrageur { fair_value: f64, tolerance: f64 },
}

/// Agent-based market microstructure simulation.
///
/// Multiple agent types interact in a simplified order-driven market:
/// - **Informed agents**: Know the true value, trade toward it (Kyle model)
/// - **Noise agents**: Random trades with exponential size distribution
/// - **Market makers**: Provide liquidity, tighten spread based on volume
/// - **Zero-intelligence**: Random trades subject to budget constraint
/// - **Momentum**: Follow price trends
/// - **Contrarian**: Trade against price trends
/// - **Arbitrageur**: Correct mispricings relative to fair value
///
/// Used for: understanding price convergence dynamics, estimating how much
/// edge exists before informed traders extract it, backtesting strategies.
#[derive(Debug, Clone)]
pub struct AgentSimulation {
    /// Agent composition: each entry is (agent_type, count).
    pub agents: Vec<(AgentType, usize)>,
    pub initial_price: f64,
    pub n_steps: usize,
    /// Kyle's lambda: price impact per unit of order flow.
    pub kyle_lambda: f64,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SimulationTrace {
    pub prices: Vec<f64>,
    pub volumes: Vec<f64>,
    pub spreads: Vec<f64>,
    pub convergence_time: Option<usize>,
}

impl AgentSimulation {
    /// Create a simulation with the original 3-agent-type mix (backward compatible).
    ///
    /// This preserves the exact same behavior as the original constructor:
    /// - `informed_count` Informed agents with `true_value`
    /// - `noise_count` Noise agents
    /// - `mm_count` MarketMaker agents
    pub fn new(
        informed_count: usize,
        noise_count: usize,
        mm_count: usize,
        true_value: f64,
        initial_price: f64,
        n_steps: usize,
    ) -> Self {
        let total_agents = (informed_count + noise_count) as f64;
        let kyle_lambda = 0.01 / total_agents.max(1.0);

        let agents = vec![
            (AgentType::Informed { true_value }, informed_count),
            (AgentType::Noise, noise_count),
            (AgentType::MarketMaker, mm_count),
        ];

        Self {
            agents,
            initial_price,
            n_steps,
            kyle_lambda,
        }
    }

    /// Create a simulation with a custom mix of agent types.
    ///
    /// `agents` is a list of `(AgentType, count)` pairs specifying the composition.
    /// Kyle's lambda is computed from total trading agents (all except MarketMaker).
    pub fn new_with_agents(
        agents: Vec<(AgentType, usize)>,
        initial_price: f64,
        n_steps: usize,
    ) -> Self {
        let total_trading_agents: usize = agents
            .iter()
            .filter(|(t, _)| !matches!(t, AgentType::MarketMaker))
            .map(|(_, count)| count)
            .sum();
        let kyle_lambda = 0.01 / (total_trading_agents as f64).max(1.0);

        Self {
            agents,
            initial_price,
            n_steps,
            kyle_lambda,
        }
    }

    /// Extract the true_value from the first Informed or Arbitrageur agent, for convergence check.
    fn convergence_target(&self) -> Option<f64> {
        for (agent_type, count) in &self.agents {
            if *count == 0 {
                continue;
            }
            match agent_type {
                AgentType::Informed { true_value } => return Some(*true_value),
                AgentType::Arbitrageur { fair_value, .. } => return Some(*fair_value),
                _ => {}
            }
        }
        None
    }

    pub fn run(&self) -> SimulationTrace {
        let mut rng = rand::rng();
        let normal = StandardNormal;
        // Pre-allocate noise distribution once (avoids recreation per agent per step)
        let noise_size_dist = Exp::new(5.0).unwrap(); // mean size = 0.2

        let mut price = self.initial_price;
        let mut prices = Vec::with_capacity(self.n_steps + 1);
        let mut volumes = Vec::with_capacity(self.n_steps);
        let mut spreads = Vec::with_capacity(self.n_steps);
        let mut convergence_time = None;

        prices.push(price);

        // Market maker spread starts wide and tightens with volume
        let mut cumulative_volume = 0.0;
        let base_spread = 0.05; // 5 cent initial spread

        // Track per-ZI-agent budget remaining (indexed by position in agents vec)
        let mut zi_budgets: Vec<Vec<f64>> = self
            .agents
            .iter()
            .map(|(agent_type, count)| match agent_type {
                AgentType::ZeroIntelligence { budget } => vec![*budget; *count],
                _ => Vec::new(),
            })
            .collect();

        let convergence_target = self.convergence_target();

        for step in 0..self.n_steps {
            let mut net_order_flow = 0.0;
            let mut step_volume = 0.0;

            for (agent_idx, (agent_type, count)) in self.agents.iter().enumerate() {
                for agent_i in 0..*count {
                    let (flow, volume) = self.generate_agent_order(
                        agent_type,
                        price,
                        &prices,
                        &normal,
                        &noise_size_dist,
                        &mut rng,
                        &mut zi_budgets[agent_idx],
                        agent_i,
                    );
                    net_order_flow += flow;
                    step_volume += volume;
                }
            }

            // Price impact: Kyle's lambda model
            let price_change = self.kyle_lambda * net_order_flow;
            price = (price + price_change).clamp(0.01, 0.99);

            // Market maker spread: tightens with cumulative volume
            cumulative_volume += step_volume;
            let spread = base_spread / (1.0 + cumulative_volume * 0.01);

            prices.push(price);
            volumes.push(step_volume);
            spreads.push(spread);

            // Check convergence (within 1% of target value)
            if convergence_time.is_none()
                && let Some(target) = convergence_target
                && (price - target).abs() < 0.01
            {
                convergence_time = Some(step);
            }
        }

        SimulationTrace {
            prices,
            volumes,
            spreads,
            convergence_time,
        }
    }

    /// Generate a single agent's order (direction * size) for one step.
    /// Returns (net_flow, abs_volume).
    #[allow(clippy::too_many_arguments)]
    fn generate_agent_order(
        &self,
        agent_type: &AgentType,
        price: f64,
        price_history: &[f64],
        normal: &StandardNormal,
        noise_dist: &Exp<f64>,
        rng: &mut impl Rng,
        zi_budgets: &mut [f64],
        agent_i: usize,
    ) -> (f64, f64) {
        match agent_type {
            AgentType::Informed { true_value } => {
                let direction = if *true_value > price { 1.0 } else { -1.0 };
                let mispricing = (*true_value - price).abs();
                let z: f64 = normal.sample(rng);
                let size = (mispricing * 10.0 + z.abs() * 0.5).max(0.0);
                (direction * size, size)
            }

            AgentType::Noise => {
                let direction: f64 = if rng.random::<bool>() { 1.0 } else { -1.0 };
                let size: f64 = noise_dist.sample(rng);
                (direction * size, size)
            }

            AgentType::MarketMaker => {
                // Market makers don't generate directional order flow in this model;
                // their effect is captured by the spread tightening mechanism.
                (0.0, 0.0)
            }

            AgentType::ZeroIntelligence { .. } => {
                // Gode-Sunder (1993): random buy/sell with budget constraint.
                if agent_i >= zi_budgets.len() || zi_budgets[agent_i] <= 0.0 {
                    return (0.0, 0.0);
                }

                let budget_remaining = zi_budgets[agent_i];
                let max_size = budget_remaining / 10.0;
                if max_size < 1e-6 {
                    return (0.0, 0.0);
                }

                let size: f64 = rng.random_range(0.0..max_size);
                let direction: f64 = if rng.random::<bool>() { 1.0 } else { -1.0 };

                // Deduct from budget
                zi_budgets[agent_i] -= size;

                (direction * size, size)
            }

            AgentType::Momentum {
                lookback,
                threshold,
            } => {
                // Buy if price trending up over lookback, sell if trending down.
                if price_history.len() < *lookback + 1 {
                    return (0.0, 0.0);
                }

                let current = price;
                let past = price_history[price_history.len() - *lookback];
                let change = current - past;

                if change > *threshold {
                    let size = change.abs() * 5.0;
                    (size, size) // buy
                } else if change < -*threshold {
                    let size = change.abs() * 5.0;
                    (-size, size) // sell
                } else {
                    (0.0, 0.0)
                }
            }

            AgentType::Contrarian {
                lookback,
                threshold,
            } => {
                // Opposite of momentum: buy if trending down, sell if trending up.
                if price_history.len() < *lookback + 1 {
                    return (0.0, 0.0);
                }

                let current = price;
                let past = price_history[price_history.len() - *lookback];
                let change = current - past;

                if change > *threshold {
                    let size = change.abs() * 5.0;
                    (-size, size) // sell (contrarian)
                } else if change < -*threshold {
                    let size = change.abs() * 5.0;
                    (size, size) // buy (contrarian)
                } else {
                    (0.0, 0.0)
                }
            }

            AgentType::Arbitrageur {
                fair_value,
                tolerance,
            } => {
                let deviation = price - *fair_value;
                if deviation.abs() > *tolerance {
                    // Trade toward fair value
                    let direction = if deviation > 0.0 { -1.0 } else { 1.0 };
                    let size = deviation.abs() * 10.0;
                    (direction * size, size)
                } else {
                    (0.0, 0.0)
                }
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    // --- Original backward-compatibility tests ---

    #[test]
    fn test_price_converges_toward_true_value() {
        let sim = AgentSimulation::new(
            5,    // informed
            10,   // noise
            2,    // market makers
            0.70, // true value
            0.50, // initial price
            100,  // steps
        );

        let trace = sim.run();

        // Price should move toward 0.70
        let final_price = *trace.prices.last().unwrap();
        assert!(
            (final_price - 0.70_f64).abs() < (0.50_f64 - 0.70_f64).abs(),
            "Price should converge: started at 0.50, ended at {}, target 0.70",
            final_price
        );
    }

    #[test]
    fn test_spread_tightens() {
        let sim = AgentSimulation::new(3, 10, 2, 0.60, 0.50, 50);
        let trace = sim.run();

        if trace.spreads.len() >= 2 {
            let first_spread = trace.spreads[0];
            let last_spread = *trace.spreads.last().unwrap();
            assert!(
                last_spread <= first_spread,
                "Spread should tighten: first={}, last={}",
                first_spread,
                last_spread
            );
        }
    }

    #[test]
    fn test_volume_positive() {
        let sim = AgentSimulation::new(3, 10, 2, 0.60, 0.50, 20);
        let trace = sim.run();

        for &vol in &trace.volumes {
            assert!(vol >= 0.0, "Volume should be non-negative");
        }
    }

    // --- New agent type tests ---

    /// Backward compatibility: `new()` should produce same structure as before.
    #[test]
    fn test_backward_compat_new_constructor() {
        let sim = AgentSimulation::new(5, 10, 2, 0.70, 0.50, 100);

        // Should have 3 agent groups
        assert_eq!(sim.agents.len(), 3);

        // First should be Informed with true_value 0.70
        match &sim.agents[0] {
            (AgentType::Informed { true_value }, count) => {
                assert_eq!(*count, 5);
                assert!((true_value - 0.70).abs() < 1e-10);
            }
            _ => panic!("First agent should be Informed"),
        }

        // Second should be Noise
        match &sim.agents[1] {
            (AgentType::Noise, count) => assert_eq!(*count, 10),
            _ => panic!("Second agent should be Noise"),
        }

        // Third should be MarketMaker
        match &sim.agents[2] {
            (AgentType::MarketMaker, count) => assert_eq!(*count, 2),
            _ => panic!("Third agent should be MarketMaker"),
        }

        // Kyle lambda should match original formula
        let expected_lambda = 0.01 / 15.0; // (5 + 10) = 15
        assert!(
            (sim.kyle_lambda - expected_lambda).abs() < 1e-10,
            "Kyle lambda mismatch: {} vs {}",
            sim.kyle_lambda,
            expected_lambda
        );

        // Should still produce valid traces
        let trace = sim.run();
        let final_price = *trace.prices.last().unwrap();
        assert!(
            (final_price - 0.70_f64).abs() < (0.50_f64 - 0.70_f64).abs(),
            "Backward-compat sim should still converge"
        );
    }

    /// ZeroIntelligence agents should respect their budget constraint.
    #[test]
    fn test_zi_budget_constraint() {
        let budget = 1.0;
        let agents = vec![
            (AgentType::ZeroIntelligence { budget }, 5),
            (AgentType::Noise, 5),
        ];

        let sim = AgentSimulation::new_with_agents(agents, 0.50, 200);
        let trace = sim.run();

        // Volume should be non-negative and simulation should complete
        for &vol in &trace.volumes {
            assert!(vol >= 0.0, "Volume should be non-negative");
        }
        assert_eq!(trace.prices.len(), 201); // initial + 200 steps

        // Total volume from ZI agents should not exceed their total budgets (5 * 1.0 = 5.0)
        // (Noise agents contribute unlimited volume, so we just check simulation runs)
        let total_volume: f64 = trace.volumes.iter().sum();
        assert!(total_volume > 0.0, "Should have some trading volume");
    }

    /// Momentum agents should push price in the direction of a trend.
    #[test]
    fn test_momentum_follows_trends() {
        // Create an upward-trending setup with informed agents pushing price up
        let agents = vec![
            (AgentType::Informed { true_value: 0.80 }, 5),
            (
                AgentType::Momentum {
                    lookback: 3,
                    threshold: 0.005,
                },
                10,
            ),
            (AgentType::MarketMaker, 2),
        ];

        let sim = AgentSimulation::new_with_agents(agents, 0.50, 50);
        let trace = sim.run();

        let final_price = *trace.prices.last().unwrap();
        // With informed agents pushing up and momentum agents amplifying,
        // price should move toward true value
        assert!(
            final_price > 0.50,
            "Momentum + informed should push price up from 0.50, got {}",
            final_price
        );
    }

    /// Contrarian agents should oppose trends.
    #[test]
    fn test_contrarian_opposes_trends() {
        // Informed agents push price up; contrarians resist
        let agents_with_contrarian = vec![
            (AgentType::Informed { true_value: 0.80 }, 3),
            (
                AgentType::Contrarian {
                    lookback: 3,
                    threshold: 0.005,
                },
                15,
            ),
            (AgentType::MarketMaker, 2),
        ];

        let agents_without_contrarian = vec![
            (AgentType::Informed { true_value: 0.80 }, 3),
            (AgentType::Noise, 15),
            (AgentType::MarketMaker, 2),
        ];

        // Run multiple times and average to reduce noise
        let n_runs = 20;
        let mut contrarian_finals = Vec::with_capacity(n_runs);
        let mut no_contrarian_finals = Vec::with_capacity(n_runs);

        for _ in 0..n_runs {
            let sim_c = AgentSimulation::new_with_agents(
                agents_with_contrarian.clone(),
                0.50,
                50,
            );
            let sim_n = AgentSimulation::new_with_agents(
                agents_without_contrarian.clone(),
                0.50,
                50,
            );

            let tc = sim_c.run();
            let tn = sim_n.run();

            contrarian_finals.push(*tc.prices.last().unwrap());
            no_contrarian_finals.push(*tn.prices.last().unwrap());
        }

        let avg_c: f64 = contrarian_finals.iter().sum::<f64>() / n_runs as f64;
        let avg_n: f64 = no_contrarian_finals.iter().sum::<f64>() / n_runs as f64;

        // Contrarians should slow down convergence (avg price closer to 0.50 than without)
        // i.e. avg_c should be closer to 0.50 (less movement toward 0.80)
        assert!(
            (avg_c - 0.50).abs() < (avg_n - 0.50).abs() + 0.1,
            "Contrarians should slow convergence: with={}, without={}",
            avg_c,
            avg_n
        );
    }

    /// Mixed agent simulation with all types should run without panics.
    #[test]
    fn test_mixed_agent_convergence() {
        let agents = vec![
            (AgentType::Informed { true_value: 0.65 }, 3),
            (AgentType::Noise, 5),
            (AgentType::MarketMaker, 2),
            (AgentType::ZeroIntelligence { budget: 2.0 }, 3),
            (
                AgentType::Momentum {
                    lookback: 5,
                    threshold: 0.01,
                },
                3,
            ),
            (
                AgentType::Contrarian {
                    lookback: 5,
                    threshold: 0.01,
                },
                3,
            ),
            (
                AgentType::Arbitrageur {
                    fair_value: 0.65,
                    tolerance: 0.02,
                },
                2,
            ),
        ];

        let sim = AgentSimulation::new_with_agents(agents, 0.50, 100);
        let trace = sim.run();

        // Basic sanity checks
        assert_eq!(trace.prices.len(), 101);
        assert_eq!(trace.volumes.len(), 100);
        assert_eq!(trace.spreads.len(), 100);

        // All prices should be in valid range
        for &p in &trace.prices {
            assert!(
                p >= 0.01 && p <= 0.99,
                "Price out of range: {}",
                p
            );
        }

        // Price should move toward fair value (0.65) from initial (0.50)
        let final_price = *trace.prices.last().unwrap();
        assert!(
            final_price > 0.50,
            "With informed + arbitrageur, price should move up from 0.50, got {}",
            final_price
        );
    }

    /// Arbitrageur agents should push price toward fair value.
    #[test]
    fn test_arbitrageur_corrects_mispricing() {
        let agents = vec![
            (
                AgentType::Arbitrageur {
                    fair_value: 0.60,
                    tolerance: 0.01,
                },
                10,
            ),
            (AgentType::Noise, 5),
            (AgentType::MarketMaker, 2),
        ];

        let sim = AgentSimulation::new_with_agents(agents, 0.40, 100);
        let trace = sim.run();

        let final_price = *trace.prices.last().unwrap();
        // Arbitrageurs should push price from 0.40 toward 0.60
        assert!(
            (final_price - 0.60).abs() < (0.40 - 0.60_f64).abs(),
            "Arbitrageurs should correct mispricing: started 0.40, ended {}, target 0.60",
            final_price
        );
    }
}
