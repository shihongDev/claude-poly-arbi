mod commands;
mod engine;

use clap::{Parser, Subcommand};

#[derive(Parser)]
#[command(
    name = "arb",
    about = "Polymarket arbitrage daemon",
    version
)]
struct Cli {
    #[command(subcommand)]
    command: Commands,
}

#[derive(Subcommand)]
enum Commands {
    /// One-shot scan: detect opportunities and print them
    Scan,
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
        Commands::Scan => commands::scan::execute().await,
        Commands::Run { live } => commands::run::execute(live).await,
        Commands::Status => commands::status::execute().await,
        Commands::Kill => commands::kill::execute(),
        Commands::Resume => commands::resume::execute(),
        Commands::History { limit } => commands::history::execute(limit),
        Commands::Config => commands::config::execute(),
        Commands::Simulate { condition_id } => {
            commands::simulate::execute(&condition_id).await
        }
    }
}
