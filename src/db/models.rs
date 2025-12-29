use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};
use sqlx::FromRow;
use uuid::Uuid;

#[derive(Debug, Clone, FromRow, Serialize, Deserialize)]
pub struct Device {
    pub id: String,
    pub name: String,
    pub description: Option<String>,
    pub platform: String,
    pub hostname: Option<String>,
    pub last_seen: Option<DateTime<Utc>>,
    pub created_at: DateTime<Utc>,
    pub updated_at: DateTime<Utc>,
    pub enabled: bool,
    pub metadata: serde_json::Value,
}

#[derive(Debug, Clone, FromRow, Serialize, Deserialize)]
pub struct BackupJob {
    pub id: Uuid,
    pub device_id: String,
    pub name: String,
    pub description: Option<String>,
    pub source_paths: Vec<String>,
    pub exclude_patterns: Option<Vec<String>>,
    pub tags: Option<Vec<String>>,
    pub restic_args: serde_json::Value,
    pub enabled: bool,
    pub created_at: DateTime<Utc>,
    pub updated_at: DateTime<Utc>,
    pub metadata: serde_json::Value,
    pub origin_name: Option<String>,
    pub origin_id: Option<Uuid>,
    pub account_id: Option<Uuid>,
}

#[derive(Debug, Clone, FromRow, Serialize, Deserialize)]
pub struct Schedule {
    pub id: i32,
    pub job_id: Uuid,
    pub schedule_type: String,
    pub cron_expression: Option<String>,
    pub interval_seconds: Option<i32>,
    pub enabled: bool,
    pub last_run_at: Option<DateTime<Utc>>,
    pub next_run_at: Option<DateTime<Utc>>,
    pub created_at: DateTime<Utc>,
    pub updated_at: DateTime<Utc>,
    pub metadata: serde_json::Value,
}

#[derive(Debug, Clone, FromRow, Serialize, Deserialize)]
pub struct Run {
    pub id: i32,
    pub job_id: Uuid,
    pub device_id: String,
    pub start_time: DateTime<Utc>,
    pub end_time: Option<DateTime<Utc>>,
    pub status: String,
    pub exit_code: Option<i32>,
    pub error_message: Option<String>,
    pub files_new: Option<i32>,
    pub files_changed: Option<i32>,
    pub files_unmodified: Option<i32>,
    pub dirs_new: Option<i32>,
    pub dirs_changed: Option<i32>,
    pub dirs_unmodified: Option<i32>,
    pub data_added_bytes: Option<i64>,
    pub total_files_processed: Option<i32>,
    pub total_bytes_processed: Option<i64>,
    pub duration_seconds: Option<i32>,
    pub snapshot_id: Option<String>,
    pub restic_output: Option<String>,
    pub restic_errors: Option<String>,
    pub triggered_by: String,
    pub created_at: DateTime<Utc>,
    pub metadata: serde_json::Value,
}

#[derive(Debug, Clone, FromRow, Serialize, Deserialize)]
pub struct Setting {
    pub id: i32,
    pub device_id: Option<String>,
    pub key: String,
    pub value: String,
    pub description: Option<String>,
    pub created_at: DateTime<Utc>,
    pub updated_at: DateTime<Utc>,
}

impl BackupJob {
    #[allow(dead_code)]
    pub fn get_restic_tags(&self) -> Vec<String> {
        let mut tags = vec![format!("backup:{}", self.id)];

        tags.push(format!("backup_name={}", self.name));

        if let Some(origin_name) = &self.origin_name {
            tags.push(format!("origin={}", origin_name));
        }

        if let Some(account_id) = self.account_id {
            tags.push(format!("account_id={}", account_id));
        }

        tags
    }
}

impl Schedule {
    #[allow(dead_code)]
    pub fn is_cron(&self) -> bool {
        self.schedule_type == "cron"
    }

    #[allow(dead_code)]
    pub fn is_interval(&self) -> bool {
        self.schedule_type == "interval"
    }
}

impl Run {
    #[allow(dead_code)]
    pub fn is_running(&self) -> bool {
        self.status == "running"
    }

    #[allow(dead_code)]
    pub fn is_success(&self) -> bool {
        self.status == "success"
    }

    #[allow(dead_code)]
    pub fn is_failed(&self) -> bool {
        self.status == "failed"
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_backup_job_restic_tags() {
        let job_id = Uuid::new_v4();
        let account_id = Uuid::new_v4();

        let job = BackupJob {
            id: job_id,
            device_id: "test-device".to_string(),
            name: "device1/home".to_string(),
            description: None,
            source_paths: vec!["/home".to_string()],
            exclude_patterns: None,
            tags: None,
            restic_args: serde_json::json!([]),
            enabled: true,
            created_at: Utc::now(),
            updated_at: Utc::now(),
            metadata: serde_json::json!({}),
            origin_name: Some("device1".to_string()),
            origin_id: None,
            account_id: Some(account_id),
        };

        let tags = job.get_restic_tags();

        assert_eq!(tags.len(), 4);
        assert_eq!(tags[0], format!("backup:{}", job_id));
        assert_eq!(tags[1], "backup_name=device1/home");
        assert_eq!(tags[2], "origin=device1");
        assert_eq!(tags[3], format!("account_id={}", account_id));
    }

    #[test]
    fn test_schedule_type_checks() {
        let cron_schedule = Schedule {
            id: 1,
            job_id: Uuid::new_v4(),
            schedule_type: "cron".to_string(),
            cron_expression: Some("0 2 * * *".to_string()),
            interval_seconds: None,
            enabled: true,
            last_run_at: None,
            next_run_at: None,
            created_at: Utc::now(),
            updated_at: Utc::now(),
            metadata: serde_json::json!({}),
        };

        assert!(cron_schedule.is_cron());
        assert!(!cron_schedule.is_interval());
    }

    #[test]
    fn test_run_status_checks() {
        let run = Run {
            id: 1,
            job_id: Uuid::new_v4(),
            device_id: "test-device".to_string(),
            start_time: Utc::now(),
            end_time: None,
            status: "running".to_string(),
            exit_code: None,
            error_message: None,
            files_new: None,
            files_changed: None,
            files_unmodified: None,
            dirs_new: None,
            dirs_changed: None,
            dirs_unmodified: None,
            data_added_bytes: None,
            total_files_processed: None,
            total_bytes_processed: None,
            duration_seconds: None,
            snapshot_id: None,
            restic_output: None,
            restic_errors: None,
            triggered_by: "schedule".to_string(),
            created_at: Utc::now(),
            metadata: serde_json::json!({}),
        };

        assert!(run.is_running());
        assert!(!run.is_success());
        assert!(!run.is_failed());
    }
}
