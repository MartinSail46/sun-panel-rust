use axum::{
    Router,
    body::Body,
    http::{Request, StatusCode},
};
use http_body_util::BodyExt;
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

async fn get(router: &Router, path: &str, encoding: &str) -> axum::response::Response {
    router
        .clone()
        .oneshot(
            Request::get(path)
                .header("accept-encoding", encoding)
                .body(Body::empty())
                .unwrap(),
        )
        .await
        .unwrap()
}

#[tokio::test]
async fn compression_negotiation_and_upgrade_safe_caches() {
    let dir = TempDir::new().unwrap();
    let config = config(&dir);
    std::fs::create_dir_all(config.web_dir.join("assets")).unwrap();
    let name = config.web_dir.join("assets/entry--abcdefg.js");
    std::fs::write(&name, include_bytes!("fixtures/static/entry.js")).unwrap();
    std::fs::write(
        name.with_extension("js.gz"),
        include_bytes!("fixtures/static/entry.js.gz"),
    )
    .unwrap();
    std::fs::write(
        name.with_extension("js.br"),
        include_bytes!("fixtures/static/entry.js.br"),
    )
    .unwrap();
    for name in ["index.html", "sw.js", "custom.js", "assets/custom.js"] {
        std::fs::write(
            config.web_dir.join(name),
            "always discover the current release",
        )
        .unwrap();
    }
    let state = initialize(config).await.unwrap();
    let router = app(state);
    for (encoding, expected, body) in [
        (
            "br, gzip",
            Some("br"),
            include_bytes!("fixtures/static/entry.js.br").as_slice(),
        ),
        (
            "gzip",
            Some("gzip"),
            include_bytes!("fixtures/static/entry.js.gz").as_slice(),
        ),
        (
            "br;q=0, gzip;q=0",
            None,
            include_bytes!("fixtures/static/entry.js").as_slice(),
        ),
    ] {
        let response = get(&router, "/assets/entry--abcdefg.js", encoding).await;
        assert_eq!(response.status(), StatusCode::OK);
        assert_eq!(
            response
                .headers()
                .get("content-encoding")
                .map(|value| value.to_str().unwrap()),
            expected
        );
        assert_eq!(
            response.headers()["cache-control"],
            "public, max-age=31536000, immutable"
        );
        assert!(
            response
                .headers()
                .get_all("vary")
                .iter()
                .any(|value| value.to_str().unwrap().contains("Accept-Encoding"))
        );
        assert_eq!(
            response
                .into_body()
                .collect()
                .await
                .unwrap()
                .to_bytes()
                .as_ref(),
            body
        );
    }
    for path in ["/", "/sw.js", "/custom.js", "/assets/custom.js"] {
        let response = get(&router, path, "identity").await;
        assert_eq!(response.headers()["cache-control"], "no-cache");
        let etag = response.headers()["etag"].clone();
        let response = router
            .clone()
            .oneshot(
                Request::get(path)
                    .header("if-none-match", etag)
                    .body(Body::empty())
                    .unwrap(),
            )
            .await
            .unwrap();
        assert_eq!(response.status(), StatusCode::NOT_MODIFIED);
        assert_eq!(response.headers()["cache-control"], "no-cache");
    }
    let missing = get(&router, "/assets/missing-abcdefgh.js", "br").await;
    assert_eq!(missing.status(), StatusCode::NOT_FOUND);
    assert!(!missing.headers().contains_key("cache-control"));
    let api = get(&router, "/api/health", "gzip").await;
    assert!(!api.headers().contains_key("cache-control"));
}

#[tokio::test]
async fn monitor_snapshot_bounds_and_partial_disk_failure() {
    let dir = TempDir::new().unwrap();
    let state = initialize(config(&dir)).await.unwrap();
    let router = app(state);
    let unavailable = dir
        .path()
        .join("missing-disk-path")
        .to_string_lossy()
        .to_string();
    for (body, valid) in [
        (serde_json::json!({"diskPaths": []}), true),
        (
            serde_json::json!({"diskPaths": [&unavailable, &unavailable]}),
            true,
        ),
        (serde_json::json!({"diskPaths": ["relative"]}), false),
        (serde_json::json!({"diskPaths": vec!["/"; 17]}), false),
    ] {
        let response = router
            .clone()
            .oneshot(
                Request::post("/api/system/monitor/snapshot")
                    .header("content-type", "application/json")
                    .body(Body::from(body.to_string()))
                    .unwrap(),
            )
            .await
            .unwrap();
        let bytes = response.into_body().collect().await.unwrap().to_bytes();
        let value: serde_json::Value = serde_json::from_slice(&bytes).unwrap();
        assert_eq!(value["code"] == 0, valid, "{value}");
        if valid {
            assert!(value["data"]["cpuInfo"].is_object());
            assert!(value["data"]["memoryInfo"].is_object());
            let disks = value["data"]["disks"].as_object().unwrap();
            if !body["diskPaths"].as_array().unwrap().is_empty() {
                assert_eq!(disks.len(), 1);
                assert!(disks[&unavailable]["error"].is_string());
            }
        }
    }
}
