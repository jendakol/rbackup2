mod backup;
mod config;
mod db;
mod error;
mod scheduler;

use clap::Parser;
use config::{load_config_from_db, LocalConfig};
use scheduler::executor::JobExecutor;
use scheduler::Scheduler;
use std::path::PathBuf;
use std::sync::Arc;
use tokio::sync::Mutex;
use tracing::{debug, error, info};
use tracing_subscriber::{fmt, prelude::*, EnvFilter};
use uuid::Uuid;

#[derive(Parser, Debug)]
#[command(name = "rbackup2")]
#[command(about = "Multiplatform backup client using restic", long_about = None)]
struct Args {
    #[arg(short, long, value_name = "FILE")]
    config: PathBuf,

    #[arg(long, value_name = "JOB_ID")]
    test_backup: Option<Uuid>,
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

    if let Some(job_id) = args.test_backup {
        info!("========================================");
        info!("Test Backup Mode");
        info!("========================================");

        let job = db::get_job_by_id(&pool, job_id).await?.ok_or_else(|| {
            error::AppError::Backup(error::BackupError::ConfigurationError(format!(
                "Job with ID {} not found",
                job_id
            )))
        })?;

        info!("Job: {} ({})", job.name, job.id);
        info!("Source paths: {:?}", job.source_paths);

        let trace_id = uuid::Uuid::new_v4().to_string();

        match backup::execute_backup(&job, &remote_config, &pool, trace_id).await {
            Ok(run_id) => {
                info!("Backup completed successfully");
                info!("Run ID: {}", run_id);

                let run = db::get_recent_runs(&pool, job.device_id, 1)
                    .await?
                    .into_iter()
                    .next()
                    .ok_or_else(|| error::DatabaseError::QueryFailed(sqlx::Error::RowNotFound))?;

                info!("========================================");
                info!("Backup Results");
                info!("========================================");
                info!("Status: {}", run.status);
                info!("Duration: {} seconds", run.duration_seconds.unwrap_or(0));
                info!("Files new: {}", run.files_new.unwrap_or(0));
                info!("Files changed: {}", run.files_changed.unwrap_or(0));
                info!("Files unmodified: {}", run.files_unmodified.unwrap_or(0));
                info!(
                    "Data added: {} MB",
                    run.data_added_bytes.unwrap_or(0) / 1024 / 1024
                );
                info!("Snapshot ID: {}", run.snapshot_id.unwrap_or_default());
                info!("========================================");

                return Ok(());
            }
            Err(e) => {
                eprintln!("Backup failed: {}", e);
                return Err(e);
            }
        }
    }

    info!("========================================");
    info!("Starting scheduler and job executor");
    info!("========================================");

    let pool_arc = Arc::new(pool);
    let config_arc = Arc::new(Mutex::new(remote_config));

    let max_concurrent = config_arc
        .lock()
        .await
        .get_setting("max_concurrent_backups")
        .and_then(|s| s.parse().ok())
        .unwrap_or(1);

    let (scheduler, job_queue_rx) = Scheduler::new(
        pool_arc.clone(),
        config_arc.clone(),
        config.device.id.clone(),
    );
    let scheduler_arc = Arc::new(scheduler);

    let executor = Arc::new(JobExecutor::new(pool_arc, config_arc, max_concurrent));

    let scheduler_handle = {
        let scheduler = scheduler_arc.clone();
        tokio::spawn(async move {
            if let Err(e) = scheduler.start().await {
                error!("Scheduler error: {}", e);
            }
        })
    };

    let executor_handle = tokio::spawn(async move {
        if let Err(e) = executor.start(job_queue_rx).await {
            error!("Executor error: {}", e);
        }
    });

    info!("========================================");
    info!("Phase 4 complete - scheduler running");
    info!("========================================");

    tokio::select! {
        _ = scheduler_handle => {
            info!("Scheduler task completed");
        }
        _ = executor_handle => {
            info!("Executor task completed");
        }
        _ = tokio::signal::ctrl_c() => {
            info!("Received shutdown signal");
        }
    }

    info!("Shutting down...");
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
