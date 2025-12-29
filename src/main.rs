mod config;
mod db;
mod error;

use clap::Parser;
use config::{load_config_from_db, LocalConfig};
use std::path::PathBuf;
use tracing::{debug, info};
use tracing_subscriber::{fmt, prelude::*, EnvFilter};

#[derive(Parser, Debug)]
#[command(name = "rbackup2")]
#[command(about = "Multiplatform backup client using restic", long_about = None)]
struct Args {
    #[arg(short, long, value_name = "FILE")]
    config: PathBuf,
}

#[tokio::main]
async fn main() {
    let args = Args::parse();

    if let Err(e) = run(args).await {
        eprintln!("Error: {}", e);
        std::process::exit(1);
    }
}

async fn run(args: Args) -> error::Result<()> {
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

    info!("Connecting to database...");
    let database_url = config.database_url();
    let pool = db::create_pool(database_url).await?;
    debug!("Database connection established");

    info!("Running database migrations...");
    db::run_migrations(&pool).await?;
    debug!("Database migrations completed");

    info!("Registering device...");
    let platform = if cfg!(target_os = "windows") {
        "windows"
    } else if cfg!(target_os = "linux") {
        "linux"
    } else {
        "unknown"
    };

    let hostname = hostname::get().ok().and_then(|h| h.into_string().ok());

    let device = db::upsert_device(
        &pool,
        config.device.id.clone(),
        config.device.id.clone(),
        platform.to_string(),
        hostname.clone(),
    )
    .await?;
    debug!("Device registered: {} ({})", device.name, device.platform);

    info!("Loading remote configuration from database...");
    let remote_config = load_config_from_db(&pool, config.device.id.clone()).await?;
    debug!("Loaded {} backup jobs", remote_config.jobs.len());
    debug!("Loaded {} schedules", remote_config.schedules.len());
    debug!("Loaded {} settings", remote_config.settings.len());

    let repo_url = remote_config
        .repository_url()
        .filter(|url| !url.is_empty())
        .ok_or_else(|| {
            error::ConfigError::ValidationFailed(
                "Repository URL is not configured in database settings".to_string(),
            )
        })?;
    debug!("Repository URL: {}", repo_url);

    info!("========================================");
    info!("Phase 2 complete - database layer ready");
    info!("========================================");

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
