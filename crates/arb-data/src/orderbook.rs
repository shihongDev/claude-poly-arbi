use arb_core::{
    OrderChunk, OrderbookLevel, OrderbookSnapshot, Side, VwapEstimate,
    error::{ArbError, Result},
    config::SlippageConfig,
    traits::SlippageEstimator,
};
use chrono::Utc;
use rust_decimal::Decimal;

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
                        size: s.parse().map_err(|e| {
                            ArbError::Orderbook(format!("Invalid size '{s}': {e}"))
                        })?,
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
        let book = make_book(
            &[(dec!(0.60), dec!(100)), (dec!(0.58), dec!(100))],
            &[],
        );
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
}
