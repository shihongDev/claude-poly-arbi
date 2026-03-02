use std::path::PathBuf;

use chrono::{DateTime, Utc};
use tracing::{error, info, warn};

/// File-based kill switch for emergency shutdown.
///
/// When active:
/// - All new trades are blocked
/// - All open orders are cancelled
/// - The daemon loop skips execution
///
/// The kill switch is checked every loop iteration by reading a file flag.
/// It can be activated:
/// - Programmatically (daily loss limit, drawdown)
/// - Manually (`arb kill` CLI command)
/// - By creating the file directly (`touch ~/.config/polymarket/KILL_SWITCH`)
///
/// Requires manual reset to resume (`arb resume` or delete the file).
pub struct KillSwitch {
    flag_path: PathBuf,
    active: bool,
    reason: Option<String>,
    activated_at: Option<DateTime<Utc>>,
}

impl KillSwitch {
    pub fn new() -> Self {
        let flag_path = dirs::config_dir()
            .unwrap_or_else(|| PathBuf::from("."))
            .join("polymarket")
            .join("KILL_SWITCH");

        let active = flag_path.exists();
        let reason = if active {
            std::fs::read_to_string(&flag_path).ok()
        } else {
            None
        };

        Self {
            flag_path,
            active,
            reason,
            activated_at: if active { Some(Utc::now()) } else { None },
        }
    }

    /// Re-read the file to check current state. Call this every tick.
    pub fn check(&mut self) -> bool {
        let file_exists = self.flag_path.exists();

        if file_exists && !self.active {
            // Externally activated (someone created the file)
            self.active = true;
            self.reason = std::fs::read_to_string(&self.flag_path).ok();
            self.activated_at = Some(Utc::now());
            warn!(
                reason = self.reason.as_deref().unwrap_or("external"),
                "Kill switch activated externally"
            );
        } else if !file_exists && self.active {
            // Externally deactivated (someone deleted the file)
            self.active = false;
            self.reason = None;
            self.activated_at = None;
            info!("Kill switch deactivated externally");
        }

        self.active
    }

    /// Activate the kill switch with a reason.
    pub fn activate(&mut self, reason: &str) {
        if let Some(parent) = self.flag_path.parent() {
            let _ = std::fs::create_dir_all(parent);
        }

        if let Err(e) = std::fs::write(&self.flag_path, reason) {
            error!(error = %e, "Failed to write kill switch file");
        }

        self.active = true;
        self.reason = Some(reason.to_string());
        self.activated_at = Some(Utc::now());

        warn!(reason = reason, "Kill switch ACTIVATED");
    }

    /// Deactivate the kill switch.
    pub fn deactivate(&mut self) {
        if self.flag_path.exists() {
            if let Err(e) = std::fs::remove_file(&self.flag_path) {
                error!(error = %e, "Failed to remove kill switch file");
            }
        }

        self.active = false;
        self.reason = None;
        self.activated_at = None;

        info!("Kill switch DEACTIVATED");
    }

    pub fn is_active(&self) -> bool {
        self.active
    }

    pub fn reason(&self) -> Option<&str> {
        self.reason.as_deref()
    }

    pub fn activated_at(&self) -> Option<DateTime<Utc>> {
        self.activated_at
    }
}

impl Default for KillSwitch {
    fn default() -> Self {
        Self::new()
    }
}
