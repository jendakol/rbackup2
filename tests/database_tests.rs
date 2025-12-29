use rbackup2::db::{
    create_pool, create_run, get_device, get_global_setting, get_job_by_id, get_jobs_for_device,
    get_recent_runs, get_schedules_for_device, get_settings_for_device, run_migrations,
    update_device_heartbeat, update_run, update_schedule_last_run, upsert_device,
};
use testcontainers::runners::AsyncRunner;
use testcontainers::ContainerAsync;
use testcontainers_modules::postgres::Postgres;

async fn setup_test_db() -> (ContainerAsync<Postgres>, sqlx::PgPool) {
    let container = Postgres::default()
        .start()
        .await
        .expect("Failed to start container");
    let port = container
        .get_host_port_ipv4(5432)
        .await
        .expect("Failed to get port");

    let connection_string = format!(
        "postgresql://postgres:postgres@localhost:{}/postgres?sslmode=disable",
        port
    );

    let pool = create_pool(connection_string)
        .await
        .expect("Failed to create pool");

    run_migrations(&pool)
        .await
        .expect("Failed to run migrations");

    (container, pool)
}

#[tokio::test]
async fn test_device_operations() {
    let (_container, pool) = setup_test_db().await;

    let device_id = "test-device-1".to_string();
    let device_name = "Test Device".to_string();
    let platform = "linux".to_string();
    let hostname = Some("test-host".to_string());

    let device = upsert_device(&pool, device_id.clone(), device_name, platform, hostname)
        .await
        .expect("Failed to upsert device");

    assert_eq!(device.id, device_id);
    assert_eq!(device.platform, "linux");
    assert!(device.enabled);

    let retrieved = get_device(&pool, device_id.clone())
        .await
        .expect("Failed to get device");

    assert!(retrieved.is_some());
    assert_eq!(retrieved.unwrap().id, device_id);

    update_device_heartbeat(
        &pool,
        device_id.clone(),
        Some("updated-host".to_string()),
        serde_json::json!({"test": "value"}),
    )
    .await
    .expect("Failed to update heartbeat");

    let updated = get_device(&pool, device_id)
        .await
        .expect("Failed to get updated device");

    assert!(updated.is_some());
    assert_eq!(updated.unwrap().hostname, Some("updated-host".to_string()));
}

#[tokio::test]
async fn test_backup_job_operations() {
    let (_container, pool) = setup_test_db().await;

    let device_id = "test-device-2".to_string();
    upsert_device(
        &pool,
        device_id.clone(),
        "Test Device".to_string(),
        "linux".to_string(),
        None,
    )
    .await
    .expect("Failed to create device");

    let job_id = uuid::Uuid::new_v4();
    sqlx::query(
        r#"
        INSERT INTO backup_jobs (id, device_id, name, source_paths, origin_name, account_id)
        VALUES ($1, $2, $3, $4, $5, $6)
        "#,
    )
    .bind(job_id)
    .bind(&device_id)
    .bind("test-device-2/home")
    .bind(vec!["/home/user"])
    .bind("test-device-2")
    .bind(uuid::Uuid::new_v4())
    .execute(&pool)
    .await
    .expect("Failed to insert job");

    let jobs = get_jobs_for_device(&pool, device_id.clone())
        .await
        .expect("Failed to get jobs");

    assert_eq!(jobs.len(), 1);
    assert_eq!(jobs[0].id, job_id);
    assert_eq!(jobs[0].name, "test-device-2/home");

    let job = get_job_by_id(&pool, job_id)
        .await
        .expect("Failed to get job by id");

    assert!(job.is_some());
    assert_eq!(job.unwrap().id, job_id);

    let tags = jobs[0].get_restic_tags();
    assert!(tags.iter().any(|t| t.starts_with("backup:")));
    assert!(tags.iter().any(|t| t == "backup_name=test-device-2/home"));
    assert!(tags.iter().any(|t| t == "origin=test-device-2"));
}

