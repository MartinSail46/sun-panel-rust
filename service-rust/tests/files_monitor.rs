//! File reference safety and truthful, partially successful monitoring responses.
use axum::{Router, body::Body, http::Request};
use http_body_util::BodyExt;
use serde_json::{Value, json};
use sun_panel_rust::{app, config::Config, initialize};
use tempfile::TempDir;
use tower::ServiceExt;

fn config(directory: &TempDir) -> Config {
    Config {
        bind: "127.0.0.1:0".into(),
        database_path: directory.path().join("database.db"),
        uploads_dir: directory.path().join("uploads"),
        web_dir: directory.path().join("web"),
        max_upload_bytes: 1024,
        monitor_interval_secs: 5,
        allow_private_favicon: false,
        worker_threads: 1,
        database_connections: 3,
    }
}

async fn call(router: &Router, path: &str, data: Value) -> Value {
    let response = router
        .clone()
        .oneshot(
            Request::post(path)
                .header("content-type", "application/json")
                .header("host", "panel.local:3002")
                .header("origin", "http://panel.local:3002")
                .body(Body::from(data.to_string()))
                .unwrap(),
        )
        .await
        .unwrap();
    assert!(response.status().is_success(), "{path}");
    serde_json::from_slice(&response.into_body().collect().await.unwrap().to_bytes()).unwrap()
}

#[tokio::test]
async fn in_use_images_are_listed_and_cannot_be_deleted_until_replaced() {
    let temporary = TempDir::new().unwrap();
    let state = initialize(config(&temporary)).await.unwrap();
    let router = app(state.clone());
    let owner = state.owner_id;
    std::fs::write(state.config.uploads_dir.join("image.png"), b"original").unwrap();
    let file = sqlx::query("INSERT INTO file(src,user_id,file_name,ext) VALUES('./uploads/image.png',?,'image.png','.png')")
        .bind(owner).execute(&state.db).await.unwrap().last_insert_rowid();
    let group = sqlx::query("INSERT INTO item_icon_group(user_id,title) VALUES(?,'Apps')")
        .bind(owner)
        .execute(&state.db)
        .await
        .unwrap()
        .last_insert_rowid();
    sqlx::query("INSERT INTO item_icon(user_id,item_icon_group_id,title,icon_json) VALUES(?,?,'Referenced app',?)")
        .bind(owner).bind(group).bind(json!({"itemType":2,"src":"http://panel.local:3002/uploads/%69mage.png?size=1#preview"}).to_string())
        .execute(&state.db).await.unwrap();
    sqlx::query("INSERT OR REPLACE INTO user_config(user_id,panel_json) VALUES(?,?)")
        .bind(owner).bind(json!({"backgroundImageSrc":"uploads/image.png","footerHtml":"<a href='./uploads/image.png'>Download</a><img srcset='/uploads/image.png 1x'><div style=\"background:url('/uploads/image.png')\"></div>"}).to_string())
        .execute(&state.db).await.unwrap();
    let listed = call(&router, "/api/file/getList", json!({})).await;
    assert_eq!(listed["data"]["list"][0]["referenceCount"], 5, "{listed}");
    let blocked = call(&router, "/api/file/deletes", json!({"ids":[file]})).await;
    assert_eq!(blocked["code"], 1310, "{blocked}");
    assert_eq!(
        blocked["data"]["files"][0]["references"]
            .as_array()
            .unwrap()
            .len(),
        5
    );
    assert!(state.config.uploads_dir.join("image.png").exists());
    // The entire batch is rejected before staging any file or deleting a row.
    assert_eq!(
        sqlx::query_scalar::<_, i64>("SELECT count(*) FROM file WHERE deleted_at IS NULL")
            .fetch_one(&state.db)
            .await
            .unwrap(),
        1
    );
    sqlx::query("UPDATE item_icon SET icon_json='{}'")
        .execute(&state.db)
        .await
        .unwrap();
    sqlx::query("UPDATE user_config SET panel_json='{}'")
        .execute(&state.db)
        .await
        .unwrap();
    assert_eq!(
        call(&router, "/api/file/deletes", json!({"ids":[file]})).await["code"],
        0
    );
    assert!(!state.config.uploads_dir.join("image.png").exists());
    state.db.close().await;
}

