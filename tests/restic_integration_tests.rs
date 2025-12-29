use chrono::Utc;
use rbackup2::backup::output::parse_restic_json_output;
use rbackup2::backup::restic::ResticCommand;
use rbackup2::config::remote::RemoteConfig;
use rbackup2::db::models::BackupJob;
use std::collections::HashMap;
use std::env;
use std::fs;
use std::path::PathBuf;
use std::process::Command;
use std::sync::Once;
use tempfile::TempDir;
use uuid::Uuid;

#[cfg(unix)]
use std::os::unix::fs::PermissionsExt;

#[cfg(windows)]
use std::os::windows::fs as windows_fs;

static INIT: Once = Once::new();

fn get_restic_binary_name() -> &'static str {
    if cfg!(target_os = "windows") {
        "restic-windows.exe"
    } else {
        "restic-linux"
    }
}

fn get_restic_binary_path() -> PathBuf {
    let manifest_dir = env::var("CARGO_MANIFEST_DIR").expect("CARGO_MANIFEST_DIR not set");
    let binary_name = get_restic_binary_name();
    PathBuf::from(manifest_dir)
        .join("testdata/restic")
        .join(binary_name)
}

fn setup_restic_in_path() {
    INIT.call_once(|| {
        let manifest_dir = env::var("CARGO_MANIFEST_DIR").expect("CARGO_MANIFEST_DIR not set");
        let restic_dir = PathBuf::from(&manifest_dir).join("testdata/restic");

        let source_binary = get_restic_binary_path();
        let target_name = if cfg!(target_os = "windows") {
            "restic.exe"
        } else {
            "restic"
        };
        let target_path = restic_dir.join(target_name);

        if !target_path.exists() {
            #[cfg(unix)]
            {
                std::os::unix::fs::symlink(&source_binary, &target_path)
                    .expect("Failed to create symlink for restic binary");
            }

            #[cfg(windows)]
            {
                windows_fs::symlink_file(&source_binary, &target_path)
                    .expect("Failed to create symlink for restic binary");
            }
        }

        let current_path = env::var("PATH").unwrap_or_default();
        let new_path = if current_path.is_empty() {
            restic_dir.to_str().unwrap().to_string()
        } else {
            format!("{}:{}", restic_dir.to_str().unwrap(), current_path)
        };

        env::set_var("PATH", new_path);
    });
}

fn create_test_config(repo_path: &str, password: &str) -> RemoteConfig {
    let mut settings = HashMap::new();
    settings.insert("repository_url".to_string(), repo_path.to_string());
    settings.insert("repository_password".to_string(), password.to_string());

    RemoteConfig {
        jobs: vec![],
        schedules: vec![],
        settings,
    }
}

fn create_test_job(source_paths: Vec<String>) -> BackupJob {
    BackupJob {
        id: Uuid::new_v4(),
        device_id: "test-device".to_string(),
        name: "test-backup".to_string(),
        description: Some("Test backup job".to_string()),
        source_paths,
        exclude_patterns: None,
        tags: Some(vec!["test".to_string()]),
        restic_args: serde_json::json!([]),
        enabled: true,
        created_at: Utc::now(),
        updated_at: Utc::now(),
        metadata: serde_json::json!({}),
        origin_name: Some("test-origin".to_string()),
        origin_id: None,
        account_id: None,
    }
}

fn init_restic_repo(repo_path: &str, password: &str) -> Result<(), Box<dyn std::error::Error>> {
    let restic_binary = get_restic_binary_path();
    assert!(
        restic_binary.exists(),
        "Restic binary not found at {:?}",
        restic_binary
    );

    let output = Command::new(&restic_binary)
        .env("RESTIC_REPOSITORY", repo_path)
        .env("RESTIC_PASSWORD", password)
        .arg("init")
        .output()?;

    if !output.status.success() {
        let stderr = String::from_utf8_lossy(&output.stderr);
        return Err(format!("Failed to init restic repo: {}", stderr).into());
    }

    Ok(())
}

#[test]
fn test_restic_binary_exists() {
    let restic_binary = get_restic_binary_path();
    let binary_name = get_restic_binary_name();

    assert!(
        restic_binary.exists(),
        "Restic binary '{}' not found at {:?}. Please ensure the platform-specific binary exists.",
        binary_name,
        restic_binary
    );
    assert!(
        restic_binary.is_file(),
        "Restic path is not a file: {:?}",
        restic_binary
    );

    let metadata = fs::metadata(&restic_binary).expect("Failed to get metadata");

    if cfg!(unix) {
        assert!(
            metadata.permissions().mode() & 0o111 != 0,
            "Restic binary is not executable"
        );
    }
}

