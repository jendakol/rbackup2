use crate::backup;
use crate::config::remote::RemoteConfig;
use crate::db;
use crate::error::Result;
use sqlx::PgPool;
use std::collections::HashMap;
use std::sync::Arc;
use tokio::sync::mpsc;
use tokio::sync::Mutex;
use tracing::{debug, error, info, warn};
use uuid::Uuid;

#[derive(Debug, Clone)]
pub struct JobExecution {
    pub job_id: Uuid,
    pub triggered_by: String,
}

pub struct JobExecutor {
    pool: Arc<PgPool>,
    config: Arc<Mutex<RemoteConfig>>,
    running_jobs: Arc<Mutex<HashMap<String, Uuid>>>,
    max_concurrent_per_device: usize,
}

impl JobExecutor {
    pub fn new(
        pool: Arc<PgPool>,
        config: Arc<Mutex<RemoteConfig>>,
        max_concurrent_per_device: usize,
    ) -> Self {
        Self {
            pool,
            config,
            running_jobs: Arc::new(Mutex::new(HashMap::new())),
            max_concurrent_per_device,
        }
    }

    pub async fn start(self: Arc<Self>, mut job_queue: mpsc::Receiver<JobExecution>) -> Result<()> {
        info!("Job executor started");

        while let Some(execution) = job_queue.recv().await {
            let executor = self.clone();
            tokio::spawn(async move {
                if let Err(e) = executor.execute_job(execution).await {
                    error!("Job execution failed: {}", e);
                }
            });
        }

        info!("Job executor stopped");
        Ok(())
    }

    async fn execute_job(&self, execution: JobExecution) -> Result<()> {
        let job = match db::get_job_by_id(&self.pool, execution.job_id).await? {
            Some(job) => job,
            None => {
                warn!(job_id = %execution.job_id, "Job not found, skipping execution");
                return Ok(());
            }
        };

        if !self.can_execute(&job.device_id).await {
            warn!(
                job_id = %execution.job_id,
                device_id = %job.device_id,
                "Device has reached max concurrent backups, skipping"
            );
            return Ok(());
        }

        self.mark_running(&job.device_id, execution.job_id).await;

        let trace_id = Uuid::new_v4().to_string();
        let config = self.config.lock().await.clone();

        info!(
            trace_id = trace_id,
            job_id = %execution.job_id,
            job_name = %job.name,
            triggered_by = %execution.triggered_by,
            "Executing scheduled backup"
        );

        let result = backup::execute_backup(&job, &config, &self.pool, trace_id.clone()).await;

        self.mark_completed(&job.device_id).await;

        match result {
            Ok(run_id) => {
                info!(
                    trace_id = trace_id,
                    job_id = %execution.job_id,
                    run_id = run_id,
                    "Backup completed successfully"
                );
            }
            Err(e) => {
                error!(
                    trace_id = trace_id,
                    job_id = %execution.job_id,
                    "Backup failed: {}",
                    e
                );
            }
        }

        Ok(())
    }

    async fn can_execute(&self, device_id: &str) -> bool {
        let running = self.running_jobs.lock().await;
        let count = running
            .iter()
            .filter(|(dev_id, _)| dev_id.as_str() == device_id)
            .count();

        debug!(
            device_id = device_id,
            running = count,
            max = self.max_concurrent_per_device,
            "Checking if device can execute backup"
        );

        count < self.max_concurrent_per_device
    }

    async fn mark_running(&self, device_id: &str, job_id: Uuid) {
        let mut running = self.running_jobs.lock().await;
        running.insert(device_id.to_string(), job_id);
        debug!(
            device_id = device_id,
            job_id = %job_id,
            total_running = running.len(),
            "Marked job as running"
        );
    }

    async fn mark_completed(&self, device_id: &str) {
        let mut running = self.running_jobs.lock().await;
        running.remove(device_id);
        debug!(
            device_id = device_id,
            total_running = running.len(),
            "Marked job as completed"
        );
    }
}
