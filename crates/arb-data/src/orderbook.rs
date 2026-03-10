use arb_core::{
    OrderChunk, OrderbookLevel, OrderbookSnapshot, Side, VwapEstimate,
    config::SlippageConfig,
    error::{ArbError, Result},
    traits::SlippageEstimator,
};
use chrono::Utc;
use rust_decimal::Decimal;
use serde::Serialize;

/// Bid-ask spread and depth profile for an orderbook.
#[derive(Debug, Clone, Serialize)]
pub struct SpreadDepthProfile {
    pub token_id: String,
    pub best_bid: Decimal,
    pub best_ask: Decimal,
    pub spread: Decimal,
    pub spread_bps: Decimal,
    pub mid: Decimal,
    pub bid_depth_3: Decimal,
    pub ask_depth_3: Decimal,
    pub bid_depth_5: Decimal,
    pub ask_depth_5: Decimal,
}

/// Converts SDK orderbook responses and computes VWAP by walking bid/ask levels.
pub struct OrderbookProcessor {
    config: SlippageConfig,
}

impl OrderbookProcessor {
    pub fn new(config: SlippageConfig) -> Self {
        Self { config }
    }

    /// Build an `OrderbookSnapshot` from raw bid/ask string tuples (price, size).
    /// This is the format returned by the Polymarket CLOB API.
    pub fn build_snapshot(
        token_id: &str,
        bids: &[(String, String)],
        asks: &[(String, String)],
    ) -> Result<OrderbookSnapshot> {
        let parse_levels = |levels: &[(String, String)]| -> Result<Vec<OrderbookLevel>> {
            levels
                .iter()
                .map(|(p, s)| {
                    Ok(OrderbookLevel {
                        price: p.parse().map_err(|e| {
                            ArbError::Orderbook(format!("Invalid price '{p}': {e}"))
                        })?,
                        size: s
                            .parse()
                            .map_err(|e| ArbError::Orderbook(format!("Invalid size '{s}': {e}")))?,
                    })
                })
                .collect()
        };

        let mut parsed_bids = parse_levels(bids)?;
        let mut parsed_asks = parse_levels(asks)?;

        // Bids sorted descending (best bid first), asks sorted ascending (best ask first)
        parsed_bids.sort_by(|a, b| b.price.cmp(&a.price));
        parsed_asks.sort_by(|a, b| a.price.cmp(&b.price));

        Ok(OrderbookSnapshot {
            token_id: token_id.to_string(),
            bids: parsed_bids,
            asks: parsed_asks,
            timestamp: Utc::now(),
        })
    }

    /// Compute VWAP estimates at multiple size tiers.
    ///
    /// For each size in `sizes`, calls `estimate_vwap`. If a tier fails
    /// (e.g., insufficient liquidity), a zero `VwapEstimate` is returned
    /// for that tier so callers always get one result per input size.
    pub fn estimate_vwap_tiers(
        &self,
        book: &OrderbookSnapshot,
        side: Side,
        sizes: &[Decimal],
    ) -> Vec<VwapEstimate> {
        sizes
            .iter()
            .map(|&size| {
                self.estimate_vwap(book, side, size)
                    .unwrap_or(VwapEstimate {
                        vwap: Decimal::ZERO,
                        total_size: Decimal::ZERO,
                        levels_consumed: 0,
                        max_available: Decimal::ZERO,
                        slippage_bps: Decimal::ZERO,
                    })
            })
            .collect()
    }

    /// Compute bid-ask spread and depth profile for the given orderbook.
    pub fn spread_depth_profile(&self, book: &OrderbookSnapshot) -> SpreadDepthProfile {
        let best_bid = book.bids.first().map_or(Decimal::ZERO, |l| l.price);
        let best_ask = book.asks.first().map_or(Decimal::ZERO, |l| l.price);
        let spread = best_ask - best_bid;
        let mid = (best_ask + best_bid) / Decimal::from(2);
        let spread_bps = if mid > Decimal::ZERO {
            spread / mid * Decimal::from(10_000)
        } else {
            Decimal::ZERO
        };

        let depth = |levels: &[OrderbookLevel], n: usize| -> Decimal {
            levels.iter().take(n).map(|l| l.size).sum()
        };

        SpreadDepthProfile {
            token_id: book.token_id.clone(),
            best_bid,
            best_ask,
            spread,
            spread_bps,
            mid,
            bid_depth_3: depth(&book.bids, 3),
            ask_depth_3: depth(&book.asks, 3),
            bid_depth_5: depth(&book.bids, 5),
            ask_depth_5: depth(&book.asks, 5),
        }
    }
}

