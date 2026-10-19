//! Complete-instance backup regressions; all data stays inside temporary dirs.
use std::{
    fs,
    path::{Path, PathBuf},
    process::{Child, Command, Stdio},
    time::Duration,
};

use serde_json::{Value, json};
use sqlx::{Connection, SqliteConnection, sqlite::SqliteConnectOptions};
use sun_panel_rust::{backup, config::Config, initialize, instance_lock::InstanceLock};
use tempfile::TempDir;

fn config(root: &Path) -> Config {
    Config {
        bind: "127.0.0.1:0".into(),
        database_path: root.join("database.db"),
        uploads_dir: root.join("uploads"),
        web_dir: root.join("web"),
        max_upload_bytes: 1024 * 1024,
        monitor_interval_secs: 5,
        allow_private_favicon: false,
        worker_threads: 1,
        database_connections: 2,
    }
}

async fn fixture(root: &Path) -> Config {
    let config = config(root);
    let state = initialize(config.clone()).await.unwrap();
    sqlx::query("INSERT INTO user_config(user_id,panel_json,search_engine_json) VALUES (?,?,?)")
        .bind(state.owner_id)
        .bind(r#"{"logoText":"Backup marker","backgroundImage":"/uploads/nested/image.png"}"#)
        .bind(r#"{"searchEngine":"https://example.com/?q="}"#)
        .execute(&state.db)
        .await
        .unwrap();
    sqlx::query("INSERT INTO module_config(user_id,name,value_json) VALUES (?,'systemMonitor',?)")
        .bind(state.owner_id)
        .bind(r#"{"cpu":true,"memory":true}"#)
        .execute(&state.db)
        .await
        .unwrap();
    fs::create_dir_all(config.uploads_dir.join("nested/empty")).unwrap();
    fs::write(
        config.uploads_dir.join("nested/image.png"),
        b"test-image-bytes",
    )
    .unwrap();
    state.db.close().await;
    config
}

fn assert_no_stage(parent: &Path) {
    assert!(fs::read_dir(parent).unwrap().all(|entry| {
        !entry
            .unwrap()
            .file_name()
            .to_string_lossy()
            .starts_with(".sun-panel-stage-")
    }));
}

fn load_manifest(bundle: &Path) -> Value {
    serde_json::from_slice(&fs::read(bundle.join("manifest.json")).unwrap()).unwrap()
}

#[tokio::test]
async fn round_trip_preserves_committed_wal_settings_and_uploads_with_custom_paths() {
    let temporary = TempDir::new().unwrap();
    let original = fixture(&temporary.path().join("original")).await;
    let mut connection = SqliteConnection::connect_with(
        &SqliteConnectOptions::new().filename(&original.database_path),
    )
    .await
    .unwrap();
    sqlx::query("PRAGMA wal_autocheckpoint=0")
        .execute(&mut connection)
        .await
        .unwrap();
    sqlx::query("PRAGMA wal_checkpoint(TRUNCATE)")
        .execute(&mut connection)
        .await
        .unwrap();
    sqlx::query("INSERT INTO system_setting(config_name,config_value) VALUES ('wal_backup_marker','committed-in-wal')")
        .execute(&mut connection).await.unwrap();
    assert!(
        fs::metadata(original.database_path.with_file_name("database.db-wal"))
            .unwrap()
            .len()
            > 32
    );
    // Prove this record has not reached the main DB file before the snapshot.
    let main_only = temporary.path().join("main-only.db");
    fs::copy(&original.database_path, &main_only).unwrap();
    let mut main_connection = SqliteConnection::connect_with(
        &SqliteConnectOptions::new()
            .filename(&main_only)
            .read_only(true),
    )
    .await
    .unwrap();
    assert_eq!(
        sqlx::query_scalar::<_, i64>(
            "SELECT count(*) FROM system_setting WHERE config_name='wal_backup_marker'"
        )
        .fetch_one(&mut main_connection)
        .await
        .unwrap(),
        0
    );
    main_connection.close().await.unwrap();
    let bundle = temporary.path().join("portable-backup");
    backup::create(&original, &bundle).await.unwrap();
    // Keep the source connection alive until VACUUM INTO has included its WAL.
    connection.close().await.unwrap();
    let manifest = load_manifest(&bundle);
    assert_eq!(manifest["format_version"], 1);
    assert_eq!(manifest["application_version"], env!("CARGO_PKG_VERSION"));
    assert_eq!(manifest["files"]["uploads/nested/image.png"]["bytes"], 16);
    assert_eq!(
        manifest["files"]["database.sqlite3"]["sha256"]
            .as_str()
            .unwrap()
            .len(),
        64
    );
    let mut target = config(&temporary.path().join("unused-default"));
    target.database_path = temporary.path().join("separate-database/custom.sqlite");
    target.uploads_dir = temporary.path().join("separate-files/images");
    backup::restore(&target, &bundle).await.unwrap();
    assert_eq!(
        fs::read(target.uploads_dir.join("nested/image.png")).unwrap(),
        b"test-image-bytes"
    );
    assert!(target.uploads_dir.join("nested/empty").is_dir());
    let restored = initialize(target).await.unwrap();
    assert_eq!(
        sqlx::query_scalar::<_, String>(
            "SELECT config_value FROM system_setting WHERE config_name='wal_backup_marker'"
        )
        .fetch_one(&restored.db)
        .await
        .unwrap(),
        "committed-in-wal"
    );
    let (panel, search): (String, String) =
        sqlx::query_as("SELECT panel_json,search_engine_json FROM user_config")
            .fetch_one(&restored.db)
            .await
            .unwrap();
    assert!(panel.contains("Backup marker") && search.contains("example.com"));
    assert_eq!(
        sqlx::query_scalar::<_, String>(
            "SELECT value_json FROM module_config WHERE name='systemMonitor'"
        )
        .fetch_one(&restored.db)
        .await
        .unwrap(),
        r#"{"cpu":true,"memory":true}"#
    );
    restored.db.close().await;
    assert_no_stage(temporary.path());
    assert_no_stage(&temporary.path().join("separate-database"));
    assert_no_stage(&temporary.path().join("separate-files"));
}

#[tokio::test]
async fn tampering_fails_without_partial_targets_and_can_be_retried() {
    let temporary = TempDir::new().unwrap();
    let original = fixture(&temporary.path().join("original")).await;
    let bundle = temporary.path().join("backup");
    backup::create(&original, &bundle).await.unwrap();
    fs::write(bundle.join("uploads/nested/image.png"), b"tampered").unwrap();
    let target = config(&temporary.path().join("target"));
    fs::create_dir_all(&target.uploads_dir).unwrap();
    let error = backup::restore(&target, &bundle).await.unwrap_err();
    assert!(error.to_string().contains("checksum mismatch"), "{error:#}");
    assert!(!target.database_path.exists());
    assert_eq!(fs::read_dir(&target.uploads_dir).unwrap().count(), 0);
    assert_no_stage(target.database_path.parent().unwrap());
    fs::write(bundle.join("uploads/nested/image.png"), b"test-image-bytes").unwrap();
    backup::restore(&target, &bundle).await.unwrap();
    assert!(target.database_path.is_file());
    assert_eq!(
        fs::read(target.uploads_dir.join("nested/image.png")).unwrap(),
        b"test-image-bytes"
    );
}

#[tokio::test]
async fn extra_files_traversal_and_invalid_database_are_rejected() {
    let temporary = TempDir::new().unwrap();
    let original = fixture(&temporary.path().join("original")).await;
    let bundle = temporary.path().join("backup");
    backup::create(&original, &bundle).await.unwrap();
    let target = config(&temporary.path().join("target"));
    fs::write(bundle.join("unexpected.txt"), "unlisted").unwrap();
    assert!(
        backup::restore(&target, &bundle)
            .await
            .unwrap_err()
            .to_string()
            .contains("file/directory set")
    );
    fs::remove_file(bundle.join("unexpected.txt")).unwrap();
    let mut manifest = load_manifest(&bundle);
    manifest["files"]["../outside.txt"] = json!({"bytes":0,"sha256":"0".repeat(64)});
    fs::write(
        bundle.join("manifest.json"),
        serde_json::to_vec(&manifest).unwrap(),
    )
    .unwrap();
    assert!(
        backup::restore(&target, &bundle)
            .await
            .unwrap_err()
            .to_string()
            .contains("path")
    );
    assert!(!temporary.path().join("outside.txt").exists());
    assert!(!target.database_path.exists());
    // A correct checksum is not sufficient for an invalid SQLite payload.
    manifest["files"]
        .as_object_mut()
        .unwrap()
        .remove("../outside.txt");
    let bytes = b"not a sqlite database";
    use sha2::{Digest, Sha256};
    let hash = Sha256::digest(bytes)
        .iter()
        .map(|v| format!("{v:02x}"))
        .collect::<String>();
    manifest["files"]["database.sqlite3"] = json!({"bytes":bytes.len(),"sha256":hash});
    fs::write(bundle.join("database.sqlite3"), bytes).unwrap();
    fs::write(
        bundle.join("manifest.json"),
        serde_json::to_vec(&manifest).unwrap(),
    )
    .unwrap();
    assert!(backup::restore(&target, &bundle).await.is_err());
    assert!(!target.database_path.exists());
    assert!(!target.uploads_dir.exists());
    assert_no_stage(target.database_path.parent().unwrap());
}

#[tokio::test]
async fn refuses_overwrite_for_database_sidecars_uploads_and_backup_destination() {
    let temporary = TempDir::new().unwrap();
    let original = fixture(&temporary.path().join("original")).await;
    let bundle = temporary.path().join("backup");
    backup::create(&original, &bundle).await.unwrap();
    assert!(backup::create(&original, &bundle).await.is_err());
    let original_bytes = fs::read(&original.database_path).unwrap();
    assert!(backup::restore(&original, &bundle).await.is_err());
    assert_eq!(fs::read(&original.database_path).unwrap(), original_bytes);
    let target = config(&temporary.path().join("target"));
    fs::create_dir_all(&target.uploads_dir).unwrap();
    let sidecar = target.database_path.with_file_name("database.db-wal");
    fs::write(&sidecar, "existing-wal").unwrap();
    assert!(backup::restore(&target, &bundle).await.is_err());
    assert_eq!(fs::read(&sidecar).unwrap(), b"existing-wal");
    fs::remove_file(sidecar).unwrap();
    fs::write(target.uploads_dir.join("existing.png"), "preserve-me").unwrap();
    assert!(backup::restore(&target, &bundle).await.is_err());
    assert_eq!(
        fs::read(target.uploads_dir.join("existing.png")).unwrap(),
        b"preserve-me"
    );
    assert!(!target.database_path.exists());
}

#[tokio::test]
async fn instance_lock_covers_shared_database_or_uploads_and_releases_on_drop() {
    let temporary = TempDir::new().unwrap();
    let original = fixture(&temporary.path().join("original")).await;
    let held = InstanceLock::acquire(&original).unwrap();
    let bundle = temporary.path().join("backup");
    assert!(
        backup::create(&original, &bundle)
            .await
            .unwrap_err()
            .to_string()
            .contains("Instance is in use")
    );
    let mut shared_db = original.clone();
    shared_db.uploads_dir = temporary.path().join("other-uploads");
    assert!(InstanceLock::acquire(&shared_db).is_err());
    let mut shared_uploads = original.clone();
    shared_uploads.database_path = temporary.path().join("other-db.sqlite");
    assert!(InstanceLock::acquire(&shared_uploads).is_err());
    drop(held);
    backup::create(&original, &bundle).await.unwrap();
    let target = config(&temporary.path().join("target"));
    let held = InstanceLock::acquire(&target).unwrap();
    assert!(
        backup::restore(&target, &bundle)
            .await
            .unwrap_err()
            .to_string()
            .contains("Instance is in use")
    );
    drop(held);
    backup::restore(&target, &bundle).await.unwrap();
}

#[cfg(unix)]
#[tokio::test]
async fn symlinks_are_not_followed_and_failed_backup_stage_is_removed() {
    use std::os::unix::fs::symlink;
    let temporary = TempDir::new().unwrap();
    let original = fixture(&temporary.path().join("original")).await;
    let outside = temporary.path().join("outside-secret.txt");
    fs::write(&outside, "do-not-copy").unwrap();
    symlink(&outside, original.uploads_dir.join("escape.png")).unwrap();
    let bundle = temporary.path().join("backup");
    assert!(
        backup::create(&original, &bundle)
            .await
            .unwrap_err()
            .to_string()
            .contains("symlink")
    );
    assert!(!bundle.exists());
    assert_no_stage(temporary.path());
    fs::remove_file(original.uploads_dir.join("escape.png")).unwrap();
    backup::create(&original, &bundle).await.unwrap();
    fs::remove_file(bundle.join("uploads/nested/image.png")).unwrap();
    symlink(&outside, bundle.join("uploads/nested/image.png")).unwrap();
    let target = config(&temporary.path().join("target"));
    assert!(
        backup::restore(&target, &bundle)
            .await
            .unwrap_err()
            .to_string()
            .contains("symlink")
    );
    assert!(!target.database_path.exists());
    assert_eq!(fs::read(outside).unwrap(), b"do-not-copy");
}

struct RunningServer(Child);
impl Drop for RunningServer {
    fn drop(&mut self) {
        let _ = self.0.kill();
        let _ = self.0.wait();
    }
}

#[tokio::test]
async fn removed_migration_command_is_rejected_before_creating_any_target() {
    let temporary = TempDir::new().unwrap();
    let target = temporary.path().join("new-target");
    let result = Command::new(env!("CARGO_BIN_EXE_sun-panel-rust"))
        .arg("migrate-legacy")
        .arg(temporary.path().join("old-source"))
        .env("SUN_PANEL_DATA_DIR", &target)
        .output()
        .unwrap();
    assert!(!result.status.success());
    assert!(String::from_utf8_lossy(&result.stderr).contains("Invalid arguments"));
    assert!(!target.exists());
}

#[tokio::test]
async fn real_server_holds_lock_before_initialization_and_cli_respects_it() {
    let temporary = TempDir::new().unwrap();
    let root = temporary.path().join("server");
    let binary = env!("CARGO_BIN_EXE_sun-panel-rust");
    let server = RunningServer(
        Command::new(binary)
            .env("SUN_PANEL_DATA_DIR", &root)
            .env("SUN_PANEL_BIND", "127.0.0.1:0")
            .stdout(Stdio::null())
            .stderr(Stdio::null())
            .spawn()
            .unwrap(),
    );
    let config = config(&root);
    let mut locked = false;
    for _ in 0..100 {
        if config.database_path.exists() && InstanceLock::acquire(&config).is_err() {
            locked = true;
            break;
        }
        tokio::time::sleep(Duration::from_millis(20)).await;
    }
    assert!(locked, "server did not acquire its lifetime lock");
    let bundle: PathBuf = temporary.path().join("backup");
    let result = Command::new(binary)
        .arg("backup")
        .arg(&bundle)
        .env("SUN_PANEL_DATA_DIR", &root)
        .output()
        .unwrap();
    assert!(!result.status.success());
    assert!(String::from_utf8_lossy(&result.stderr).contains("Instance is in use"));
    assert!(!bundle.exists());
    drop(server);
    // The same CLI works after shutdown; stale lock files do not block it.
    let result = Command::new(binary)
        .arg("backup")
        .arg(&bundle)
        .env("SUN_PANEL_DATA_DIR", &root)
        .output()
        .unwrap();
    assert!(
        result.status.success(),
        "{}",
        String::from_utf8_lossy(&result.stderr)
    );
    let restored_root = temporary.path().join("cli-restored");
    let result = Command::new(binary)
        .arg("restore")
        .arg(&bundle)
        .env("SUN_PANEL_DATA_DIR", &restored_root)
        .output()
        .unwrap();
    assert!(
        result.status.success(),
        "{}",
        String::from_utf8_lossy(&result.stderr)
    );
    assert!(restored_root.join("database.db").is_file());
    assert!(restored_root.join("uploads").is_dir());
}
