# Implementation Phases

## Overview

This document breaks down the implementation into **7 logical phases**, each delivering a functional increment that can
be tested independently.

The phases follow a **bottom-up approach**: foundational components first, then integration, and finally optional
enhancements.

**Relica Compatibility**: The implementation includes support for migrating from Relica Backup. Database schema uses
UUIDs for job IDs, supports origin/account fields, and implements Relica-compatible restic tagging. See
`doc/03-relica-compatibility.md` for detailed requirements.

---

## Phase 1: Project Foundation & Configuration

**Goal**: Set up project structure, dependencies, and local configuration loading.

### Tasks

1. **Project Structure**
    - Create module directories (`config/`, `db/`, `scheduler/`, `backup/`, `api/`, `metrics/`, `ui/`)
    - Set up `error.rs` for application-wide error types

2. **Cargo.toml Dependencies**
    - Core: `tokio`, `serde`, `serde_json`, `serde_yaml`
    - Database: `sqlx` with PostgreSQL feature
    - HTTP: `axum`, `tower`, `tower-http`
    - Logging: `tracing`, `tracing-subscriber`
    - Scheduling: `cron` (cron parser), `chrono`
    - CLI: `clap`
    - Metrics: `prometheus` (client library)

3. **Error Types** (`src/error.rs`)
    - Define `AppError` enum
    - Implement `From` conversions for common error types
    - Implement `Display` and `Error` traits
    - Add helper `Result<T>` type alias

4. **Local Configuration** (`src/config/local.rs`)
    - Define `LocalConfig` struct matching YAML schema
    - Implement YAML loading from file path
    - Add validation logic
    - Handle default values

5. **Configuration Module** (`src/config.rs`)
    - Declare submodules (local, remote)
    - Re-export local config
    - Define in-memory config cache structures
    - Stub for remote config (implemented in Phase 2)

6. **Main Entry Point** (`src/main.rs`)
    - Set up CLI argument parsing (config file path)
    - Initialize tracing/logging to file
    - Load local config
    - Print startup banner with config summary

7. **Example Config File** (`config.example.yaml`)
    - Create example configuration
    - Document all fields with comments

### Deliverables

- ✅ Project compiles
- ✅ Can load local YAML config
- ✅ Logging to file works
- ✅ Error handling framework in place

### Testing

```bash
cargo build
cargo run -- --config config.example.yaml
# Should load config and exit gracefully
```

---

## Phase 2: Database Layer

**Goal**: Implement PostgreSQL connection, schema migrations, and data models.

### Tasks

1. **Database Models** (`src/db/models.rs`)
    - Define structs for all database tables:
        - `Device`
        - `Repository`
        - `BackupJob`
        - `Schedule`
        - `Run`
        - `Setting`
    - Add `sqlx` derive macros (`FromRow`)
    - Implement helper methods (e.g., `Job::with_repository()`)

2. **SQL Migrations** (`migrations/`)
    - `001_initial_schema.sql`: All CREATE TABLE statements
    - `002_initial_data.sql`: Default settings
    - `003_views.sql`: Create views (latest_runs, job_summary)

3. **Database Queries** (`src/db/queries.rs`)
    - Connection pool setup
    - Device queries:
        - `get_device()`
        - `update_device_heartbeat()`
        - `upsert_device()`
    - Job queries:
        - `get_jobs_for_device()`
        - `get_job_by_id()`
    - Schedule queries:
        - `get_schedules_for_device()`
        - `update_schedule_last_run()`
    - Run queries:
        - `create_run()`
        - `update_run()`
        - `get_recent_runs()`
    - Settings queries:
        - `get_settings_for_device()`

4. **Database Module** (`src/db.rs`)
    - Declare submodules (models, queries, schema)
    - Export models and queries
    - Define connection pool type
    - Add retry logic for connection failures

5. **Remote Configuration** (`src/config/remote.rs`)
    - Implement `load_config_from_db(device_id, pool)`
    - Load jobs, schedules, and global settings (including repository config)
    - Build in-memory config structure
    - Handle missing device (first-time registration)

6. **Integration in Main**
    - Connect to PostgreSQL using local config
    - Run migrations
    - Register/update device in database
    - Load remote configuration into memory
    - Log configuration summary

### Deliverables

- ✅ Database schema created
- ✅ Connection to PostgreSQL works
- ✅ Can load device configuration from DB
- ✅ Migrations run automatically

### Testing

