pub mod executor;
pub mod missed_runs;
pub mod schedule_calc;

use crate::config::remote::RemoteConfig;
use crate::db;
use crate::db::models::Schedule;
use crate::error::Result;
use chrono::Utc;
use executor::JobExecution;
use schedule_calc::{calculate_next_run, is_due};
use sqlx::PgPool;
use std::collections::HashMap;
use std::sync::Arc;
use tokio::sync::{mpsc, Mutex};
use tokio::time::{interval, Duration};
use tracing::{debug, error, info, warn};
use uuid::Uuid;

const SCHEDULER_CHECK_INTERVAL_SECONDS: u64 = 60;

pub struct Scheduler {
    pool: Arc<PgPool>,
    #[allow(dead_code)]
    config: Arc<Mutex<RemoteConfig>>,
    device_id: String,
    schedules: Arc<Mutex<HashMap<i32, Schedule>>>,
    job_queue_tx: mpsc::Sender<JobExecution>,
}

impl Scheduler {
    pub fn new(
        pool: Arc<PgPool>,
        config: Arc<Mutex<RemoteConfig>>,
        device_id: String,
    ) -> (Self, mpsc::Receiver<JobExecution>) {
        let (tx, rx) = mpsc::channel(100);

        let scheduler = Self {
            pool,
            config,
            device_id,
            schedules: Arc::new(Mutex::new(HashMap::new())),
            job_queue_tx: tx,
        };

        (scheduler, rx)
    }

    pub async fn start(self: Arc<Self>) -> Result<()> {
        info!("Scheduler started");

        self.reload_schedules().await?;

        let mut check_interval = interval(Duration::from_secs(SCHEDULER_CHECK_INTERVAL_SECONDS));

        loop {
            check_interval.tick().await;

            if let Err(e) = self.check_schedules().await {
                error!("Error checking schedules: {}", e);
            }
        }
    }

    pub async fn reload_schedules(&self) -> Result<()> {
        info!("Reloading schedules from database");

        let db_schedules = db::get_schedules_for_device(&self.pool, self.device_id.clone()).await?;

        let mut schedules = self.schedules.lock().await;
        schedules.clear();

        for mut schedule in db_schedules {
            let now = Utc::now();

            if schedule.next_run_at.is_none() {
                let next_run = calculate_next_run(&schedule, schedule.last_run_at, now)?;
                schedule.next_run_at = Some(next_run);

                if let Err(e) = db::update_schedule_last_run(
                    &self.pool,
                    schedule.job_id,
                    schedule.last_run_at.unwrap_or(now),
                    Some(next_run),
                )
                .await
                {
                    warn!(
                        schedule_id = schedule.id,
                        "Failed to update schedule next_run_at: {}", e
                    );
                }
            }

            debug!(
                schedule_id = schedule.id,
                job_id = %schedule.job_id,
                schedule_type = %schedule.schedule_type,
                next_run = ?schedule.next_run_at,
                "Loaded schedule"
            );

            schedules.insert(schedule.id, schedule);
        }

        info!("Loaded {} schedules", schedules.len());

        Ok(())
    }

    async fn check_schedules(&self) -> Result<()> {
        let now = Utc::now();
        let schedules = self.schedules.lock().await.clone();

        debug!("Checking {} schedules", schedules.len());

        for schedule in schedules.values() {
            if !schedule.enabled {
                continue;
            }

            if is_due(schedule, now) {
                info!(
                    schedule_id = schedule.id,
                    job_id = %schedule.job_id,
                    "Schedule is due, queueing job"
                );

                if let Err(e) = self.queue_job(schedule).await {
                    error!(
                        schedule_id = schedule.id,
                        job_id = %schedule.job_id,
                        "Failed to queue job: {}",
                        e
                    );
                }
            }
        }

        Ok(())
    }

    async fn queue_job(&self, schedule: &Schedule) -> Result<()> {
        let execution = JobExecution {
            job_id: schedule.job_id,
            triggered_by: "schedule".to_string(),
        };

        self.job_queue_tx
            .send(execution)
            .await
            .map_err(|e| crate::error::SchedulerError::JobNotFound(e.to_string()))?;

        let now = Utc::now();
        let next_run = calculate_next_run(schedule, Some(now), now)?;

        db::update_schedule_last_run(&self.pool, schedule.job_id, now, Some(next_run)).await?;

        let mut schedules = self.schedules.lock().await;
        if let Some(s) = schedules.get_mut(&schedule.id) {
            s.last_run_at = Some(now);
            s.next_run_at = Some(next_run);
        }

        debug!(
            schedule_id = schedule.id,
            job_id = %schedule.job_id,
            next_run = %next_run,
            "Updated schedule after queueing"
        );

        Ok(())
    }

    #[allow(dead_code)]
    pub async fn trigger_manual_backup(&self, job_id: Uuid) -> Result<()> {
        info!(job_id = %job_id, "Triggering manual backup");

        let execution = JobExecution {
            job_id,
            triggered_by: "manual".to_string(),
        };

        self.job_queue_tx
            .send(execution)
            .await
            .map_err(|e| crate::error::SchedulerError::JobNotFound(e.to_string()))?;

        Ok(())
    }
}
