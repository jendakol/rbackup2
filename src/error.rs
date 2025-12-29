use std::fmt;

#[derive(Debug)]
pub enum AppError {
    Config(ConfigError),
    Database(DatabaseError),
    Backup(BackupError),
    Scheduler(SchedulerError),
    Api(ApiError),
}

#[derive(Debug)]
#[allow(clippy::enum_variant_names)]
pub enum ConfigError {
    LoadFailed(String),
    ParseFailed(String),
    ValidationFailed(String),
}

#[derive(Debug)]
#[allow(clippy::enum_variant_names)]
#[allow(dead_code)]
pub enum DatabaseError {
    ConnectionFailed(sqlx::Error),
    QueryFailed(sqlx::Error),
    MigrationFailed(sqlx::Error),
}

#[derive(Debug)]
#[allow(dead_code)]
pub enum BackupError {
    ResticNotFound(String),
    ExecutionFailed(String),
    OutputParseFailed(String),
    ConfigurationError(String),
}

#[derive(Debug)]
#[allow(dead_code)]
pub enum SchedulerError {
    InvalidCronExpression(String),
    InvalidInterval(String),
    JobNotFound(String),
}

#[derive(Debug)]
#[allow(dead_code)]
pub enum ApiError {
    InvalidRequest(String),
    NotFound(String),
    InternalError(String),
}

impl fmt::Display for AppError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            AppError::Config(e) => write!(f, "Configuration error: {}", e),
            AppError::Database(e) => write!(f, "Database error: {}", e),
            AppError::Backup(e) => write!(f, "Backup error: {}", e),
            AppError::Scheduler(e) => write!(f, "Scheduler error: {}", e),
            AppError::Api(e) => write!(f, "API error: {}", e),
        }
    }
}

impl fmt::Display for ConfigError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            ConfigError::LoadFailed(msg) => write!(f, "Failed to load configuration: {}", msg),
            ConfigError::ParseFailed(msg) => write!(f, "Failed to parse configuration: {}", msg),
            ConfigError::ValidationFailed(msg) => {
                write!(f, "Configuration validation failed: {}", msg)
            }
        }
    }
}

impl fmt::Display for DatabaseError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            DatabaseError::ConnectionFailed(e) => write!(f, "Database connection failed: {}", e),
            DatabaseError::QueryFailed(e) => write!(f, "Database query failed: {}", e),
            DatabaseError::MigrationFailed(e) => write!(f, "Database migration failed: {}", e),
        }
    }
}

impl fmt::Display for BackupError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            BackupError::ResticNotFound(msg) => write!(f, "restic binary not found: {}", msg),
            BackupError::ExecutionFailed(msg) => write!(f, "Backup execution failed: {}", msg),
            BackupError::OutputParseFailed(msg) => {
                write!(f, "Failed to parse backup output: {}", msg)
            }
            BackupError::ConfigurationError(msg) => {
                write!(f, "Backup configuration error: {}", msg)
            }
        }
    }
}

impl fmt::Display for SchedulerError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            SchedulerError::InvalidCronExpression(msg) => {
                write!(f, "Invalid cron expression: {}", msg)
            }
            SchedulerError::InvalidInterval(msg) => write!(f, "Invalid interval: {}", msg),
            SchedulerError::JobNotFound(msg) => write!(f, "Job not found: {}", msg),
        }
    }
}

impl fmt::Display for ApiError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            ApiError::InvalidRequest(msg) => write!(f, "Invalid request: {}", msg),
            ApiError::NotFound(msg) => write!(f, "Not found: {}", msg),
            ApiError::InternalError(msg) => write!(f, "Internal error: {}", msg),
        }
    }
}

impl std::error::Error for AppError {}
impl std::error::Error for ConfigError {}
impl std::error::Error for DatabaseError {}
impl std::error::Error for BackupError {}
impl std::error::Error for SchedulerError {}
impl std::error::Error for ApiError {}

impl From<ConfigError> for AppError {
    fn from(err: ConfigError) -> Self {
        AppError::Config(err)
    }
}

impl From<DatabaseError> for AppError {
    fn from(err: DatabaseError) -> Self {
        AppError::Database(err)
    }
}

impl From<BackupError> for AppError {
    fn from(err: BackupError) -> Self {
        AppError::Backup(err)
    }
}

impl From<SchedulerError> for AppError {
    fn from(err: SchedulerError) -> Self {
        AppError::Scheduler(err)
    }
}

impl From<ApiError> for AppError {
    fn from(err: ApiError) -> Self {
        AppError::Api(err)
    }
}

impl From<sqlx::Error> for DatabaseError {
    fn from(err: sqlx::Error) -> Self {
        DatabaseError::QueryFailed(err)
    }
}

impl From<sqlx::Error> for AppError {
    fn from(err: sqlx::Error) -> Self {
        AppError::Database(DatabaseError::QueryFailed(err))
    }
}

impl From<std::io::Error> for ConfigError {
    fn from(err: std::io::Error) -> Self {
        ConfigError::LoadFailed(err.to_string())
    }
}

impl From<serde_yaml::Error> for ConfigError {
    fn from(err: serde_yaml::Error) -> Self {
        ConfigError::ParseFailed(err.to_string())
    }
}

pub type Result<T> = std::result::Result<T, AppError>;