```bash
# Set up test PostgreSQL database
createdb backup_control_test

# Run migrations manually
sqlx migrate run --database-url "postgresql://user:pass@localhost/backup_control_test"

# Run client
cargo run -- --config config.example.yaml
# Should connect to DB and load config
```

---

## Phase 3: Backup Executor (restic Integration)

**Goal**: Execute restic backups and parse output.

### Tasks

1. **restic Command Builder** (`src/backup/restic.rs`)
    - Define `ResticCommand` struct
    - Method to build `Command` with:
        - Environment variables (RESTIC_REPOSITORY, RESTIC_PASSWORD, etc.)
        - Arguments (backup, source paths, excludes, tags)
    - Handle platform-specific restic binary path
    - Validation (check restic binary exists)

2. **restic Output Parser** (`src/backup/output.rs`)
    - Define `BackupStats` struct
    - Parse restic JSON output (if using `--json`)
    - Extract:
        - Files new/changed/unmodified
        - Data added
        - Snapshot ID
    - Fallback: parse text output with regex

3. **Backup Executor** (`src/backup.rs`)
    - Declare submodules (restic, output)
    - `execute_backup(job, repo_config, pool)` function
    - Steps:
        1. Create run record in DB (status=running)
        2. Build restic command (using repo URL and password from global config)
        3. Execute via `tokio::process::Command`
        4. Capture stdout/stderr
        5. Parse output
        6. Update run record (status, stats, snapshot_id)
        7. Return `Result<RunId>`
    - Handle errors gracefully
    - Use `spawn_blocking` if needed for synchronous operations

4. **Manual Backup Test**
    - Add CLI command: `--test-backup <job_id>`
    - Trigger single backup execution
    - Print results to console

### Deliverables

- ✅ Can execute restic backup
- ✅ Output parsed correctly
- ✅ Run record stored in database
- ✅ Errors handled and logged

### Testing

```bash
# Prepare test repository
restic init --repo /tmp/test-restic-repo

# Add test job to database manually
# Run test backup
cargo run -- --config config.example.yaml --test-backup 1
# Check database for run record
```

---

## Phase 4: Scheduler

**Goal**: Implement job scheduling with cron/interval support and missed run detection.

### Tasks

1. **Schedule Parsing** (`src/scheduler/cron.rs`)
    - Parse cron expressions using `cron` crate
    - Parse interval schedules (seconds)
    - Calculate next run time
    - Function: `calculate_next_run(schedule, last_run) -> DateTime`

2. **Missed Run Detection** (`src/scheduler/missed_runs.rs`)
    - Logic: `is_run_missed(schedule, last_run, now) -> bool`
    - Account for grace period (e.g., 5 minutes)
    - Calculate how many runs were missed (for intervals)

3. **Scheduler Core** (`src/scheduler.rs`)
    - Declare submodules (cron, missed_runs, executor)
    - Define `Scheduler` struct
    - Fields:
        - In-memory schedule map
        - Job queue (tokio channels)
        - Reference to DB pool
        - Reference to config
    - Methods:
        - `new()`
        - `add_schedule()`
        - `remove_schedule()`
        - `reload_schedules()` (from config)
        - `check_schedules()` (called in loop)
    - Spawn background task that:
        - Checks schedules every minute
        - Queues jobs that are due
        - Handles missed runs

4. **Job Executor** (`src/scheduler/executor.rs`)
    - Background task that consumes job queue
    - For each job:
        - Call `backup::execute_backup()`
        - Handle concurrency (max 1 backup at a time per device)
    - Log execution

5. **Integration in Main**
    - Initialize scheduler with loaded config
    - Start scheduler background tasks
    - Keep main thread alive (wait on signal or forever)

### Deliverables

- ✅ Scheduler detects due jobs
- ✅ Executes backups automatically
- ✅ Missed runs detected and executed
- ✅ Handles schedule updates dynamically

### Testing

```bash
# Add test job with interval schedule (every 2 minutes)
# Run client
cargo run -- --config config.example.yaml
# Wait and observe logs
# Check database for scheduled runs
```

---

## Phase 5: HTTP API & Static UI

**Goal**: Expose REST API and serve basic web UI.

### Tasks

1. **API Models** (`src/api/models.rs`)
    - Request/response structs:
        - `StatusResponse`
        - `ConfigResponse`
        - `UpdateConfigRequest`
        - `TriggerJobRequest`
        - `RunHistoryResponse`
    - Implement `Serialize`/`Deserialize`

