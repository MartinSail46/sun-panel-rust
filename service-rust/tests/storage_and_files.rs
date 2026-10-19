use axum::{
    Router,
    body::Body,
    http::{Request, StatusCode},
};
use http_body_util::BodyExt;
use serde_json::{Value, json};
use sun_panel_rust::{app, config::Config, initialize};
use tempfile::TempDir;
use tower::ServiceExt;

fn config(dir: &TempDir) -> Config {
    Config {
        bind: "127.0.0.1:0".into(),
        database_path: dir.path().join("database.db"),
        uploads_dir: dir.path().join("uploads"),
        web_dir: dir.path().join("web"),
        max_upload_bytes: 1024,
        monitor_interval_secs: 5,
        allow_private_favicon: false,
        worker_threads: 2,
        database_connections: 2,
    }
}

async fn call(router: &Router, path: &str, token: &str, data: Value) -> Value {
    let mut request = Request::post(path).header("content-type", "application/json");
    if !token.is_empty() {
        request = request.header("token", token);
    }
    let response = router
        .clone()
        .oneshot(request.body(Body::from(data.to_string())).unwrap())
        .await
        .unwrap();
    assert_eq!(response.status(), StatusCode::OK, "{path}");
    serde_json::from_slice(&response.into_body().collect().await.unwrap().to_bytes()).unwrap()
}

async fn upload(router: &Router, token: &str, name: &str, bytes: &[u8]) -> Value {
    let boundary = "sun-panel-test-boundary";
    let mut body = format!("--{boundary}\r\nContent-Disposition: form-data; name=\"imgfile\"; filename=\"{name}\"\r\nContent-Type: image/png\r\n\r\n").into_bytes();
    body.extend_from_slice(bytes);
    body.extend_from_slice(format!("\r\n--{boundary}--\r\n").as_bytes());
    let mut request = Request::post("/api/file/uploadImg").header(
        "content-type",
        format!("multipart/form-data; boundary={boundary}"),
    );
    if !token.is_empty() {
        request = request.header("token", token);
    }
    let response = router
        .clone()
        .oneshot(request.body(Body::from(body)).unwrap())
        .await
        .unwrap();
    serde_json::from_slice(&response.into_body().collect().await.unwrap().to_bytes()).unwrap()
}

#[tokio::test]
async fn streamed_uploads_have_limits_ownership_and_safe_static_headers() {
    let dir = TempDir::new().unwrap();
    let state = initialize(config(&dir)).await.unwrap();
    let router = app(state.clone());
    let admin = "";
    // Archived owners remain in storage, but no account API can select them.
    let foreign_owner = sqlx::query("INSERT INTO user(username,name,password,status,role) VALUES('archived','Archived','unchanged',1,2)")
        .execute(&state.db).await.unwrap().last_insert_rowid();
    let foreign_file = sqlx::query("INSERT INTO file(src,user_id,file_name,ext) VALUES('/uploads/archived.png',?,'archived.png','.png')")
        .bind(foreign_owner).execute(&state.db).await.unwrap().last_insert_rowid();
    let image = b"\x89\x50\x4e\x47\x0d\x0a\x1a\x0a\x00\x00\x00\x0d\x49\x48\x44\x52\x00\x00\x00\x01\x00\x00\x00\x01\x08\x04\x00\x00\x00\xb5\x1c\x0c\x02\x00\x00\x00\x0b\x49\x44\x41\x54\x78\xda\x63\xfc\xff\x1f\x00\x03\x03\x02\x00\xef\xbf\x69\xb6\x00\x00\x00\x00\x49\x45\x4e\x44\xae\x42\x60\x82";
    let saved = upload(&router, admin, "../../image.png", image).await;
    assert_eq!(saved["code"], 0, "{saved}");
    let url = saved["data"]["imageUrl"].as_str().unwrap();
    assert!(url.starts_with("/uploads/"));
    let response = router
        .clone()
        .oneshot(Request::get(url).body(Body::empty()).unwrap())
        .await
        .unwrap();
    assert_eq!(response.status(), StatusCode::OK);
    assert_eq!(response.headers()["x-content-type-options"], "nosniff");
    assert!(
        response.headers()["content-security-policy"]
            .to_str()
            .unwrap()
            .contains("sandbox")
    );
    assert_eq!(
        response
            .into_body()
            .collect()
            .await
            .unwrap()
            .to_bytes()
            .as_ref(),
        image
    );
    let files = call(&router, "/api/file/getList", admin, json!({})).await;
    let id = files["data"]["list"][0]["id"].as_i64().unwrap();
    assert_ne!(
        call(
            &router,
            "/api/file/deletes",
            admin,
            json!({"ids":[id,foreign_file]})
        )
        .await["code"],
        0
    );
    assert_eq!(
        call(&router, "/api/file/deletes", admin, json!({"ids":[id]})).await["code"],
        0
    );
    assert_eq!(
        router
            .clone()
            .oneshot(Request::get(url).body(Body::empty()).unwrap())
            .await
            .unwrap()
            .status(),
        StatusCode::NOT_FOUND
    );
    assert_ne!(
        upload(&router, admin, "fake.png", b"<svg onload='alert(1)'></svg>").await["code"],
        0
    );
    let mut oversized = image.to_vec();
    oversized.resize(2048, 0);
    assert_ne!(
        upload(&router, admin, "large.png", &oversized).await["code"],
        0
    );
    assert_eq!(
        call(&router, "/api/file/getList", admin, json!({})).await["data"]["list"],
        json!([])
    );
    state.db.close().await;
}

