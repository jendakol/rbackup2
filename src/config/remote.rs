use crate::db::models::{BackupJob, Schedule};
use crate::error::Result;
use sqlx::PgPool;
use std::collections::HashMap;

#[derive(Debug, Clone)]
pub struct RemoteConfig {
    pub jobs: Vec<BackupJob>,
    pub schedules: Vec<Schedule>,
    pub settings: HashMap<String, String>,
}

impl RemoteConfig {
    pub fn get_setting(&self, key: &str) -> Option<&String> {
        self.settings.get(key)
    }

    #[allow(dead_code)]
    pub fn repository_url(&self) -> Option<&String> {
        self.get_setting("repository_url")
    }

    #[allow(dead_code)]
    pub fn repository_password(&self) -> Option<&String> {
        self.get_setting("repository_password")
    }

    #[allow(dead_code)]
    pub fn repository_cache_dir(&self) -> Option<&String> {
        self.get_setting("repository_cache_dir")
    }

    #[allow(dead_code)]
    pub fn sync_interval_seconds(&self) -> u64 {
        self.get_setting("sync_interval_seconds")
            .and_then(|s| s.parse().ok())
            .unwrap_or(300)
    }
}

pub async fn load_config_from_db(pool: &PgPool, device_id: String) -> Result<RemoteConfig> {
    let jobs = crate::db::get_jobs_for_device(pool, device_id.clone()).await?;
    let schedules = crate::db::get_schedules_for_device(pool, device_id.clone()).await?;
    let settings_vec = crate::db::get_settings_for_device(pool, device_id).await?;

    let settings: HashMap<String, String> =
        settings_vec.into_iter().map(|s| (s.key, s.value)).collect();

    Ok(RemoteConfig {
        jobs,
        schedules,
        settings,
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_remote_config_getters() {
        let mut settings = HashMap::new();
        settings.insert(
            "repository_url".to_string(),
            "sftp:user@host:/path".to_string(),
        );
        settings.insert("repository_password".to_string(), "secret".to_string());
        settings.insert("sync_interval_seconds".to_string(), "600".to_string());

        let config = RemoteConfig {
            jobs: vec![],
            schedules: vec![],
            settings,
        };

        assert_eq!(
            config.repository_url(),
            Some(&"sftp:user@host:/path".to_string())
        );
        assert_eq!(config.repository_password(), Some(&"secret".to_string()));
        assert_eq!(config.sync_interval_seconds(), 600);
    }

    #[test]
    fn test_remote_config_defaults() {
        let config = RemoteConfig {
            jobs: vec![],
            schedules: vec![],
            settings: HashMap::new(),
        };

        assert_eq!(config.repository_url(), None);
        assert_eq!(config.sync_interval_seconds(), 300);
    }
}
