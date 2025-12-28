use crate::error::{ConfigError, Result};
use serde::{Deserialize, Serialize};
use std::path::Path;

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct LocalConfig {
    pub device: DeviceConfig,
    pub database: DatabaseConfig,
    pub client: ClientConfig,
    #[serde(default)]
    pub metrics: MetricsConfig,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct DeviceConfig {
    pub id: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct DatabaseConfig {
    pub host: String,
    pub port: u16,
    pub user: String,
    pub password: String,
    #[serde(default = "default_ssl_mode")]
    pub ssl_mode: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ClientConfig {
    #[serde(default = "default_http_bind")]
    pub http_bind: String,
    pub log_file: String,
}

#[derive(Debug, Clone, Serialize, Deserialize, Default)]
pub struct MetricsConfig {
    #[serde(default)]
    pub enabled: bool,
    #[serde(default)]
    pub prometheus_pushgateway: Option<String>,
}

fn default_ssl_mode() -> String {
    "require".to_string()
}

fn default_http_bind() -> String {
    "127.0.0.1:1201".to_string()
}

impl LocalConfig {
    pub fn from_file<P: AsRef<Path>>(path: P) -> Result<Self> {
        let content = std::fs::read_to_string(path.as_ref())
            .map_err(|e| ConfigError::LoadFailed(format!("Failed to read config file: {}", e)))?;

        let config: LocalConfig = serde_yaml::from_str(&content)
            .map_err(|e| ConfigError::ParseFailed(format!("Failed to parse YAML: {}", e)))?;

        config.validate()?;
        Ok(config)
    }

    fn validate(&self) -> Result<()> {
        if self.device.id.is_empty() {
            return Err(
                ConfigError::ValidationFailed("device.id cannot be empty".to_string()).into(),
            );
        }

        if self.database.host.is_empty() {
            return Err(
                ConfigError::ValidationFailed("database.host cannot be empty".to_string()).into(),
            );
        }

        if self.database.user.is_empty() {
            return Err(
                ConfigError::ValidationFailed("database.user cannot be empty".to_string()).into(),
            );
        }

        if self.client.log_file.is_empty() {
            return Err(ConfigError::ValidationFailed(
                "client.log_file cannot be empty".to_string(),
            )
            .into());
        }

        Ok(())
    }

    #[allow(dead_code)]
    pub fn database_url(&self) -> String {
        format!(
            "postgresql://{}:{}@{}:{}/{}?sslmode={}",
            self.database.user,
            self.database.password,
            self.database.host,
            self.database.port,
            self.database.user,
            self.database.ssl_mode
        )
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_database_url_generation() {
        let config = LocalConfig {
            device: DeviceConfig {
                id: "test-device".to_string(),
            },
            database: DatabaseConfig {
                host: "localhost".to_string(),
                port: 5432,
                user: "backup_user".to_string(),
                password: "secret".to_string(),
                ssl_mode: "require".to_string(),
            },
            client: ClientConfig {
                http_bind: "127.0.0.1:1201".to_string(),
                log_file: "/var/log/rbackup2.log".to_string(),
            },
            metrics: MetricsConfig {
                enabled: false,
                prometheus_pushgateway: None,
            },
        };

        let url = config.database_url();
        assert_eq!(
            url,
            "postgresql://backup_user:secret@localhost:5432/backup_user?sslmode=require"
        );
    }

    #[test]
    fn test_validation_empty_device_id() {
        let config = LocalConfig {
            device: DeviceConfig { id: "".to_string() },
            database: DatabaseConfig {
                host: "localhost".to_string(),
                port: 5432,
                user: "backup_user".to_string(),
                password: "secret".to_string(),
                ssl_mode: "require".to_string(),
            },
            client: ClientConfig {
                http_bind: "127.0.0.1:1201".to_string(),
                log_file: "/var/log/rbackup2.log".to_string(),
            },
            metrics: MetricsConfig {
                enabled: false,
                prometheus_pushgateway: None,
            },
        };

        assert!(config.validate().is_err());
    }
}
