//! SQLite-backed append-only price store with rolling retention.
//!
//! Records price ticks from [`MarketState`] snapshots and provides
//! time-range and recent-N queries. A `cleanup` method enforces
//! a configurable retention window (default 30 days).

use chrono::{DateTime, TimeZone, Utc};
use rust_decimal::Decimal;
use rusqlite::{Connection, params};
use serde::{Deserialize, Serialize};
use std::path::Path;
use std::sync::Mutex;

use arb_core::MarketState;

/// A single price observation for one token in a market.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct PriceTick {
    pub condition_id: String,
    pub token_id: String,
    pub price: Decimal,
    pub best_bid: Option<Decimal>,
    pub best_ask: Option<Decimal>,
    pub volume_24h: Option<Decimal>,
    pub timestamp: DateTime<Utc>,
}

/// SQLite-backed append-only price history store.
///
/// Uses a `Mutex<Connection>` for thread-safe access. The SQLite database
/// is opened in WAL mode for better concurrent read performance.
pub struct PriceHistoryStore {
    conn: Mutex<Connection>,
}

/// Convert a `Decimal` to `f64` for SQLite storage.
fn decimal_to_f64(d: &Decimal) -> f64 {
    d.to_string().parse::<f64>().unwrap_or(0.0)
}

/// Convert an `Option<Decimal>` to `Option<f64>` for SQLite storage.
fn opt_decimal_to_f64(d: &Option<Decimal>) -> Option<f64> {
    d.as_ref().map(decimal_to_f64)
}

/// Convert an `f64` back to `Decimal`.
fn f64_to_decimal(f: f64) -> Decimal {
    Decimal::from_f64_retain(f).unwrap_or_default()
}

/// Convert an `Option<f64>` back to `Option<Decimal>`.
fn opt_f64_to_decimal(f: Option<f64>) -> Option<Decimal> {
    f.map(f64_to_decimal)
}

impl PriceHistoryStore {
    /// Open or create the SQLite database at the given path.
    pub fn open(db_path: &Path) -> anyhow::Result<Self> {
        let conn = Connection::open(db_path)?;
        let store = Self {
            conn: Mutex::new(conn),
        };
        store.init_schema()?;
        Ok(store)
    }

    /// Open an in-memory database (for testing).
    pub fn open_in_memory() -> anyhow::Result<Self> {
        let conn = Connection::open_in_memory()?;
        let store = Self {
            conn: Mutex::new(conn),
        };
        store.init_schema()?;
        Ok(store)
    }

