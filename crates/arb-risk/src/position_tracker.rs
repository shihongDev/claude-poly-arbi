use std::collections::HashMap;
use std::fs::OpenOptions;
use std::io::{BufRead, BufReader, Write as _};
use std::path::{Path, PathBuf};

use arb_core::{
    ExecutionReport, Position, Side,
    error::{ArbError, Result},
};
use chrono::{DateTime, Utc};
use rust_decimal::Decimal;
use serde::{Deserialize, Serialize};

/// A single write-ahead log entry representing one fill.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct WalEntry {
    pub token_id: String,
    pub side: String,
    pub price: Decimal,
    pub size: Decimal,
    pub ts: DateTime<Utc>,
}

/// Tracks virtual and real positions by token_id.
///
/// Updated on every ExecutionReport. Persisted to a JSON state file
/// on graceful shutdown so state survives restarts.
///
/// Optionally writes a write-ahead log (WAL) so positions can be
/// reconstructed from the fill history after a crash.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct PositionTracker {
    positions: HashMap<String, Position>,
    #[serde(skip)]
    wal_path: Option<PathBuf>,
}

impl PositionTracker {
    pub fn new() -> Self {
        Self {
            positions: HashMap::new(),
            wal_path: None,
        }
    }

    /// Builder method to enable the write-ahead log at the given path.
    pub fn with_wal(mut self, path: PathBuf) -> Self {
        self.wal_path = Some(path);
        self
    }

    /// Update positions from an execution report.
    pub fn update(&mut self, report: &ExecutionReport) {
        for leg in &report.legs {
            let side_str = match leg.side {
                Side::Buy => "Buy",
                Side::Sell => "Sell",
            };
            self.append_wal(
                &leg.token_id,
                side_str,
                leg.actual_fill_price,
                leg.filled_size,
            );

            self.apply_fill(
                &leg.token_id,
                &leg.condition_id,
                leg.side,
                leg.actual_fill_price,
                leg.filled_size,
            );
        }
    }

    /// Apply a single fill to the position state (shared by `update` and `restore_from_wal`).
    fn apply_fill(
        &mut self,
        token_id: &str,
        condition_id: &str,
        side: Side,
        price: Decimal,
        size: Decimal,
    ) {
        let pos = self
            .positions
            .entry(token_id.to_string())
            .or_insert_with(|| Position {
                token_id: token_id.to_string(),
                condition_id: condition_id.to_string(),
                size: Decimal::ZERO,
                avg_entry_price: Decimal::ZERO,
                current_price: price,
                unrealized_pnl: Decimal::ZERO,
            });

        match side {
            Side::Buy => {
                let new_cost = pos.avg_entry_price * pos.size + price * size;
                pos.size += size;
                if pos.size > Decimal::ZERO {
                    pos.avg_entry_price = new_cost / pos.size;
                }
            }
            Side::Sell => {
                pos.size -= size;
                if pos.size <= Decimal::ZERO {
                    pos.size = Decimal::ZERO;
                    pos.avg_entry_price = Decimal::ZERO;
                }
            }
        }

        pos.current_price = price;
        pos.unrealized_pnl = (pos.current_price - pos.avg_entry_price) * pos.size;
    }

    /// Append a fill entry to the write-ahead log.
    ///
    /// If `wal_path` is `None`, this is a no-op.
    /// Errors are logged but never propagated so the trading system
    /// does not crash due to a WAL I/O failure.
    pub fn append_wal(&self, token_id: &str, side: &str, price: Decimal, size: Decimal) {
        let Some(path) = &self.wal_path else {
            return;
        };

        let entry = WalEntry {
            token_id: token_id.to_string(),
            side: side.to_string(),
            price,
            size,
            ts: Utc::now(),
        };

        let line = match serde_json::to_string(&entry) {
            Ok(l) => l,
            Err(e) => {
                tracing::error!("WAL serialize error: {e}");
                return;
            }
        };

        let result = (|| -> std::io::Result<()> {
            if let Some(parent) = path.parent() {
                std::fs::create_dir_all(parent)?;
            }
            let mut file = OpenOptions::new().create(true).append(true).open(path)?;
            writeln!(file, "{line}")?;
            Ok(())
        })();

        if let Err(e) = result {
            tracing::error!("WAL write error: {e}");
        }
    }

    /// Read all entries from a WAL file.
    ///
    /// Malformed lines are logged and skipped.
    pub fn replay_wal(path: &Path) -> Vec<WalEntry> {
        let file = match std::fs::File::open(path) {
            Ok(f) => f,
            Err(e) => {
                tracing::warn!("Cannot open WAL for replay: {e}");
                return Vec::new();
            }
        };

        let reader = BufReader::new(file);
        let mut entries = Vec::new();

        for (i, line) in reader.lines().enumerate() {
            let line = match line {
                Ok(l) => l,
                Err(e) => {
                    tracing::warn!("WAL read error at line {i}: {e}");
                    continue;
                }
            };

            if line.trim().is_empty() {
                continue;
            }

            match serde_json::from_str::<WalEntry>(&line) {
                Ok(entry) => entries.push(entry),
                Err(e) => {
                    tracing::warn!("WAL parse error at line {i}: {e}");
                }
            }
        }

        entries
    }

