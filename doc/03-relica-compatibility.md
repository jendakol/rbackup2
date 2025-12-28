# Relica Backup Compatibility

## Overview

This document defines the compatibility requirements with the existing Relica Backup system to ensure seamless migration
and continued operation using the same restic repositories and snapshot tags.

## Relica Backup Structure Analysis

Based on the provided `relica-account-info.json` configuration file, the following key structures and conventions were
identified:

### 1. Backup Job Structure

```json
{
  "id": "cb3104d6-5d04-4cd6-9a8e-73c6c4b61fe9",
  "name": "device1/Dokumenty",
  "origin_id": "f213f03b-c8e1-47a5-8bf5-f7c8dd76217d",
  "account_id": "003dbcee-4f0a-4a55-a7bb-a6511b263558",
  "include": [
    "/home/user/Documents"
  ],
  "exclude": [
    "/home/user/.cache",
    ...
  ],
  "destinations": [
    "cb871492-76dc-4683-8ecd-d9a6c84ae798"
  ],
  "schedule": {
    "scheduled": true,
    "flexible": true,
    "times_of_day": [
      "13:00",
      "23:00"
    ],
    "days_of_week": [],
    "days_of_month": [],
    "months_of_year": []
  },
  "hooks": [
    ...
  ]
}
```

### 2. Naming Convention

**Pattern**: `origin_name/backup_name`

Examples:

- `device1/Dokumenty`
- `device1/home`
- `device3/Fotky`
- `device4/Dokumenty`
- `cloud/backups`

Where:

- `origin_name` = device/client name (from "origins" table)
- `backup_name` = descriptive name for the backup job

### 3. Origin (Device) Structure

```json
{
  "id": "f213f03b-c8e1-47a5-8bf5-f7c8dd76217d",
  "name": "device1",
  "account_id": "003dbcee-4f0a-4a55-a7bb-a6511b263558",
  "key": "<DEVICE_KEY_1>",
  "last_connected": "2025-12-15T16:05:22.89479Z",
  "last_ip": "192.0.2.100"
}
```

### 4. Destination (Repository) Structure

```json
{
  "id": "cb871492-76dc-4683-8ecd-d9a6c84ae798",
  "name": "Cloud",
  "type": "cloud",
  "cloud_account": {
    "provider_name": "SFTP",
    "remote_type": "sftp",
    "remote_base_path": "/data/backups/",
    "credentials": [
      {
        "key": "HOST",
        "value": "backup.example.com"
      },
      {
        "key": "PORT",
        "value": "22"
      },
      {
        "key": "USER",
        "value": "backup_user"
      },
      {
        "key": "PASS",
        "value": "<PASSWORD_HASH>"
      }
    ]
  }
}
```

### 5. Schedule Format

Relica uses a flexible schedule model:

- `scheduled`: boolean - whether the backup is scheduled
- `flexible`: boolean - allow schedule flexibility (likely for missed runs)
- `times_of_day`: array of "HH:MM" strings
- `days_of_week`: array of integers (1=Monday, 7=Sunday)
- `days_of_month`: array of integers (1-31)
- `months_of_year`: array of integers (1-12)

Empty arrays mean "all" for that dimension.

### 6. Last Backup Tracking

```json
"last_backups_done": {
"backup_id": {
"destination_id": "2025-12-15T03:01:07.580094317Z"
}
}
```

## Compatibility Requirements

### R1: Backup Job ID (UUID)

**Requirement**: Use the same backup job UUID from Relica configuration.

**Implementation**:

- Database schema: `backup_jobs.id` should accept UUID strings, not auto-generated serials
- Migration path: Allow importing Relica backup jobs with their existing UUIDs
- Format: Standard UUID v4 (e.g., `cb3104d6-5d04-4cd6-9a8e-73c6c4b61fe9`)

**Schema Change**:

```sql
-- Instead of:
-- id SERIAL PRIMARY KEY
-- Use:
id
UUID PRIMARY KEY DEFAULT gen_random_uuid()
```

### R2: Backup Naming Convention

**Requirement**: Support the `origin_name/backup_name` naming pattern.

**Implementation**:

- `backup_jobs.name` should store the full name including the origin prefix
- Example: `"device1/home"`, `"device4/Dokumenty"`
- The name appears in restic tags and must be preserved exactly

