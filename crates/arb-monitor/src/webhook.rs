use std::collections::HashMap;
use std::fmt;
use std::sync::Mutex;
use std::time::Instant;

use serde_json::json;
use tracing::{error, info, warn};

/// Severity level for webhook alerts.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum AlertLevel {
    Critical,
    Warning,
    Info,
}

impl fmt::Display for AlertLevel {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            AlertLevel::Critical => write!(f, "CRITICAL"),
            AlertLevel::Warning => write!(f, "WARNING"),
            AlertLevel::Info => write!(f, "INFO"),
        }
    }
}

/// Rate-limit window in seconds. Alerts for the same category are suppressed
/// if they arrive within this window of the previous send.
const RATE_LIMIT_SECS: u64 = 60;

/// Sends alerts to Discord and/or Telegram webhooks with per-category rate limiting.
///
/// Errors in sending are logged but never propagated — alerting must not crash the system.
pub struct WebhookAlerter {
    discord_url: Option<String>,
    telegram_bot_token: Option<String>,
    telegram_chat_id: Option<String>,
    http: reqwest::Client,
    /// Tracks the last send time per category for rate limiting.
    last_sent: Mutex<HashMap<String, Instant>>,
}

impl WebhookAlerter {
    /// Create a new alerter with optional webhook configurations.
    pub fn new(
        discord_url: Option<String>,
        telegram_bot_token: Option<String>,
        telegram_chat_id: Option<String>,
    ) -> Self {
        Self {
            discord_url,
            telegram_bot_token,
            telegram_chat_id,
            http: reqwest::Client::new(),
            last_sent: Mutex::new(HashMap::new()),
        }
    }

    /// Returns `true` if at least one webhook destination is configured.
    pub fn is_configured(&self) -> bool {
        self.discord_url.is_some()
            || (self.telegram_bot_token.is_some() && self.telegram_chat_id.is_some())
    }

    /// Format the alert message consistently for all destinations.
    pub fn format_message(level: AlertLevel, category: &str, message: &str) -> String {
        format!("[{level}] {category}: {message}")
    }

    /// Send an alert to all configured webhook destinations.
    ///
    /// Rate-limited: at most one alert per category per 60 seconds.
    /// Errors are logged but never returned — alerting must not crash the system.
    pub async fn send(&self, level: AlertLevel, category: &str, message: &str) {
        // Rate limiting check
        if !self.should_send(category) {
            info!(
                category = category,
                "Webhook alert rate-limited, skipping duplicate"
            );
            return;
        }

        let formatted = Self::format_message(level, category, message);

        match level {
            AlertLevel::Critical => {
                warn!(webhook_alert = %formatted, "Sending critical webhook alert")
            }
            AlertLevel::Warning => {
                info!(webhook_alert = %formatted, "Sending warning webhook alert")
            }
            AlertLevel::Info => info!(webhook_alert = %formatted, "Sending info webhook alert"),
        }

        if let Some(url) = &self.discord_url {
            self.send_discord(url, &formatted).await;
        }

        if let (Some(token), Some(chat_id)) = (&self.telegram_bot_token, &self.telegram_chat_id) {
            self.send_telegram(token, chat_id, &formatted).await;
        }
    }

    /// Check rate limit and record current send if allowed.
    /// Returns `true` if the alert should be sent.
    fn should_send(&self, category: &str) -> bool {
        let mut last_sent = self.last_sent.lock().unwrap_or_else(|e| e.into_inner());
        let now = Instant::now();
        let limit = std::time::Duration::from_secs(RATE_LIMIT_SECS);

        if let Some(last) = last_sent.get(category)
            && now.duration_since(*last) < limit
        {
            return false;
        }

        last_sent.insert(category.to_string(), now);
        true
    }

    /// POST to a Discord webhook URL.
    async fn send_discord(&self, url: &str, message: &str) {
        let payload = json!({ "content": message });

        match self.http.post(url).json(&payload).send().await {
            Ok(resp) if resp.status().is_success() => {
                info!("Discord webhook sent successfully");
            }
            Ok(resp) => {
                error!(
                    status = %resp.status(),
                    "Discord webhook returned non-success status"
                );
            }
            Err(e) => {
                error!(error = %e, "Failed to send Discord webhook");
            }
        }
    }

