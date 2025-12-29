use crate::config::remote::RemoteConfig;
use crate::db::models::BackupJob;
use crate::error::{AppError, BackupError, Result};
use std::collections::HashMap;
use std::path::PathBuf;
use std::process::Stdio;
use tokio::process::Command;
use tracing::debug;

pub struct ResticCommand {
    binary_path: PathBuf,
    repository_url: String,
    repository_password: String,
    cache_dir: Option<String>,
    environment: HashMap<String, String>,
}

impl ResticCommand {
    pub fn new(config: &RemoteConfig) -> Result<Self> {
        let repository_url = config
            .repository_url()
            .ok_or_else(|| BackupError::ConfigurationError("Repository URL not set".to_string()))?
            .clone();

        let repository_password = config
            .repository_password()
            .ok_or_else(|| {
                BackupError::ConfigurationError("Repository password not set".to_string())
            })?
            .clone();

        let cache_dir = config.repository_cache_dir().cloned();

        let binary_path = Self::find_restic_binary()?;

        Ok(Self {
            binary_path,
            repository_url,
            repository_password,
            cache_dir,
            environment: HashMap::new(),
        })
    }

    fn find_restic_binary() -> Result<PathBuf> {
        let binary_name = if cfg!(target_os = "windows") {
            "restic.exe"
        } else {
            "restic"
        };

        if let Ok(path) = which::which(binary_name) {
            debug!("Found restic binary at: {}", path.display());
            return Ok(path);
        }

        Err(AppError::Backup(BackupError::ResticNotFound(
            "restic binary not found in PATH. Please install restic: https://restic.net/"
                .to_string(),
        )))
    }

    pub fn build_backup_command(&self, job: &BackupJob) -> Command {
        let mut cmd = Command::new(&self.binary_path);

        cmd.env("RESTIC_REPOSITORY", &self.repository_url);
        cmd.env("RESTIC_PASSWORD", &self.repository_password);

        if let Some(cache_dir) = &self.cache_dir {
            if !cache_dir.is_empty() {
                cmd.env("RESTIC_CACHE_DIR", cache_dir);
            }
        }

        for (key, value) in &self.environment {
            cmd.env(key, value);
        }

        cmd.arg("backup");
        cmd.arg("--json");

        for path in &job.source_paths {
            cmd.arg(path);
        }

        if let Some(exclude_patterns) = &job.exclude_patterns {
            for pattern in exclude_patterns {
                cmd.arg("--exclude").arg(pattern);
            }
        }

        let tags = job.get_restic_tags();
        for tag in tags {
            cmd.arg("--tag").arg(tag);
        }

        if let Some(custom_tags) = &job.tags {
            for tag in custom_tags {
                cmd.arg("--tag").arg(tag);
            }
        }

        if let Some(args_array) = job.restic_args.as_array() {
            for arg in args_array {
                if let Some(arg_str) = arg.as_str() {
                    cmd.arg(arg_str);
                }
            }
        }

        cmd.stdout(Stdio::piped());
        cmd.stderr(Stdio::piped());

        cmd
    }

    #[allow(dead_code)]
    pub fn add_environment(&mut self, key: String, value: String) {
        self.environment.insert(key, value);
    }

    #[allow(dead_code)]
    pub async fn check_repository(&self) -> Result<()> {
        let mut cmd = Command::new(&self.binary_path);

        cmd.env("RESTIC_REPOSITORY", &self.repository_url);
        cmd.env("RESTIC_PASSWORD", &self.repository_password);

        if let Some(cache_dir) = &self.cache_dir {
            if !cache_dir.is_empty() {
                cmd.env("RESTIC_CACHE_DIR", cache_dir);
            }
        }

        cmd.arg("snapshots");
        cmd.arg("--json");
        cmd.arg("--last");

        cmd.stdout(Stdio::piped());
        cmd.stderr(Stdio::piped());

        let output = cmd.output().await.map_err(|e| {
            AppError::Backup(BackupError::ExecutionFailed(format!(
                "Failed to execute restic: {}",
                e
            )))
        })?;

        if !output.status.success() {
            let stderr = String::from_utf8_lossy(&output.stderr);
            return Err(AppError::Backup(BackupError::ExecutionFailed(format!(
                "Repository check failed: {}",
                stderr
            ))));
        }

        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::collections::HashMap;

    fn create_test_config() -> RemoteConfig {
        let mut settings = HashMap::new();
        settings.insert(
            "repository_url".to_string(),
            "sftp:user@host:/path".to_string(),
        );
        settings.insert("repository_password".to_string(), "secret".to_string());

        RemoteConfig {
            jobs: vec![],
            schedules: vec![],
            settings,
        }
    }

    #[test]
    fn test_restic_command_creation() {
        let config = create_test_config();
        let result = ResticCommand::new(&config);

        if which::which(if cfg!(target_os = "windows") {
            "restic.exe"
        } else {
            "restic"
        })
        .is_ok()
        {
            assert!(result.is_ok());
            let cmd = result.expect("Failed to create ResticCommand");
            assert_eq!(cmd.repository_url, "sftp:user@host:/path");
            assert_eq!(cmd.repository_password, "secret");
        } else {
            assert!(result.is_err());
        }
    }

    #[test]
    fn test_restic_command_with_cache_dir() {
        let mut settings = HashMap::new();
        settings.insert(
            "repository_url".to_string(),
            "sftp:user@host:/path".to_string(),
        );
        settings.insert("repository_password".to_string(), "secret".to_string());
        settings.insert("repository_cache_dir".to_string(), "/tmp/cache".to_string());

        let config = RemoteConfig {
            jobs: vec![],
            schedules: vec![],
            settings,
        };

        if which::which(if cfg!(target_os = "windows") {
            "restic.exe"
        } else {
            "restic"
        })
        .is_ok()
        {
            let result = ResticCommand::new(&config);
            assert!(result.is_ok());
            let cmd = result.expect("Failed to create ResticCommand");
            assert_eq!(cmd.cache_dir, Some("/tmp/cache".to_string()));
        }
    }
}