#[tokio::test]
async fn test_schedule_operations() {
    let (_container, pool) = setup_test_db().await;

    let device_id = "test-device-3".to_string();
    upsert_device(
        &pool,
        device_id.clone(),
        "Test Device".to_string(),
        "linux".to_string(),
        None,
    )
    .await
    .expect("Failed to create device");

    let job_id = uuid::Uuid::new_v4();
    sqlx::query(
        r#"
        INSERT INTO backup_jobs (id, device_id, name, source_paths)
        VALUES ($1, $2, $3, $4)
        "#,
    )
    .bind(job_id)
    .bind(&device_id)
    .bind("test-job")
    .bind(vec!["/data"])
    .execute(&pool)
    .await
    .expect("Failed to insert job");

    sqlx::query(
        r#"
        INSERT INTO schedules (job_id, schedule_type, cron_expression)
        VALUES ($1, 'cron', '0 2 * * *')
        "#,
    )
    .bind(job_id)
    .execute(&pool)
    .await
    .expect("Failed to insert schedule");

    let schedules = get_schedules_for_device(&pool, device_id)
        .await
        .expect("Failed to get schedules");

    assert_eq!(schedules.len(), 1);
    assert_eq!(schedules[0].job_id, job_id);
    assert!(schedules[0].is_cron());
    assert!(!schedules[0].is_interval());

    update_schedule_last_run(
        &pool,
        job_id,
        chrono::Utc::now(),
        Some(chrono::Utc::now() + chrono::Duration::hours(24)),
    )
    .await
    .expect("Failed to update schedule");
}

#[tokio::test]
async fn test_run_operations() {
    let (_container, pool) = setup_test_db().await;

    let device_id = "test-device-4".to_string();
    upsert_device(
        &pool,
        device_id.clone(),
        "Test Device".to_string(),
        "linux".to_string(),
        None,
    )
    .await
    .expect("Failed to create device");

    let job_id = uuid::Uuid::new_v4();
    sqlx::query(
        r#"
        INSERT INTO backup_jobs (id, device_id, name, source_paths)
        VALUES ($1, $2, $3, $4)
        "#,
    )
    .bind(job_id)
    .bind(&device_id)
    .bind("test-job")
    .bind(vec!["/data"])
    .execute(&pool)
    .await
    .expect("Failed to insert job");

    let run_id = create_run(&pool, job_id, device_id.clone(), "manual".to_string())
        .await
        .expect("Failed to create run");

    assert!(run_id > 0);

    update_run(
        &pool,
        run_id,
        chrono::Utc::now(),
        "success".to_string(),
        Some(0),
        None,
        Some(10),
        Some(5),
        Some(100),
        Some(1024000),
        Some("snapshot123".to_string()),
        Some("backup output".to_string()),
        None,
    )
    .await
    .expect("Failed to update run");

    let runs = get_recent_runs(&pool, device_id, 10)
        .await
        .expect("Failed to get recent runs");

    assert_eq!(runs.len(), 1);
    assert_eq!(runs[0].id, run_id);
    assert!(runs[0].is_success());
    assert_eq!(runs[0].snapshot_id, Some("snapshot123".to_string()));
}

#[tokio::test]
async fn test_settings_operations() {
    let (_container, pool) = setup_test_db().await;

    let device_id = "test-device-5".to_string();
    upsert_device(
        &pool,
        device_id.clone(),
        "Test Device".to_string(),
        "linux".to_string(),
        None,
    )
    .await
    .expect("Failed to create device");

    let settings = get_settings_for_device(&pool, device_id.clone())
        .await
        .expect("Failed to get settings");

    assert!(!settings.is_empty());

    let has_repo_url = settings.iter().any(|s| s.key == "repository_url");
    assert!(has_repo_url);

    let repo_url = get_global_setting(&pool, "repository_url".to_string())
        .await
        .expect("Failed to get global setting");

    assert!(repo_url.is_some());
}

#[tokio::test]
async fn test_migrations_create_all_tables() {
    let (_container, pool) = setup_test_db().await;

    let tables: Vec<(String,)> = sqlx::query_as(
        r#"
        SELECT table_name
        FROM information_schema.tables
        WHERE table_schema = 'public'
        AND table_type = 'BASE TABLE'
        ORDER BY table_name
        "#,
    )
    .fetch_all(&pool)
    .await
    .expect("Failed to query tables");

    let table_names: Vec<String> = tables.into_iter().map(|t| t.0).collect();

    assert!(table_names.contains(&"devices".to_string()));
    assert!(table_names.contains(&"backup_jobs".to_string()));
    assert!(table_names.contains(&"schedules".to_string()));
    assert!(table_names.contains(&"runs".to_string()));
    assert!(table_names.contains(&"settings".to_string()));
}

#[tokio::test]
async fn test_migrations_create_views() {
    let (_container, pool) = setup_test_db().await;

    let views: Vec<(String,)> = sqlx::query_as(
        r#"
        SELECT table_name
        FROM information_schema.views
        WHERE table_schema = 'public'
        ORDER BY table_name
        "#,
    )
    .fetch_all(&pool)
    .await
    .expect("Failed to query views");

    let view_names: Vec<String> = views.into_iter().map(|v| v.0).collect();

    assert!(view_names.contains(&"latest_runs".to_string()));
    assert!(view_names.contains(&"job_summary".to_string()));
}
