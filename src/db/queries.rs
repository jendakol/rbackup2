use crate::db::models::{BackupJob, Device, Run, Schedule, Setting};
use crate::error::{DatabaseError, Result};
use sqlx::postgres::{PgConnectOptions, PgPoolOptions};
use sqlx::{ConnectOptions, PgPool};
use std::time::Duration;
use tracing::log::LevelFilter;
use uuid::Uuid;

pub async fn create_pool(connection_string: String) -> Result<PgPool> {
    let mut connect_options: PgConnectOptions = connection_string
        .parse()
        .map_err(|e| DatabaseError::ConnectionFailed(sqlx::Error::Configuration(Box::new(e))))?;

    connect_options = connect_options.log_statements(LevelFilter::Debug);

    let pool = PgPoolOptions::new()
        .max_connections(5)
        .acquire_timeout(Duration::from_secs(10))
        .connect_with(connect_options)
        .await
        .map_err(DatabaseError::ConnectionFailed)?;

    Ok(pool)
}

pub async fn run_migrations(pool: &PgPool) -> Result<()> {
    sqlx::migrate!("./migrations")
        .run(pool)
        .await
        .map_err(|e| DatabaseError::MigrationFailed(sqlx::Error::Migrate(Box::new(e))))?;
    Ok(())
}

#[allow(dead_code)]
pub async fn get_device(pool: &PgPool, device_id: String) -> Result<Option<Device>> {
    let device = sqlx::query_as::<_, Device>("SELECT * FROM devices WHERE id = $1")
        .bind(device_id)
        .fetch_optional(pool)
        .await?;
    Ok(device)
}

pub async fn upsert_device(
    pool: &PgPool,
    device_id: String,
    name: String,
    platform: String,
    hostname: Option<String>,
) -> Result<Device> {
    let device = sqlx::query_as::<_, Device>(
        r#"
        INSERT INTO devices (id, name, platform, hostname, last_seen, enabled)
        VALUES ($1, $2, $3, $4, NOW(), true)
        ON CONFLICT (id) DO UPDATE
        SET name = EXCLUDED.name,
            platform = EXCLUDED.platform,
            hostname = EXCLUDED.hostname,
            last_seen = NOW(),
            updated_at = NOW()
        RETURNING *
        "#,
    )
    .bind(device_id)
    .bind(name)
    .bind(platform)
    .bind(hostname)
    .fetch_one(pool)
    .await?;
    Ok(device)
}

#[allow(dead_code)]
pub async fn update_device_heartbeat(
    pool: &PgPool,
    device_id: String,
    hostname: Option<String>,
    metadata: serde_json::Value,
) -> Result<()> {
    sqlx::query(
        r#"
        UPDATE devices
        SET last_seen = NOW(),
            hostname = $2,
            metadata = $3,
            updated_at = NOW()
        WHERE id = $1
        "#,
    )
    .bind(device_id)
    .bind(hostname)
    .bind(metadata)
    .execute(pool)
    .await?;
    Ok(())
}

pub async fn get_jobs_for_device(pool: &PgPool, device_id: String) -> Result<Vec<BackupJob>> {
    let jobs = sqlx::query_as::<_, BackupJob>(
        "SELECT * FROM backup_jobs WHERE device_id = $1 AND enabled = true",
    )
    .bind(device_id)
    .fetch_all(pool)
    .await?;
    Ok(jobs)
}

#[allow(dead_code)]
pub async fn get_job_by_id(pool: &PgPool, job_id: Uuid) -> Result<Option<BackupJob>> {
    let job = sqlx::query_as::<_, BackupJob>("SELECT * FROM backup_jobs WHERE id = $1")
        .bind(job_id)
        .fetch_optional(pool)
        .await?;
    Ok(job)
}

pub async fn get_schedules_for_device(pool: &PgPool, device_id: String) -> Result<Vec<Schedule>> {
    let schedules = sqlx::query_as::<_, Schedule>(
        r#"
        SELECT s.*
        FROM schedules s
        JOIN backup_jobs j ON s.job_id = j.id
        WHERE j.device_id = $1
          AND j.enabled = true
          AND s.enabled = true
        "#,
    )
    .bind(device_id)
    .fetch_all(pool)
    .await?;
    Ok(schedules)
}

