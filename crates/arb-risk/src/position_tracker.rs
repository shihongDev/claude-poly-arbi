use std::collections::HashMap;
use std::path::Path;

use arb_core::{
    ExecutionReport, Position, Side,
    error::{ArbError, Result},
};
use rust_decimal::Decimal;
use serde::{Deserialize, Serialize};

/// Tracks virtual and real positions by token_id.
///
/// Updated on every ExecutionReport. Persisted to a JSON state file
/// on graceful shutdown so state survives restarts.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct PositionTracker {
    positions: HashMap<String, Position>,
}

impl PositionTracker {
    pub fn new() -> Self {
        Self {
            positions: HashMap::new(),
        }
    }

    /// Update positions from an execution report.
    pub fn update(&mut self, report: &ExecutionReport) {
        for leg in &report.legs {
            let pos = self
                .positions
                .entry(leg.token_id.clone())
                .or_insert_with(|| Position {
                    token_id: leg.token_id.clone(),
                    condition_id: String::new(),
                    size: Decimal::ZERO,
                    avg_entry_price: Decimal::ZERO,
                    current_price: leg.actual_fill_price,
                    unrealized_pnl: Decimal::ZERO,
                });

            match leg.side {
                Side::Buy => {
                    let new_cost = pos.avg_entry_price * pos.size
                        + leg.actual_fill_price * leg.filled_size;
                    pos.size += leg.filled_size;
                    if pos.size > Decimal::ZERO {
                        pos.avg_entry_price = new_cost / pos.size;
                    }
                }
                Side::Sell => {
                    pos.size -= leg.filled_size;
                    if pos.size <= Decimal::ZERO {
                        pos.size = Decimal::ZERO;
                        pos.avg_entry_price = Decimal::ZERO;
                    }
                }
            }

            pos.current_price = leg.actual_fill_price;
            pos.unrealized_pnl = (pos.current_price - pos.avg_entry_price) * pos.size;
        }
    }

    pub fn get(&self, token_id: &str) -> Option<&Position> {
        self.positions.get(token_id)
    }

    pub fn all_positions(&self) -> Vec<&Position> {
        self.positions.values().collect()
    }

    /// Total exposure: sum of |size × current_price| for all positions.
    pub fn total_exposure(&self) -> Decimal {
        self.positions
            .values()
            .map(|p| (p.size * p.current_price).abs())
            .sum()
    }

    /// Exposure for a specific market (condition_id).
    pub fn market_exposure(&self, condition_id: &str) -> Decimal {
        self.positions
            .values()
            .filter(|p| p.condition_id == condition_id)
            .map(|p| (p.size * p.current_price).abs())
            .sum()
    }

    /// Persist positions to a JSON file.
    pub fn save(&self, path: &Path) -> Result<()> {
        let json = serde_json::to_string_pretty(self)?;
        if let Some(parent) = path.parent() {
            std::fs::create_dir_all(parent)?;
        }
        std::fs::write(path, json)?;
        Ok(())
    }

    /// Load positions from a JSON file.
    pub fn load(path: &Path) -> Result<Self> {
        let content = std::fs::read_to_string(path).map_err(|e| {
            ArbError::Config(format!(
                "Cannot read position state from {}: {e}",
                path.display()
            ))
        })?;
        let tracker: Self = serde_json::from_str(&content)?;
        Ok(tracker)
    }

    /// Number of active positions (size > 0).
    pub fn active_count(&self) -> usize {
        self.positions
            .values()
            .filter(|p| p.size > Decimal::ZERO)
            .count()
    }

    /// Clear all positions.
    pub fn clear(&mut self) {
        self.positions.clear();
    }
}

impl Default for PositionTracker {
    fn default() -> Self {
        Self::new()
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use arb_core::{FillStatus, LegReport, TradingMode};
    use chrono::Utc;
    use rust_decimal_macros::dec;
    use uuid::Uuid;

    fn make_report(token: &str, side: Side, price: Decimal, size: Decimal) -> ExecutionReport {
        ExecutionReport {
            opportunity_id: Uuid::new_v4(),
            legs: vec![LegReport {
                order_id: "ord1".into(),
                token_id: token.into(),
                side,
                expected_vwap: price,
                actual_fill_price: price,
                filled_size: size,
                status: FillStatus::FullyFilled,
            }],
            realized_edge: Decimal::ZERO,
            slippage: Decimal::ZERO,
            total_fees: Decimal::ZERO,
            timestamp: Utc::now(),
            mode: TradingMode::Paper,
        }
    }

    #[test]
    fn test_buy_updates_position() {
        let mut tracker = PositionTracker::new();
        tracker.update(&make_report("tok_a", Side::Buy, dec!(0.50), dec!(100)));

        let pos = tracker.get("tok_a").unwrap();
        assert_eq!(pos.size, dec!(100));
        assert_eq!(pos.avg_entry_price, dec!(0.50));
    }

    #[test]
    fn test_sell_reduces_position() {
        let mut tracker = PositionTracker::new();
        tracker.update(&make_report("tok_a", Side::Buy, dec!(0.50), dec!(100)));
        tracker.update(&make_report("tok_a", Side::Sell, dec!(0.55), dec!(40)));

        let pos = tracker.get("tok_a").unwrap();
        assert_eq!(pos.size, dec!(60));
    }

    #[test]
    fn test_total_exposure() {
        let mut tracker = PositionTracker::new();
        tracker.update(&make_report("tok_a", Side::Buy, dec!(0.50), dec!(100)));
        tracker.update(&make_report("tok_b", Side::Buy, dec!(0.30), dec!(200)));

        // 100 * 0.50 + 200 * 0.30 = 50 + 60 = 110
        assert_eq!(tracker.total_exposure(), dec!(110));
    }
}
