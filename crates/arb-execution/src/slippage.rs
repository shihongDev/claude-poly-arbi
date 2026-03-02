use arb_core::{
    OrderChunk, OrderbookSnapshot, Side, VwapEstimate,
    config::SlippageConfig,
    error::{ArbError, Result},
    traits::SlippageEstimator,
};
use rust_decimal::Decimal;

/// Standalone VWAP slippage estimator for the execution layer.
///
/// Delegates to the same algorithm as `OrderbookProcessor` but lives in
/// arb-execution so the executor can estimate slippage without depending
/// on the full arb-data crate's poller infrastructure.
pub struct VwapSlippageEstimator {
    config: SlippageConfig,
}

impl VwapSlippageEstimator {
    pub fn new(config: SlippageConfig) -> Self {
        Self { config }
    }

    /// Check if an order should be split based on size threshold.
    pub fn should_split(&self, size: Decimal) -> bool {
        size > Decimal::from(self.config.order_split_threshold)
    }
}

impl SlippageEstimator for VwapSlippageEstimator {
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
            delay_ms += 500;
        }

        if remaining > Decimal::ZERO && chunks.is_empty() {
            return Err(ArbError::SlippageTooHigh {
                actual_bps: Decimal::from(10_000),
                max_bps: max_slippage_bps,
            });
        }

        Ok(chunks)
    }
}
