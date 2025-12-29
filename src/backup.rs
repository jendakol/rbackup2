pub mod output;
pub mod restic;

use crate::config::remote::RemoteConfig;
use crate::db;
use crate::db::models::BackupJob;
use crate::error::Result;
use chrono::Utc;
use output::{parse_restic_json_output, BackupStats};
use restic::ResticCommand;
use sqlx::PgPool;
use std::process::Output;
use tracing::{debug, error, info, warn};

async fn update_run_with_failure(
    pool: &PgPool,
    run_id: i32,
    error_msg: String,
    exit_code: Option<i32>,
    stdout: Option<String>,
    stderr: Option<String>,
) -> Result<()> {
    db::update_run(
        pool,
        run_id,
        Utc::now(),
        "failed".to_string(),
        exit_code,
        Some(error_msg),
        None,
        None,
        None,
        None,
        None,
        stdout,
        stderr,
    )
    .await?;
    Ok(())
}

async fn update_run_with_success(
    pool: &PgPool,
    run_id: i32,
    exit_code: i32,
    stats: &BackupStats,
    stdout: String,
    stderr: Option<String>,
) -> Result<()> {
    db::update_run(
        pool,
        run_id,
        Utc::now(),
        "success".to_string(),
        Some(exit_code),
        None,
        Some(stats.files_new),
        Some(stats.files_changed),
        Some(stats.files_unmodified),
        Some(stats.data_added_bytes),
        Some(stats.snapshot_id.clone()),
        Some(stdout),
        stderr,
    )
    .await?;
    Ok(())
}

async fn execute_restic_command(
    restic_cmd: &ResticCommand,
    job: &BackupJob,
    trace_id: &str,
) -> Result<Output> {
    let mut command = restic_cmd.build_backup_command(job);

    debug!(
        trace_id = trace_id,
        "Executing restic backup command for job '{}'", job.name
    );

    command.output().await.map_err(|e| {
        let error_msg = format!("Failed to execute restic: {}", e);
        error!(trace_id = trace_id, "{}", error_msg);
        crate::error::BackupError::ExecutionFailed(error_msg).into()
    })
}

fn extract_error_message(stderr: &str) -> String {
    if !stderr.is_empty() {
        stderr.to_string()
    } else {
        "Backup failed with no error message".to_string()
    }
}

pub async fn execute_backup(
    job: &BackupJob,
    config: &RemoteConfig,
    pool: &PgPool,
    trace_id: String,
) -> Result<i32> {
    info!(
        trace_id = trace_id,
        job_id = %job.id,
        job_name = %job.name,
        "Starting backup execution"
    );

    let run_id = db::create_run(pool, job.id, job.device_id.clone(), "manual".to_string()).await?;
    debug!(trace_id = trace_id, run_id = run_id, "Created run record");

    let restic_cmd = ResticCommand::new(config)?;

    let output = match execute_restic_command(&restic_cmd, job, &trace_id).await {
        Ok(output) => output,
        Err(e) => {
            let error_msg = e.to_string();
            update_run_with_failure(pool, run_id, error_msg.clone(), None, None, None).await?;
            return Err(e);
        }
    };

    let exit_code = output.status.code().unwrap_or(-1);
    let stdout = String::from_utf8_lossy(&output.stdout).to_string();
    let stderr = String::from_utf8_lossy(&output.stderr).to_string();

    debug!(
        trace_id = trace_id,
        exit_code = exit_code,
        "Backup command completed"
    );

    if !output.status.success() {
        let error_msg = extract_error_message(&stderr);
        warn!(
            trace_id = trace_id,
            exit_code = exit_code,
            "Backup failed: {}",
            error_msg
        );

        update_run_with_failure(
            pool,
            run_id,
            error_msg.clone(),
            Some(exit_code),
            Some(stdout),
            Some(stderr),
        )
        .await?;

        return Err(crate::error::BackupError::ExecutionFailed(error_msg).into());
    }

    let stats = match parse_restic_json_output(&stdout) {
        Ok(stats) => stats,
        Err(e) => {
            let error_msg = format!("Failed to parse restic output: {}", e);
            error!(trace_id = trace_id, "{}", error_msg);

            update_run_with_failure(
                pool,
                run_id,
                error_msg,
                Some(exit_code),
                Some(stdout),
                Some(stderr),
            )
            .await?;

            return Err(e);
        }
    };

    info!(
        trace_id = trace_id,
        snapshot_id = %stats.snapshot_id,
        files_new = stats.files_new,
        files_changed = stats.files_changed,
        data_added_mb = stats.data_added_bytes / 1024 / 1024,
        "Backup completed successfully"
    );

    let stderr_opt = if !stderr.is_empty() {
        Some(stderr)
    } else {
        None
    };

    update_run_with_success(pool, run_id, exit_code, &stats, stdout, stderr_opt).await?;

    Ok(run_id)
}
