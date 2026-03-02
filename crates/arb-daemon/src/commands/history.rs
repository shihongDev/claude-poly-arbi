use arb_core::config::ArbConfig;

pub fn execute(limit: usize) -> anyhow::Result<()> {
    let config = ArbConfig::load();
    let state_path = config.state_file_path();

    if !state_path.exists() {
        println!("No trade history found.");
        return Ok(());
    }

    // The state file contains a PositionTracker; trade history is logged separately.
    // For now, point users to the log file.
    println!("=== Trade History ===\n");
    println!(
        "Trade history is recorded in structured JSON logs at:"
    );
    println!(
        "  {}",
        config
            .general
            .log_file
            .as_deref()
            .unwrap_or("(stdout — set log_file in config)")
    );
    println!("\nFilter with: cat <log_file> | jq 'select(.event == \"trade_executed\")'");
    println!("\nShowing last {} entries from log.", limit);

    // If log file exists, tail it
    if let Some(log_file) = &config.general.log_file {
        let path = if log_file.starts_with("~/") {
            dirs::home_dir()
                .unwrap_or_default()
                .join(log_file.strip_prefix("~/").unwrap())
        } else {
            std::path::PathBuf::from(log_file)
        };

        if path.exists() {
            let content = std::fs::read_to_string(&path)?;
            let lines: Vec<&str> = content.lines().collect();
            let trade_lines: Vec<&&str> = lines
                .iter()
                .filter(|l| l.contains("trade_executed"))
                .collect();

            let start = trade_lines.len().saturating_sub(limit);
            for line in &trade_lines[start..] {
                println!("{line}");
            }

            if trade_lines.is_empty() {
                println!("\n(No trades recorded yet)");
            }
        }
    }

    Ok(())
}
