mod config;
mod error;

use clap::Parser;
use config::LocalConfig;
use std::path::PathBuf;
use tracing::info;
use tracing_subscriber::{fmt, prelude::*, EnvFilter};

#[derive(Parser, Debug)]
#[command(name = "rbackup2")]
#[command(about = "Multiplatform backup client using restic", long_about = None)]
struct Args {
    #[arg(short, long, value_name = "FILE")]
    config: PathBuf,
}

fn main() {
    let args = Args::parse();

    if let Err(e) = run(args) {
        eprintln!("Error: {}", e);
        std::process::exit(1);
    }
}

fn run(args: Args) -> error::Result<()> {
    let config = LocalConfig::from_file(&args.config)?;

    setup_logging(&config)?;

    info!("========================================");
    info!("  rbackup2 - Backup Client");
    info!("========================================");
    info!("Device ID: {}", config.device.id);
    info!(
        "Database: {}:{}/{}",
        config.database.host, config.database.port, config.database.user
    );
    info!("HTTP Bind: {}", config.client.http_bind);
    info!("Log File: {}", config.client.log_file);
    if config.metrics.enabled {
        info!(
            "Metrics: enabled (Pushgateway: {})",
            config
                .metrics
                .prometheus_pushgateway
                .as_deref()
                .unwrap_or("not configured")
        );
    } else {
        info!("Metrics: disabled");
    }
    info!("========================================");

    info!("Configuration loaded successfully");
    info!("Phase 1 complete - project foundation ready");

    Ok(())
}

fn setup_logging(config: &LocalConfig) -> error::Result<()> {
    let file_appender = tracing_appender::rolling::daily(
        std::path::Path::new(&config.client.log_file)
            .parent()
            .expect("Log file must have a parent directory"),
        std::path::Path::new(&config.client.log_file)
            .file_name()
            .expect("Log file must have a filename"),
    );

    let env_filter = EnvFilter::try_from_default_env()
        .or_else(|_| EnvFilter::try_new("info"))
        .map_err(|e| error::ConfigError::ValidationFailed(format!("Invalid log filter: {}", e)))?;

    tracing_subscriber::registry()
        .with(env_filter)
        .with(fmt::layer().with_writer(file_appender))
        .with(fmt::layer().with_writer(std::io::stdout))
        .init();

    Ok(())
}
