-- Initial database schema for rbackup2
-- Compatible with Relica Backup repositories

-- Enable UUID extension
CREATE EXTENSION IF NOT EXISTS "pgcrypto";

-- Devices table
CREATE TABLE devices
(
    id          VARCHAR(255) PRIMARY KEY,
    name        VARCHAR(255)             NOT NULL,
    description TEXT,
    platform    VARCHAR(50)              NOT NULL,
    hostname    VARCHAR(255),
    last_seen   TIMESTAMP WITH TIME ZONE,
    created_at  TIMESTAMP WITH TIME ZONE NOT NULL DEFAULT NOW(),
    updated_at  TIMESTAMP WITH TIME ZONE NOT NULL DEFAULT NOW(),
    enabled     BOOLEAN                  NOT NULL DEFAULT true,
    metadata    JSONB                             DEFAULT '{}'::jsonb
);

CREATE INDEX idx_devices_enabled ON devices (enabled);
CREATE INDEX idx_devices_last_seen ON devices (last_seen);

COMMENT ON TABLE devices IS 'Registered backup client devices';
COMMENT ON COLUMN devices.id IS 'Unique device identifier (from local config)';
COMMENT ON COLUMN devices.platform IS 'Operating system platform';
COMMENT ON COLUMN devices.last_seen IS 'Last heartbeat/sync timestamp';
COMMENT ON COLUMN devices.metadata IS 'Extensible device metadata (version, etc.)';

-- Backup Jobs table
CREATE TABLE backup_jobs
(
    id               UUID PRIMARY KEY DEFAULT gen_random_uuid(),
    device_id        VARCHAR(255)             NOT NULL REFERENCES devices (id) ON DELETE CASCADE,
    name             VARCHAR(255)             NOT NULL,
    description      TEXT,
    source_paths     TEXT[]                   NOT NULL,
    exclude_patterns TEXT[],
    tags             TEXT[],
    restic_args      JSONB                             DEFAULT '[]'::jsonb,
    enabled          BOOLEAN                  NOT NULL DEFAULT true,
    created_at       TIMESTAMP WITH TIME ZONE NOT NULL DEFAULT NOW(),
    updated_at       TIMESTAMP WITH TIME ZONE NOT NULL DEFAULT NOW(),
    metadata         JSONB                             DEFAULT '{}'::jsonb,
    origin_name      VARCHAR(255),
    origin_id        UUID,
    account_id       UUID,
    UNIQUE (device_id, name)
);

CREATE INDEX idx_backup_jobs_device ON backup_jobs (device_id);
CREATE INDEX idx_backup_jobs_enabled ON backup_jobs (device_id, enabled);
CREATE INDEX idx_backup_jobs_origin ON backup_jobs (origin_id);
CREATE INDEX idx_backup_jobs_account ON backup_jobs (account_id);

COMMENT ON TABLE backup_jobs IS 'Backup job definitions per device';
COMMENT ON COLUMN backup_jobs.id IS 'UUID (for Relica compatibility)';
COMMENT ON COLUMN backup_jobs.name IS 'Backup name (may include origin prefix like "blade/home")';
COMMENT ON COLUMN backup_jobs.source_paths IS 'Directories/files to back up';
COMMENT ON COLUMN backup_jobs.exclude_patterns IS 'Patterns to exclude (restic --exclude)';
COMMENT ON COLUMN backup_jobs.tags IS 'Custom tags (deprecated - standard tags now auto-generated)';
COMMENT ON COLUMN backup_jobs.restic_args IS 'Additional restic CLI arguments as JSON array';
COMMENT ON COLUMN backup_jobs.origin_name IS 'Origin/device name for Relica-style naming';
COMMENT ON COLUMN backup_jobs.origin_id IS 'Origin UUID for Relica compatibility (not used for paths in single-repo model)';
COMMENT ON COLUMN backup_jobs.account_id IS 'Account UUID for multi-tenancy';

-- Schedules table
CREATE TABLE schedules
(
    id               SERIAL PRIMARY KEY,
    job_id           UUID                     NOT NULL REFERENCES backup_jobs (id) ON DELETE CASCADE,
    schedule_type    VARCHAR(50)              NOT NULL,
    cron_expression  VARCHAR(255),
    interval_seconds INTEGER,
    enabled          BOOLEAN                  NOT NULL DEFAULT true,
    last_run_at      TIMESTAMP WITH TIME ZONE,
    next_run_at      TIMESTAMP WITH TIME ZONE,
    created_at       TIMESTAMP WITH TIME ZONE NOT NULL DEFAULT NOW(),
    updated_at       TIMESTAMP WITH TIME ZONE NOT NULL DEFAULT NOW(),
    metadata         JSONB                             DEFAULT '{}'::jsonb,
    CONSTRAINT check_schedule_type CHECK (schedule_type IN ('cron', 'interval')),
    CONSTRAINT check_cron_expression CHECK (
        (schedule_type = 'cron' AND cron_expression IS NOT NULL) OR
        (schedule_type = 'interval' AND interval_seconds IS NOT NULL)
        )
);

