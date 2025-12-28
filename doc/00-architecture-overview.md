# Architecture Overview

## System Design

**Note**: This system is designed as a replacement for Relica Backup and maintains compatibility with existing Relica
restic repositories. See `doc/03-relica-compatibility.md` for migration details.

### High-Level Components

```
┌─────────────────────────────────────────┐
│        PostgreSQL Database              │
│  (Remote - Single Source of Truth)      │
│                                          │
│  Tables:                                 │
│  - devices                               │
│  - backup_jobs                           │
│  - schedules                             │
│  - runs (execution history)              │
│  - settings (includes repository config) │
└──────────────▲──────────────────────────┘
               │
               │ sqlx (async)
               │ periodic sync
               │
┌──────────────┴──────────────────────────┐
│     Rust Client (Multiplatform)         │
│                                          │
│  ┌────────────────────────────────────┐ │
│  │   Configuration Layer              │ │
│  │  - Bootstrap from local YAML       │ │
│  │  - In-memory config cache          │ │
│  │  - Periodic DB sync                │ │
│  └────────────────────────────────────┘ │
│                                          │
│  ┌────────────────────────────────────┐ │
│  │   Scheduler                        │ │
│  │  - Cron/interval parsing           │ │
│  │  - Missed run detection            │ │
│  │  - Job queue management            │ │
│  └────────────────────────────────────┘ │
│                                          │
│  ┌────────────────────────────────────┐ │
│  │   Backup Executor                  │ │
│  │  - restic command builder          │ │
│  │  - Process management              │ │
│  │  - Output capture                  │ │
│  │  - Error handling                  │ │
│  └────────────────────────────────────┘ │
│                                          │
│  ┌────────────────────────────────────┐ │
│  │   HTTP API (axum)                  │ │
│  │  - REST endpoints                  │ │
│  │  - JSON responses                  │ │
│  │  - Static file serving             │ │
│  └────────────────────────────────────┘ │
│                                          │
│  ┌────────────────────────────────────┐ │
│  │   Metrics Reporter (Optional)      │ │
│  │  - Prometheus Pushgateway client   │ │
│  └────────────────────────────────────┘ │
│                                          │
│  ┌────────────────────────────────────┐ │
│  │   Logging (tracing)                │ │
│  │  - File output                     │ │
│  │  - Structured logs                 │ │
│  └────────────────────────────────────┘ │
└─────────────────────────────────────────┘
               │
               ▼
         restic binary
      (external process)
```

## Core Principles

### 1. Database as Source of Truth

- All configuration, schedules, and state in PostgreSQL
- Client operates on in-memory cache
- Periodic sync keeps client up-to-date
- Database writes capture execution results

### 2. Resilience

- Client continues with last-known-good config if DB unavailable
- No unwrap() in production code
- Graceful error handling throughout
- Retry logic for transient failures

### 3. Autonomous Operation

- Client self-schedules after startup
- Detects and executes missed runs
- No external orchestration needed
- Local web UI for monitoring and control

### 4. Multiplatform

- Single codebase for Windows and Linux
- Platform-specific paths handled gracefully
- restic binary location configurable

## Technology Stack

### Core Runtime

- **Language**: Rust (stable)
- **Async Runtime**: tokio
- **HTTP Server**: axum
- **Database**: sqlx (PostgreSQL, async, compile-time checked)

### Configuration & Serialization

- **Config Format**: YAML (local bootstrap only)
- **Serialization**: serde, serde_yaml, serde_json

### Observability

- **Logging**: tracing + tracing-subscriber (file output)
- **Metrics**: Prometheus client + Pushgateway integration (optional)

### Scheduling

- **Cron Parsing**: cron library
- **Time Handling**: chrono

### Additional

- **CLI Parsing**: clap (for client arguments)
- **Environment Variables**: dotenvy (optional, for overrides)

## Data Flow

### Startup Sequence

1. Load local YAML config (device_id, DB connection, HTTP bind)
2. Connect to PostgreSQL
3. Load device-specific configuration into memory:
    - Backup jobs
    - Schedules
    - Global settings (including shared repository config)
    - Last run timestamps
4. Initialize scheduler with loaded schedules
5. Start HTTP server
6. Begin periodic config refresh loop
7. Start metrics reporter (if enabled)

### Backup Execution Flow

1. Scheduler triggers job (or manual trigger via API)
2. Executor looks up job definition and shared repository config (from settings)
3. Build restic command with repository URL and password
4. Execute restic as subprocess
5. Capture stdout/stderr
6. Parse restic output for statistics
7. Write run record to database:
    - start_time, end_time
    - status (success/failure)
    - files_new, files_changed, data_added
    - error messages
8. Update last_run timestamp in database
9. Push metrics (if enabled)

### Configuration Update Flow

1. User edits config via web UI
2. POST /config endpoint receives changes
3. Client writes changes to PostgreSQL
4. Immediate config reload from database
5. Scheduler updated with new schedules (if changed)
6. UI reflects updated state

### Periodic Sync

- Every N minutes (configurable, default: 5)
- Reload all device-specific config from DB
- Compare with in-memory state
- Apply changes:
    - New/updated jobs → update scheduler
    - Removed jobs → cancel scheduled runs
    - Repository settings changes → update executor config