impl SlippageEstimator for OrderbookProcessor {
    /// Walk the orderbook to estimate VWAP for a given fill size.
    ///
    /// For Buy: walk asks (ascending price).
    /// For Sell: walk bids (descending price).
    ///
    /// Returns VwapEstimate with the volume-weighted average price, levels consumed,
    /// and slippage in basis points from best available price.
    fn estimate_vwap(
        &self,
        book: &OrderbookSnapshot,
        side: Side,
        size: Decimal,
    ) -> Result<VwapEstimate> {
        if size <= Decimal::ZERO {
            return Err(ArbError::Orderbook("Size must be positive".into()));
        }

        let levels = match side {
            Side::Buy => &book.asks,
            Side::Sell => &book.bids,
        };

        if levels.is_empty() {
            return Err(ArbError::InsufficientLiquidity {
                needed: size,
                available: Decimal::ZERO,
            });
        }

        let mut remaining = size;
        let mut total_cost = Decimal::ZERO;
        let mut levels_consumed = 0usize;
        let mut max_available = Decimal::ZERO;

        for level in levels.iter().take(self.config.vwap_depth_levels) {
            max_available += level.size;
            let fill = remaining.min(level.size);
            total_cost += fill * level.price;
            remaining -= fill;
            levels_consumed += 1;

            if remaining <= Decimal::ZERO {
                break;
            }
        }

        if remaining > Decimal::ZERO {
            return Err(ArbError::InsufficientLiquidity {
                needed: size,
                available: max_available,
            });
        }

        let vwap = total_cost / size;
        let best_price = levels[0].price;
        let slippage_bps = if best_price > Decimal::ZERO {
            ((vwap - best_price).abs() / best_price) * Decimal::from(10_000)
        } else {
            Decimal::ZERO
        };

        Ok(VwapEstimate {
            vwap,
            total_size: size,
            levels_consumed,
            max_available,
            slippage_bps,
        })
    }