#[tokio::test]
async fn remote_paths_are_not_false_references_and_trash_icons_are_protected_until_purged() {
    let temporary = TempDir::new().unwrap();
    let state = initialize(config(&temporary)).await.unwrap();
    let router = app(state.clone());
    let owner = state.owner_id;
    std::fs::write(state.config.uploads_dir.join("image.png"), b"original").unwrap();
    let file = sqlx::query(
        "INSERT INTO file(src,user_id,file_name) VALUES('/uploads/image.png',?,'image.png')",
    )
    .bind(owner)
    .execute(&state.db)
    .await
    .unwrap()
    .last_insert_rowid();
    sqlx::query("INSERT OR REPLACE INTO user_config(user_id,panel_json) VALUES(?,?)")
        .bind(owner).bind(json!({"logoText":"/uploads/image.png","backgroundImageSrc":"https://remote.local/uploads/image.png","unused":"http://panel.local:4000/uploads/image.png"}).to_string())
        .execute(&state.db).await.unwrap();
    let listed = call(&router, "/api/file/getList", json!({})).await;
    assert_eq!(listed["data"]["list"][0]["references"], json!([]));
    let item = sqlx::query("INSERT INTO item_icon(user_id,item_icon_group_id,title,icon_json,deleted_at) VALUES(?,0,'Old app',?,'deleted')")
        .bind(owner).bind(json!({"itemType":2,"src":"/uploads/image.png"}).to_string()).execute(&state.db).await.unwrap().last_insert_rowid();
    let listed = call(&router, "/api/file/getList", json!({})).await;
    assert_eq!(listed["data"]["list"][0]["referenceCount"], 1);
    assert_eq!(listed["data"]["list"][0]["references"][0]["inTrash"], true);
    assert_eq!(
        call(&router, "/api/file/deletes", json!({"ids":[file]})).await["code"],
        1310
    );
    assert!(state.config.uploads_dir.join("image.png").exists());
    assert_eq!(
        call(
            &router,
            "/api/panel/navigation/trash/purge",
            json!({"kind":"item","id":item})
        )
        .await["code"],
        0
    );
    assert_eq!(
        call(&router, "/api/file/deletes", json!({"ids":[file]})).await["code"],
        0
    );
    state.db.close().await;
}

#[tokio::test]
async fn deletion_waits_for_a_concurrent_config_write_before_checking_references() {
    let temporary = TempDir::new().unwrap();
    let state = initialize(config(&temporary)).await.unwrap();
    let router = app(state.clone());
    let owner = state.owner_id;
    std::fs::write(state.config.uploads_dir.join("image.png"), b"original").unwrap();
    let file = sqlx::query(
        "INSERT INTO file(src,user_id,file_name) VALUES('./uploads/image.png',?,'image.png')",
    )
    .bind(owner)
    .execute(&state.db)
    .await
    .unwrap()
    .last_insert_rowid();
    let mut tx = state.db.begin_with("BEGIN IMMEDIATE").await.unwrap();
    sqlx::query("INSERT OR REPLACE INTO user_config(user_id,panel_json) VALUES(?,?)")
        .bind(owner)
        .bind(json!({"backgroundImageSrc":"/uploads/image.png"}).to_string())
        .execute(&mut *tx)
        .await
        .unwrap();
    let worker =
        tokio::spawn(
            async move { call(&router, "/api/file/deletes", json!({"ids":[file]})).await },
        );
    tokio::task::yield_now().await;
    assert!(!worker.is_finished());
    tx.commit().await.unwrap();
    assert_eq!(worker.await.unwrap()["code"], 1310);
    assert!(state.config.uploads_dir.join("image.png").exists());
    state.db.close().await;
}

#[tokio::test]
async fn missing_disk_is_a_per_card_error_and_metrics_explain_their_scope() {
    let temporary = TempDir::new().unwrap();
    let state = initialize(config(&temporary)).await.unwrap();
    let router = app(state.clone());
    let snapshot = call(
        &router,
        "/api/system/monitor/snapshot",
        json!({"diskPaths":["/definitely-missing-sun-panel-test-path"]}),
    )
    .await;
    assert_eq!(snapshot["code"], 0, "{snapshot}");
    let data = &snapshot["data"];
    assert!(data["sampledAt"].as_str().is_some());
    assert_eq!(data["scope"]["source"], "operating-system");
    assert_eq!(data["scope"]["containerQuotaApplied"], false);
    assert!(data["memoryInfo"]["available"].is_u64());
    assert!(data["disks"]["/definitely-missing-sun-panel-test-path"]["error"].is_string());
    assert!(data["disks"]["/definitely-missing-sun-panel-test-path"]["data"].is_null());
    // This is still used by the disk editor, unlike the removed per-metric APIs.
    let mounts = call(&router, "/api/system/monitor/getDiskMountpoints", json!({})).await;
    assert_eq!(mounts["code"], 0, "{mounts}");
    assert!(
        mounts["data"]
            .as_array()
            .unwrap()
            .iter()
            .all(|mount| { mount["mountpoint"].as_str().is_some() && mount["total"].is_u64() })
    );
    state.db.close().await;
}
