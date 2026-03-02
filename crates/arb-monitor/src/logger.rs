use std::path::PathBuf;

use arb_core::config::GeneralConfig;
use arb_core::error::{ArbError, Result};
use tracing_appender::non_blocking::WorkerGuard;
use tracing_subscriber::{EnvFilter, fmt, layer::SubscriberExt, util::SubscriberInitExt};

/// Initialize structured logging with tracing.
///
/// Sets up:
/// - JSON format for structured log events
/// - Configurable level filter (from config or RUST_LOG env)
/// - Stdout output + optional file appender
/// - Non-blocking file writes (returns WorkerGuard that must be held)
///
/// The returned `WorkerGuard` must be kept alive for the duration of the program.
/// When dropped, it flushes pending log writes.
pub fn init_logging(config: &GeneralConfig) -> Result<Option<WorkerGuard>> {
    let level = &config.log_level;

    let env_filter = EnvFilter::try_from_default_env()
        .unwrap_or_else(|_| EnvFilter::new(level));

    let is_json = config.log_format == "json";

    match &config.log_file {
        Some(log_file) => {
            // Expand ~ in path
            let path = if log_file.starts_with("~/") {
                dirs::home_dir()
                    .unwrap_or_default()
                    .join(log_file.strip_prefix("~/").unwrap())
            } else {
                PathBuf::from(log_file)
            };

            // Ensure parent directory exists
            if let Some(parent) = path.parent() {
                std::fs::create_dir_all(parent).map_err(|e| {
                    ArbError::Config(format!("Cannot create log directory: {e}"))
                })?;
            }

            let file_name = path
                .file_name()
                .unwrap_or_default()
                .to_str()
                .unwrap_or("arb.log");
            let directory = path.parent().unwrap_or_else(|| std::path::Path::new("."));

            let file_appender = tracing_appender::rolling::daily(directory, file_name);
            let (non_blocking, guard) = tracing_appender::non_blocking(file_appender);

            if is_json {
                tracing_subscriber::registry()
                    .with(env_filter)
                    .with(
                        fmt::layer()
                            .json()
                            .with_writer(non_blocking)
                            .with_target(false),
                    )
                    .with(
                        fmt::layer()
                            .compact()
                            .with_writer(std::io::stderr),
                    )
                    .init();
            } else {
                tracing_subscriber::registry()
                    .with(env_filter)
                    .with(
                        fmt::layer()
                            .compact()
                            .with_writer(non_blocking),
                    )
                    .with(
                        fmt::layer()
                            .compact()
                            .with_writer(std::io::stderr),
                    )
                    .init();
            }

            Ok(Some(guard))
        }
        None => {
            // Stdout only
            if is_json {
                tracing_subscriber::registry()
                    .with(env_filter)
                    .with(fmt::layer().json().with_target(false))
                    .init();
            } else {
                tracing_subscriber::registry()
                    .with(env_filter)
                    .with(fmt::layer().compact())
                    .init();
            }

            Ok(None)
        }
    }
}