2. **API Handlers** (`src/api/handlers.rs`)
    - `GET /health` → simple "OK" response
    - `GET /status` → device status, last backup times
    - `GET /config` → current device configuration
    - `POST /config` → update configuration in DB
    - `POST /jobs/{job_id}/run` → manually trigger job
    - `GET /runs` → recent run history (last N runs)
    - All handlers receive app state (DB pool, config, scheduler handle)

3. **API Server** (`src/api/server.rs`)
    - Define app state struct
    - Create axum router with routes
    - Add middleware:
        - CORS (for local UI)
        - Request logging
        - Error handling
    - Serve static files from `src/ui/static/`
    - Start server on configured bind address

4. **Static UI Files** (`src/ui/static/`)
    - `index.html`:
        - Header with device name
        - Status section (last backup, next scheduled)
        - Jobs list with status indicators
        - Run history table
        - Manual trigger buttons
        - Config editor (modal)
    - `styles.css`:
        - Simple, clean design
        - Responsive layout
        - Status color coding (green/red/yellow)
    - `app.js`:
        - Fetch status on load
        - Auto-refresh every 30 seconds
        - Handle button clicks (trigger backup)
        - Form submission for config updates

5. **Integration in Main**
    - Start HTTP server in background task
    - Pass app state (DB pool, config handle, scheduler handle)

### Deliverables

- ✅ API endpoints functional
- ✅ Web UI accessible at `http://127.0.0.1:1201`
- ✅ Can view status and history
- ✅ Can trigger manual backups
- ✅ Can view configuration

### Testing

```bash
cargo run -- --config config.example.yaml
# Open browser: http://127.0.0.1:1201
# Test API endpoints with curl:
curl http://127.0.0.1:1201/health
curl http://127.0.0.1:1201/status
curl -X POST http://127.0.0.1:1201/jobs/1/run
```

---

## Phase 6: Configuration Reload & Periodic Sync

**Goal**: Implement periodic configuration refresh from database.

### Tasks

1. **Config Reload Logic** (`src/config/remote.rs`)
    - Function: `reload_config_from_db(device_id, pool) -> Result<Config>`
    - Compare new config with current in-memory config
    - Return diff or full new config

2. **Config Sync Task** (`src/config.rs`)
    - Background tokio task
    - Every N seconds (from settings or default 300s):
        1. Load config from DB
        2. Compare with current config
        3. If changed:
            - Update in-memory config
            - Notify scheduler to reload schedules
            - Log changes
    - Handle DB unavailability gracefully (use last-known-good)

3. **Scheduler Reload** (`src/scheduler.rs`)
    - Add `reload_schedules()` method
    - Diff old vs new schedules:
        - Add new jobs
        - Remove deleted jobs
        - Update modified jobs
    - Preserve running jobs

4. **Configuration Update via API** (`src/api/handlers.rs`)
    - `POST /config` implementation:
        1. Validate incoming config
        2. Write changes to database
        3. Trigger immediate config reload
        4. Return success

5. **Integration in Main**
    - Start config sync background task
    - Pass notification channel to scheduler

### Deliverables

- ✅ Config reloads periodically
- ✅ Changes via API persist to DB
- ✅ Scheduler updates when config changes
- ✅ Client continues with last-known-good if DB unavailable

### Testing

```bash
# Start client
cargo run -- --config config.example.yaml

# In another terminal, update a job in the database
psql backup_control -c "UPDATE backup_jobs SET source_paths = ARRAY['/tmp/test'] WHERE id = 1;"

# Wait for sync interval
# Check logs for config reload
# Verify UI shows updated config
```

---

## Phase 7: Metrics & Polish

**Goal**: Add Prometheus metrics and final production polish.

### Tasks