CREATE INDEX idx_schedules_enabled ON schedules (enabled);
CREATE INDEX idx_schedules_next_run ON schedules (next_run_at) WHERE enabled = true;
CREATE INDEX idx_schedules_job ON schedules (job_id);

COMMENT ON TABLE schedules IS 'Execution schedules for backup jobs (multiple allowed per job)';
COMMENT ON COLUMN schedules.job_id IS 'References backup_jobs(id) - UUID type';
COMMENT ON COLUMN schedules.schedule_type IS 'Type of schedule: cron expression or fixed interval';
COMMENT ON COLUMN schedules.cron_expression IS 'Cron expression (e.g., "0 2 * * *" for daily at 2 AM)';
COMMENT ON COLUMN schedules.interval_seconds IS 'Fixed interval in seconds (e.g., 3600 for hourly)';
COMMENT ON COLUMN schedules.last_run_at IS 'Timestamp of last execution';
COMMENT ON COLUMN schedules.next_run_at IS 'Calculated next execution time';

-- Runs table
CREATE TABLE runs
(
    id                    SERIAL PRIMARY KEY,
    job_id                UUID                     NOT NULL REFERENCES backup_jobs (id) ON DELETE CASCADE,
    device_id             VARCHAR(255)             NOT NULL REFERENCES devices (id) ON DELETE CASCADE,
    start_time            TIMESTAMP WITH TIME ZONE NOT NULL,
    end_time              TIMESTAMP WITH TIME ZONE,
    status                VARCHAR(50)              NOT NULL,
    exit_code             INTEGER,
    error_message         TEXT,
    files_new             INTEGER,
    files_changed         INTEGER,
    files_unmodified      INTEGER,
    dirs_new              INTEGER,
    dirs_changed          INTEGER,
    dirs_unmodified       INTEGER,
    data_added_bytes      BIGINT,
    total_files_processed INTEGER,
    total_bytes_processed BIGINT,
    duration_seconds      INTEGER,
    snapshot_id           VARCHAR(255),
    restic_output         TEXT,
    restic_errors         TEXT,
    triggered_by          VARCHAR(50),
    created_at            TIMESTAMP WITH TIME ZONE NOT NULL DEFAULT NOW(),
    metadata              JSONB                             DEFAULT '{}'::jsonb,
    CONSTRAINT check_status CHECK (status IN ('running', 'success', 'failed', 'cancelled'))
);

CREATE INDEX idx_runs_job ON runs (job_id);
CREATE INDEX idx_runs_device ON runs (device_id);
CREATE INDEX idx_runs_start_time ON runs (start_time DESC);
CREATE INDEX idx_runs_status ON runs (status);
CREATE INDEX idx_runs_job_status ON runs (job_id, status);
CREATE INDEX idx_runs_device_start ON runs (device_id, start_time DESC);

COMMENT ON TABLE runs IS 'Execution history of all backup runs';
COMMENT ON COLUMN runs.job_id IS 'References backup_jobs(id) - UUID type';
COMMENT ON COLUMN runs.status IS 'Current status of the backup run';
COMMENT ON COLUMN runs.exit_code IS 'restic process exit code';
COMMENT ON COLUMN runs.error_message IS 'Human-readable error message';
COMMENT ON COLUMN runs.files_new IS 'Number of new files backed up';
COMMENT ON COLUMN runs.data_added_bytes IS 'Bytes of data added in this backup';
COMMENT ON COLUMN runs.snapshot_id IS 'restic snapshot identifier';
COMMENT ON COLUMN runs.triggered_by IS 'How the backup was initiated';

-- Settings table
CREATE TABLE settings
(
    id          SERIAL PRIMARY KEY,
    device_id   VARCHAR(255) REFERENCES devices (id) ON DELETE CASCADE,
    key         VARCHAR(255)             NOT NULL,
    value       TEXT                     NOT NULL,
    description TEXT,
    created_at  TIMESTAMP WITH TIME ZONE NOT NULL DEFAULT NOW(),
    updated_at  TIMESTAMP WITH TIME ZONE NOT NULL DEFAULT NOW(),
    UNIQUE (device_id, key)
);

CREATE INDEX idx_settings_device ON settings (device_id);
CREATE INDEX idx_settings_key ON settings (key);

COMMENT ON TABLE settings IS 'Configuration settings (global and per-device)';
COMMENT ON COLUMN settings.device_id IS 'NULL for global settings, device_id for device-specific';
COMMENT ON COLUMN settings.key IS 'Setting key (e.g., "sync_interval_seconds")';
COMMENT ON COLUMN settings.value IS 'Setting value (as string, parsed by client)';
