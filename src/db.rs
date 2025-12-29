pub mod models;
pub mod queries;

// Re-export functions for use in tests and future phases
#[allow(unused_imports)]
pub use queries::{
    create_pool, create_run, get_device, get_global_setting, get_job_by_id, get_jobs_for_device,
    get_recent_runs, get_schedules_for_device, get_settings_for_device, run_migrations,
    update_device_heartbeat, update_run, update_schedule_last_run, upsert_device,
};