    /// Create the schema and set pragmas.
    fn init_schema(&self) -> anyhow::Result<()> {
        let conn = self.conn.lock().unwrap();
        conn.execute_batch("PRAGMA journal_mode = WAL;")?;
        conn.execute_batch(
            "CREATE TABLE IF NOT EXISTS price_ticks (
                condition_id TEXT NOT NULL,
                token_id TEXT NOT NULL,
                price REAL NOT NULL,
                best_bid REAL,
                best_ask REAL,
                volume_24h REAL,
                ts INTEGER NOT NULL
            );
            CREATE INDEX IF NOT EXISTS idx_ticks_cid_ts ON price_ticks(condition_id, ts);",
        )?;
        Ok(())
    }

    /// Record a price tick from a `MarketState`.
    ///
    /// Extracts the first outcome price as the tick price, and records
    /// `best_bid`, `best_ask`, and `volume_24hr` from the market state.
    /// Each token_id in the market gets its own tick row with the
    /// corresponding outcome price.
    pub fn record_market(&self, market: &MarketState) -> anyhow::Result<()> {
        let conn = self.conn.lock().unwrap();
        let now_ms = Utc::now().timestamp_millis();

        // Record a tick for each token/outcome pair
        for (i, token_id) in market.token_ids.iter().enumerate() {
            let price = market
                .outcome_prices
                .get(i)
                .copied()
                .unwrap_or(Decimal::ZERO);

            conn.execute(
                "INSERT INTO price_ticks (condition_id, token_id, price, best_bid, best_ask, volume_24h, ts)
                 VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7)",
                params![
                    market.condition_id,
                    token_id,
                    decimal_to_f64(&price),
                    opt_decimal_to_f64(&market.best_bid),
                    opt_decimal_to_f64(&market.best_ask),
                    opt_decimal_to_f64(&market.volume_24hr),
                    now_ms,
                ],
            )?;
        }

        Ok(())
    }

    /// Record multiple markets in a single transaction (for engine cycle bulk writes).
    pub fn record_markets(&self, markets: &[MarketState]) -> anyhow::Result<()> {
        let conn = self.conn.lock().unwrap();
        let now_ms = Utc::now().timestamp_millis();

        let tx = conn.unchecked_transaction()?;
        {
            let mut stmt = tx.prepare_cached(
                "INSERT INTO price_ticks (condition_id, token_id, price, best_bid, best_ask, volume_24h, ts)
                 VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7)",
            )?;

            for market in markets {
                for (i, token_id) in market.token_ids.iter().enumerate() {
                    let price = market
                        .outcome_prices
                        .get(i)
                        .copied()
                        .unwrap_or(Decimal::ZERO);

                    stmt.execute(params![
                        market.condition_id,
                        token_id,
                        decimal_to_f64(&price),
                        opt_decimal_to_f64(&market.best_bid),
                        opt_decimal_to_f64(&market.best_ask),
                        opt_decimal_to_f64(&market.volume_24hr),
                        now_ms,
                    ])?;
                }
            }
        }
        tx.commit()?;

        Ok(())
    }

    /// Get price history for a `condition_id` between two timestamps.
    pub fn get_history(
        &self,
        condition_id: &str,
        since: DateTime<Utc>,
        until: DateTime<Utc>,
    ) -> anyhow::Result<Vec<PriceTick>> {
        let conn = self.conn.lock().unwrap();
        let since_ms = since.timestamp_millis();
        let until_ms = until.timestamp_millis();

        let mut stmt = conn.prepare(
            "SELECT condition_id, token_id, price, best_bid, best_ask, volume_24h, ts
             FROM price_ticks
             WHERE condition_id = ?1 AND ts >= ?2 AND ts <= ?3
             ORDER BY ts ASC",
        )?;

        let rows = stmt.query_map(params![condition_id, since_ms, until_ms], |row| {
            let ts_ms: i64 = row.get(6)?;
            Ok(PriceTick {
                condition_id: row.get(0)?,
                token_id: row.get(1)?,
                price: f64_to_decimal(row.get(2)?),
                best_bid: opt_f64_to_decimal(row.get(3)?),
                best_ask: opt_f64_to_decimal(row.get(4)?),
                volume_24h: opt_f64_to_decimal(row.get(5)?),
                timestamp: Utc.timestamp_millis_opt(ts_ms).single().unwrap_or_default(),
            })
        })?;

        let mut ticks = Vec::new();
        for row in rows {
            ticks.push(row?);
        }
        Ok(ticks)
    }

    /// Get the most recent N ticks for a `condition_id`.
    pub fn get_recent(&self, condition_id: &str, n: usize) -> anyhow::Result<Vec<PriceTick>> {
        let conn = self.conn.lock().unwrap();

        let mut stmt = conn.prepare(
            "SELECT condition_id, token_id, price, best_bid, best_ask, volume_24h, ts
             FROM price_ticks
             WHERE condition_id = ?1
             ORDER BY ts DESC
             LIMIT ?2",
        )?;

        let rows = stmt.query_map(params![condition_id, n as i64], |row| {
            let ts_ms: i64 = row.get(6)?;
            Ok(PriceTick {
                condition_id: row.get(0)?,
                token_id: row.get(1)?,
                price: f64_to_decimal(row.get(2)?),
                best_bid: opt_f64_to_decimal(row.get(3)?),
                best_ask: opt_f64_to_decimal(row.get(4)?),
                volume_24h: opt_f64_to_decimal(row.get(5)?),
                timestamp: Utc.timestamp_millis_opt(ts_ms).single().unwrap_or_default(),
            })
        })?;

        let mut ticks = Vec::new();
        for row in rows {
            ticks.push(row?);
        }
        // Reverse to return in chronological order (oldest first)
        ticks.reverse();
        Ok(ticks)
    }

    /// Calculate realized volatility (standard deviation of log returns) for a market.
    ///
    /// Queries price ticks for the given `condition_id` over the last `days` days,
    /// computes log returns `ln(price[i+1] / price[i])`, and returns their standard
    /// deviation. Returns `None` if fewer than 20 ticks are available (insufficient
    /// data for a meaningful volatility estimate).
    ///
    /// The returned value is the raw standard deviation of log returns, NOT annualized.
    /// The caller can annualize by multiplying by `sqrt(ticks_per_year)` if desired.
    pub fn realized_volatility(&self, condition_id: &str, days: u32) -> Option<f64> {
        let conn = self.conn.lock().unwrap();
        let now = Utc::now();
        let since = now - chrono::Duration::days(i64::from(days));
        let since_ms = since.timestamp_millis();
        let now_ms = now.timestamp_millis();

        // Query prices directly as f64 from SQLite, ordered chronologically.
        let mut stmt = conn
            .prepare(
                "SELECT price FROM price_ticks
                 WHERE condition_id = ?1 AND ts >= ?2 AND ts <= ?3
                 ORDER BY ts ASC",
            )
            .ok()?;

        let prices: Vec<f64> = stmt
            .query_map(params![condition_id, since_ms, now_ms], |row| {
                row.get::<_, f64>(0)
            })
            .ok()?
            .filter_map(|r| r.ok())
            .filter(|&p| p > 0.0)
            .collect();

        // Need at least 20 ticks to compute meaningful volatility.
        if prices.len() < 20 {
            return None;
        }

        // Compute log returns: ln(price[i+1] / price[i])
        let log_returns: Vec<f64> = prices
            .windows(2)
            .map(|w| (w[1] / w[0]).ln())
            .collect();

        if log_returns.is_empty() {
            return None;
        }

        // Mean of log returns
        let n = log_returns.len() as f64;
        let mean = log_returns.iter().sum::<f64>() / n;

        // Sample standard deviation (using n-1 for unbiased estimator)
        let variance = log_returns.iter().map(|r| (r - mean).powi(2)).sum::<f64>() / (n - 1.0);
        let std_dev = variance.sqrt();

        Some(std_dev)
    }

    /// Delete ticks older than `retention_days`.
    ///
    /// Returns the number of rows deleted.
    pub fn cleanup(&self, retention_days: u32) -> anyhow::Result<usize> {
        let conn = self.conn.lock().unwrap();
        let cutoff = Utc::now() - chrono::Duration::days(i64::from(retention_days));
        let cutoff_ms = cutoff.timestamp_millis();

        let deleted = conn.execute(
            "DELETE FROM price_ticks WHERE ts < ?1",
            params![cutoff_ms],
        )?;

        Ok(deleted)
    }

    /// Count total ticks in the database.
    pub fn tick_count(&self) -> anyhow::Result<usize> {
        let conn = self.conn.lock().unwrap();
        let count: i64 = conn.query_row("SELECT COUNT(*) FROM price_ticks", [], |row| row.get(0))?;
        Ok(count as usize)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use rust_decimal_macros::dec;

    /// Assert that two Decimals are approximately equal (within f64 roundtrip tolerance).
    fn assert_decimal_approx(actual: Decimal, expected: Decimal, label: &str) {
        let diff = (actual - expected).abs();
        assert!(
            diff < dec!(0.0001),
            "{label}: expected ~{expected}, got {actual} (diff={diff})"
        );
    }

    /// Helper: create a minimal `MarketState` for testing.
    fn make_market(condition_id: &str, price_yes: Decimal, price_no: Decimal) -> MarketState {
        MarketState {
            condition_id: condition_id.to_string(),
            question: "Test market?".to_string(),
            outcomes: vec!["Yes".to_string(), "No".to_string()],
            token_ids: vec!["tok_yes_1".to_string(), "tok_no_1".to_string()],
            outcome_prices: vec![price_yes, price_no],
            orderbooks: vec![],
            volume_24hr: Some(dec!(50000)),
            liquidity: Some(dec!(100000)),
            active: true,
            neg_risk: false,
            best_bid: Some(dec!(0.59)),
            best_ask: Some(dec!(0.62)),
            spread: Some(dec!(0.03)),
            last_trade_price: Some(dec!(0.60)),
            description: None,
            end_date_iso: None,
            slug: None,
            one_day_price_change: None,
            event_id: None,
            last_updated_gen: 0,
        }
    }

    /// Helper: insert N price ticks at evenly spaced timestamps via raw SQL.
    fn insert_ticks(store: &PriceHistoryStore, condition_id: &str, prices: &[f64]) {
        let conn = store.conn.lock().unwrap();
        let now_ms = Utc::now().timestamp_millis();
        // Space ticks 1 second apart, ending at now
        for (i, &price) in prices.iter().enumerate() {
            let ts = now_ms - ((prices.len() - 1 - i) as i64) * 1000;
            conn.execute(
                "INSERT INTO price_ticks (condition_id, token_id, price, best_bid, best_ask, volume_24h, ts)
                 VALUES (?1, ?2, ?3, NULL, NULL, NULL, ?4)",
                params![condition_id, "tok_vol", price, ts],
            )
            .unwrap();
        }
    }

    #[test]
    fn test_empty_store() {
        let store = PriceHistoryStore::open_in_memory().unwrap();
        assert_eq!(store.tick_count().unwrap(), 0);

        let ticks = store.get_recent("nonexistent", 10).unwrap();
        assert!(ticks.is_empty());
    }

    #[test]
    fn test_record_single_market_and_retrieve() {
        let store = PriceHistoryStore::open_in_memory().unwrap();
        let market = make_market("cid_001", dec!(0.60), dec!(0.40));

        store.record_market(&market).unwrap();

        // Should have 2 ticks (one per token_id)
        assert_eq!(store.tick_count().unwrap(), 2);

        let ticks = store.get_recent("cid_001", 10).unwrap();
        assert_eq!(ticks.len(), 2);

        // Verify first tick (Yes token)
        assert_eq!(ticks[0].condition_id, "cid_001");
        assert_eq!(ticks[0].token_id, "tok_yes_1");
        assert_decimal_approx(ticks[0].price, dec!(0.60), "yes price");
        assert_decimal_approx(ticks[0].best_bid.unwrap(), dec!(0.59), "best_bid");
        assert_decimal_approx(ticks[0].best_ask.unwrap(), dec!(0.62), "best_ask");
        assert_decimal_approx(ticks[0].volume_24h.unwrap(), dec!(50000), "volume_24h");

        // Verify second tick (No token)
        assert_eq!(ticks[1].condition_id, "cid_001");
        assert_eq!(ticks[1].token_id, "tok_no_1");
        assert_decimal_approx(ticks[1].price, dec!(0.40), "no price");
    }

    #[test]
    fn test_record_multiple_markets_bulk() {
        let store = PriceHistoryStore::open_in_memory().unwrap();

        let markets = vec![
            make_market("cid_bulk_1", dec!(0.70), dec!(0.30)),
            make_market("cid_bulk_2", dec!(0.55), dec!(0.45)),
            make_market("cid_bulk_3", dec!(0.80), dec!(0.20)),
        ];

        store.record_markets(&markets).unwrap();

        // 3 markets * 2 tokens = 6 ticks
        assert_eq!(store.tick_count().unwrap(), 6);

        // Verify each market's ticks are retrievable
        let ticks1 = store.get_recent("cid_bulk_1", 10).unwrap();
        assert_eq!(ticks1.len(), 2);
        assert_decimal_approx(ticks1[0].price, dec!(0.70), "bulk1 price");

        let ticks2 = store.get_recent("cid_bulk_2", 10).unwrap();
        assert_eq!(ticks2.len(), 2);
        assert_decimal_approx(ticks2[0].price, dec!(0.55), "bulk2 price");

        let ticks3 = store.get_recent("cid_bulk_3", 10).unwrap();
        assert_eq!(ticks3.len(), 2);
        assert_decimal_approx(ticks3[0].price, dec!(0.80), "bulk3 price");
    }

    #[test]
    fn test_get_history_time_range() {
        let store = PriceHistoryStore::open_in_memory().unwrap();

        // Insert ticks with specific timestamps directly via SQL
        {
            let conn = store.conn.lock().unwrap();
            let base_ts = Utc::now().timestamp_millis();

            // Insert ticks at known offsets: -3h, -2h, -1h, now
            for (i, offset_ms) in [
                -3 * 3600 * 1000i64,
                -2 * 3600 * 1000,
                -1 * 3600 * 1000,
                0,
            ]
            .iter()
            .enumerate()
            {
                conn.execute(
                    "INSERT INTO price_ticks (condition_id, token_id, price, best_bid, best_ask, volume_24h, ts)
                     VALUES (?1, ?2, ?3, NULL, NULL, NULL, ?4)",
                    params![
                        "cid_range",
                        format!("tok_{i}"),
                        0.50 + (i as f64) * 0.05,
                        base_ts + offset_ms,
                    ],
                )
                .unwrap();
            }
        }

        // Query: ticks from 2.5h ago to 0.5h ago => should get ticks at -2h and -1h
        let now = Utc::now();
        let since = now - chrono::Duration::minutes(150); // 2.5h ago
        let until = now - chrono::Duration::minutes(30); // 0.5h ago

        let ticks = store.get_history("cid_range", since, until).unwrap();
        assert_eq!(ticks.len(), 2);
    }

    #[test]
    fn test_get_recent_n_ticks() {
        let store = PriceHistoryStore::open_in_memory().unwrap();

        // Insert 5 ticks with distinct timestamps
        {
            let conn = store.conn.lock().unwrap();
            let base_ts = Utc::now().timestamp_millis();

            for i in 0..5 {
                conn.execute(
                    "INSERT INTO price_ticks (condition_id, token_id, price, best_bid, best_ask, volume_24h, ts)
                     VALUES (?1, ?2, ?3, NULL, NULL, NULL, ?4)",
                    params!["cid_recent", "tok_a", 0.50 + (i as f64) * 0.01, base_ts + i * 1000],
                )
                .unwrap();
            }
        }

        // Request only 3 most recent
        let ticks = store.get_recent("cid_recent", 3).unwrap();
        assert_eq!(ticks.len(), 3);

        // Should be in chronological order (oldest first)
        assert!(ticks[0].timestamp <= ticks[1].timestamp);
        assert!(ticks[1].timestamp <= ticks[2].timestamp);

        // The most recent tick should have the highest price
        assert_eq!(ticks[2].price, f64_to_decimal(0.54));
    }

    #[test]
    fn test_cleanup_removes_old_keeps_recent() {
        let store = PriceHistoryStore::open_in_memory().unwrap();

        {
            let conn = store.conn.lock().unwrap();
            let now_ms = Utc::now().timestamp_millis();

            // Insert an old tick (60 days ago)
            let old_ts = now_ms - 60 * 24 * 3600 * 1000;
            conn.execute(
                "INSERT INTO price_ticks (condition_id, token_id, price, best_bid, best_ask, volume_24h, ts)
                 VALUES (?1, ?2, ?3, NULL, NULL, NULL, ?4)",
                params!["cid_old", "tok_old", 0.50, old_ts],
            )
            .unwrap();

            // Insert a recent tick (1 day ago)
            let recent_ts = now_ms - 1 * 24 * 3600 * 1000;
            conn.execute(
                "INSERT INTO price_ticks (condition_id, token_id, price, best_bid, best_ask, volume_24h, ts)
                 VALUES (?1, ?2, ?3, NULL, NULL, NULL, ?4)",
                params!["cid_new", "tok_new", 0.70, recent_ts],
            )
            .unwrap();
        }

        assert_eq!(store.tick_count().unwrap(), 2);

        // Cleanup with 30-day retention
        let deleted = store.cleanup(30).unwrap();
        assert_eq!(deleted, 1);
        assert_eq!(store.tick_count().unwrap(), 1);

        // The recent tick should survive
        let ticks = store.get_recent("cid_new", 10).unwrap();
        assert_eq!(ticks.len(), 1);
        assert_eq!(ticks[0].price, f64_to_decimal(0.70));

        // The old tick should be gone
        let old_ticks = store.get_recent("cid_old", 10).unwrap();
        assert!(old_ticks.is_empty());
    }

    #[test]
    fn test_multiple_condition_ids_isolation() {
        let store = PriceHistoryStore::open_in_memory().unwrap();

        let market_a = make_market("cid_alpha", dec!(0.65), dec!(0.35));
        let market_b = make_market("cid_beta", dec!(0.80), dec!(0.20));

        store.record_market(&market_a).unwrap();
        store.record_market(&market_b).unwrap();

        // Total: 4 ticks (2 per market)
        assert_eq!(store.tick_count().unwrap(), 4);

        // Query alpha: should only get alpha's ticks
        let alpha_ticks = store.get_recent("cid_alpha", 10).unwrap();
        assert_eq!(alpha_ticks.len(), 2);
        for tick in &alpha_ticks {
            assert_eq!(tick.condition_id, "cid_alpha");
        }

        // Query beta: should only get beta's ticks
        let beta_ticks = store.get_recent("cid_beta", 10).unwrap();
        assert_eq!(beta_ticks.len(), 2);
        for tick in &beta_ticks {
            assert_eq!(tick.condition_id, "cid_beta");
        }
    }

    #[test]
    fn test_empty_results_for_unknown_condition_id() {
        let store = PriceHistoryStore::open_in_memory().unwrap();

        // Record some data for one market
        let market = make_market("cid_known", dec!(0.50), dec!(0.50));
        store.record_market(&market).unwrap();

        // Query an unknown condition_id
        let ticks = store.get_recent("cid_unknown", 10).unwrap();
        assert!(ticks.is_empty());

        let history = store.get_history(
            "cid_unknown",
            Utc::now() - chrono::Duration::hours(1),
            Utc::now(),
        )
        .unwrap();
        assert!(history.is_empty());
    }

    #[test]
    fn test_file_backed_store() {
        let dir = std::env::temp_dir().join("arb_data_test_price_history");
        let _ = std::fs::create_dir_all(&dir);
        let db_path = dir.join("test_prices.db");

        // Clean up any previous test run
        let _ = std::fs::remove_file(&db_path);

        {
            let store = PriceHistoryStore::open(&db_path).unwrap();
            let market = make_market("cid_file", dec!(0.75), dec!(0.25));
            store.record_market(&market).unwrap();
            assert_eq!(store.tick_count().unwrap(), 2);
        }

        // Re-open and verify data persists
        {
            let store = PriceHistoryStore::open(&db_path).unwrap();
            assert_eq!(store.tick_count().unwrap(), 2);
            let ticks = store.get_recent("cid_file", 10).unwrap();
            assert_eq!(ticks.len(), 2);
            assert_eq!(ticks[0].price, dec!(0.75));
        }

        // Clean up
        let _ = std::fs::remove_file(&db_path);
        let _ = std::fs::remove_dir(&dir);
    }

    #[test]
    fn test_decimal_roundtrip_precision() {
        let store = PriceHistoryStore::open_in_memory().unwrap();

        // Use precise decimal values
        let mut market = make_market("cid_precision", dec!(0.6543), dec!(0.3457));
        market.best_bid = Some(dec!(0.6500));
        market.best_ask = Some(dec!(0.6600));
        market.volume_24hr = Some(dec!(123456.789));

        store.record_market(&market).unwrap();

        let ticks = store.get_recent("cid_precision", 10).unwrap();
        assert_eq!(ticks.len(), 2);

        // Verify reasonable precision (f64 roundtrip may lose some precision)
        let yes_tick = &ticks[0];
        let price_diff = (yes_tick.price - dec!(0.6543)).abs();
        assert!(
            price_diff < dec!(0.0001),
            "Price roundtrip lost too much precision: {price_diff}"
        );
    }

    // ---------------------------------------------------------------
    // Realized volatility tests
    // ---------------------------------------------------------------

    #[test]
    fn test_realized_volatility_insufficient_data() {
        let store = PriceHistoryStore::open_in_memory().unwrap();

        // Insert only 10 ticks (below the 20-tick minimum)
        let prices: Vec<f64> = (0..10).map(|i| 0.50 + (i as f64) * 0.01).collect();
        insert_ticks(&store, "cid_few", &prices);

        let vol = store.realized_volatility("cid_few", 30);
        assert!(vol.is_none(), "Expected None for fewer than 20 ticks");
    }

    #[test]
    fn test_realized_volatility_constant_price() {
        let store = PriceHistoryStore::open_in_memory().unwrap();

        // Insert 50 ticks all at the same price
        let prices: Vec<f64> = vec![0.60; 50];
        insert_ticks(&store, "cid_const", &prices);

        let vol = store.realized_volatility("cid_const", 30);
        assert!(vol.is_some(), "Should have enough data");
        let vol = vol.unwrap();
        assert!(
            vol.abs() < 1e-12,
            "Constant price should yield zero volatility, got {vol}"
        );
    }

    #[test]
    fn test_realized_volatility_known_series() {
        let store = PriceHistoryStore::open_in_memory().unwrap();

        // Create a known price series: start at 0.50, alternate +1%/-1%
        // This produces log returns that alternate between ~+0.00995 and ~-0.01005,
        // giving a predictable standard deviation.
        let mut prices = Vec::with_capacity(30);
        let mut p = 0.50;
        for i in 0..30 {
            prices.push(p);
            if i % 2 == 0 {
                p *= 1.01; // +1%
            } else {
                p *= 0.99; // -1%
            }
        }
        insert_ticks(&store, "cid_known_vol", &prices);

        let vol = store.realized_volatility("cid_known_vol", 30);
        assert!(vol.is_some(), "Should have enough data (30 ticks)");
        let vol = vol.unwrap();

        // The log returns alternate between ln(1.01)~0.00995 and ln(0.99)~-0.01005.
        // Mean ~ -0.00005, each return deviates from mean by ~0.01.
        // Std dev should be approximately 0.01.
        assert!(
            vol > 0.005 && vol < 0.02,
            "Expected volatility near 0.01, got {vol}"
        );
    }

    #[test]
    fn test_realized_volatility_unknown_market() {
        let store = PriceHistoryStore::open_in_memory().unwrap();

        // Insert data for a different market
        let prices: Vec<f64> = vec![0.50; 50];
        insert_ticks(&store, "cid_other", &prices);

        // Query volatility for a condition_id with no data
        let vol = store.realized_volatility("cid_nonexistent", 30);
        assert!(
            vol.is_none(),
            "Unknown condition_id should return None"
        );
    }
}
