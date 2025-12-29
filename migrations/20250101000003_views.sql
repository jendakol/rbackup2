-- Create useful views for queries

-- Latest run per job
CREATE VIEW latest_runs AS
SELECT DISTINCT ON (job_id)
    id,
    job_id,
    device_id,
    start_time,
    end_time,
    status,
    error_message,
    snapshot_id,
    triggered_by
FROM runs
ORDER BY job_id, start_time DESC;

COMMENT ON VIEW latest_runs IS 'Most recent run for each backup job';

-- Job summary with schedule and last run info
CREATE VIEW job_summary AS
SELECT j.id           AS job_id,
       j.device_id,
       j.name         AS job_name,
       j.enabled      AS job_enabled,
       s.schedule_type,
       s.cron_expression,
       s.interval_seconds,
       s.enabled      AS schedule_enabled,
       s.last_run_at,
       s.next_run_at,
       lr.start_time  AS last_run_start,
       lr.status      AS last_run_status,
       lr.snapshot_id AS last_snapshot_id
FROM backup_jobs j
         LEFT JOIN schedules s ON j.id = s.job_id
         LEFT JOIN latest_runs lr ON j.id = lr.job_id;

COMMENT ON VIEW job_summary IS 'Complete overview of jobs with schedule and last run info';
