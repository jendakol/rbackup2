# Database Schema

## Overview

The PostgreSQL database serves as the **single source of truth** for all configuration, state, and execution history
across all client devices.

**Relica Compatibility**: This schema has been designed to support migration from Relica Backup, including:
- UUID-based backup job IDs (preserves existing job identifiers)
- Origin/account fields for multi-tenancy and repository path construction
- Multiple schedules per job (for Relica's times_of_day arrays)
- Repository base URLs for per-job repository paths

See `doc/03-relica-compatibility.md` for detailed migration requirements.

## Schema Design Principles

1. **Device-centric**: All configuration is scoped to specific devices
2. **Temporal tracking**: Timestamps for creation, updates, and last execution
3. **Audit trail**: Complete history of all backup runs
4. **Flexibility**: JSON columns for extensible metadata
5. **Normalization**: Separate tables for logical entities (devices, jobs, schedules)
6. **Single shared repository**: All devices backup to one restic repository
7. **Relica migration support**: UUID job IDs, origin/account fields for tagging

## Entity Relationship Diagram

```
┌─────────────┐
│   devices   │
└──────┬──────┘
       │
       │ 1:N
       ▼
┌─────────────┐
│backup_jobs  │
└──────┬──────┘
       │
       │ 1:N
       ▼
┌─────────────┐
│ schedules   │
└─────────────┘
       │
       │ 1:N (reference)
       ▼
┌─────────────┐
│    runs     │
└─────────────┘
       
┌─────────────┐
│  settings   │  (contains global repository config)
└─────────────┘
```

## Tables

### 1. devices

Represents registered client devices.

```sql
CREATE TABLE devices
(
    id          VARCHAR(255) PRIMARY KEY,
    name        VARCHAR(255)             NOT NULL,
    description TEXT,
    platform    VARCHAR(50)              NOT NULL, -- 'linux', 'windows'
    hostname    VARCHAR(255),
    last_seen   TIMESTAMP WITH TIME ZONE,
    created_at  TIMESTAMP WITH TIME ZONE NOT NULL DEFAULT NOW(),
    updated_at  TIMESTAMP WITH TIME ZONE NOT NULL DEFAULT NOW(),
    enabled     BOOLEAN                  NOT NULL DEFAULT true,
    metadata    JSONB                             DEFAULT '{}'::jsonb
);

CREATE INDEX idx_devices_enabled ON devices (enabled);
CREATE INDEX idx_devices_last_seen ON devices (last_seen);

COMMENT
ON TABLE devices IS 'Registered backup client devices';
COMMENT
ON COLUMN devices.id IS 'Unique device identifier (from local config)';
COMMENT
ON COLUMN devices.platform IS 'Operating system platform';
COMMENT
ON COLUMN devices.last_seen IS 'Last heartbeat/sync timestamp';
COMMENT
ON COLUMN devices.metadata IS 'Extensible device metadata (version, etc.)';
```

### 2. Global Repository Settings

The system uses a single shared restic repository for all devices and backup jobs. Repository configuration is stored in the `settings` table as global settings.

**Required Settings**:
- `repository_url`: Full restic repository URL (e.g., `sftp:user@host:/path/to/repo`)
- `repository_password`: Repository password (stored directly in database)
- `repository_cache_dir`: Custom restic cache directory (optional)

**Note**: All devices share the same repository. Individual backups are distinguished by restic tags.

### 3. backup_jobs

Defines backup jobs for specific devices.

**Note**: Uses UUID for `id` to support Relica migration (preserves existing backup job IDs).

```sql
CREATE TABLE backup_jobs
(
    id               UUID PRIMARY KEY DEFAULT gen_random_uuid(),
    device_id        VARCHAR(255)             NOT NULL REFERENCES devices (id) ON DELETE CASCADE,
    name             VARCHAR(255)             NOT NULL,
    description      TEXT,
    source_paths     TEXT[] NOT NULL,                                       -- Array of paths to back up
    exclude_patterns TEXT[],                                                -- Array of exclude patterns
    tags             TEXT[],                                                -- Array of restic tags (deprecated, use standard tags)
    restic_args      JSONB                             DEFAULT '[]'::jsonb, -- Additional restic arguments
    enabled          BOOLEAN                  NOT NULL DEFAULT true,
    created_at       TIMESTAMP WITH TIME ZONE NOT NULL DEFAULT NOW(),
    updated_at       TIMESTAMP WITH TIME ZONE NOT NULL DEFAULT NOW(),
    metadata         JSONB                             DEFAULT '{}'::jsonb,
    -- Relica compatibility fields
    origin_name      VARCHAR(255),                                          -- Origin/device name for backup naming (e.g., "blade")
    origin_id        UUID,                                                  -- Origin UUID (deprecated, not used for path construction)
    account_id       UUID,                                                  -- Account UUID for multi-tenancy and tagging
    UNIQUE (device_id, name)
);

CREATE INDEX idx_backup_jobs_device ON backup_jobs (device_id);
CREATE INDEX idx_backup_jobs_enabled ON backup_jobs (device_id, enabled);
CREATE INDEX idx_backup_jobs_origin ON backup_jobs (origin_id);
CREATE INDEX idx_backup_jobs_account ON backup_jobs (account_id);

COMMENT
ON TABLE backup_jobs IS 'Backup job definitions per device';
COMMENT
ON COLUMN backup_jobs.id IS 'UUID (for Relica compatibility)';
COMMENT
ON COLUMN backup_jobs.name IS 'Backup name (may include origin prefix like "blade/home")';
COMMENT
ON COLUMN backup_jobs.source_paths IS 'Directories/files to back up';
COMMENT
ON COLUMN backup_jobs.exclude_patterns IS 'Patterns to exclude (restic --exclude)';
COMMENT
ON COLUMN backup_jobs.tags IS 'Custom tags (deprecated - standard tags now auto-generated)';
COMMENT
ON COLUMN backup_jobs.restic_args IS 'Additional restic CLI arguments as JSON array';
COMMENT
ON COLUMN backup_jobs.origin_name IS 'Origin/device name for Relica-style naming';
COMMENT
ON COLUMN backup_jobs.origin_id IS 'Origin UUID for Relica compatibility (not used for paths in single-repo model)';
COMMENT
ON COLUMN backup_jobs.account_id IS 'Account UUID for multi-tenancy';
```

### 4. schedules

Defines execution schedules for backup jobs.

**Note**: Multiple schedules allowed per job (removed UNIQUE constraint) to support Relica's multiple times_of_day per backup.

```sql
CREATE TABLE schedules
(
    id               SERIAL PRIMARY KEY,
    job_id           UUID                     NOT NULL REFERENCES backup_jobs (id) ON DELETE CASCADE,
    schedule_type    VARCHAR(50)              NOT NULL, -- 'cron' or 'interval'
    cron_expression  VARCHAR(255),                      -- Cron expression (if type=cron)
    interval_seconds INTEGER,                           -- Interval in seconds (if type=interval)
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

COMMENT
ON TABLE schedules IS 'Execution schedules for backup jobs (multiple allowed per job)';
COMMENT
ON COLUMN schedules.job_id IS 'References backup_jobs(id) - UUID type';
COMMENT
ON COLUMN schedules.schedule_type IS 'Type of schedule: cron expression or fixed interval';
COMMENT
ON COLUMN schedules.cron_expression IS 'Cron expression (e.g., "0 2 * * *" for daily at 2 AM)';
COMMENT
ON COLUMN schedules.interval_seconds IS 'Fixed interval in seconds (e.g., 3600 for hourly)';
COMMENT
ON COLUMN schedules.last_run_at IS 'Timestamp of last execution';
COMMENT
ON COLUMN schedules.next_run_at IS 'Calculated next execution time';
```

### 5. runs

Records all backup execution attempts and results.

```sql
CREATE TABLE runs
(
    id                    SERIAL PRIMARY KEY,
    job_id                UUID                     NOT NULL REFERENCES backup_jobs (id) ON DELETE CASCADE,
    device_id             VARCHAR(255)             NOT NULL REFERENCES devices (id) ON DELETE CASCADE,
    start_time            TIMESTAMP WITH TIME ZONE NOT NULL,
    end_time              TIMESTAMP WITH TIME ZONE,
    status                VARCHAR(50)              NOT NULL, -- 'running', 'success', 'failed', 'cancelled'
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
    snapshot_id           VARCHAR(255),                      -- restic snapshot ID
    restic_output         TEXT,                              -- Full restic stdout
    restic_errors         TEXT,                              -- Full restic stderr
    triggered_by          VARCHAR(50),                       -- 'schedule', 'manual', 'missed'
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

COMMENT
ON TABLE runs IS 'Execution history of all backup runs';
COMMENT
ON COLUMN runs.job_id IS 'References backup_jobs(id) - UUID type';
COMMENT
ON COLUMN runs.status IS 'Current status of the backup run';
COMMENT
ON COLUMN runs.exit_code IS 'restic process exit code';
COMMENT
ON COLUMN runs.error_message IS 'Human-readable error message';
COMMENT
ON COLUMN runs.files_new IS 'Number of new files backed up';
COMMENT
ON COLUMN runs.data_added_bytes IS 'Bytes of data added in this backup';
COMMENT
ON COLUMN runs.snapshot_id IS 'restic snapshot identifier';
COMMENT
ON COLUMN runs.triggered_by IS 'How the backup was initiated';
```

### 6. settings

Global and device-specific settings.

```sql
CREATE TABLE settings
(
    id          SERIAL PRIMARY KEY,
    device_id   VARCHAR(255) REFERENCES devices (id) ON DELETE CASCADE, -- NULL for global settings
    key         VARCHAR(255)             NOT NULL,
    value       TEXT                     NOT NULL,
    description TEXT,
    created_at  TIMESTAMP WITH TIME ZONE NOT NULL DEFAULT NOW(),
    updated_at  TIMESTAMP WITH TIME ZONE NOT NULL DEFAULT NOW(),
    UNIQUE (device_id, key)
);

CREATE INDEX idx_settings_device ON settings (device_id);
CREATE INDEX idx_settings_key ON settings (key);

COMMENT
ON TABLE settings IS 'Configuration settings (global and per-device)';
COMMENT
ON COLUMN settings.device_id IS 'NULL for global settings, device_id for device-specific';
COMMENT
ON COLUMN settings.key IS 'Setting key (e.g., "sync_interval_seconds")';
COMMENT
ON COLUMN settings.value IS 'Setting value (as string, parsed by client)';
```

## Initial Data Migration

### Default Settings

```sql
INSERT INTO settings (device_id, key, value, description)
VALUES (NULL, 'sync_interval_seconds', '300', 'How often clients sync config from database'),
       (NULL, 'max_concurrent_backups', '1', 'Maximum concurrent backup jobs per device'),
       (NULL, 'run_retention_days', '90', 'How long to keep run history in database'),
       (NULL, 'repository_url', '', 'Shared restic repository URL (e.g., sftp:user@host:/path/to/repo)'),
       (NULL, 'repository_password', '', 'Repository password for restic'),
       (NULL, 'repository_cache_dir', '', 'Restic cache directory (empty = restic default)');
```

## Views

### Useful query views for the client and UI.

#### Latest Run Per Job

```sql
CREATE VIEW latest_runs AS
SELECT DISTINCT
        ON (job_id)
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

COMMENT
ON VIEW latest_runs IS 'Most recent run for each backup job';
```

#### Job Summary

```sql
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

COMMENT
ON VIEW job_summary IS 'Complete overview of jobs with schedule and last run info';
```

## Queries for Client

### Load Device Configuration

```sql
-- Load all jobs for a device
SELECT *
FROM backup_jobs
WHERE device_id = $1
  AND enabled = true;

-- Load all schedules for device's jobs
SELECT s.*
FROM schedules s
         JOIN backup_jobs j ON s.job_id = j.id
WHERE j.device_id = $1
  AND j.enabled = true
  AND s.enabled = true;

-- Load device-specific and global settings (includes repository config)
SELECT key, value
FROM settings
WHERE device_id IS NULL OR device_id = $1
ORDER BY device_id NULLS FIRST; -- Global settings first, then device-specific
```

### Record Backup Run

```sql
-- Start a run
INSERT INTO runs (job_id, device_id, start_time, status, triggered_by)
VALUES ($1, $2, NOW(), 'running', $3) RETURNING id;

-- Update run on completion
UPDATE runs
SET end_time         = $1,
    status           = $2,
    exit_code        = $3,
    error_message    = $4,
    files_new        = $5,
    files_changed    = $6,
    files_unmodified = $7,
    data_added_bytes = $8,
    snapshot_id      = $9,
    restic_output    = $10,
    restic_errors    = $11,
    duration_seconds = EXTRACT(EPOCH FROM ($1 - start_time)) ::INTEGER
WHERE id = $12;

-- Update schedule last_run_at
UPDATE schedules
SET last_run_at = $1,
    next_run_at = $2
WHERE job_id = $3;
```

### Update Device Heartbeat

```sql
UPDATE devices
SET last_seen = NOW(),
    hostname  = $2,
    metadata  = $3
WHERE id = $1;
```

## Indexes Summary

The schema includes indexes optimized for:

- Device-scoped queries (most common)
- Schedule lookups for next run calculations
- Run history queries (recent runs, by job, by status)
- Settings lookups (by device and key)

## Migration Strategy

Using `sqlx` migrations:

1. **001_initial_schema.sql**: Create all tables
2. **002_initial_data.sql**: Insert default settings
3. **003_views.sql**: Create views

Each migration is versioned and applied automatically by sqlx on first connection.

## Database Permissions

Recommended PostgreSQL roles:

```sql
-- Admin role (for setup)
CREATE ROLE backup_admin WITH LOGIN PASSWORD 'secure_password';
GRANT
ALL
PRIVILEGES
ON
DATABASE
backup_control TO backup_admin;

-- Client role (for runtime)
CREATE ROLE backup_client WITH LOGIN PASSWORD 'client_password';
GRANT
SELECT,
INSERT
,
UPDATE
ON ALL TABLES IN SCHEMA public TO backup_client;
GRANT
USAGE,
SELECT
ON ALL SEQUENCES IN SCHEMA public TO backup_client;
GRANT
SELECT
ON ALL VIEWS IN SCHEMA public TO backup_client;

-- Future: read-only role for monitoring/dashboards
CREATE ROLE backup_viewer WITH LOGIN PASSWORD 'viewer_password';
GRANT
SELECT
ON ALL TABLES IN SCHEMA public TO backup_viewer;
GRANT
SELECT
ON ALL VIEWS IN SCHEMA public TO backup_viewer;
```

## Backup & Maintenance

### Database Backup

The PostgreSQL database itself should be backed up regularly:

```bash
pg_dump -h db.example.com -U backup_admin backup_control > backup_control.sql
```

### Run History Cleanup

Periodically clean old run records:

```sql
DELETE
FROM runs
WHERE start_time < NOW() - INTERVAL '90 days'
  AND status IN ('success'
    , 'failed'
    , 'cancelled');
```

This can be automated via a PostgreSQL cron job or a separate maintenance script.

## Scalability Considerations

For the expected load (dozens of devices, 1-10 jobs per device, runs every few hours):

- No partitioning needed
- Standard indexes sufficient
- Connection pooling in client (sqlx default)
- Database size: ~1-10 GB per year (depends on run history retention)

Future optimizations (if needed):

- Partition `runs` table by date
- Archive old runs to separate table/database
- Add materialized views for dashboard queries