1. **Metrics Reporter** (`src/metrics/pushgateway.rs`)
    - Define metrics:
        - `backup_last_success_timestamp` (gauge)
        - `backup_duration_seconds` (histogram)
        - `backup_files_total` (gauge)
        - `backup_bytes_added` (gauge)
        - `backup_runs_total` (counter by status)
    - Function: `push_metrics(pushgateway_url, job_name, metrics)`
    - Handle Pushgateway unavailability (don't block backup)

2. **Metrics Integration**
    - Update metrics after each backup run
    - Background task: push metrics every N minutes
    - Include device and job labels

3. **Graceful Shutdown** (`src/main.rs`)
    - Handle SIGTERM/SIGINT
    - Steps:
        1. Stop accepting new backups
        2. Wait for running backup to complete (with timeout)
        3. Close DB connections
        4. Stop HTTP server
        5. Exit cleanly

4. **Heartbeat Task**
    - Background task: update `devices.last_seen` every N minutes
    - Include hostname and metadata

5. **README & Documentation**
    - `README.md`:
        - Project overview
        - Prerequisites
        - Build instructions (Linux/Windows)
        - Configuration guide
        - Running as service (systemd/Windows Service)
    - `doc/deployment.md`:
        - Deployment checklist
        - Database setup
        - Service configuration examples

6. **Build Scripts**
    - `build.sh` for release builds
    - Platform-specific build instructions
    - Optional: Docker build for testing

7. **Production Hardening**
    - Review all `unwrap()` calls → replace with proper error handling
    - Add rate limiting to API (prevent abuse)
    - Add request size limits
    - Security: validate all inputs
    - Add log rotation configuration examples

### Deliverables

- ✅ Metrics pushed to Prometheus
- ✅ Graceful shutdown works
- ✅ Complete documentation
- ✅ Production-ready error handling
- ✅ Build instructions for both platforms

### Testing

```bash
# Full integration test
cargo build --release
./target/release/rbackup2 --config config.example.yaml

# Test graceful shutdown
# Send SIGTERM, verify backup completes

# Test metrics
# Check Pushgateway for metrics

# Cross-compile for Windows
cargo build --release --target x86_64-pc-windows-gnu
```

---

## Testing Strategy per Phase

### Unit Tests

- Config parsing (Phase 1)
- Database model serialization (Phase 2)
- restic output parsing (Phase 3)
- Schedule calculation (Phase 4)
- API request/response models (Phase 5)

### Integration Tests

- Database queries with test DB (Phase 2)
- Full backup execution with mock restic (Phase 3)
- Scheduler with mock time (Phase 4)
- API endpoints with test server (Phase 5)

### Manual Testing

- End-to-end on Linux (all phases)
- End-to-end on Windows (Phase 7)
- Network failure scenarios (Phase 6)
- DB unavailability (Phase 6)

---

## Implementation Order Rationale

1. **Phase 1**: Foundation needed for everything else
2. **Phase 2**: Database is the source of truth; needed before any logic
3. **Phase 3**: Core functionality (backup execution)
4. **Phase 4**: Automation (scheduler)
5. **Phase 5**: User interface and monitoring
6. **Phase 6**: Dynamic behavior (config reload)
7. **Phase 7**: Production readiness and observability

Each phase builds on previous phases and can be tested independently.

---

## Timeline Estimate

Assuming focused development:

- **Phase 1**: 2-3 hours
- **Phase 2**: 4-5 hours
- **Phase 3**: 3-4 hours
- **Phase 4**: 5-6 hours
- **Phase 5**: 4-5 hours
- **Phase 6**: 2-3 hours
- **Phase 7**: 3-4 hours

**Total**: ~25-30 hours (3-4 days of focused work)

---

## Dependencies Between Phases

```
Phase 1 (Foundation)
    ↓
Phase 2 (Database) ←──┐
    ↓                 │
Phase 3 (Backup)      │
    ↓                 │
Phase 4 (Scheduler)   │
    ↓                 │
Phase 5 (API/UI)      │
    ↓                 │
Phase 6 (Sync) ───────┘
    ↓
Phase 7 (Polish)
```

Phases 1-5 are sequential; Phase 6 integrates with Phase 2; Phase 7 is final polish.

---

## Success Criteria

The implementation is complete when:

1. ✅ Client runs on both Linux and Windows
2. ✅ Can load configuration from PostgreSQL
3. ✅ Executes scheduled backups using restic
4. ✅ Detects and executes missed runs
5. ✅ Provides web UI for monitoring and control
6. ✅ Updates configuration dynamically from DB
7. ✅ Handles network/DB failures gracefully
8. ✅ Pushes metrics to Prometheus (optional but working)
9. ✅ Production-ready error handling (no unwrap())
10. ✅ Complete documentation for deployment

---

## Post-Implementation Enhancements (Future)

Not in current scope, but potential future work:

- **Authentication**: Add auth to web UI
- **Retention policies**: Client-side retention management
- **Email notifications**: Alert on backup failures
- **Web dashboard**: Separate web app for multi-device monitoring
- **Backup verification**: Periodic `restic check` runs
- **Bandwidth limiting**: Rate limit backup uploads
- **Remote log shipping**: Send logs to central collector
- **Prometheus exporter**: Metrics pull endpoint (alternative to push)
- **Docker image**: Containerized client for easy deployment