#[test]
fn test_restic_version() {
    let restic_binary = get_restic_binary_path();

    let output = Command::new(&restic_binary)
        .arg("version")
        .output()
        .expect("Failed to execute restic version");

    assert!(output.status.success(), "Restic version command failed");

    let stdout = String::from_utf8_lossy(&output.stdout);
    assert!(
        stdout.contains("restic"),
        "Restic version output doesn't contain 'restic'"
    );
}

#[test]
fn test_command_builder_basic() {
    setup_restic_in_path();

    let temp_dir = TempDir::new().expect("Failed to create temp dir");
    let repo_path = temp_dir.path().join("test-repo");
    let config = create_test_config(repo_path.to_str().unwrap(), "test-password");

    let restic_cmd = ResticCommand::new(&config).expect("Failed to create ResticCommand");

    let job = create_test_job(vec![temp_dir.path().to_str().unwrap().to_string()]);
    let command = restic_cmd.build_backup_command(&job);

    let program = command.as_std().get_program().to_str().unwrap();
    assert!(
        program.ends_with("restic") || program.ends_with("restic.exe"),
        "Command should use restic binary"
    );
}

#[test]
fn test_command_builder_with_excludes() {
    setup_restic_in_path();

    let temp_dir = TempDir::new().expect("Failed to create temp dir");
    let repo_path = temp_dir.path().join("test-repo");
    let config = create_test_config(repo_path.to_str().unwrap(), "test-password");

    let restic_cmd = ResticCommand::new(&config).expect("Failed to create ResticCommand");

    let mut job = create_test_job(vec![temp_dir.path().to_str().unwrap().to_string()]);
    job.exclude_patterns = Some(vec!["*.tmp".to_string(), "*.log".to_string()]);

    let _command = restic_cmd.build_backup_command(&job);
}

#[tokio::test]
async fn test_restic_init_and_backup() {
    setup_restic_in_path();

    let temp_dir = TempDir::new().expect("Failed to create temp dir");
    let repo_path = temp_dir.path().join("test-repo");
    let source_dir = temp_dir.path().join("source");
    fs::create_dir_all(&source_dir).expect("Failed to create source dir");

    fs::write(source_dir.join("test1.txt"), "test content 1").expect("Failed to write test file 1");
    fs::write(source_dir.join("test2.txt"), "test content 2").expect("Failed to write test file 2");

    let password = "test-password-123";

    init_restic_repo(repo_path.to_str().unwrap(), password)
        .expect("Failed to initialize restic repository");

    let config = create_test_config(repo_path.to_str().unwrap(), password);
    let restic_cmd = ResticCommand::new(&config).expect("Failed to create ResticCommand");

    let job = create_test_job(vec![source_dir.to_str().unwrap().to_string()]);
    let mut command = restic_cmd.build_backup_command(&job);

    let output = command
        .output()
        .await
        .expect("Failed to execute restic backup");

    assert!(
        output.status.success(),
        "Restic backup failed: {}",
        String::from_utf8_lossy(&output.stderr)
    );

    let stdout = String::from_utf8_lossy(&output.stdout);
    assert!(!stdout.is_empty(), "Restic backup produced no output");
}

#[tokio::test]
async fn test_restic_backup_with_json_output() {
    setup_restic_in_path();

    let temp_dir = TempDir::new().expect("Failed to create temp dir");
    let repo_path = temp_dir.path().join("test-repo");
    let source_dir = temp_dir.path().join("source");
    fs::create_dir_all(&source_dir).expect("Failed to create source dir");

    fs::write(source_dir.join("file1.txt"), "content 1").expect("Failed to write test file");
    fs::write(source_dir.join("file2.txt"), "content 2").expect("Failed to write test file");

    let password = "test-password-456";

    init_restic_repo(repo_path.to_str().unwrap(), password)
        .expect("Failed to initialize restic repository");

    let config = create_test_config(repo_path.to_str().unwrap(), password);
    let restic_cmd = ResticCommand::new(&config).expect("Failed to create ResticCommand");

    let job = create_test_job(vec![source_dir.to_str().unwrap().to_string()]);
    let mut command = restic_cmd.build_backup_command(&job);

    let output = command
        .output()
        .await
        .expect("Failed to execute restic backup");

    assert!(
        output.status.success(),
        "Restic backup failed: {}",
        String::from_utf8_lossy(&output.stderr)
    );

    let stdout = String::from_utf8_lossy(&output.stdout);

    let stats = parse_restic_json_output(&stdout).expect("Failed to parse restic JSON output");

    assert!(
        !stats.snapshot_id.is_empty(),
        "Snapshot ID should not be empty"
    );
    assert!(stats.files_new >= 2, "Should have at least 2 new files");
    assert!(stats.data_added_bytes > 0, "Should have added some data");
}

