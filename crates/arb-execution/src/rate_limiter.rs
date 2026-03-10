//! Token bucket rate limiter for managing Polymarket API request budgets.
//!
//! Prevents HTTP 429 (rate limit) responses by tracking request budgets
//! per endpoint category. Each bucket refills linearly over its window.

use std::sync::{Arc, Mutex};
use std::time::{Duration, Instant};

/// Internal token bucket state.
struct TokenBucket {
    max_tokens: u32,
    available: f64,
    window: Duration,
    last_refill: Instant,
}

impl TokenBucket {
    fn new(max_tokens: u32, window: Duration) -> Self {
        Self {
            max_tokens,
            available: f64::from(max_tokens),
            window,
            last_refill: Instant::now(),
        }
    }

    /// Refill tokens based on elapsed time since last refill.
    fn refill(&mut self) {
        let now = Instant::now();
        let elapsed = now.duration_since(self.last_refill);
        let tokens_to_add =
            (elapsed.as_secs_f64() / self.window.as_secs_f64()) * f64::from(self.max_tokens);
        self.available = (self.available + tokens_to_add).min(f64::from(self.max_tokens));
        self.last_refill = now;
    }

    /// Try to consume one token. Returns true if successful.
    fn try_acquire(&mut self) -> bool {
        self.refill();
        if self.available >= 1.0 {
            self.available -= 1.0;
            true
        } else {
            false
        }
    }

    /// Current number of whole tokens available.
    fn tokens_remaining(&mut self) -> u32 {
        self.refill();
        self.available as u32
    }
}

/// Thread-safe, cloneable rate limiter backed by a token bucket.
///
/// Tokens refill linearly: `max_tokens` are restored over `window`.
/// For example, 500 tokens over 10s means ~50 tokens/second.
#[derive(Clone)]
pub struct RateLimiter {
    inner: Arc<Mutex<TokenBucket>>,
}

impl RateLimiter {
    /// Create a new rate limiter that allows `max_tokens` requests per `window`.
    pub fn new(max_tokens: u32, window: Duration) -> Self {
        Self {
            inner: Arc::new(Mutex::new(TokenBucket::new(max_tokens, window))),
        }
    }

    /// Attempt to acquire a token without blocking.
    /// Returns `true` if a token was consumed, `false` if the bucket is empty.
    pub fn try_acquire(&self) -> bool {
        self.inner.lock().unwrap().try_acquire()
    }

    /// Asynchronously wait until a token is available, polling every 10ms.
    pub async fn acquire(&self) {
        loop {
            if self.try_acquire() {
                return;
            }
            tokio::time::sleep(Duration::from_millis(10)).await;
        }
    }

    /// Number of whole tokens currently available (triggers a refill check).
    pub fn tokens_remaining(&self) -> u32 {
        self.inner.lock().unwrap().tokens_remaining()
    }
}

/// Pre-configured rate limiters for each Polymarket API endpoint category.
///
/// Default budgets (per 10-second window):
/// - `orders`: 500 — order placement
/// - `cancels`: 500 — order cancellation
/// - `data`: 1000 — CLOB data queries (orderbooks, prices)
/// - `gamma`: 300 — Gamma market metadata
pub struct ApiRateLimiters {
    /// Order placement endpoint limiter.
    pub orders: RateLimiter,
    /// Order cancellation endpoint limiter.
    pub cancels: RateLimiter,
    /// CLOB data query endpoint limiter.
    pub data: RateLimiter,
    /// Gamma market metadata endpoint limiter.
    pub gamma: RateLimiter,
}

impl Default for ApiRateLimiters {
    fn default() -> Self {
        let window = Duration::from_secs(10);
        Self {
            orders: RateLimiter::new(500, window),
            cancels: RateLimiter::new(500, window),
            data: RateLimiter::new(1000, window),
            gamma: RateLimiter::new(300, window),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_rate_limiter_allows_within_budget() {
        let limiter = RateLimiter::new(10, Duration::from_secs(1));

        // Should be able to acquire all 10 tokens
        for i in 0..10 {
            assert!(limiter.try_acquire(), "should succeed on token {i}");
        }
    }

    #[test]
    fn test_rate_limiter_blocks_over_budget() {
        let limiter = RateLimiter::new(5, Duration::from_secs(1));

        // Drain the bucket
        for _ in 0..5 {
            assert!(limiter.try_acquire());
        }

        // Should fail — no tokens left
        assert!(!limiter.try_acquire(), "should fail when bucket is empty");
        assert_eq!(limiter.tokens_remaining(), 0);
    }

    #[tokio::test]
    async fn test_rate_limiter_refills_over_time() {
        let limiter = RateLimiter::new(100, Duration::from_millis(100));

        // Drain the bucket
        for _ in 0..100 {
            assert!(limiter.try_acquire());
        }
        assert!(!limiter.try_acquire());

        // Wait for the full window to elapse so all tokens refill
        tokio::time::sleep(Duration::from_millis(120)).await;

        // Should have refilled close to 100 tokens
        let remaining = limiter.tokens_remaining();
        assert!(
            remaining >= 90,
            "expected at least 90 tokens after full refill, got {remaining}"
        );
    }

    #[test]
    fn test_api_rate_limiters_default() {
        let limiters = ApiRateLimiters::default();

        assert_eq!(limiters.orders.tokens_remaining(), 500);
        assert_eq!(limiters.cancels.tokens_remaining(), 500);
        assert_eq!(limiters.data.tokens_remaining(), 1000);
        assert_eq!(limiters.gamma.tokens_remaining(), 300);
    }

    #[tokio::test]
    async fn test_acquire_waits_for_token() {
        let limiter = RateLimiter::new(1, Duration::from_millis(50));

        // Drain the single token
        assert!(limiter.try_acquire());
        assert!(!limiter.try_acquire());

        // acquire() should block until a token refills, then succeed
        let start = Instant::now();
        limiter.acquire().await;
        let elapsed = start.elapsed();

        // Should have waited at least some time for refill
        assert!(
            elapsed >= Duration::from_millis(10),
            "acquire should have waited for refill, elapsed: {elapsed:?}"
        );
    }

    #[test]
    fn test_clone_shares_state() {
        let limiter = RateLimiter::new(10, Duration::from_secs(1));
        let clone = limiter.clone();

        // Drain 5 via original
        for _ in 0..5 {
            assert!(limiter.try_acquire());
        }

        // Clone should see only 5 remaining
        assert_eq!(clone.tokens_remaining(), 5);
    }

    #[test]
    fn test_partial_refill() {
        let limiter = RateLimiter::new(100, Duration::from_millis(100));

        // Drain all tokens
        for _ in 0..100 {
            limiter.try_acquire();
        }
        assert_eq!(limiter.tokens_remaining(), 0);

        // Wait for ~half the window
        std::thread::sleep(Duration::from_millis(55));

        // Should have roughly half the tokens back (allow some timing slack)
        let remaining = limiter.tokens_remaining();
        assert!(
            remaining >= 40 && remaining <= 70,
            "expected roughly 50 tokens after half window, got {remaining}"
        );
    }
}
