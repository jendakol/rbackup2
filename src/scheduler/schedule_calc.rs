use crate::db::models::Schedule;
use crate::error::{Result, SchedulerError};
use chrono::{DateTime, Duration, Utc};
use cron::Schedule as CronSchedule;
use std::str::FromStr;
use tracing::debug;

pub fn calculate_next_run(
    schedule: &Schedule,
    last_run: Option<DateTime<Utc>>,
    now: DateTime<Utc>,
) -> Result<DateTime<Utc>> {
    if schedule.is_cron() {
        calculate_next_cron_run(schedule, now)
    } else if schedule.is_interval() {
        calculate_next_interval_run(schedule, last_run, now)
    } else {
        Err(SchedulerError::InvalidCronExpression(format!(
            "Unknown schedule type: {}",
            schedule.schedule_type
        ))
        .into())
    }
}

fn calculate_next_cron_run(schedule: &Schedule, now: DateTime<Utc>) -> Result<DateTime<Utc>> {
    let cron_expr = schedule.cron_expression.as_ref().ok_or_else(|| {
        SchedulerError::InvalidCronExpression("Cron expression is missing".to_string())
    })?;

    let cron_expr_with_seconds = format!("0 {}", cron_expr);

    let cron_schedule = CronSchedule::from_str(&cron_expr_with_seconds).map_err(|e| {
        SchedulerError::InvalidCronExpression(format!("Failed to parse cron expression: {}", e))
    })?;

    let next = cron_schedule
        .after(&now)
        .next()
        .ok_or_else(|| SchedulerError::InvalidCronExpression("No next run time".to_string()))?;

    debug!(
        "Calculated next cron run for schedule {}: {}",
        schedule.id, next
    );

    Ok(next)
}

fn calculate_next_interval_run(
    schedule: &Schedule,
    last_run: Option<DateTime<Utc>>,
    now: DateTime<Utc>,
) -> Result<DateTime<Utc>> {
    let interval_seconds = schedule.interval_seconds.ok_or_else(|| {
        SchedulerError::InvalidInterval("Interval seconds is missing".to_string())
    })?;

    if interval_seconds <= 0 {
        return Err(SchedulerError::InvalidInterval(format!(
            "Interval must be positive, got: {}",
            interval_seconds
        ))
        .into());
    }

    let interval = Duration::seconds(interval_seconds as i64);

    let next = if let Some(last_run_time) = last_run {
        last_run_time + interval
    } else {
        now + interval
    };

    debug!(
        "Calculated next interval run for schedule {}: {} (interval: {}s)",
        schedule.id, next, interval_seconds
    );

    Ok(next)
}

pub fn is_due(schedule: &Schedule, now: DateTime<Utc>) -> bool {
    if let Some(next_run) = schedule.next_run_at {
        next_run <= now
    } else {
        true
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use chrono::{TimeZone, Timelike};

    fn create_cron_schedule(id: i32, cron_expr: &str) -> Schedule {
        Schedule {
            id,
            job_id: uuid::Uuid::new_v4(),
            schedule_type: "cron".to_string(),
            cron_expression: Some(cron_expr.to_string()),
            interval_seconds: None,
            enabled: true,
            last_run_at: None,
            next_run_at: None,
            created_at: Utc::now(),
            updated_at: Utc::now(),
            metadata: serde_json::json!({}),
        }
    }

    fn create_interval_schedule(id: i32, interval_seconds: i32) -> Schedule {
        Schedule {
            id,
            job_id: uuid::Uuid::new_v4(),
            schedule_type: "interval".to_string(),
            cron_expression: None,
            interval_seconds: Some(interval_seconds),
            enabled: true,
            last_run_at: None,
            next_run_at: None,
            created_at: Utc::now(),
            updated_at: Utc::now(),
            metadata: serde_json::json!({}),
        }
    }

    #[test]
    fn test_calculate_next_cron_run() {
        let schedule = create_cron_schedule(1, "0 2 * * *");
        let now = Utc.with_ymd_and_hms(2025, 1, 1, 0, 0, 0).unwrap();

        let result = calculate_next_run(&schedule, None, now);
        match &result {
            Ok(next) => {
                assert_eq!(next.hour(), 2);
                assert_eq!(next.minute(), 0);
            }
            Err(e) => {
                panic!("Failed to calculate next cron run: {}", e);
            }
        }
    }

    #[test]
    fn test_calculate_next_interval_run_no_last_run() {
        let schedule = create_interval_schedule(1, 3600);
        let now = Utc.with_ymd_and_hms(2025, 1, 1, 12, 0, 0).unwrap();

        let result = calculate_next_run(&schedule, None, now);
        assert!(result.is_ok());

        let next = result.unwrap();
        assert_eq!(next, now + Duration::seconds(3600));
    }

    #[test]
    fn test_calculate_next_interval_run_with_last_run() {
        let schedule = create_interval_schedule(1, 3600);
        let last_run = Utc.with_ymd_and_hms(2025, 1, 1, 10, 0, 0).unwrap();
        let now = Utc.with_ymd_and_hms(2025, 1, 1, 12, 0, 0).unwrap();

        let result = calculate_next_run(&schedule, Some(last_run), now);
        assert!(result.is_ok());

        let next = result.unwrap();
        assert_eq!(next, last_run + Duration::seconds(3600));
    }

    #[test]
    fn test_invalid_cron_expression() {
        let schedule = create_cron_schedule(1, "invalid cron");
        let now = Utc::now();

        let result = calculate_next_run(&schedule, None, now);
        assert!(result.is_err());
    }

    #[test]
    fn test_invalid_interval() {
        let schedule = create_interval_schedule(1, -100);
        let now = Utc::now();

        let result = calculate_next_run(&schedule, None, now);
        assert!(result.is_err());
    }

    #[test]
    fn test_is_due() {
        let mut schedule = create_interval_schedule(1, 3600);
        let now = Utc::now();

        schedule.next_run_at = Some(now - Duration::minutes(5));
        assert!(is_due(&schedule, now));

        schedule.next_run_at = Some(now + Duration::minutes(5));
        assert!(!is_due(&schedule, now));

        schedule.next_run_at = None;
        assert!(is_due(&schedule, now));
    }
}