    /// Split a large order into chunks that each stay within the max slippage threshold.
    fn split_order(
        &self,
        book: &OrderbookSnapshot,
        side: Side,
        total_size: Decimal,
        max_slippage_bps: Decimal,
    ) -> Result<Vec<OrderChunk>> {
        let levels = match side {
            Side::Buy => &book.asks,
            Side::Sell => &book.bids,
        };

        if levels.is_empty() {
            return Err(ArbError::InsufficientLiquidity {
                needed: total_size,
                available: Decimal::ZERO,
            });
        }

        let mut chunks = Vec::new();
        let mut remaining = total_size;
        let best_price = levels[0].price;
        let mut delay_ms = 0u64;

        for level in levels.iter().take(self.config.vwap_depth_levels) {
            if remaining <= Decimal::ZERO {
                break;
            }

            // Check if adding this level would exceed slippage
            let level_slippage_bps = if best_price > Decimal::ZERO {
                ((level.price - best_price).abs() / best_price) * Decimal::from(10_000)
            } else {
                Decimal::ZERO
            };

            if level_slippage_bps > max_slippage_bps {
                break;
            }

            let fill = remaining.min(level.size);
            chunks.push(OrderChunk {
                size: fill,
                limit_price: level.price,
                delay_ms,
            });
            remaining -= fill;
            delay_ms += 500; // stagger by 500ms to reduce self-impact
        }

        if remaining > Decimal::ZERO && chunks.is_empty() {
            return Err(ArbError::SlippageTooHigh {
                actual_bps: Decimal::from(10_000), // entire book exceeds limit
                max_bps: max_slippage_bps,
            });
        }

        Ok(chunks)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use rust_decimal_macros::dec;

    fn make_book(bids: &[(Decimal, Decimal)], asks: &[(Decimal, Decimal)]) -> OrderbookSnapshot {
        OrderbookSnapshot {
            token_id: "test_token".to_string(),
            bids: bids
                .iter()
                .map(|&(price, size)| OrderbookLevel { price, size })
                .collect(),
            asks: asks
                .iter()
                .map(|&(price, size)| OrderbookLevel { price, size })
                .collect(),
            timestamp: Utc::now(),
        }
    }

    fn default_processor() -> OrderbookProcessor {
        OrderbookProcessor::new(SlippageConfig {
            max_slippage_bps: 100,
            order_split_threshold: 500,
            prefer_post_only: true,
            vwap_depth_levels: 10,
        })
    }

    #[test]
    fn test_vwap_single_level_buy() {
        let book = make_book(&[], &[(dec!(0.55), dec!(100))]);
        let proc = default_processor();
        let result = proc.estimate_vwap(&book, Side::Buy, dec!(50)).unwrap();
        assert_eq!(result.vwap, dec!(0.55));
        assert_eq!(result.slippage_bps, dec!(0)); // single level, no slippage
        assert_eq!(result.levels_consumed, 1);
    }

    #[test]
    fn test_vwap_multi_level_buy() {
        // 100 @ 0.50, 100 @ 0.52, 100 @ 0.55
        let book = make_book(
            &[],
            &[
                (dec!(0.50), dec!(100)),
                (dec!(0.52), dec!(100)),
                (dec!(0.55), dec!(100)),
            ],
        );
        let proc = default_processor();

        // Buy 150: 100 @ 0.50 + 50 @ 0.52 = (50 + 26) / 150 = 76/150 ≈ 0.5066...
        let result = proc.estimate_vwap(&book, Side::Buy, dec!(150)).unwrap();
        let expected_vwap = (dec!(100) * dec!(0.50) + dec!(50) * dec!(0.52)) / dec!(150);
        assert_eq!(result.vwap, expected_vwap);
        assert_eq!(result.levels_consumed, 2);
        assert!(result.slippage_bps > Decimal::ZERO);
    }

    #[test]
    fn test_vwap_sell() {
        let book = make_book(&[(dec!(0.60), dec!(100)), (dec!(0.58), dec!(100))], &[]);
        let proc = default_processor();

        // Sell 150: 100 @ 0.60 + 50 @ 0.58
        let result = proc.estimate_vwap(&book, Side::Sell, dec!(150)).unwrap();
        let expected_vwap = (dec!(100) * dec!(0.60) + dec!(50) * dec!(0.58)) / dec!(150);
        assert_eq!(result.vwap, expected_vwap);
        assert_eq!(result.levels_consumed, 2);
    }

    #[test]
    fn test_vwap_insufficient_liquidity() {
        let book = make_book(&[], &[(dec!(0.55), dec!(50))]);
        let proc = default_processor();
        let result = proc.estimate_vwap(&book, Side::Buy, dec!(100));
        assert!(result.is_err());
        match result.unwrap_err() {
            ArbError::InsufficientLiquidity { needed, available } => {
                assert_eq!(needed, dec!(100));
                assert_eq!(available, dec!(50));
            }
            other => panic!("Expected InsufficientLiquidity, got: {other:?}"),
        }
    }

    #[test]
    fn test_vwap_empty_book() {
        let book = make_book(&[], &[]);
        let proc = default_processor();
        let result = proc.estimate_vwap(&book, Side::Buy, dec!(10));
        assert!(result.is_err());
    }

    #[test]
    fn test_split_order() {
        let book = make_book(
            &[],
            &[
                (dec!(0.50), dec!(200)),
                (dec!(0.51), dec!(200)),
                (dec!(0.55), dec!(200)),
            ],
        );
        let proc = default_processor();
        let chunks = proc
            .split_order(&book, Side::Buy, dec!(500), dec!(300)) // 3% max slippage
            .unwrap();

        // 0.50 → 0bps, 0.51 → 200bps, 0.55 → 1000bps (exceeds 300bps)
        assert_eq!(chunks.len(), 2);
        assert_eq!(chunks[0].size, dec!(200));
        assert_eq!(chunks[0].limit_price, dec!(0.50));
        assert_eq!(chunks[1].size, dec!(200));
        assert_eq!(chunks[1].limit_price, dec!(0.51));
    }

    #[test]
    fn test_build_snapshot_from_strings() {
        let bids = vec![
            ("0.55".to_string(), "100".to_string()),
            ("0.54".to_string(), "200".to_string()),
        ];
        let asks = vec![
            ("0.57".to_string(), "150".to_string()),
            ("0.58".to_string(), "250".to_string()),
        ];
        let snap = OrderbookProcessor::build_snapshot("token123", &bids, &asks).unwrap();
        assert_eq!(snap.token_id, "token123");
        assert_eq!(snap.bids[0].price, dec!(0.55)); // best bid first (descending)
        assert_eq!(snap.asks[0].price, dec!(0.57)); // best ask first (ascending)
    }

    #[test]
    fn test_vwap_tiers() {
        // 3 ask levels: 200 @ 0.50, 200 @ 0.52, 200 @ 0.55
        let book = make_book(
            &[],
            &[
                (dec!(0.50), dec!(200)),
                (dec!(0.52), dec!(200)),
                (dec!(0.55), dec!(200)),
            ],
        );
        let proc = default_processor();
        let tiers = proc.estimate_vwap_tiers(&book, Side::Buy, &[dec!(100), dec!(300), dec!(1000)]);

        assert_eq!(tiers.len(), 3);

        // Tier 1: 100 @ 0.50 — fits in first level
        assert_eq!(tiers[0].vwap, dec!(0.50));
        assert_eq!(tiers[0].total_size, dec!(100));
        assert_eq!(tiers[0].levels_consumed, 1);

        // Tier 2: 200 @ 0.50 + 100 @ 0.52 = (100 + 52) / 300
        let expected = (dec!(200) * dec!(0.50) + dec!(100) * dec!(0.52)) / dec!(300);
        assert_eq!(tiers[1].vwap, expected);
        assert_eq!(tiers[1].total_size, dec!(300));
        assert_eq!(tiers[1].levels_consumed, 2);

        // Tier 3: 1000 exceeds total liquidity (600), should return zero estimate
        assert_eq!(tiers[2].vwap, Decimal::ZERO);
        assert_eq!(tiers[2].total_size, Decimal::ZERO);
        assert_eq!(tiers[2].levels_consumed, 0);
    }

    #[test]
    fn test_spread_depth_profile() {
        let book = make_book(
            &[
                (dec!(0.55), dec!(100)),
                (dec!(0.54), dec!(80)),
                (dec!(0.53), dec!(60)),
                (dec!(0.52), dec!(40)),
                (dec!(0.51), dec!(20)),
            ],
            &[
                (dec!(0.57), dec!(90)),
                (dec!(0.58), dec!(70)),
                (dec!(0.59), dec!(50)),
                (dec!(0.60), dec!(30)),
                (dec!(0.61), dec!(10)),
            ],
        );
        let proc = default_processor();
        let profile = proc.spread_depth_profile(&book);

        assert_eq!(profile.token_id, "test_token");
        assert_eq!(profile.best_bid, dec!(0.55));
        assert_eq!(profile.best_ask, dec!(0.57));
        assert_eq!(profile.spread, dec!(0.02));
        assert_eq!(profile.mid, dec!(0.56));
        // spread_bps = 0.02 / 0.56 * 10000
        let expected_bps = dec!(0.02) / dec!(0.56) * dec!(10000);
        assert_eq!(profile.spread_bps, expected_bps);

        // bid_depth_3 = 100 + 80 + 60 = 240
        assert_eq!(profile.bid_depth_3, dec!(240));
        // ask_depth_3 = 90 + 70 + 50 = 210
        assert_eq!(profile.ask_depth_3, dec!(210));
        // bid_depth_5 = 100 + 80 + 60 + 40 + 20 = 300
        assert_eq!(profile.bid_depth_5, dec!(300));
        // ask_depth_5 = 90 + 70 + 50 + 30 + 10 = 250
        assert_eq!(profile.ask_depth_5, dec!(250));
    }

    #[test]
    fn test_spread_depth_empty_book() {
        let book = make_book(&[], &[]);
        let proc = default_processor();
        let profile = proc.spread_depth_profile(&book);

        assert_eq!(profile.best_bid, Decimal::ZERO);
        assert_eq!(profile.best_ask, Decimal::ZERO);
        assert_eq!(profile.spread, Decimal::ZERO);
        assert_eq!(profile.mid, Decimal::ZERO);
        assert_eq!(profile.spread_bps, Decimal::ZERO);
        assert_eq!(profile.bid_depth_3, Decimal::ZERO);
        assert_eq!(profile.ask_depth_3, Decimal::ZERO);
        assert_eq!(profile.bid_depth_5, Decimal::ZERO);
        assert_eq!(profile.ask_depth_5, Decimal::ZERO);
    }
}