**Why**: Existing restic snapshots are tagged with these names, and changing them would break snapshot identification
and filtering.

### R3: Restic Snapshot Tags

**Requirement**: Tag restic snapshots with backup identifiers that match Relica's convention.

**Relica Snapshot Tag Format**:
Snapshots are tagged with a single primary tag in the format: `backup:<backup_job_uuid>`

Example: `backup:6f82fc40-b82b-43d2-b4f0-72c12deca9fa`

This tag format allows:

- Easy identification of snapshots by backup job
- Filtering snapshots for a specific backup: `restic snapshots --tag backup:<uuid>`
- Compatibility with existing Relica restic repositories

**Implementation**:

```bash
restic backup \
  --tag "backup:${backup_id}" \
  /path/to/backup
```

**Storage in Database**:

```sql
ALTER TABLE backup_jobs
    ADD COLUMN origin_name VARCHAR(255);
ALTER TABLE backup_jobs
    ADD COLUMN origin_id UUID;
ALTER TABLE backup_jobs
    ADD COLUMN account_id UUID;
```

**Note**: While Relica may use the `backup:<uuid>` as the primary tag, additional tags (like origin name, account ID)may
also be used but are not required for basic compatibility.

### R4: Repository Configuration

**Requirement**: Use a single shared SFTP repository for all devices and backups.

**Relica Repository Format**:

Relica used per-job repository paths like:

```
sftp:backup_user@backup.example.com:22//data/backups/${origin_id}/${backup_id}
```

**rbackup2 Simplified Approach**:

- Single shared repository URL stored in global settings
- Example: `sftp:backup_user@backup.example.com:22//data/backups/`
- All devices and backup jobs use the same repository
- Individual backups are distinguished by restic tags (backup ID, origin, etc.)

**Schema Change**:

```sql
-- Store repository config in settings table:
INSERT INTO settings (device_id, key, value, description)
VALUES (NULL, 'repository_url', 'sftp:backup_user@backup.example.com:22//data/backups/', 'Shared restic repository URL'),
       (NULL, 'repository_password', '...', 'Repository password');
```

**Note**: The `origin_id` field is kept in `backup_jobs` for Relica compatibility and tagging, but is not used for
repository path construction.

### R5: Schedule Compatibility

**Requirement**: Support Relica's flexible time-based scheduling.

**Mapping**:

- Relica `times_of_day` → multiple cron expressions or time triggers
- Relica `flexible: true` → enable missed run detection and catchup
- Relica `days_of_week` → cron day-of-week field

**Example Conversion**:

```
Relica:
{
  "times_of_day": ["13:00", "23:00"],
  "days_of_week": [1, 3, 5]  // Mon, Wed, Fri
}

→ Cron:
"0 13 * * 1,3,5"
"0 23 * * 1,3,5"

Or use multiple schedule entries per job.
```

**Schema Change**:
Allow multiple schedules per job:

```sql
-- Remove: job_id INTEGER NOT NULL UNIQUE
-- Change to: job_id INTEGER NOT NULL (remove UNIQUE constraint)
-- Allow multiple schedules for the same job_id
```

### R6: Device/Origin Identification

**Requirement**: Preserve origin (device) IDs and names from Relica.

**Implementation**:

- `devices.id` should accept UUID strings from Relica origins
- `devices.name` should match origin names (e.g., `device1`, `device4`)
- Store original origin_id in backup_jobs for tagging purposes

**Schema Enhancement**:

```sql
-- devices table already uses VARCHAR(255) for id, which is compatible
-- No additional columns needed for devices table
-- origin_id is stored in backup_jobs table for Relica compatibility
```

### R7: Last Backup Timestamp

**Requirement**: Track last successful backup per job.

**Implementation**:

- Tracked by `schedules.last_run_at` for each schedule
- Run history stored in `runs` table with full execution details
- Single shared repository means no need for per-destination tracking

### R8: Hooks/Post-Backup Commands

**Requirement**: Support post-backup hook commands (optional for initial implementation).

**Relica Hooks**:

```json
{
  "command": "/usr/bin/endpoint-watcher device1 relica-backup-end home",
  "timing": "end",
  "on_error": "stop"
}
```

**Implementation** (future enhancement):