## Module Structure

```
rbackup2/
├── src/
│   ├── main.rs                    # Entry point, tokio runtime setup
│   ├── lib.rs                     # Library root, declares all modules
│   ├── config.rs                  # Config module (or config/ directory with lib.rs)
│   ├── config/
│   │   ├── local.rs               # Local YAML config parsing
│   │   └── remote.rs              # DB config loading
│   ├── db.rs                      # Database module
│   ├── db/
│   │   ├── schema.rs              # sqlx migrations/schema
│   │   ├── models.rs              # Database model structs
│   │   └── queries.rs             # Database query functions
│   ├── scheduler.rs               # Scheduler module
│   ├── scheduler/
│   │   ├── cron.rs                # Cron expression handling
│   │   ├── missed_runs.rs         # Missed run detection
│   │   └── executor.rs            # Job execution orchestration
│   ├── backup.rs                  # Backup module
│   ├── backup/
│   │   ├── restic.rs              # restic command builder/executor
│   │   └── output.rs              # restic output parsing
│   ├── api.rs                     # API module
│   ├── api/
│   │   ├── server.rs              # axum server setup
│   │   ├── handlers.rs            # Request handlers
│   │   └── models.rs              # API request/response models
│   ├── metrics.rs                 # Metrics module
│   ├── metrics/
│   │   └── pushgateway.rs         # Prometheus Pushgateway client
│   ├── ui/
│   │   └── static/                # Static web UI files
│   │       ├── index.html
│   │       ├── styles.css
│   │       └── app.js
│   └── error.rs                   # Error types
├── migrations/                    # sqlx database migrations
│   └── 001_initial_schema.sql
├── doc/                           # Documentation
├── config.example.yaml            # Example local config
└── Cargo.toml
```

**Note**: Each module directory (e.g., `config/`, `db/`) has a corresponding `.rs` file (e.g., `config.rs`, `db.rs`)
that declares the module and its submodules. This follows the modern Rust module system without `mod.rs` files.

## Error Handling Strategy

### Error Types Hierarchy

```rust
pub enum AppError {
    Config(ConfigError),
    Database(DatabaseError),
    Backup(BackupError),
    Scheduler(SchedulerError),
    Api(ApiError),
}

pub enum ConfigError {
    LoadFailed(String),
    ParseFailed(String),
    ValidationFailed(String),
}

pub enum DatabaseError {
    ConnectionFailed(sqlx::Error),
    QueryFailed(sqlx::Error),
    MigrationFailed(sqlx::Error),
}

pub enum BackupError {
    ResticNotFound,
    ExecutionFailed(String),
    OutputParseFailed(String),
}

// ... etc
```

### Recovery Strategies

- **DB unavailable**: Use last-known-good config, log warning, retry periodically
- **restic failure**: Log error, write failed run to DB (when DB available), continue with next scheduled job
- **Metrics push failure**: Log warning, never block backup execution
- **API handler error**: Return 500 with error details, log full error

## Security Considerations

### Local Deployment Scope

- No authentication on local UI (runs on 127.0.0.1)
- Database credentials in local YAML (file permissions: 600)
- Repository passwords stored in database (as env var names or file paths), cached in-memory on client
- SSL/TLS for database connection (configurable)

### Future Enhancements (Out of Scope)

- Authentication/authorization for web UI
- Encrypted local config
- Vault integration for secrets

## Performance Characteristics

### Expected Load

- Single device client (not a server)
- 1-10 backup jobs per device
- Backup frequency: hourly to daily
- HTTP API: low traffic (manual UI interactions only)
- Database queries: lightweight, infrequent (sync every 5 min)

### Resource Usage

- Memory: ~10-50 MB (config cache, scheduler state)
- CPU: idle most of time, spike during restic execution
- Disk I/O: minimal (logs only, restic handles backup I/O)
- Network: periodic DB sync, restic SFTP traffic

### Concurrency

- Single backup execution at a time (per device)
- Queue subsequent jobs if one is running
- Async I/O for HTTP API and DB queries
- Blocking restic subprocess execution (wrapped in tokio::task::spawn_blocking)

## Testing Strategy

### Unit Tests

- Config parsing and validation
- Cron expression parsing
- restic command builder
- API request/response serialization

### Integration Tests

- Database queries with test database
- Scheduler with mock time
- restic executor with mock binary
- HTTP API with test client

### Manual Testing

- Full end-to-end on Linux and Windows
- Network failure scenarios
- Database unavailability
- restic binary failures

## Build & Deployment

### Prerequisites

- Rust toolchain (stable)
- PostgreSQL database (remote)
- restic binary installed on client machine

### Build Process

```bash
# Development build
cargo build

# Release build
cargo build --release

# Run tests
cargo test

# Run with local config
cargo run -- --config /path/to/config.yaml
```

### Cross-Platform Builds

```bash
# Linux
cargo build --release --target x86_64-unknown-linux-gnu

# Windows (from Linux with cross)
cargo build --release --target x86_64-pc-windows-gnu
```

### Deployment

1. Copy binary to target machine
2. Create config.yaml with device-specific settings
3. Ensure restic binary in PATH or specify full path
4. Run as systemd service (Linux) or Windows Service
5. Access UI at http://127.0.0.1:1201
