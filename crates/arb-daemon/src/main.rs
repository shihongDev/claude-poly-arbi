mod commands;
mod engine;
mod export;

use clap::{Parser, Subcommand};

#[derive(Parser)]
#[command(name = "arb", about = "Polymarket arbitrage daemon", version)]
struct Cli {
    #[command(subcommand)]
    command: Commands,
}

#[derive(Subcommand)]
enum Commands {
    /// One-shot scan: detect opportunities and print them
    Scan {
        /// Run comprehensive scan of ALL active markets
        #[arg(long)]
        comprehensive: bool,
        /// Minimum net edge in basis points to display
        #[arg(long, default_value = "0")]
        min_edge: u64,
        /// Comma-separated VWAP size tiers
        #[arg(long, default_value = "100,500,1000,5000")]
        size_tiers: String,
        /// Export full results to JSON file
        #[arg(long)]
        export: Option<String>,
        /// Export opportunities to CSV file
        #[arg(long)]
        export_csv: Option<String>,
        /// Max concurrent API requests
        #[arg(long, default_value = "10")]
        max_concurrent: usize,
        /// Per-request timeout in seconds
        #[arg(long, default_value = "15")]
        timeout: u64,
        /// Minimum 24h volume (USD) to include a market (filters out dead markets)
        #[arg(long, default_value = "0")]
        min_volume: u64,
        /// Show per-market scan progress
        #[arg(long)]
        verbose: bool,
    },
    /// Start the arbitrage daemon
    Run {
        /// Enable live trading (default: paper mode)
        #[arg(long)]
        live: bool,
    },
    /// Show current status: positions, PnL, exposure, kill switch state
    Status,
    /// Activate kill switch: cancel all orders and halt trading
    Kill,
    /// Deactivate kill switch and resume trading
    Resume,
    /// Print recent trade history
    History {
        /// Number of recent trades to show
        #[arg(short, long, default_value = "20")]
        limit: usize,
    },
    /// Validate and display current configuration
    Config,
    /// Run simulation engine on a specific market
    Simulate {
        /// Market condition ID to simulate
        condition_id: String,
    },
}

#[tokio::main]
async fn main() -> anyhow::Result<()> {
    let cli = Cli::parse();

    match cli.command {
        Commands::Scan {
            comprehensive,
            min_edge,
            size_tiers,
            export,
            export_csv,
            max_concurrent,
            timeout,
            min_volume,
            verbose,
        } => {
            if comprehensive {
                commands::scan::execute_comprehensive(
                    min_edge,
                    &size_tiers,
                    export,
                    export_csv,
                    max_concurrent,
                    timeout,
                    min_volume,
                    verbose,
                )
                .await
            } else {
                commands::scan::execute().await
            }
        }
        Commands::Run { live } => commands::run::execute(live).await,
        Commands::Status => commands::status::execute().await,
        Commands::Kill => commands::kill::execute(),
        Commands::Resume => commands::resume::execute(),
        Commands::History { limit } => commands::history::execute(limit),
        Commands::Config => commands::config::execute(),
        Commands::Simulate { condition_id } => commands::simulate::execute(&condition_id).await,
    }
}
