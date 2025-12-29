# rbackup2

A multiplatform backup client using [restic](https://restic.net/) with centralized configuration and state management
via PostgreSQL.

## Overview

**rbackup2** is a Rust-based backup orchestration client designed to automate and manage backups across multiple devices
using restic as the backup engine. It follows a client-server control plane architecture where configuration, schedules,
and execution history are stored in a remote PostgreSQL database, while the client handles autonomous backup execution.

This project is designed as a replacement for Relica Backup and maintains compatibility with existing Relica restic
repositories.

**IMPORTANT NOTE**: This project is currently in development and is not yet ready for production use. Also, it's 99%
vibe-coded 🙈

### Key Features

- **Autonomous Operation**: Self-scheduling client that executes backups independently after startup
- **Centralized Configuration**: All backup jobs, schedules, and settings stored in PostgreSQL
- **Missed Run Detection**: Automatically catches up on missed backups when offline
- **Local Web UI**: Monitor status and trigger backups via browser at `http://127.0.0.1:1201`
- **Multiplatform**: Single codebase runs on Windows and Linux
- **Resilient**: Continues operation with last-known-good configuration if database is temporarily unavailable
- **Optional Metrics**: Push metrics to Prometheus Pushgateway
- **Production-Ready**: No `unwrap()` in production code, comprehensive error handling

## Architecture

```
┌──────────────────────────────┐
│       PostgreSQL Database     │
│  (Remote - Source of Truth)   │
│  - devices                    │
│  - backup_jobs                │
│  - schedules                  │
│  - runs (history)             │
│  - settings                   │
└──────────────▲───────────────┘
               │
     periodic sync / updates
               │
┌──────────────┴───────────────┐
│         Rust Client           │
│  - tokio async runtime        │
│  - restic executor            │
│  - cron/interval scheduler    │
│  - axum HTTP server           │
│  - local Web UI               │
│  - Prometheus push (optional) │
└──────────────────────────────┘
```

The client loads configuration from the database into memory, schedules backup jobs, executes them using restic, and
writes execution results back to the database.

## Prerequisites

- **Rust** (stable toolchain) - for building from source
- **PostgreSQL** (remote database) - for configuration and state storage
- **restic** - backup engine (must be installed and accessible in PATH)

## Quick Start

### 1. Build

```bash
cargo build --release
```

### 2. Configure

Create a local configuration file (e.g., `config.yaml`):

```yaml
device:
  id: "my-workstation"

database:
  host: "db.example.com"
  port: 5432
  user: "backup_client"
  password: "your_password"
  ssl_mode: "require"

client:
  http_bind: "127.0.0.1:1201"
  log_file: "/var/log/rbackup2.log"

metrics:
  enabled: false
```

See `config.example.yaml` for a complete example with all options.

### 3. Set Up Database

Create the PostgreSQL database and run migrations (details in `doc/01-database-schema.md`).

### 4. Run

```bash
./target/release/rbackup2 --config config.yaml
```

### 5. Access Web UI

Open your browser to `http://127.0.0.1:1201` to monitor backup status and trigger manual backups.

## Documentation

Comprehensive documentation is available in the `doc/` directory:

- **[00-architecture-overview.md](doc/00-architecture-overview.md)** - System design and architecture
- **[01-database-schema.md](doc/01-database-schema.md)** - PostgreSQL schema and queries
- **[02-implementation-phases.md](doc/02-implementation-phases.md)** - Development roadmap and phases
- **[03-relica-compatibility.md](doc/03-relica-compatibility.md)** - Migration guide from Relica Backup

## Project Status

**Current Phase**: Phase 4 Complete (Scheduler)

- ✅ Phase 1: Project Foundation & Configuration
- ✅ Phase 2: Database Layer
- ✅ Phase 3: Backup Executor (restic Integration)
- ✅ Phase 4: Scheduler
- ⏳ Phase 5: HTTP API & Web UI
- ⏳ Phase 6: Configuration Reload & Periodic Sync
- ⏳ Phase 7: Metrics & Polish

See `doc/02-implementation-phases.md` for the complete implementation plan.

## Technology Stack

- **Language**: Rust (stable)
- **Async Runtime**: tokio
- **HTTP Server**: axum
- **Database**: sqlx with PostgreSQL
- **Configuration**: YAML (serde_yaml)
- **Logging**: tracing + tracing-subscriber
- **Scheduling**: cron expressions + intervals

## Development

### Running Tests

The project includes comprehensive unit and integration tests:

```bash
# Run all tests
cargo test

# Run only unit tests
cargo test --lib

# Run only integration tests
cargo test --test '*'
```

The repository includes platform-specific restic binaries (`testdata/restic/restic-linux` and `testdata/restic/restic-windows.exe`) used for integration testing. These binaries are automatically used by the test suite and do not need to be installed separately for testing purposes. The test suite automatically detects the platform and uses the appropriate binary.

### Building for Production

```bash
# Linux
cargo build --release --target x86_64-unknown-linux-gnu

# Windows (from Linux with cross)
cargo build --release --target x86_64-pc-windows-gnu
```

## Deployment

See `doc/00-architecture-overview.md` for deployment instructions, including running as a systemd service (Linux) or
Windows Service.

## License

Licensed under the Apache License, Version 2.0. See [LICENSE](LICENSE) for details.

## Contributing

This is currently a personal project. If you're interested in contributing or have questions, please open an issue.

## Relica Compatibility

This project is designed as a drop-in replacement for Relica Backup. It maintains compatibility with existing Relica
restic repositories through:

- UUID-based backup job identifiers
- Relica-compatible restic snapshot tagging
- Support for existing repository structures

See `doc/03-relica-compatibility.md` for migration details.
