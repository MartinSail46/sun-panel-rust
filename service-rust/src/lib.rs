pub mod backup;
pub mod config;
pub mod db;
pub mod error;
pub mod instance_lock;
pub mod navigation;
pub mod state;
pub mod system;
pub mod workspace;

use crate::{
    config::Config,
    error::{ApiResult, ok},
    state::AppState,
};
use axum::{
    Router,
    extract::Request,
    extract::{DefaultBodyLimit, Path, State},
    http::{HeaderValue, StatusCode, header},
    middleware::{self, Next},
    response::{IntoResponse, Response},
    routing::{any, get},
};
use std::{sync::Arc, time::Duration};
use tower_http::{
    services::{ServeDir, ServeFile},
    set_header::SetResponseHeaderLayer,
    timeout::TimeoutLayer,
    trace::TraceLayer,
};

pub async fn initialize(mut config: Config) -> anyhow::Result<AppState> {
    tokio::fs::create_dir_all(&config.uploads_dir).await?;
    config.uploads_dir = tokio::fs::canonicalize(&config.uploads_dir).await?;
    let db = db::connect(&config).await?;
    let owner_id = workspace::resolve_owner_id(&db).await?;
    let http = system::http_client(config.allow_private_favicon)?;
    let state = AppState {
        db,
        http,
        config: Arc::new(config),
        monitor: Arc::new(system::new_monitor()),
        owner_id,
    };
    Ok(state)
}

pub fn app(state: AppState) -> Router {
    let web = ServeDir::new(&state.config.web_dir)
        .precompressed_br()
        .precompressed_gzip();
    let body_limit = state.config.max_upload_bytes + 1024 * 1024;
    let request_slots = Arc::new(tokio::sync::Semaphore::new(64));
    Router::new()
        .route("/api/health", get(health))
        .merge(navigation::routes())
        .merge(system::routes())
        .route("/api/{*path}", any(|| async { StatusCode::NOT_FOUND }))
        .route("/uploads/{*path}", get(uploaded_file))
        .fallback_service(web)
        .layer(middleware::from_fn(static_cache_headers))
        .layer(DefaultBodyLimit::max(body_limit))
        .layer(SetResponseHeaderLayer::if_not_present(
            header::X_CONTENT_TYPE_OPTIONS,
            HeaderValue::from_static("nosniff"),
        ))
        .layer(TimeoutLayer::with_status_code(
            StatusCode::REQUEST_TIMEOUT,
            Duration::from_secs(30),
        ))
        .layer(TraceLayer::new_for_http())
        .layer(middleware::from_fn(
            move |mut request: Request, next: Next| {
                let slots = request_slots.clone();
                async move {
                    match slots.try_acquire_owned() {
                        Ok(permit) => {
                            let _permit = permit;
                            if ignores_json_body(&request) {
                                let (mut parts, body) = request.into_parts();
                                // Hyper cannot safely reuse HTTP/1 connections when an
                                // ignored request body arrives after the response.
                                // Drain only the small JSON bodies of bodyless routes;
                                // uploads and actual JSON extractors remain streaming.
                                match tokio::time::timeout(
                                    Duration::from_secs(5),
                                    axum::body::to_bytes(body, 65536),
                                )
                                .await
                                {
                                    Ok(Ok(_)) => {
                                        parts.headers.remove(header::CONTENT_LENGTH);
                                        request =
                                            Request::from_parts(parts, axum::body::Body::empty());
                                    }
                                    _ => {
                                        return error::AppError::bad_request(
                                            "Invalid or oversized request body",
                                        )
                                        .into_response();
                                    }
                                }
                            }
                            next.run(request).await
                        }
                        Err(_) => (
                            StatusCode::SERVICE_UNAVAILABLE,
                            axum::Json(
                                serde_json::json!({"code":1400,"msg":"Server busy; retry shortly"}),
                            ),
                        )
                            .into_response(),
                    }
                }
            },
        ))
        .with_state(state)
}

async fn static_cache_headers(request: Request, next: Next) -> Response {
    let path = request.uri().path().to_owned();
    let mut response = next.run(request).await;
    if path.starts_with("/api/") || path.starts_with("/uploads/") {
        return response;
    }
    if !response.status().is_success() && response.status() != StatusCode::NOT_MODIFIED {
        return response;
    }
    // Only Vite's fingerprinted files are immutable. HTML, custom files and
    // service workers must revalidate so an upgrade can discover new assets.
    let name = path.rsplit('/').next().unwrap_or_default();
    let fingerprinted = path.starts_with("/assets/")
        && name.rsplit_once('.').is_some_and(|(stem, _)| {
            let suffix = &stem.as_bytes()[stem.len().saturating_sub(9)..];
            suffix.len() == 9
                && suffix[0] == b'-'
                && suffix[1..]
                    .iter()
                    .all(|c| c.is_ascii_alphanumeric() || *c == b'_' || *c == b'-')
        });
    response.headers_mut().insert(
        header::CACHE_CONTROL,
        HeaderValue::from_static(if fingerprinted {
            "public, max-age=31536000, immutable"
        } else {
            "no-cache"
        }),
    );
    response
        .headers_mut()
        .append(header::VARY, HeaderValue::from_static("Accept-Encoding"));
    response
}

fn ignores_json_body(request: &Request) -> bool {
    request.method() == axum::http::Method::POST
        && matches!(
            request.uri().path(),
            "/api/about"
                | "/api/panel/itemIconGroup/getList"
                | "/api/panel/userConfig/get"
                | "/api/panel/navigation"
                | "/api/panel/navigation/export"
                | "/api/file/getList"
                | "/api/system/monitor/getDiskMountpoints"
        )
}

async fn uploaded_file(
    State(state): State<AppState>,
    Path(path): Path<String>,
    request: Request,
) -> Response {
    use std::path::Component;
    use tower::ServiceExt;
    let relative = std::path::Path::new(&path);
    if !relative
        .components()
        .all(|part| matches!(part, Component::Normal(_)))
    {
        return StatusCode::NOT_FOUND.into_response();
    }
    let Ok(file) = tokio::fs::canonicalize(state.config.uploads_dir.join(relative)).await else {
        return StatusCode::NOT_FOUND.into_response();
    };
    // Reject migrated symbolic links which point outside the uploads root.
    if !file.starts_with(&state.config.uploads_dir) {
        return StatusCode::NOT_FOUND.into_response();
    }
    let mut response = match ServeFile::new(file).oneshot(request).await {
        Ok(response) => response.into_response(),
        Err(never) => match never {},
    };
    response.headers_mut().insert(
        header::CONTENT_SECURITY_POLICY,
        HeaderValue::from_static("sandbox; default-src 'none'"),
    );
    response
}

async fn health(State(state): State<AppState>) -> ApiResult {
    sqlx::query("SELECT 1").execute(&state.db).await?;
    Ok(ok(
        serde_json::json!({"status":"ok","backend":"rust","version":env!("CARGO_PKG_VERSION")}),
    ))
}