    /// POST to the Telegram Bot API sendMessage endpoint.
    async fn send_telegram(&self, token: &str, chat_id: &str, message: &str) {
        let url = format!("https://api.telegram.org/bot{token}/sendMessage");
        let payload = json!({
            "chat_id": chat_id,
            "text": message,
        });

        match self.http.post(&url).json(&payload).send().await {
            Ok(resp) if resp.status().is_success() => {
                info!("Telegram webhook sent successfully");
            }
            Ok(resp) => {
                error!(
                    status = %resp.status(),
                    "Telegram webhook returned non-success status"
                );
            }
            Err(e) => {
                error!(error = %e, "Failed to send Telegram webhook");
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_is_configured_all_none() {
        let alerter = WebhookAlerter::new(None, None, None);
        assert!(!alerter.is_configured());
    }

    #[test]
    fn test_is_configured_discord_only() {
        let alerter = WebhookAlerter::new(
            Some("https://discord.com/api/webhooks/123/abc".into()),
            None,
            None,
        );
        assert!(alerter.is_configured());
    }

    #[test]
    fn test_is_configured_telegram_only() {
        let alerter = WebhookAlerter::new(None, Some("bot123:ABC".into()), Some("987654".into()));
        assert!(alerter.is_configured());
    }

    #[test]
    fn test_is_configured_telegram_incomplete() {
        // Only bot token, no chat ID — not fully configured for Telegram
        let alerter = WebhookAlerter::new(None, Some("bot123:ABC".into()), None);
        assert!(!alerter.is_configured());
    }

    #[test]
    fn test_is_configured_both() {
        let alerter = WebhookAlerter::new(
            Some("https://discord.com/api/webhooks/123/abc".into()),
            Some("bot123:ABC".into()),
            Some("987654".into()),
        );
        assert!(alerter.is_configured());
    }

    #[test]
    fn test_message_formatting() {
        assert_eq!(
            WebhookAlerter::format_message(AlertLevel::Critical, "kill_switch", "System halted"),
            "[CRITICAL] kill_switch: System halted"
        );
        assert_eq!(
            WebhookAlerter::format_message(AlertLevel::Warning, "high_slippage", "Slip 2.5%"),
            "[WARNING] high_slippage: Slip 2.5%"
        );
        assert_eq!(
            WebhookAlerter::format_message(AlertLevel::Info, "trade_executed", "Bought YES @ 0.55"),
            "[INFO] trade_executed: Bought YES @ 0.55"
        );
    }

    #[test]
    fn test_alert_level_display() {
        assert_eq!(format!("{}", AlertLevel::Critical), "CRITICAL");
        assert_eq!(format!("{}", AlertLevel::Warning), "WARNING");
        assert_eq!(format!("{}", AlertLevel::Info), "INFO");
    }

    #[test]
    fn test_rate_limiting_same_category() {
        let alerter = WebhookAlerter::new(None, None, None);

        // First call should be allowed
        assert!(alerter.should_send("kill_switch"));

        // Second call within 60s should be blocked
        assert!(!alerter.should_send("kill_switch"));
    }

    #[test]
    fn test_rate_limiting_different_categories() {
        let alerter = WebhookAlerter::new(None, None, None);

        // First category
        assert!(alerter.should_send("kill_switch"));

        // Different category should still be allowed
        assert!(alerter.should_send("trade_executed"));

        // Original category still blocked
        assert!(!alerter.should_send("kill_switch"));
    }

    #[test]
    fn test_rate_limiting_tracks_multiple_categories() {
        let alerter = WebhookAlerter::new(None, None, None);

        let categories = [
            "kill_switch",
            "trade_executed",
            "daily_pnl",
            "api_error",
            "high_slippage",
        ];

        // All first sends should succeed
        for cat in &categories {
            assert!(
                alerter.should_send(cat),
                "First send for {cat} should succeed"
            );
        }

        // All second sends should be rate-limited
        for cat in &categories {
            assert!(
                !alerter.should_send(cat),
                "Second send for {cat} should be rate-limited"
            );
        }
    }

    #[tokio::test]
    async fn test_send_with_no_webhooks_configured() {
        // Should complete without error even with no webhooks
        let alerter = WebhookAlerter::new(None, None, None);
        alerter.send(AlertLevel::Info, "test", "hello").await;
        // No panic = success
    }

    #[tokio::test]
    async fn test_send_rate_limited_second_call() {
        let alerter = WebhookAlerter::new(None, None, None);

        // Send once
        alerter.send(AlertLevel::Info, "test_cat", "first").await;

        // Verify the category is now in the rate limit map
        let last_sent = alerter.last_sent.lock().unwrap();
        assert!(last_sent.contains_key("test_cat"));
    }
}
