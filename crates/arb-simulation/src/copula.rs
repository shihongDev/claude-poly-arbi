use nalgebra::DMatrix;
use rand::Rng;
use rand_distr::{Distribution, StandardNormal};
use statrs::distribution::{ContinuousCDF, StudentsT};

use arb_core::error::{ArbError, Result};

/// Student-t copula for modeling tail dependence between correlated markets.
///
/// Unlike the Gaussian copula (which has zero tail dependence λ = 0),
/// the t-copula captures symmetric tail dependence: when one market crashes,
/// correlated markets are more likely to crash too.
///
/// Implementation:
/// 1. Cholesky decomposition of correlation matrix
/// 2. Generate independent t-distributed random variables
/// 3. Transform through Cholesky to induce correlation
/// 4. Apply Student-t CDF to get uniform marginals
pub struct TCopula {
    correlation_matrix: DMatrix<f64>,
    degrees_of_freedom: f64,
    cholesky: DMatrix<f64>,
    dim: usize,
}

impl TCopula {
    pub fn new(correlation_matrix: DMatrix<f64>, df: f64) -> Result<Self> {
        let dim = correlation_matrix.nrows();
        if correlation_matrix.ncols() != dim {
            return Err(ArbError::Simulation("Correlation matrix must be square".into()));
        }
        if df <= 2.0 {
            return Err(ArbError::Simulation("Degrees of freedom must be > 2".into()));
        }

        // Cholesky decomposition: R = L * L^T
        let cholesky = correlation_matrix
            .clone()
            .cholesky()
            .ok_or_else(|| {
                ArbError::Simulation("Correlation matrix is not positive definite".into())
            })?
            .l();

        Ok(Self {
            correlation_matrix,
            degrees_of_freedom: df,
            cholesky,
            dim,
        })
    }

    /// Generate `n` correlated samples from the t-copula.
    ///
    /// Returns `n` vectors of length `dim`, each element in [0, 1].
    pub fn sample(&self, n: usize) -> Vec<Vec<f64>> {
        let mut rng = rand::rng();
        let normal = StandardNormal;
        let t_dist = StudentsT::new(0.0, 1.0, self.degrees_of_freedom).unwrap();

        let mut results = Vec::with_capacity(n);

        for _ in 0..n {
            // Generate chi-squared via sum of squared normals
            let chi2: f64 = (0..self.degrees_of_freedom as usize)
                .map(|_| {
                    let z: f64 = normal.sample(&mut rng);
                    z * z
                })
                .sum();
            let w = (self.degrees_of_freedom / chi2).sqrt();

            // Generate independent normals
            let z: Vec<f64> = (0..self.dim).map(|_| normal.sample(&mut rng)).collect();
            let z_vec = DMatrix::from_vec(self.dim, 1, z);

            // Correlate via Cholesky: Y = L * Z
            let y = &self.cholesky * z_vec;

            // Scale by sqrt(ν/χ²) to get multivariate t
            // Then apply t-CDF to get uniform copula samples
            let uniform: Vec<f64> = (0..self.dim)
                .map(|i| {
                    let t_val = y[(i, 0)] * w;
                    t_dist.cdf(t_val)
                })
                .collect();

            results.push(uniform);
        }

        results
    }

    /// Estimate the joint probability that all marginals fall below their thresholds.
    ///
    /// `thresholds` are in uniform [0,1] space (i.e., probability levels).
    pub fn joint_probability(&self, thresholds: &[f64], n_samples: usize) -> f64 {
        let samples = self.sample(n_samples);
        let count = samples
            .iter()
            .filter(|s| {
                s.iter()
                    .zip(thresholds.iter())
                    .all(|(&val, &thresh)| val <= thresh)
            })
            .count();

        count as f64 / n_samples as f64
    }

    /// Theoretical tail dependence coefficient for the t-copula.
    /// λ = 2 * t_{ν+1}(-√((ν+1)(1-ρ)/(1+ρ)))
    /// where ρ is the pairwise correlation and ν is degrees of freedom.
    pub fn tail_dependence(&self, i: usize, j: usize) -> f64 {
        if i >= self.dim || j >= self.dim {
            return 0.0;
        }

        let rho = self.correlation_matrix[(i, j)];
        let nu = self.degrees_of_freedom;

        let t_dist = StudentsT::new(0.0, 1.0, nu + 1.0).unwrap();
        let arg = -((nu + 1.0) * (1.0 - rho) / (1.0 + rho)).sqrt();
        2.0 * t_dist.cdf(arg)
    }

