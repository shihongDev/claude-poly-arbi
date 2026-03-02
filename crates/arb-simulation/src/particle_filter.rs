use arb_core::ProbEstimate;
use rand::Rng;
use rand_distr::{Distribution, StandardNormal};

/// Sequential Monte Carlo (particle filter) for real-time probability updating.
///
/// State model: logit(p_true) follows a random walk.
/// Observation model: market price is a noisy reading of the true probability.
///
/// Algorithm per update:
/// 1. Propagate particles: `x_t = x_{t-1} + N(0, process_vol)`
/// 2. Reweight: `w_t ∝ likelihood(observed_price | particle)`
/// 3. Resample if ESS < N/2 (systematic resampling)
///
/// Why logit space? Probabilities are bounded [0,1] but logit maps to (-∞, +∞),
/// allowing a Gaussian random walk without boundary issues.
pub struct ParticleFilter {
    particles: Vec<f64>,
    weights: Vec<f64>,
    process_vol: f64,
    obs_noise: f64,
}

impl ParticleFilter {
    /// Create a new particle filter.
    ///
    /// - `n_particles`: Number of particles (500-5000 typical)
    /// - `initial_price`: Starting market price (probability)
    /// - `process_vol`: Standard deviation of the random walk in logit space (0.03-0.10)
    /// - `obs_noise`: Observation noise standard deviation (0.01-0.05)
    pub fn new(n_particles: usize, initial_price: f64, process_vol: f64, obs_noise: f64) -> Self {
        let initial_logit = logit(initial_price.clamp(0.001, 0.999));

        // Initialize particles around the initial price with some spread
        let mut rng = rand::rng();
        let normal = StandardNormal;
        let particles: Vec<f64> = (0..n_particles)
            .map(|_| {
                let z: f64 = normal.sample(&mut rng);
                initial_logit + z * process_vol * 2.0 // wider initial spread
            })
            .collect();

        let weight = 1.0 / n_particles as f64;
        let weights = vec![weight; n_particles];

        Self {
            particles,
            weights,
            process_vol,
            obs_noise,
        }
    }

    /// Update the filter with a new observed market price.
    pub fn update(&mut self, observed_price: f64) {
        let observed = observed_price.clamp(0.001, 0.999);
        let mut rng = rand::rng();
        let normal = StandardNormal;

        // Step 1: Propagate particles (random walk in logit space)
        for particle in &mut self.particles {
            let z: f64 = normal.sample(&mut rng);
            *particle += z * self.process_vol;
        }

        // Step 2: Reweight based on likelihood
        let mut max_log_w = f64::NEG_INFINITY;
        let mut log_weights: Vec<f64> = Vec::with_capacity(self.particles.len());

        for (i, particle) in self.particles.iter().enumerate() {
            let p = sigmoid(*particle);
            // Gaussian likelihood in probability space
            let diff = p - observed;
            let log_likelihood = -0.5 * (diff / self.obs_noise).powi(2);
            let log_w = self.weights[i].ln() + log_likelihood;
            log_weights.push(log_w);
            if log_w > max_log_w {
                max_log_w = log_w;
            }
        }

        // Normalize weights using log-sum-exp for numerical stability
        let sum_exp: f64 = log_weights.iter().map(|&lw| (lw - max_log_w).exp()).sum();
        let log_sum = max_log_w + sum_exp.ln();

        for (i, lw) in log_weights.iter().enumerate() {
            self.weights[i] = (lw - log_sum).exp();
        }

        // Step 3: Resample if ESS too low
        if self.effective_sample_size() < self.particles.len() as f64 / 2.0 {
            self.systematic_resample();
        }
    }

    /// Get the current probability estimate with credible interval.
    pub fn estimate(&self) -> ProbEstimate {
        let mean_prob = self.weighted_mean();

        // Compute weighted percentiles for credible interval
        let mut prob_weight_pairs: Vec<(f64, f64)> = self
            .particles
            .iter()
            .zip(self.weights.iter())
            .map(|(&x, &w)| (sigmoid(x), w))
            .collect();
        prob_weight_pairs.sort_by(|a, b| a.0.partial_cmp(&b.0).unwrap());

        let lower = weighted_quantile(&prob_weight_pairs, 0.025);
        let upper = weighted_quantile(&prob_weight_pairs, 0.975);

        ProbEstimate {
            probabilities: vec![mean_prob],
            confidence_interval: vec![(lower, upper)],
            method: "particle_filter".to_string(),
        }
    }

