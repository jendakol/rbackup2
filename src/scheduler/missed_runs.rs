use crate::db::models::Schedule;
use chrono::{DateTime, Duration, Utc};
use tracing::warn;

#[allow(dead_code)]
const DEFAULT_GRACE_PERIOD_MINUTES: i64 = 5;

#[allow(dead_code)]
pub fn is_run_missed(
    schedule: &Schedule,
    now: DateTime<Utc>,
    grace_period_minutes: Option<i64>,
) -> bool {
    let grace_period = grace_period_minutes.unwrap_or(DEFAULT_GRACE_PERIOD_MINUTES);

    if let Some(next_run) = schedule.next_run_at {
        if next_run + Duration::minutes(grace_period) < now {
            warn!(
                schedule_id = schedule.id,
                job_id = %schedule.job_id,
                next_run = %next_run,
                "Schedule missed its run window"
            );
            return true;
        }
    }

    false
}

#[allow(dead_code)]
pub fn count_missed_interval_runs(
    schedule: &Schedule,
    last_run: Option<DateTime<Utc>>,
    now: DateTime<Utc>,
) -> usize {
    if !schedule.is_interval() {
        return 0;
    }

    let interval_seconds = match schedule.interval_seconds {
        Some(s) if s > 0 => s as i64,
        _ => return 0,
    };

    let last_run_time = match last_run.or(schedule.last_run_at) {
        Some(lr) => lr,
        None => return 0,
    };

    let elapsed = now.signed_duration_since(last_run_time);
    let interval_duration = Duration::seconds(interval_seconds);

    if elapsed < interval_duration {
        return 0;
    }

    let missed = (elapsed.num_seconds() / interval_seconds) as usize;

    if missed > 1 {
        warn!(
            schedule_id = schedule.id,
            job_id = %schedule.job_id,
            missed_runs = missed,
            "Multiple interval runs were missed"
        );
    }

    missed.saturating_sub(1)
}

#[cfg(test)]
mod tests {
    use super::*;

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

    fn create_cron_schedule(id: i32) -> Schedule {
        Schedule {
            id,
            job_id: uuid::Uuid::new_v4(),
            schedule_type: "cron".to_string(),
            cron_expression: Some("0 2 * * *".to_string()),
            interval_seconds: None,
            enabled: true,
            last_run_at: None,
            next_run_at: None,
            created_at: Utc::now(),
            updated_at: Utc::now(),
            metadata: serde_json::json!({}),
        }
    }

    #[test]
    fn test_is_run_missed_within_grace_period() {
        let now = Utc::now();
        let mut schedule = create_interval_schedule(1, 3600);
        schedule.next_run_at = Some(now - Duration::minutes(3));

        assert!(!is_run_missed(&schedule, now, Some(5)));
    }

    #[test]
    fn test_is_run_missed_outside_grace_period() {
        let now = Utc::now();
        let mut schedule = create_interval_schedule(1, 3600);
        schedule.next_run_at = Some(now - Duration::minutes(10));

        assert!(is_run_missed(&schedule, now, Some(5)));
    }

    #[test]
    fn test_is_run_missed_no_next_run() {
        let now = Utc::now();
        let schedule = create_interval_schedule(1, 3600);

        assert!(!is_run_missed(&schedule, now, Some(5)));
    }

    #[test]
    fn test_count_missed_interval_runs_none() {
        let now = Utc::now();
        let schedule = create_interval_schedule(1, 3600);
        let last_run = Some(now - Duration::minutes(30));

        assert_eq!(count_missed_interval_runs(&schedule, last_run, now), 0);
    }

    #[test]
    fn test_count_missed_interval_runs_one() {
        let now = Utc::now();
        let schedule = create_interval_schedule(1, 3600);
        let last_run = Some(now - Duration::hours(2));

        assert_eq!(count_missed_interval_runs(&schedule, last_run, now), 1);
    }

    #[test]
    fn test_count_missed_interval_runs_multiple() {
        let now = Utc::now();
        let schedule = create_interval_schedule(1, 3600);
        let last_run = Some(now - Duration::hours(5));

        assert_eq!(count_missed_interval_runs(&schedule, last_run, now), 4);
    }

    #[test]
    fn test_count_missed_interval_runs_cron_schedule() {
        let now = Utc::now();
        let schedule = create_cron_schedule(1);
        let last_run = Some(now - Duration::hours(5));

        assert_eq!(count_missed_interval_runs(&schedule, last_run, now), 0);
    }

    #[test]
    fn test_count_missed_interval_runs_no_last_run() {
        let now = Utc::now();
        let schedule = create_interval_schedule(1, 3600);

        assert_eq!(count_missed_interval_runs(&schedule, None, now), 0);
    }
}