    pub fn dim(&self) -> usize {
        self.dim
    }
}

/// Clayton copula for lower tail dependence.
///
/// C(u, v) = (u^{-θ} + v^{-θ} - 1)^{-1/θ}
///
/// Good for modeling asymmetric dependence: markets that crash together
/// but don't necessarily boom together.
pub struct ClaytonCopula {
    theta: f64,
}

impl ClaytonCopula {
    pub fn new(theta: f64) -> Result<Self> {
        if theta <= 0.0 {
            return Err(ArbError::Simulation(
                "Clayton theta must be positive".into(),
            ));
        }
        Ok(Self { theta })
    }

    /// Generate bivariate samples from the Clayton copula.
    pub fn sample_bivariate(&self, n: usize) -> Vec<(f64, f64)> {
        let mut rng = rand::rng();
        let mut results = Vec::with_capacity(n);

        for _ in 0..n {
            let u: f64 = rng.random();
            let w: f64 = rng.random();

            // Conditional method: v = u * (w^{-θ/(1+θ)} - 1 + u^{-θ})^{-1/θ}
            // Simplified: generate u, then v conditional on u
            let t = w.powf(-self.theta / (1.0 + self.theta));
            let v = (t - 1.0 + u.powf(-self.theta)).powf(-1.0 / self.theta);

            results.push((u, v.clamp(0.0, 1.0)));
        }

        results
    }

    /// Lower tail dependence coefficient: λ_L = 2^{-1/θ}
    pub fn lower_tail_dependence(&self) -> f64 {
        2.0_f64.powf(-1.0 / self.theta)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_t_copula_creation() {
        let corr = DMatrix::from_row_slice(2, 2, &[1.0, 0.5, 0.5, 1.0]);
        let copula = TCopula::new(corr, 5.0);
        assert!(copula.is_ok());
    }

    #[test]
    fn test_t_copula_samples_in_range() {
        let corr = DMatrix::from_row_slice(2, 2, &[1.0, 0.7, 0.7, 1.0]);
        let copula = TCopula::new(corr, 5.0).unwrap();
        let samples = copula.sample(1000);

        for s in &samples {
            for &val in s {
                assert!(val >= 0.0 && val <= 1.0, "Sample out of [0,1]: {}", val);
            }
        }
    }

    #[test]
    fn test_tail_dependence_positive() {
        let corr = DMatrix::from_row_slice(2, 2, &[1.0, 0.5, 0.5, 1.0]);
        let copula = TCopula::new(corr, 5.0).unwrap();
        let td = copula.tail_dependence(0, 1);
        assert!(td > 0.0, "Tail dependence should be positive for t-copula");
        assert!(td < 1.0);
    }

    #[test]
    fn test_tail_dependence_increases_with_correlation() {
        let corr_low = DMatrix::from_row_slice(2, 2, &[1.0, 0.3, 0.3, 1.0]);
        let corr_high = DMatrix::from_row_slice(2, 2, &[1.0, 0.8, 0.8, 1.0]);

        let cop_low = TCopula::new(corr_low, 5.0).unwrap();
        let cop_high = TCopula::new(corr_high, 5.0).unwrap();

        assert!(cop_high.tail_dependence(0, 1) > cop_low.tail_dependence(0, 1));
    }

    #[test]
    fn test_clayton_copula() {
        let copula = ClaytonCopula::new(2.0).unwrap();
        let samples = copula.sample_bivariate(1000);

        for &(u, v) in &samples {
            assert!(u >= 0.0 && u <= 1.0);
            assert!(v >= 0.0 && v <= 1.0);
        }
    }

    #[test]
    fn test_clayton_tail_dependence() {
        let copula = ClaytonCopula::new(2.0).unwrap();
        let td = copula.lower_tail_dependence();
        // λ_L = 2^{-1/2} ≈ 0.707
        assert!((td - 0.707).abs() < 0.01);
    }
}