    /// Restore position state by replaying all fills from a WAL file.
    ///
    /// This reads the WAL and applies each fill to the current position
    /// state. The WAL itself is not re-written (no `append_wal` calls).
    pub fn restore_from_wal(&mut self, path: &Path) {
        let entries = Self::replay_wal(path);
        for entry in &entries {
            let side = match entry.side.as_str() {
                "Buy" => Side::Buy,
                "Sell" => Side::Sell,
                other => {
                    tracing::warn!("WAL unknown side '{other}', skipping");
                    continue;
                }
            };
            // When restoring, we don't have the original condition_id, so we
            // use the existing position's condition_id if available, or empty string.
            let condition_id = self
                .positions
                .get(&entry.token_id)
                .map(|p| p.condition_id.clone())
                .unwrap_or_default();
            self.apply_fill(
                &entry.token_id,
                &condition_id,
                side,
                entry.price,
                entry.size,
            );
        }
    }

    pub fn get(&self, token_id: &str) -> Option<&Position> {
        self.positions.get(token_id)
    }

    pub fn all_positions(&self) -> Vec<&Position> {
        self.positions.values().collect()
    }

    /// Total exposure: sum of |size * current_price| for all positions.
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
                condition_id: String::new(),
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

    // --- WAL tests ---

    /// Helper: create a unique temp WAL path that won't collide with parallel tests.
    fn temp_wal_path(name: &str) -> PathBuf {
        let dir = std::env::temp_dir().join("arb_risk_wal_tests");
        std::fs::create_dir_all(&dir).unwrap();
        dir.join(format!("{name}_{}.wal", Uuid::new_v4()))
    }

    #[test]
    fn test_wal_append_and_replay() {
        let wal_path = temp_wal_path("append_replay");

        // Write entries via append_wal
        let tracker = PositionTracker::new().with_wal(wal_path.clone());
        tracker.append_wal("tok_a", "Buy", dec!(0.45), dec!(100));
        tracker.append_wal("tok_b", "Sell", dec!(0.60), dec!(50));
        tracker.append_wal("tok_a", "Buy", dec!(0.47), dec!(200));

        // Replay and verify
        let entries = PositionTracker::replay_wal(&wal_path);
        assert_eq!(entries.len(), 3);

        assert_eq!(entries[0].token_id, "tok_a");
        assert_eq!(entries[0].side, "Buy");
        assert_eq!(entries[0].price, dec!(0.45));
        assert_eq!(entries[0].size, dec!(100));

        assert_eq!(entries[1].token_id, "tok_b");
        assert_eq!(entries[1].side, "Sell");
        assert_eq!(entries[1].price, dec!(0.60));
        assert_eq!(entries[1].size, dec!(50));

        assert_eq!(entries[2].token_id, "tok_a");
        assert_eq!(entries[2].side, "Buy");
        assert_eq!(entries[2].price, dec!(0.47));
        assert_eq!(entries[2].size, dec!(200));

        // Cleanup
        let _ = std::fs::remove_file(&wal_path);
    }

    #[test]
    fn test_wal_restores_positions() {
        let wal_path = temp_wal_path("restore");

        // Tracker 1: record fills through the normal update path (which writes WAL)
        let mut tracker1 = PositionTracker::new().with_wal(wal_path.clone());
        tracker1.update(&make_report("tok_a", Side::Buy, dec!(0.45), dec!(100)));
        tracker1.update(&make_report("tok_a", Side::Buy, dec!(0.55), dec!(100)));
        tracker1.update(&make_report("tok_b", Side::Buy, dec!(0.30), dec!(200)));
        tracker1.update(&make_report("tok_a", Side::Sell, dec!(0.50), dec!(50)));

        // Tracker 2: fresh tracker restored from WAL
        let mut tracker2 = PositionTracker::new();
        tracker2.restore_from_wal(&wal_path);

        // Both trackers should have identical position state
        let pos1a = tracker1.get("tok_a").unwrap();
        let pos2a = tracker2.get("tok_a").unwrap();
        assert_eq!(pos1a.size, pos2a.size);
        assert_eq!(pos1a.avg_entry_price, pos2a.avg_entry_price);
        assert_eq!(pos1a.current_price, pos2a.current_price);

        let pos1b = tracker1.get("tok_b").unwrap();
        let pos2b = tracker2.get("tok_b").unwrap();
        assert_eq!(pos1b.size, pos2b.size);
        assert_eq!(pos1b.avg_entry_price, pos2b.avg_entry_price);

        // Verify actual values
        assert_eq!(pos2a.size, dec!(150)); // 100 + 100 - 50
        assert_eq!(pos2b.size, dec!(200));

        // Cleanup
        let _ = std::fs::remove_file(&wal_path);
    }

    #[test]
    fn test_wal_empty_file() {
        let wal_path = temp_wal_path("empty");

        // Create an empty file
        std::fs::write(&wal_path, "").unwrap();

        let entries = PositionTracker::replay_wal(&wal_path);
        assert!(entries.is_empty());

        // Cleanup
        let _ = std::fs::remove_file(&wal_path);
    }

    #[test]
    fn test_wal_disabled() {
        // When wal_path is None, append_wal is a no-op (should not panic or create files)
        let tracker = PositionTracker::new();
        assert!(tracker.wal_path.is_none());

        // This should silently do nothing
        tracker.append_wal("tok_a", "Buy", dec!(0.50), dec!(100));

        // No file created, no panic -- test passes if we get here
    }
}