#[tokio::test]
async fn test_restic_backup_with_tags() {
    setup_restic_in_path();

    let temp_dir = TempDir::new().expect("Failed to create temp dir");
    let repo_path = temp_dir.path().join("test-repo");
    let source_dir = temp_dir.path().join("source");
    fs::create_dir_all(&source_dir).expect("Failed to create source dir");

    fs::write(source_dir.join("tagged-file.txt"), "tagged content")
        .expect("Failed to write test file");

    let password = "test-password-789";

    init_restic_repo(repo_path.to_str().unwrap(), password)
        .expect("Failed to initialize restic repository");

    let config = create_test_config(repo_path.to_str().unwrap(), password);
    let restic_cmd = ResticCommand::new(&config).expect("Failed to create ResticCommand");

    let mut job = create_test_job(vec![source_dir.to_str().unwrap().to_string()]);
    job.tags = Some(vec![
        "integration-test".to_string(),
        "automated".to_string(),
    ]);

    let mut command = restic_cmd.build_backup_command(&job);

    let output = command
        .output()
        .await
        .expect("Failed to execute restic backup");

    assert!(
        output.status.success(),
        "Restic backup with tags failed: {}",
        String::from_utf8_lossy(&output.stderr)
    );

    let restic_binary = get_restic_binary_path();
    let snapshots_output = Command::new(&restic_binary)
        .env("RESTIC_REPOSITORY", repo_path.to_str().unwrap())
        .env("RESTIC_PASSWORD", password)
        .arg("snapshots")
        .arg("--json")
        .output()
        .expect("Failed to list snapshots");

    let snapshots_json = String::from_utf8_lossy(&snapshots_output.stdout);
    assert!(
        snapshots_json.contains("integration-test"),
        "Snapshots should contain integration-test tag"
    );
}

#[tokio::test]
async fn test_restic_incremental_backup() {
    setup_restic_in_path();

    let temp_dir = TempDir::new().expect("Failed to create temp dir");
    let repo_path = temp_dir.path().join("test-repo");
    let source_dir = temp_dir.path().join("source");
    fs::create_dir_all(&source_dir).expect("Failed to create source dir");

    fs::write(source_dir.join("file1.txt"), "initial content").expect("Failed to write test file");

    let password = "test-password-incr";

    init_restic_repo(repo_path.to_str().unwrap(), password)
        .expect("Failed to initialize restic repository");

    let config = create_test_config(repo_path.to_str().unwrap(), password);
    let restic_cmd = ResticCommand::new(&config).expect("Failed to create ResticCommand");

    let job = create_test_job(vec![source_dir.to_str().unwrap().to_string()]);

    let mut command1 = restic_cmd.build_backup_command(&job);
    let output1 = command1
        .output()
        .await
        .expect("Failed to execute first backup");

    assert!(output1.status.success(), "First backup failed");

    let stdout1 = String::from_utf8_lossy(&output1.stdout);
    let stats1 = parse_restic_json_output(&stdout1).expect("Failed to parse first backup output");

    assert!(stats1.files_new >= 1, "First backup should have new files");

    fs::write(source_dir.join("file2.txt"), "new file content")
        .expect("Failed to write new test file");

    let mut command2 = restic_cmd.build_backup_command(&job);
    let output2 = command2
        .output()
        .await
        .expect("Failed to execute second backup");

    assert!(output2.status.success(), "Second backup failed");

    let stdout2 = String::from_utf8_lossy(&output2.stdout);
    let stats2 = parse_restic_json_output(&stdout2).expect("Failed to parse second backup output");

    assert!(
        stats2.files_unmodified >= 1,
        "Second backup should have unmodified files from first backup"
    );
    assert!(
        stats2.files_new >= 1,
        "Second backup should have the new file"
    );
    assert_ne!(
        stats1.snapshot_id, stats2.snapshot_id,
        "Snapshots should have different IDs"
    );
}
