use std::collections::VecDeque;
use std::time::{Duration, Instant};

use rust_decimal::Decimal;

/// Configuration for circuit breaker thresholds and windows.
#[derive(Debug, Clone)]
pub struct CircuitBreakerConfig {
    /// Maximum daily loss before tripping (positive value, compared against negative PnL).
    pub daily_loss_limit: Decimal,
    /// Maximum API error rate (0.0 to 1.0) before tripping.
    pub max_error_rate: f64,
    /// Time window for computing error rate.
    pub error_window: Duration,
    /// Maximum acceptable latency in milliseconds.
    pub max_latency_ms: u64,
    /// Time window for latency measurements.
    pub latency_window: Duration,
}

impl Default for CircuitBreakerConfig {
    fn default() -> Self {
        Self {
            daily_loss_limit: Decimal::from(1000),
            max_error_rate: 0.5,
            error_window: Duration::from_secs(60),
            max_latency_ms: 500,
            latency_window: Duration::from_secs(60),
        }
    }
}

/// Automatic safety triggers that detect dangerous conditions.
///
/// The engine loop calls `check()` each tick. If it returns `Some(reason)`,
/// the kill switch should be activated. This module only detects conditions;
/// wiring to the kill switch is handled by the caller.
///
/// Three triggers:
/// 1. **Daily loss**: PnL below negative `daily_loss_limit`
/// 2. **API error rate**: Error rate exceeds `max_error_rate` (min 10 samples)
/// 3. **Latency spike**: ALL recent latencies exceed `max_latency_ms` (min 5 samples)
pub struct CircuitBreaker {
    config: CircuitBreakerConfig,
    daily_pnl: Decimal,
    api_results: VecDeque<(Instant, bool)>,
    latencies: VecDeque<(Instant, u64)>,
}

impl CircuitBreaker {
    pub fn new(config: CircuitBreakerConfig) -> Self {
        Self {
            config,
            daily_pnl: Decimal::ZERO,
            api_results: VecDeque::new(),
            latencies: VecDeque::new(),
        }
    }

    /// Set the current daily PnL value.
    pub fn record_pnl(&mut self, pnl: Decimal) {
        self.daily_pnl = pnl;
    }

    /// Record a successful API call.
    pub fn record_api_success(&mut self) {
        self.api_results.push_back((Instant::now(), true));
    }

    /// Record a failed API call.
    pub fn record_api_error(&mut self) {
        self.api_results.push_back((Instant::now(), false));
    }

    /// Record an API latency measurement in milliseconds.
    pub fn record_latency(&mut self, ms: u64) {
        self.latencies.push_back((Instant::now(), ms));
    }

    /// Check all circuit breaker conditions.
    ///
    /// Returns `Some(reason)` if any trigger should fire, `None` if all clear.
    pub fn check(&mut self) -> Option<String> {
        self.prune_old_entries();

        // 1. Daily loss check
        if self.daily_pnl < -self.config.daily_loss_limit {
            return Some(format!(
                "Circuit breaker: daily loss {} exceeds limit of {}",
                self.daily_pnl, self.config.daily_loss_limit
            ));
        }

        // 2. API error rate check (need at least 10 samples)
        if self.api_results.len() >= 10 {
            let error_count = self.api_results.iter().filter(|(_, ok)| !ok).count();
            let error_rate = error_count as f64 / self.api_results.len() as f64;
            if error_rate > self.config.max_error_rate {
                return Some(format!(
                    "Circuit breaker: API error rate {:.1}% exceeds limit of {:.1}%",
                    error_rate * 100.0,
                    self.config.max_error_rate * 100.0,
                ));
            }
        }

        // 3. Latency check (need at least 5 samples, ALL must exceed threshold)
        if self.latencies.len() >= 5
            && self
                .latencies
                .iter()
                .all(|(_, ms)| *ms > self.config.max_latency_ms)
        {
            return Some(format!(
                "Circuit breaker: all {} recent latencies exceed {}ms",
                self.latencies.len(),
                self.config.max_latency_ms,
            ));
        }

        None
    }

    /// Reset daily PnL to zero (called at day boundary).
    pub fn reset_daily(&mut self) {
        self.daily_pnl = Decimal::ZERO;
    }

