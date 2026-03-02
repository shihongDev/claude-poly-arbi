use rand::Rng;
use rand_distr::{Distribution, Exp, StandardNormal};
use serde::{Deserialize, Serialize};

/// Agent-based market microstructure simulation.
///
/// Three agent types interact in a simplified order-driven market:
/// - **Informed agents**: Know the true value, trade toward it (Kyle model)
/// - **Noise agents**: Random trades with exponential size distribution
/// - **Market makers**: Provide liquidity, tighten spread based on volume
///
/// Used for: understanding price convergence dynamics, estimating how much
/// edge exists before informed traders extract it, backtesting strategies.
#[derive(Debug, Clone)]
pub struct AgentSimulation {
    pub informed_count: usize,
    pub noise_count: usize,
    pub mm_count: usize,
    pub true_value: f64,
    pub initial_price: f64,
    pub n_steps: usize,
    /// Kyle's lambda: price impact per unit of informed order flow
    /// λ = σ_v / (2 × σ_u) where σ_v is value uncertainty, σ_u is noise volume
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
    pub fn new(
        informed_count: usize,
        noise_count: usize,
        mm_count: usize,
        true_value: f64,
        initial_price: f64,
        n_steps: usize,
    ) -> Self {
        // Default Kyle's lambda based on agent counts.
        // Scale by total agent flow to keep price changes within a stable range.
        let total_agents = (informed_count + noise_count) as f64;
        let kyle_lambda = 0.01 / total_agents.max(1.0);

        Self {
            informed_count,
            noise_count,
            mm_count,
            true_value,
            initial_price,
            n_steps,
            kyle_lambda,
        }
    }

    pub fn run(&self) -> SimulationTrace {
        let mut rng = rand::rng();
        let normal = StandardNormal;

        let mut price = self.initial_price;
        let mut prices = Vec::with_capacity(self.n_steps + 1);
        let mut volumes = Vec::with_capacity(self.n_steps);
        let mut spreads = Vec::with_capacity(self.n_steps);
        let mut convergence_time = None;

        prices.push(price);

        // Market maker spread starts wide and tightens with volume
        let mut cumulative_volume = 0.0;
        let base_spread = 0.05; // 5 cent initial spread

        for step in 0..self.n_steps {
            let mut net_order_flow = 0.0;
            let mut step_volume = 0.0;

            // Informed agents: trade toward true value
            for _ in 0..self.informed_count {
                let direction = if self.true_value > price { 1.0 } else { -1.0 };
                // Size proportional to mispricing, with noise
                let mispricing = (self.true_value - price).abs();
                let z: f64 = normal.sample(&mut rng);
                let size = (mispricing * 10.0 + z.abs() * 0.5).max(0.0);
                net_order_flow += direction * size;
                step_volume += size;
            }

            // Noise agents: random trades with exponential size
            let exp_dist = Exp::new(5.0).unwrap(); // mean size = 0.2
            for _ in 0..self.noise_count {
                let direction: f64 = if rng.random::<bool>() { 1.0 } else { -1.0 };
                let size: f64 = exp_dist.sample(&mut rng);
                net_order_flow += direction * size;
                step_volume += size;
            }

            // Price impact: Kyle's lambda model
            // Δp = λ × net_order_flow
            let price_change = self.kyle_lambda * net_order_flow;
            price = (price + price_change).clamp(0.01, 0.99);

            // Market maker spread: tightens with cumulative volume
            cumulative_volume += step_volume;
            let spread = base_spread / (1.0 + cumulative_volume * 0.01);

            prices.push(price);
            volumes.push(step_volume);
            spreads.push(spread);

            // Check convergence (within 1% of true value)
            if convergence_time.is_none() && (price - self.true_value).abs() < 0.01 {
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
}

#[cfg(test)]
mod tests {
    use super::*;

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
}
