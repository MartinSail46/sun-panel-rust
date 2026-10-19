//! Exercise the HTTP favicon pipeline against a deterministic local fixture.
//! The test client alone maps a public-style hostname to loopback; production
//! address validation and the production DNS resolver remain unchanged.
use axum::{
    Router,
    body::Body,
    http::{Request, StatusCode, header},
    response::{IntoResponse, Redirect, Response},
    routing::get,
};
use http_body_util::BodyExt;
use serde_json::{Value, json};
use sun_panel_rust::{app, config::Config, initialize, state::AppState};
use tempfile::TempDir;
use tower::ServiceExt;

const PNG: &[u8] = b"\x89PNG\r\n\x1a\n0000000000000000";

struct Fixture {
    _directory: TempDir,
    server: tokio::task::JoinHandle<()>,
    state: AppState,
    router: Router,
    base: String,
}

impl Drop for Fixture {
    fn drop(&mut self) {
        self.server.abort();
    }
}

impl Fixture {
    async fn new() -> Self {
        let routes = Router::new()
            .route(
                "/page",
                get(|| async {
                    r#"<link rel="icon" href="/active.svg"><link rel="shortcut icon" href="icons/image.png">"#
                }),
            )
            .route("/icons/image.png", get(|| async { PNG }))
            .route("/redirect", get(|| async { Redirect::temporary("/page") }))
            .route("/fallback", get(|| async { "<html>No declared icon</html>" }))
            .route("/favicon.ico", get(|| async { PNG }))
            .route(
                "/active-page",
                get(|| async { r#"<link rel="icon" href="/nonimage.png">"# }),
            )
            .route(
                "/private-redirect",
                get(|| async { Redirect::temporary("http://127.0.0.1/secret") }),
            )
            .route("/invalid-redirect", get(|| async { StatusCode::FOUND }))
            .route("/loop", get(|| async { Redirect::temporary("/loop") }))
            .route(
                "/large",
                get(|| async { vec![0_u8; 1024 * 1024 + 1] }),
            )
            .route("/nonimage.png", get(nonimage));
        let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
        let address = listener.local_addr().unwrap();
        let server = tokio::spawn(async move {
            axum::serve(listener, routes).await.unwrap();
        });
        let directory = TempDir::new().unwrap();
        let config = Config {
            bind: "127.0.0.1:0".into(),
            database_path: directory.path().join("database.db"),
            uploads_dir: directory.path().join("uploads"),
            web_dir: directory.path().join("web"),
            max_upload_bytes: 1024 * 1024,
            monitor_interval_secs: 5,
            allow_private_favicon: false,
            worker_threads: 2,
            database_connections: 2,
        };
        let mut state = initialize(config).await.unwrap();
        state.http = reqwest::Client::builder()
            .no_proxy()
            .redirect(reqwest::redirect::Policy::none())
            .resolve("favicon.test", address)
            .build()
            .unwrap();
        let router = app(state.clone());
        Self {
            _directory: directory,
            server,
            state,
            router,
            base: format!("http://favicon.test:{}", address.port()),
        }
    }

    async fn fetch(&self, path: &str) -> Value {
        let response = self
            .router
            .clone()
            .oneshot(
                Request::post("/api/panel/itemIcon/getSiteFavicon")
                    .header(header::CONTENT_TYPE, "application/json")
                    .body(Body::from(
                        json!({"url":format!("{}{path}", self.base)}).to_string(),
                    ))
                    .unwrap(),
            )
            .await
            .unwrap();
        assert_eq!(response.status(), StatusCode::OK);
        serde_json::from_slice(&response.into_body().collect().await.unwrap().to_bytes()).unwrap()
    }
}

async fn nonimage() -> Response {
    // The first response is treated as HTML, and its linked icon must still pass
    // the raster-content check before any file or database row is committed.
    ([(header::CONTENT_TYPE, "image/png")], "<svg onload='bad'/>").into_response()
}

#[tokio::test]
async fn favicon_download_follows_safe_links_redirects_and_fallbacks_and_saves_real_files() {
    let fixture = Fixture::new().await;
    for path in ["/redirect", "/fallback", "/icons/image.png"] {
        let result = fixture.fetch(path).await;
        assert_eq!(result["code"], 0, "{path}: {result}");
        let src = result["data"]["iconUrl"].as_str().unwrap();
        assert_eq!(result["data"]["src"], src);
        let relative = src.strip_prefix("/uploads/").unwrap();
        assert_eq!(
            tokio::fs::read(fixture.state.config.uploads_dir.join(relative))
                .await
                .unwrap(),
            PNG
        );
    }
    let count: i64 = sqlx::query_scalar("SELECT count(*) FROM file WHERE user_id=?")
        .bind(fixture.state.owner_id)
        .fetch_one(&fixture.state.db)
        .await
        .unwrap();
    assert_eq!(count, 3);
}

#[tokio::test]
async fn favicon_download_errors_never_create_files_or_database_rows() {
    let fixture = Fixture::new().await;
    for (path, message) in [
        ("/private-redirect", "address is not permitted"),
        ("/invalid-redirect", "Invalid favicon redirect"),
        ("/loop", "Too many favicon redirects"),
        ("/missing", "unsuccessful status"),
        ("/large", "exceeds size limit"),
        (
            "/active-page",
            "Active SVG/HTML/script content is not allowed",
        ),
    ] {
        let result = fixture.fetch(path).await;
        assert_ne!(result["code"], 0, "{path}: {result}");
        assert!(
            result["msg"].as_str().unwrap().contains(message),
            "{path}: {result}"
        );
    }
    let count: i64 = sqlx::query_scalar("SELECT count(*) FROM file")
        .fetch_one(&fixture.state.db)
        .await
        .unwrap();
    assert_eq!(count, 0);
    for entry in std::fs::read_dir(&fixture.state.config.uploads_dir).unwrap() {
        let entry = entry.unwrap();
        assert_eq!(entry.file_name(), ".staging");
        assert!(std::fs::read_dir(entry.path()).unwrap().next().is_none());
    }
}
