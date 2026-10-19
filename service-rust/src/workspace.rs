//! Stable ownership for a single local workspace. Account rows are retained as
//! database ownership records; no runtime account or credential API is exposed.

use anyhow::{Context, ensure};
use sqlx::SqlitePool;

const LOCAL_OWNER_KEY: &str = "rust_local_owner_id";

/// Use only the persisted workspace owner. A new owner is created exclusively
/// for an empty user table; damaged metadata never selects another existing row.
pub async fn resolve_owner_id(db: &SqlitePool) -> anyhow::Result<i64> {
    let mut tx = db.begin().await?;
    let persisted: Option<String> =
        sqlx::query_scalar("SELECT config_value FROM system_setting WHERE config_name=?")
            .bind(LOCAL_OWNER_KEY)
            .fetch_optional(&mut *tx)
            .await?;
    if let Some(persisted) = persisted {
        let owner_id = persisted
            .parse::<i64>()
            .context("Workspace owner metadata is invalid; refusing to select another owner")?;
        let exists: bool = sqlx::query_scalar(
            "SELECT EXISTS(SELECT 1 FROM \"user\" WHERE id=? AND deleted_at IS NULL)",
        )
        .bind(owner_id)
        .fetch_one(&mut *tx)
        .await?;
        ensure!(
            owner_id > 0 && exists,
            "Workspace owner is missing or deleted; refusing to select another owner"
        );
        tx.commit().await?;
        return Ok(owner_id);
    }
    let has_users: bool = sqlx::query_scalar("SELECT EXISTS(SELECT 1 FROM \"user\")")
        .fetch_one(&mut *tx)
        .await?;
    ensure!(
        !has_users,
        "Workspace owner metadata is missing for an existing database; refusing to select an owner"
    );
    let owner_id = sqlx::query(
        "INSERT INTO \"user\" (username,password,name,role,status) \
        VALUES ('workspace','','Workspace',1,1)",
    )
    .execute(&mut *tx)
    .await?
    .last_insert_rowid();
    sqlx::query("INSERT INTO system_setting(config_name,config_value) VALUES (?,?)")
        .bind(LOCAL_OWNER_KEY)
        .bind(owner_id.to_string())
        .execute(&mut *tx)
        .await?;
    tx.commit().await?;
    Ok(owner_id)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::state::AppState;
    use axum::{
        body::Body,
        http::{Request, StatusCode},
    };
    use tower::ServiceExt;

    async fn database() -> (tempfile::TempDir, SqlitePool) {
        let directory = tempfile::tempdir().unwrap();
        let mut config = crate::config::Config::from_env().unwrap();
        config.database_path = directory.path().join("database.db");
        config.database_connections = 1;
        let db = crate::db::connect(&config).await.unwrap();
        (directory, db)
    }

    async fn fixture() -> (tempfile::TempDir, AppState) {
        let directory = tempfile::tempdir().unwrap();
        let mut config = crate::config::Config::from_env().unwrap();
        config.database_path = directory.path().join("database.db");
        config.uploads_dir = directory.path().join("uploads");
        config.web_dir = directory.path().join("absent-web");
        config.database_connections = 1;
        let state = crate::initialize(config).await.unwrap();
        (directory, state)
    }

    async fn seed_user(db: &SqlitePool, id: i64, role: i64, deleted: bool) {
        sqlx::query(
            "INSERT INTO \"user\"(id,username,password,name,role,status,deleted_at) \
            VALUES (?,?,'preserved-password','Stored owner',?,3,?)",
        )
        .bind(id)
        .bind(format!("stored-{id}"))
        .bind(role)
        .bind(deleted.then_some("2026-01-01"))
        .execute(db)
        .await
        .unwrap();
    }

    #[tokio::test]
    async fn empty_database_creates_one_passwordless_workspace_without_sessions() {
        let (_directory, db) = database().await;
        let owner = resolve_owner_id(&db).await.unwrap();
        let again = resolve_owner_id(&db).await.unwrap();
        assert_eq!(owner, again);
        let (count, password): (i64, String) =
            sqlx::query_as("SELECT COUNT(*), password FROM \"user\"")
                .fetch_one(&db)
                .await
                .unwrap();
        assert_eq!(count, 1);
        assert_eq!(password, "");
        let sessions_exist: bool = sqlx::query_scalar(
            "SELECT EXISTS(SELECT 1 FROM sqlite_master WHERE type='table' AND name='sessions')",
        )
        .fetch_one(&db)
        .await
        .unwrap();
        assert!(!sessions_exist);
    }

    async fn snapshot(db: &SqlitePool) -> (String, i64) {
        let data: String = sqlx::query_scalar(
            "SELECT json_object('users',(SELECT json_group_array(json_array(id,username,password,name,role,status,deleted_at)) FROM user),'settings',(SELECT json_group_array(json_array(config_name,config_value)) FROM system_setting))",
        ).fetch_one(db).await.unwrap();
        let changes: i64 = sqlx::query_scalar("SELECT total_changes()")
            .fetch_one(db)
            .await
            .unwrap();
        (data, changes)
    }

    #[tokio::test]
    async fn persisted_native_owner_survives_restart_without_selecting_roles_or_writing_rows() {
        let (directory, db) = database().await;
        seed_user(&db, 7, 1, false).await;
        seed_user(&db, 11, 1, true).await;
        seed_user(&db, 42, 2, false).await;
        sqlx::query("INSERT INTO system_setting(config_name,config_value) VALUES (?, '42')")
            .bind(LOCAL_OWNER_KEY)
            .execute(&db)
            .await
            .unwrap();
        sqlx::query("INSERT INTO item_icon_group(title,user_id) VALUES ('Existing navigation',42)")
            .execute(&db)
            .await
            .unwrap();
        sqlx::query(
            "INSERT INTO user_config(user_id,panel_json) VALUES (42,'{\"theme\":\"dark\"}')",
        )
        .execute(&db)
        .await
        .unwrap();
        let before = snapshot(&db).await;
        assert_eq!(resolve_owner_id(&db).await.unwrap(), 42);
        assert_eq!(snapshot(&db).await, before);
        db.close().await;
        let mut config = crate::config::Config::from_env().unwrap();
        config.database_path = directory.path().join("database.db");
        config.database_connections = 1;
        let reopened = crate::db::connect(&config).await.unwrap();
        let reopened_before = snapshot(&reopened).await;
        assert_eq!(resolve_owner_id(&reopened).await.unwrap(), 42);
        assert_eq!(snapshot(&reopened).await, reopened_before);
        assert_eq!(reopened_before.0, before.0);
        assert_eq!(
            sqlx::query_scalar::<_, i64>("SELECT user_id FROM item_icon_group")
                .fetch_one(&reopened)
                .await
                .unwrap(),
            42
        );
        assert_eq!(
            sqlx::query_scalar::<_, String>("SELECT panel_json FROM user_config WHERE user_id=42")
                .fetch_one(&reopened)
                .await
                .unwrap(),
            "{\"theme\":\"dark\"}"
        );
        reopened.close().await;
    }

    #[tokio::test]
    async fn missing_owner_metadata_never_adopts_existing_or_deleted_users() {
        for deleted in [false, true] {
            let (_directory, db) = database().await;
            seed_user(&db, 7, 1, deleted).await;
            let before = snapshot(&db).await;
            let error = resolve_owner_id(&db).await.unwrap_err().to_string();
            assert!(error.contains("metadata is missing"), "{error}");
            assert_eq!(snapshot(&db).await, before);
            db.close().await;
        }
    }

    #[tokio::test]
    async fn invalid_missing_or_deleted_owner_references_fail_without_changes() {
        for value in ["invalid", "0", "-7", "999", "11"] {
            let (_directory, db) = database().await;
            seed_user(&db, 7, 1, false).await;
            seed_user(&db, 11, 1, true).await;
            sqlx::query("INSERT INTO system_setting(config_name,config_value) VALUES (?,?)")
                .bind(LOCAL_OWNER_KEY)
                .bind(value)
                .execute(&db)
                .await
                .unwrap();
            let before = snapshot(&db).await;
            assert!(resolve_owner_id(&db).await.is_err(), "{value}");
            assert_eq!(snapshot(&db).await, before);
            db.close().await;
        }
    }

    #[tokio::test]
    async fn removed_compatibility_endpoints_return_not_found() {
        let (_directory, state) = fixture().await;
        let app = crate::app(state);
        for path in [
            "/api/openness/getDisclaimer",
            "/api/openness/getAboutDescription",
            "/api/notice/getListByDisplayType",
            "/api/file/uploadFiles",
            "/api/system/monitor/getAll",
            "/api/system/monitor/getCpuState",
            "/api/system/monitor/getMemonyState",
            "/api/system/monitor/getDiskStateByPath",
            "/api/panel/itemIcon/addMultiple",
            "/api/panel/itemIcon/getListByGroupId",
            "/api/panel/itemIcon/saveSort",
            "/api/user/getInfo",
            "/api/user/getAuthInfo",
            "/api/login",
            "/api/logout",
            "/api/register",
            "/api/user/updateInfo",
            "/api/user/updatePassword",
            "/api/user/getReferralCode",
            "/api/panel/users/create",
            "/api/panel/users/update",
            "/api/panel/users/getList",
            "/api/panel/users/deletes",
            "/api/panel/users/getPublicVisitUser",
            "/api/panel/users/setPublicVisitUser",
        ] {
            for method in ["GET", "POST"] {
                let response = app
                    .clone()
                    .oneshot(
                        Request::builder()
                            .method(method)
                            .uri(path)
                            .header("content-type", "application/json")
                            .header("token", "old-session-token")
                            .body(Body::from("{}"))
                            .unwrap(),
                    )
                    .await
                    .unwrap();
                assert_eq!(response.status(), StatusCode::NOT_FOUND, "{method} {path}");
            }
        }
    }
}