    /// Effective sample size: `1 / Σ(w_i²)`.
    /// When ESS is low, the filter is relying on too few particles.
    pub fn effective_sample_size(&self) -> f64 {
        let sum_sq: f64 = self.weights.iter().map(|w| w * w).sum();
        if sum_sq > 0.0 {
            1.0 / sum_sq
        } else {
            0.0
        }
    }

    /// Weighted mean probability.
    fn weighted_mean(&self) -> f64 {
        self.particles
            .iter()
            .zip(self.weights.iter())
            .map(|(&x, &w)| sigmoid(x) * w)
            .sum()
    }

    /// Systematic resampling: more efficient than multinomial resampling.
    fn systematic_resample(&mut self) {
        let n = self.particles.len();
        let mut rng = rand::rng();
        let u: f64 = rng.random::<f64>() / n as f64;

        let mut cumulative = vec![0.0; n];
        cumulative[0] = self.weights[0];
        for i in 1..n {
            cumulative[i] = cumulative[i - 1] + self.weights[i];
        }

        let mut new_particles = Vec::with_capacity(n);
        let mut j = 0;

        for i in 0..n {
            let threshold = u + i as f64 / n as f64;
            while j < n - 1 && cumulative[j] < threshold {
                j += 1;
            }
            new_particles.push(self.particles[j]);
        }

        self.particles = new_particles;
        let uniform_weight = 1.0 / n as f64;
        self.weights = vec![uniform_weight; n];
    }
}

/// Logit transform: log(p / (1-p))
fn logit(p: f64) -> f64 {
    (p / (1.0 - p)).ln()
}

/// Sigmoid (inverse logit): 1 / (1 + exp(-x))
fn sigmoid(x: f64) -> f64 {
    1.0 / (1.0 + (-x).exp())
}

/// Weighted quantile from sorted (value, weight) pairs.
fn weighted_quantile(sorted_pairs: &[(f64, f64)], q: f64) -> f64 {
    let mut cum_weight = 0.0;
    for &(value, weight) in sorted_pairs {
        cum_weight += weight;
        if cum_weight >= q {
            return value;
        }
    }
    sorted_pairs.last().map(|&(v, _)| v).unwrap_or(0.5)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_particle_filter_tracks_stable_price() {
        let mut pf = ParticleFilter::new(1000, 0.5, 0.03, 0.02);

        // Feed consistent observations at 0.60
        for _ in 0..20 {
            pf.update(0.60);
        }

        let est = pf.estimate();
        assert!(
            (est.probabilities[0] - 0.60).abs() < 0.05,
            "Expected ~0.60, got {}",
            est.probabilities[0]
        );
    }

    #[test]
    fn test_particle_filter_smooths_spike() {
        let mut pf = ParticleFilter::new(1000, 0.50, 0.03, 0.02);

        // Stable at 0.50 for a while
        for _ in 0..10 {
            pf.update(0.50);
        }

        // Sudden spike to 0.65
        pf.update(0.65);

        let est = pf.estimate();
        // Should NOT jump all the way to 0.65 — should be tempered
        assert!(
            est.probabilities[0] < 0.63,
            "Filter should smooth the spike, got {}",
            est.probabilities[0]
        );
        assert!(
            est.probabilities[0] > 0.50,
            "Filter should partially update, got {}",
            est.probabilities[0]
        );
    }

    #[test]
    fn test_ess_after_updates() {
        let mut pf = ParticleFilter::new(500, 0.5, 0.05, 0.03);
        pf.update(0.55);

        // After resampling, ESS should be healthy
        let ess = pf.effective_sample_size();
        assert!(
            ess > 100.0,
            "ESS too low after single update: {}",
            ess
        );
    }

    #[test]
    fn test_confidence_interval() {
        let mut pf = ParticleFilter::new(1000, 0.5, 0.03, 0.02);
        for _ in 0..10 {
            pf.update(0.50);
        }

        let est = pf.estimate();
        let (lower, upper) = est.confidence_interval[0];
        assert!(lower < est.probabilities[0]);
        assert!(upper > est.probabilities[0]);
        assert!(upper - lower < 0.3, "CI too wide: ({}, {})", lower, upper);
    }
}