#[tokio::test]
async fn anonymous_health_monitor_and_local_owner_survive_restart() {
    let dir = TempDir::new().unwrap();
    let cfg = config(&dir);
    let state = initialize(cfg.clone()).await.unwrap();
    let router = app(state.clone());
    let token = "";
    let owner_id = state.owner_id;
    let monitor = call(
        &router,
        "/api/system/monitor/snapshot",
        token,
        json!({"diskPaths":[]}),
    )
    .await;
    assert_eq!(monitor["code"], 0, "{monitor}");
    let cpu = monitor["data"]["cpuInfo"]["usages"][0].as_f64().unwrap();
    assert!((0.0..=100.0).contains(&cpu));
    assert!(monitor["data"]["memoryInfo"]["total"].as_u64().unwrap() > 0);
    let health = router
        .clone()
        .oneshot(Request::get("/api/health").body(Body::empty()).unwrap())
        .await
        .unwrap();
    assert_eq!(health.status(), StatusCode::OK);
    drop(router);
    state.db.close().await;
    let next = initialize(cfg).await.unwrap();
    let router = app(next.clone());
    assert_eq!(next.owner_id, owner_id);
    assert_ne!(
        call(
            &router,
            "/api/panel/itemIcon/getSiteFavicon",
            token,
            json!({"url":"http://127.0.0.1:3002/api/health"})
        )
        .await["code"],
        0
    );
    next.db.close().await;
}

#[cfg(unix)]
#[tokio::test]
async fn migrated_upload_symlinks_cannot_expose_database_or_external_files() {
    let dir = TempDir::new().unwrap();
    let state = initialize(config(&dir)).await.unwrap();
    let secret = dir.path().join("secret.txt");
    tokio::fs::write(&secret, b"must-not-be-public")
        .await
        .unwrap();
    std::os::unix::fs::symlink(&secret, state.config.uploads_dir.join("leak.txt")).unwrap();
    std::os::unix::fs::symlink(
        &state.config.database_path,
        state.config.uploads_dir.join("database.db"),
    )
    .unwrap();
    let router = app(state.clone());
    for url in [
        "/uploads/leak.txt",
        "/uploads/database.db",
        "/uploads/../secret.txt",
    ] {
        let response = router
            .clone()
            .oneshot(Request::get(url).body(Body::empty()).unwrap())
            .await
            .unwrap();
        assert_eq!(response.status(), StatusCode::NOT_FOUND, "{url}");
    }
    state.db.close().await;
}