    /// Prune entries older than their respective windows.
    fn prune_old_entries(&mut self) {
        let now = Instant::now();

        let error_cutoff = now - self.config.error_window;
        while let Some(&(ts, _)) = self.api_results.front() {
            if ts < error_cutoff {
                self.api_results.pop_front();
            } else {
                break;
            }
        }

        let latency_cutoff = now - self.config.latency_window;
        while let Some(&(ts, _)) = self.latencies.front() {
            if ts < latency_cutoff {
                self.latencies.pop_front();
            } else {
                break;
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use rust_decimal_macros::dec;

    /// Helper: create a circuit breaker with default config.
    fn default_breaker() -> CircuitBreaker {
        CircuitBreaker::new(CircuitBreakerConfig::default())
    }

    #[test]
    fn test_no_trigger_under_threshold() {
        let mut cb = default_breaker();
        // Small loss, well under the 1000 limit
        cb.record_pnl(dec!(-500));
        // Some API calls, all successful
        for _ in 0..15 {
            cb.record_api_success();
        }
        // Some latencies, all under threshold
        for _ in 0..10 {
            cb.record_latency(200);
        }

        assert!(cb.check().is_none(), "Should not trigger under threshold");
    }

    #[test]
    fn test_daily_loss_trigger() {
        let mut cb = default_breaker();
        // Loss exceeding the 1000 limit
        cb.record_pnl(dec!(-1500));

        let result = cb.check();
        assert!(result.is_some(), "Should trigger on excessive daily loss");
        let reason = result.unwrap();
        assert!(
            reason.contains("daily loss"),
            "Reason should mention 'daily loss', got: {}",
            reason
        );
    }

    #[test]
    fn test_error_rate_trigger() {
        let mut cb = default_breaker();
        // 8 errors out of 12 calls = 66.7% error rate, above the 50% threshold
        for _ in 0..8 {
            cb.record_api_error();
        }
        for _ in 0..4 {
            cb.record_api_success();
        }

        let result = cb.check();
        assert!(result.is_some(), "Should trigger on high error rate");
        let reason = result.unwrap();
        assert!(
            reason.contains("error rate"),
            "Reason should mention 'error rate', got: {}",
            reason
        );
    }

    #[test]
    fn test_latency_trigger() {
        let mut cb = default_breaker();
        // All 6 latencies above 500ms threshold
        for _ in 0..6 {
            cb.record_latency(800);
        }

        let result = cb.check();
        assert!(result.is_some(), "Should trigger on all-high latencies");
        let reason = result.unwrap();
        assert!(
            reason.contains("latenc"),
            "Reason should mention latency, got: {}",
            reason
        );
    }

    #[test]
    fn test_no_trigger_insufficient_samples() {
        let mut cb = default_breaker();

        // Only 5 API results (need 10 for error rate trigger)
        for _ in 0..5 {
            cb.record_api_error();
        }
        // Only 3 latencies (need 5 for latency trigger)
        for _ in 0..3 {
            cb.record_latency(1000);
        }

        assert!(
            cb.check().is_none(),
            "Should not trigger with insufficient samples"
        );
    }

    #[test]
    fn test_reset_daily() {
        let mut cb = default_breaker();
        // Set loss that would trigger
        cb.record_pnl(dec!(-1500));
        assert!(cb.check().is_some(), "Should trigger before reset");

        // Reset daily PnL
        cb.reset_daily();
        assert!(cb.check().is_none(), "Should not trigger after daily reset");
    }

    #[test]
    fn test_latency_mixed_does_not_trigger() {
        let mut cb = default_breaker();
        // Mix of high and low latencies — should NOT trigger since not ALL are above threshold
        cb.record_latency(800);
        cb.record_latency(800);
        cb.record_latency(100); // one under threshold
        cb.record_latency(800);
        cb.record_latency(800);
        cb.record_latency(800);

        assert!(
            cb.check().is_none(),
            "Mixed latencies should not trigger (not ALL above threshold)"
        );
    }

    #[test]
    fn test_error_rate_exactly_at_threshold_does_not_trigger() {
        let mut cb = default_breaker();
        // 5 errors out of 10 = exactly 50%, which does NOT exceed 50%
        for _ in 0..5 {
            cb.record_api_error();
        }
        for _ in 0..5 {
            cb.record_api_success();
        }

        assert!(
            cb.check().is_none(),
            "Error rate at exactly the threshold should not trigger (must exceed, not equal)"
        );
    }

    #[test]
    fn test_daily_loss_at_boundary_does_not_trigger() {
        let mut cb = default_breaker();
        // Loss exactly at the limit (-1000) should NOT trigger (must be below, i.e. more negative)
        cb.record_pnl(dec!(-1000));

        assert!(
            cb.check().is_none(),
            "Loss exactly at limit should not trigger (need to exceed limit)"
        );
    }

    #[test]
    fn test_default_config_values() {
        let config = CircuitBreakerConfig::default();
        assert_eq!(config.daily_loss_limit, dec!(1000));
        assert!((config.max_error_rate - 0.5).abs() < f64::EPSILON);
        assert_eq!(config.error_window, Duration::from_secs(60));
        assert_eq!(config.max_latency_ms, 500);
        assert_eq!(config.latency_window, Duration::from_secs(60));
    }
}
