-- Insert default global settings

INSERT INTO settings (device_id, key, value, description)
VALUES (NULL, 'sync_interval_seconds', '300', 'How often clients sync config from database'),
       (NULL, 'max_concurrent_backups', '1', 'Maximum concurrent backup jobs per device'),
       (NULL, 'run_retention_days', '90', 'How long to keep run history in database'),
       (NULL, 'repository_url', '', 'Shared restic repository URL (e.g., sftp:user@host:/path/to/repo)'),
       (NULL, 'repository_password', '', 'Repository password for restic'),
       (NULL, 'repository_cache_dir', '', 'Restic cache directory (empty = restic default)');
