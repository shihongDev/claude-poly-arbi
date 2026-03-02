use arb_core::config::ArbConfig;
use arb_risk::kill_switch::KillSwitch;
use arb_risk::position_tracker::PositionTracker;

pub async fn execute() -> anyhow::Result<()> {
    let config = ArbConfig::load();

    println!("=== Polymarket Arb Status ===\n");

    // Kill switch
    let mut ks = KillSwitch::new();
    ks.check();
    if ks.is_active() {
        println!("Kill Switch:  ACTIVE ({})", ks.reason().unwrap_or("unknown"));
    } else {
        println!("Kill Switch:  inactive");
    }

    // Trading mode
    println!("Mode:         {}", config.general.trading_mode);
    println!("Min Edge:     {}bps", config.strategy.min_edge_bps);
    println!();

    // Positions
    let state_path = config.state_file_path();
    if state_path.exists() {
        match PositionTracker::load(&state_path) {
            Ok(tracker) => {
                let positions = tracker.all_positions();
                if positions.is_empty() {
                    println!("Positions:    None");
                } else {
                    println!("Positions ({}):", positions.len());
                    println!(
                        "  {:<20} {:<10} {:<12} {:<12} {:<12}",
                        "Token", "Size", "Entry", "Current", "PnL"
                    );
                    for pos in &positions {
                        let token_short = if pos.token_id.len() > 18 {
                            &pos.token_id[..18]
                        } else {
                            &pos.token_id
                        };
                        println!(
                            "  {:<20} {:<10} {:<12} {:<12} {:<12}",
                            token_short,
                            pos.size,
                            format!("${:.4}", pos.avg_entry_price),
                            format!("${:.4}", pos.current_price),
                            format!("${:.4}", pos.unrealized_pnl),
                        );
                    }
                    println!("\n  Total Exposure: ${}", tracker.total_exposure());
                }
            }
            Err(_) => {
                println!("Positions:    No state file found");
            }
        }
    } else {
        println!("Positions:    No state file");
    }

    println!();
    println!("Config:       {}", ArbConfig::default_path().display());
    println!("State:        {}", state_path.display());

    Ok(())
}