#[allow(dead_code)]
pub async fn update_schedule_last_run(
    pool: &PgPool,
    job_id: Uuid,
    last_run_at: chrono::DateTime<chrono::Utc>,
    next_run_at: Option<chrono::DateTime<chrono::Utc>>,
) -> Result<()> {
    sqlx::query(
        r#"
        UPDATE schedules
        SET last_run_at = $2,
            next_run_at = $3,
            updated_at = NOW()
        WHERE job_id = $1
        "#,
    )
    .bind(job_id)
    .bind(last_run_at)
    .bind(next_run_at)
    .execute(pool)
    .await?;
    Ok(())
}

#[allow(dead_code)]
pub async fn create_run(
    pool: &PgPool,
    job_id: Uuid,
    device_id: String,
    triggered_by: String,
) -> Result<i32> {
    let run_id: (i32,) = sqlx::query_as(
        r#"
        INSERT INTO runs (job_id, device_id, start_time, status, triggered_by)
        VALUES ($1, $2, NOW(), 'running', $3)
        RETURNING id
        "#,
    )
    .bind(job_id)
    .bind(device_id)
    .bind(triggered_by)
    .fetch_one(pool)
    .await?;
    Ok(run_id.0)
}

// Allow many arguments: this function mirrors the database schema columns for run updates
#[allow(clippy::too_many_arguments)]
#[allow(dead_code)]
pub async fn update_run(
    pool: &PgPool,
    run_id: i32,
    end_time: chrono::DateTime<chrono::Utc>,
    status: String,
    exit_code: Option<i32>,
    error_message: Option<String>,
    files_new: Option<i32>,
    files_changed: Option<i32>,
    files_unmodified: Option<i32>,
    data_added_bytes: Option<i64>,
    snapshot_id: Option<String>,
    restic_output: Option<String>,
    restic_errors: Option<String>,
) -> Result<()> {
    sqlx::query(
        r#"
        UPDATE runs
        SET end_time = $2,
            status = $3,
            exit_code = $4,
            error_message = $5,
            files_new = $6,
            files_changed = $7,
            files_unmodified = $8,
            data_added_bytes = $9,
            snapshot_id = $10,
            restic_output = $11,
            restic_errors = $12,
            duration_seconds = EXTRACT(EPOCH FROM ($2 - start_time))::INTEGER
        WHERE id = $1
        "#,
    )
    .bind(run_id)
    .bind(end_time)
    .bind(status)
    .bind(exit_code)
    .bind(error_message)
    .bind(files_new)
    .bind(files_changed)
    .bind(files_unmodified)
    .bind(data_added_bytes)
    .bind(snapshot_id)
    .bind(restic_output)
    .bind(restic_errors)
    .execute(pool)
    .await?;
    Ok(())
}

#[allow(dead_code)]
pub async fn get_recent_runs(pool: &PgPool, device_id: String, limit: i64) -> Result<Vec<Run>> {
    let runs = sqlx::query_as::<_, Run>(
        r#"
        SELECT * FROM runs
        WHERE device_id = $1
        ORDER BY start_time DESC
        LIMIT $2
        "#,
    )
    .bind(device_id)
    .bind(limit)
    .fetch_all(pool)
    .await?;
    Ok(runs)
}

pub async fn get_settings_for_device(pool: &PgPool, device_id: String) -> Result<Vec<Setting>> {
    let settings = sqlx::query_as::<_, Setting>(
        r#"
        SELECT * FROM settings
        WHERE device_id IS NULL OR device_id = $1
        ORDER BY device_id NULLS FIRST
        "#,
    )
    .bind(device_id)
    .fetch_all(pool)
    .await?;
    Ok(settings)
}

#[allow(dead_code)]
pub async fn get_global_setting(pool: &PgPool, key: String) -> Result<Option<String>> {
    let setting: Option<(String,)> = sqlx::query_as(
        r#"
        SELECT value FROM settings
        WHERE device_id IS NULL AND key = $1
        "#,
    )
    .bind(key)
    .fetch_optional(pool)
    .await?;
    Ok(setting.map(|s| s.0))
}