```sql
CREATE TABLE backup_hooks
(
    id       SERIAL PRIMARY KEY,
    job_id   UUID        NOT NULL REFERENCES backup_jobs (id),
    command  TEXT        NOT NULL,
    timing   VARCHAR(50) NOT NULL, -- 'start', 'end'
    on_error VARCHAR(50),          -- 'stop', 'continue'
    enabled  BOOLEAN DEFAULT true
);
```

## Migration Strategy

### Phase 1: Schema Updates

1. Change `backup_jobs.id` from SERIAL to UUID
2. Add `backup_jobs.origin_name`, `backup_jobs.origin_id`, `backup_jobs.account_id`
3. Add repository settings to global settings (repository_url, repository_password)
4. Allow multiple schedules per job

### Phase 2: Import Tool

Create a migration tool to import Relica configuration:

```bash
rbackup2-migrate --from-relica /path/to/relica-account-info.json --device device1
```

The tool should:

1. Parse Relica JSON
2. Filter backups by origin name
3. Create device record
4. Extract repository credentials and store in global settings (repository_url, repository_password)
5. Create backup_jobs with preserved UUIDs and names
6. Convert schedules to cron/interval format
7. Set last_run_at from last_backups_done

### Phase 3: Restic Tag Compatibility

Ensure all new snapshots use the same tagging convention:

```rust
pub fn build_restic_command(job: &BackupJob, repo_url: &str, repo_password: &str) -> Command {
    let mut cmd = Command::new("restic");
    cmd.arg("backup")
        .env("RESTIC_REPOSITORY", repo_url)
        .env("RESTIC_PASSWORD", repo_password)
        .arg("--tag").arg(format!("backup:{}", job.id))
        .arg("--tag").arg(format!("backup_name={}", job.name))
        .arg("--tag").arg(format!("origin={}", job.origin_name.as_deref().unwrap_or("unknown")))
        .arg("--tag").arg(format!("account_id={}", job.account_id.map(|id| id.to_string()).unwrap_or_default()));

    for path in &job.source_paths {
        cmd.arg(path);
    }

    for exclude in &job.exclude_patterns {
        cmd.arg("--exclude").arg(exclude);
    }

    cmd
}
```

**Note**: Repository URL and password are loaded from global settings, not per-job configuration.

## Validation Checklist

To ensure compatibility with existing Relica repositories:

- [ ] Backup job UUIDs preserved from Relica
- [ ] Backup names follow `origin/name` pattern
- [ ] Restic snapshots tagged with `backup:<uuid>`, `backup_name`, `origin`, `account_id`
- [ ] Single shared repository URL configured in global settings
- [ ] Repository password stored in global settings
- [ ] Schedules converted correctly from Relica format
- [ ] Last backup timestamps imported
- [ ] Can list existing snapshots: `restic --tag backup:${uuid} snapshots`
- [ ] New snapshots created with same tags
- [ ] Existing snapshots remain accessible after migration

## Testing with Existing Repository

After implementing compatibility changes, test with shared Relica repository:

```bash
# List existing snapshots for a specific backup
export RESTIC_REPOSITORY="sftp:backup_user@backup.example.com:22//data/backups/"
export RESTIC_PASSWORD="..."
restic snapshots --tag backup:cb3104d6-5d04-4cd6-9a8e-73c6c4b61fe9

# Run new backup with rbackup2
rbackup2 --config config.yaml

# Verify new snapshot has correct tags
restic snapshots --latest 1 --json | jq '.tags'
```

**Note**: The single shared repository URL is configured in the database settings, not per-job.

## Summary

Key compatibility points:

1. **UUIDs**: Use Relica's backup and origin UUIDs
2. **Names**: Preserve `origin/name` format
3. **Tags**: Tag snapshots with `backup:<uuid>`, `backup_name`, `origin`, `account_id`
4. **Single repository**: Use one shared repository URL stored in global settings
5. **Repository password**: Store directly in database settings
6. **Schedules**: Convert Relica's flexible schedule to cron/interval
7. **Migration tool**: Import existing configuration

This ensures that rbackup2 can continue backing up to the same restic repository without breaking existing snapshot
identification or restore workflows. The simplified single-repository approach reduces complexity while maintaining
full compatibility through restic tags.
