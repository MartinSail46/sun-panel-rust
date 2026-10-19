//! Image storage and demand-driven system sampling with bounded I/O.
use std::{
    collections::{HashMap, HashSet},
    io,
    net::{IpAddr, SocketAddr},
    path::{Component, Path, PathBuf},
    sync::{
        Arc, Mutex, RwLock,
        atomic::{AtomicBool, Ordering},
    },
    time::{Duration, Instant},
};

use axum::{
    Json, Router,
    extract::{Multipart, State},
    http::HeaderMap,
    routing::post,
};
use chrono::{Datelike, Utc};
use reqwest::{
    Url,
    dns::{Addrs, Name, Resolve, Resolving},
};
use scraper::{Html, Selector};
use serde::Deserialize;
use serde_json::{Value, json};
use sqlx::{QueryBuilder, Row, Sqlite, SqliteConnection};
use sysinfo::{Disks, System};
use tokio::{io::AsyncWriteExt, sync::Notify};
use uuid::Uuid;

use crate::{
    AppState,
    error::{self, ApiResult, AppError},
};

fn ok(data: impl serde::Serialize) -> ApiResult {
    Ok(error::ok(data))
}
fn list(data: impl serde::Serialize, count: i64) -> ApiResult {
    Ok(error::list(data, count))
}
fn success() -> ApiResult {
    Ok(error::success())
}

pub fn routes() -> Router<AppState> {
    Router::new()
        .route("/api/about", post(about))
        .route("/api/file/uploadImg", post(upload_image))
        .route("/api/file/getList", post(files))
        .route("/api/file/deletes", post(delete_files))
        .route("/api/system/monitor/snapshot", post(monitor_batch))
        .route("/api/system/monitor/getDiskMountpoints", post(mountpoints))
        .route("/api/panel/itemIcon/getSiteFavicon", post(favicon))
}

async fn about() -> ApiResult {
    ok(
        json!({"backend": "rust", "backendVersion": env!("CARGO_PKG_VERSION"),
        "sourceBaseline": "v1.8.1 / 25f4620"}),
    )
}

// Only passive raster formats are served as images. Client names and Content-Type
// never determine the stored extension.
fn image_extension(data: &[u8]) -> Option<&'static str> {
    if data.len() >= 24 && data.starts_with(b"\x89PNG\r\n\x1a\n") {
        Some("png")
    } else if data.len() >= 12 && data.starts_with(b"\xff\xd8\xff") {
        Some("jpg")
    } else if data.len() >= 13 && (data.starts_with(b"GIF87a") || data.starts_with(b"GIF89a")) {
        Some("gif")
    } else if data.len() >= 16 && data.starts_with(b"RIFF") && &data[8..12] == b"WEBP" {
        Some("webp")
    } else if data.len() >= 22 && data.starts_with(b"\x00\x00\x01\x00") {
        Some("ico")
    } else {
        None
    }
}
fn active_content(data: &[u8], name: &str) -> bool {
    let extension = Path::new(name)
        .extension()
        .and_then(|s| s.to_str())
        .unwrap_or_default()
        .to_ascii_lowercase();
    if ["svg", "svgz", "html", "htm", "xhtml", "xml", "js", "mjs"].contains(&extension.as_str()) {
        return true;
    }
    let prefix = String::from_utf8_lossy(&data[..data.len().min(1024)])
        .trim_start_matches('\u{feff}')
        .trim_start()
        .to_ascii_lowercase();
    prefix.starts_with("<svg")
        || prefix.starts_with("<!doctype html")
        || prefix.starts_with("<html")
        || prefix.starts_with("<?xml")
}
fn clean_filename(name: &str) -> String {
    name.rsplit(['/', '\\'])
        .next()
        .unwrap_or("upload")
        .chars()
        .filter(|c| !c.is_control())
        .take(255)
        .collect()
}

// A failed upload never leaves its temporary/final file behind. The guard also
// handles cancellation (for example the request timeout) while writing a file.
type UploadLease = Arc<tokio::sync::OwnedSemaphorePermit>;
struct PendingUpload {
    path: PathBuf,
    keep: bool,
    _lease: UploadLease,
}
impl Drop for PendingUpload {
    fn drop(&mut self) {
        if !self.keep
            && let Err(err) = std::fs::remove_file(&self.path)
            && err.kind() != io::ErrorKind::NotFound
        {
            tracing::warn!(path = %self.path.display(), %err, "Could not clean up failed upload");
        }
    }
}
async fn staging_upload(
    state: &AppState,
    lease: &UploadLease,
) -> Result<(PendingUpload, tokio::fs::File), AppError> {
    let directory = state.config.uploads_dir.join(".staging");
    tokio::fs::create_dir_all(&directory).await?;
    let path = directory.join(Uuid::new_v4().to_string());
    let lease = lease.clone();
    // Register cleanup in the same worker that creates the file. A cancelled
    // await cannot leave a file created later by the blocking filesystem call.
    tokio::task::spawn_blocking(move || {
        let file = std::fs::OpenOptions::new()
            .create_new(true)
            .write(true)
            .open(&path)?;
        Ok((
            PendingUpload {
                path,
                keep: false,
                _lease: lease,
            },
            tokio::fs::File::from_std(file),
        ))
    })
    .await
    .map_err(|_| AppError::new(1200, "Upload staging failed"))?
}
async fn move_staged_upload(
    mut pending: PendingUpload,
    path: PathBuf,
) -> Result<PendingUpload, AppError> {
    tokio::task::spawn_blocking(move || {
        std::fs::rename(&pending.path, &path)?;
        pending.path = path;
        Ok(pending)
    })
    .await
    .map_err(|_| AppError::new(1200, "Upload finalization failed"))?
}
async fn finish_upload(
    state: &AppState,
    user_id: i64,
    filename: &str,
    preview: &[u8],
    size: usize,
    pending: PendingUpload,
) -> Result<String, AppError> {
    if size == 0 || size > state.config.max_upload_bytes {
        return Err(AppError::bad_request(
            "Upload is empty or exceeds the configured limit",
        ));
    }
    let filename = clean_filename(filename);
    if active_content(preview, &filename) {
        return Err(AppError::bad_request(
            "Active SVG/HTML/script content is not allowed",
        ));
    }
    let ext = image_extension(preview).ok_or_else(|| {
        AppError::new(
            1301,
            "Only PNG, JPEG, GIF, WebP and ICO images are supported",
        )
    })?;
    let date = Utc::now();
    let relative = format!(
        "{}/{}/{}/{}.{}",
        date.year(),
        date.month(),
        date.day(),
        Uuid::new_v4(),
        ext
    );
    let path = state.config.uploads_dir.join(&relative);
    let state = state.clone();
    // Ownership crosses the request boundary before rename/INSERT. If the HTTP
    // future disappears, this operation still commits both parts or cleans up.
    // PendingUpload owns the semaphore lease until the real operation finishes.
    tokio::spawn(async move {
        tokio::fs::create_dir_all(path.parent().expect("generated upload directory")).await?;
        let mut pending = move_staged_upload(pending,path).await?;
        let src = format!("./uploads/{relative}");
        let timestamp = date.to_rfc3339();
        sqlx::query("INSERT INTO file (created_at,updated_at,src,user_id,file_name,method,ext) VALUES (?,?,?,?,?,0,?)")
            .bind(&timestamp).bind(&timestamp).bind(&src).bind(user_id).bind(&filename).bind(format!(".{ext}"))
            .execute(&state.db).await?;
        pending.keep = true;
        Ok::<String,AppError>(format!("/uploads/{relative}"))
    }).await.map_err(|_|AppError::new(1200,"Upload finalization failed"))?
}
async fn save_upload(
    state: &AppState,
    user_id: i64,
    filename: &str,
    bytes: &[u8],
    lease: &UploadLease,
) -> Result<String, AppError> {
    if bytes.is_empty() || bytes.len() > state.config.max_upload_bytes {
        return Err(AppError::bad_request(
            "Upload is empty or exceeds the configured limit",
        ));
    }
    let (pending, mut file) = staging_upload(state, lease).await?;
    file.write_all(bytes).await?;
    file.flush().await?;
    drop(file);
    finish_upload(state, user_id, filename, bytes, bytes.len(), pending).await
}
async fn stream_upload(
    state: &AppState,
    user_id: i64,
    mut field: axum::extract::multipart::Field<'_>,
    lease: &UploadLease,
) -> Result<String, AppError> {
    let filename = clean_filename(field.file_name().unwrap_or("upload"));
    let (pending, mut file) = staging_upload(state, lease).await?;
    let mut preview = Vec::with_capacity(1024);
    let mut size = 0usize;
    while let Some(chunk) = field
        .chunk()
        .await
        .map_err(|_| AppError::bad_request("Invalid multipart upload"))?
    {
        size = size.saturating_add(chunk.len());
        if size > state.config.max_upload_bytes {
            return Err(AppError::bad_request("Upload exceeds the configured limit"));
        }
        let remaining = 1024usize.saturating_sub(preview.len());
        preview.extend_from_slice(&chunk[..remaining.min(chunk.len())]);
        file.write_all(&chunk).await?;
    }
    file.flush().await?;
    drop(file);
    finish_upload(state, user_id, &filename, &preview, size, pending).await
}
async fn owned_upload<F>(lease: UploadLease, work: F) -> ApiResult
where
    F: std::future::Future<Output = ApiResult> + Send + 'static,
{
    tokio::spawn(async move {
        let _lease = lease;
        tokio::time::timeout(Duration::from_secs(30), work)
            .await
            .map_err(|_| AppError::new(1300, "Upload timed out"))?
    })
    .await
    .map_err(|_| AppError::new(1300, "Upload failed"))?
}
async fn upload_image(State(state): State<AppState>, mut multipart: Multipart) -> ApiResult {
    let lease = Arc::new(
        state
            .monitor
            .upload_jobs
            .clone()
            .try_acquire_owned()
            .map_err(|_| AppError::bad_request("Too many concurrent uploads; retry shortly"))?,
    );
    let worker_lease = lease.clone();
    owned_upload(lease, async move {
        while let Some(field) = multipart
            .next_field()
            .await
            .map_err(|_| AppError::bad_request("Invalid multipart upload"))?
        {
            if field.name() != Some("imgfile") {
                continue;
            }
            let src = stream_upload(&state, state.owner_id, field, &worker_lease).await?;
            return ok(json!({"imageUrl":src}));
        }
        Err(AppError::new(1300, "Missing imgfile upload"))
    })
    .await
}
pub(crate) fn upload_reference_key(src: &str, headers: &HeaderMap) -> Option<String> {
    let src = src.trim();
    let absolute =
        src.starts_with("http://") || src.starts_with("https://") || src.starts_with("//");
    let base = Url::parse("http://uploads.invalid/").ok()?;
    let url = if src.starts_with("//") {
        let scheme = headers
            .get("origin")
            .and_then(|value| value.to_str().ok())
            .and_then(|value| Url::parse(value).ok())
            .map(|origin| origin.scheme().to_owned())
            .filter(|scheme| matches!(scheme.as_str(), "http" | "https"))
            .unwrap_or_else(|| "http".to_owned());
        Url::parse(&format!("{scheme}:{src}")).ok()?
    } else {
        base.join(src).ok()?
    };
    if absolute {
        let host = headers.get("host")?.to_str().ok()?;
        let request_base = Url::parse(&format!("{}://{host}/", url.scheme())).ok()?;
        if url.host_str() != request_base.host_str()
            || url.port_or_known_default() != request_base.port_or_known_default()
        {
            return None;
        }
        if let Some(origin) = headers
            .get("origin")
            .and_then(|v| v.to_str().ok())
            .and_then(|v| Url::parse(v).ok())
            && url.origin() != origin.origin()
        {
            return None;
        }
    } else if url.host_str() != Some("uploads.invalid") {
        return None;
    }
    let relative = url.path().strip_prefix("/uploads/")?;
    let bytes = relative.as_bytes();
    let mut decoded = Vec::with_capacity(bytes.len());
    let mut index = 0;
    while index < bytes.len() {
        if bytes[index] == b'%' {
            let high = (char::from(*bytes.get(index + 1)?)).to_digit(16)?;
            let low = (char::from(*bytes.get(index + 2)?)).to_digit(16)?;
            decoded.push((high * 16 + low) as u8);
            index += 3;
        } else {
            decoded.push(bytes[index]);
            index += 1;
        }
    }
    let decoded = String::from_utf8(decoded).ok()?;
    if decoded.is_empty()
        || decoded.contains(['\\', '\0'])
        || !Path::new(&decoded)
            .components()
            .all(|part| matches!(part, Component::Normal(_)))
    {
        return None;
    }
    Some(
        Path::new(&decoded)
            .components()
            .map(|part| part.as_os_str().to_string_lossy())
            .collect::<Vec<_>>()
            .join("/"),
    )
}

fn css_upload_sources(css: &str) -> Vec<String> {
    let lower = css.to_ascii_lowercase();
    let mut sources = Vec::new();
    let mut offset = 0;
    while let Some(start) = lower[offset..].find("url(") {
        let body = &css[offset + start + 4..];
        let Some(end) = body.find(')') else { break };
        let candidate = body[..end].trim();
        let candidate = if candidate.starts_with(['\'', '"']) {
            candidate
                .get(1..candidate.len().saturating_sub(1))
                .unwrap_or_default()
        } else {
            candidate
        };
        sources.push(candidate.to_owned());
        offset += start + 4 + end + 1;
    }
    sources
}

fn footer_upload_sources(footer: &str) -> Vec<(String, String)> {
    let document = Html::parse_fragment(footer);
    let selector = Selector::parse("[src],[href],[poster],[srcset],[style],style")
        .expect("constant upload selector");
    let mut sources = Vec::new();
    for element in document.select(&selector) {
        for field in ["src", "href", "poster"] {
            if let Some(src) = element.value().attr(field) {
                sources.push((src.to_owned(), field.to_owned()));
            }
        }
        if let Some(srcset) = element.value().attr("srcset") {
            sources.extend(
                srcset
                    .split(',')
                    .filter_map(|candidate| candidate.split_whitespace().next())
                    .map(|src| (src.to_owned(), "srcset".to_owned())),
            );
        }
        if let Some(style) = element.value().attr("style") {
            sources.extend(
                css_upload_sources(style)
                    .into_iter()
                    .map(|src| (src, "style".to_owned())),
            );
        }
        if element.value().name() == "style" {
            sources.extend(
                css_upload_sources(&element.text().collect::<String>())
                    .into_iter()
                    .map(|src| (src, "style".to_owned())),
            );
        }
    }
    sources
}

pub(crate) fn panel_upload_sources(panel: &Value) -> Vec<String> {
    let mut sources: Vec<String> = ["backgroundImageSrc"]
        .into_iter()
        .filter_map(|field| panel[field].as_str().map(str::to_owned))
        .collect();
    if let Some(footer) = panel["footerHtml"].as_str() {
        sources.extend(
            footer_upload_sources(footer)
                .into_iter()
                .map(|(src, _)| src),
        );
    }
    sources
}

async fn upload_references(
    connection: &mut SqliteConnection,
    headers: &HeaderMap,
) -> Result<HashMap<String, Vec<Value>>, AppError> {
    let mut references: HashMap<String, Vec<Value>> = HashMap::new();
    let mut add = |src: &str, reference: Value| {
        if let Some(key) = upload_reference_key(src, headers) {
            let entries = references.entry(key).or_default();
            if !entries.contains(&reference) {
                entries.push(reference);
            }
        }
    };
    for row in sqlx::query("SELECT id,user_id,title,icon_json,deleted_at FROM item_icon")
        .fetch_all(&mut *connection)
        .await?
    {
        if let Ok(icon) = serde_json::from_str::<Value>(row.get::<&str, _>("icon_json"))
            && icon["itemType"].as_i64() == Some(2)
            && let Some(src) = icon["src"].as_str()
        {
            add(
                src,
                json!({"type":"navigation","recordId":row.get::<i64,_>("id"),"userId":row.get::<i64,_>("user_id"),"title":row.get::<&str,_>("title"),"field":"icon.src","inTrash":row.get::<Option<String>,_>("deleted_at").is_some()}),
            );
        }
    }
    for row in sqlx::query("SELECT id,user_id,title,icon,deleted_at FROM item_icon_group")
        .fetch_all(&mut *connection)
        .await?
    {
        add(
            row.get::<&str, _>("icon"),
            json!({"type":"group","recordId":row.get::<i64,_>("id"),"userId":row.get::<i64,_>("user_id"),"title":row.get::<&str,_>("title"),"field":"icon","inTrash":row.get::<Option<String>,_>("deleted_at").is_some()}),
        );
    }
    for row in sqlx::query("SELECT user_id,panel_json FROM user_config")
        .fetch_all(&mut *connection)
        .await?
    {
        let user_id = row.get::<i64, _>("user_id");
        if let Ok(panel) = serde_json::from_str::<Value>(row.get::<&str, _>("panel_json")) {
            if let Some(src) = panel["backgroundImageSrc"].as_str() {
                add(
                    src,
                    json!({"type":"wallpaper","userId":user_id,"field":"backgroundImageSrc"}),
                );
            }
            if let Some(footer) = panel["footerHtml"].as_str() {
                for (src, field) in footer_upload_sources(footer) {
                    add(
                        &src,
                        json!({"type":"footer","userId":user_id,"field":format!("footerHtml.{field}")}),
                    );
                }
            }
        }
    }
    Ok(references)
}

async fn files(State(state): State<AppState>, headers: HeaderMap) -> ApiResult {
    let mut connection = state.db.acquire().await?;
    let references = upload_references(&mut connection, &headers).await?;
    let rows = sqlx::query("SELECT id,src,file_name,created_at,updated_at FROM file WHERE user_id = ? AND deleted_at IS NULL ORDER BY created_at DESC,id DESC")
        .bind(state.owner_id).fetch_all(&mut *connection).await?;
    let data: Vec<Value> = rows.into_iter().map(|row| {
        let src: String = row.get("src");
        let usages = upload_reference_key(&src, &headers).and_then(|key| references.get(&key)).cloned().unwrap_or_default();
        json!({"id":row.get::<i64,_>("id"),"src":src.strip_prefix('.').unwrap_or(&src),"path":src,
            "fileName":row.get::<String,_>("file_name"),"createTime":row.get::<Option<String>,_>("created_at"),"updateTime":row.get::<Option<String>,_>("updated_at"),"referenceCount":usages.len(),"references":usages})
    }).collect();
    let count = data.len() as i64;
    list(data, count)
}

#[derive(Deserialize)]
struct DeleteRequest {
    ids: Vec<i64>,
}
fn upload_relative(src: &str) -> Result<&Path, AppError> {
    let relative = src
        .strip_prefix("./uploads/")
        .or_else(|| src.strip_prefix("/uploads/"))
        .or_else(|| src.strip_prefix("uploads/"))
        .ok_or_else(|| AppError::bad_request("File record is outside the upload directory"))?;
    let path = Path::new(relative);
    if path.as_os_str().is_empty()
        || !path
            .components()
            .all(|part| matches!(part, Component::Normal(_)))
    {
        return Err(AppError::bad_request("Invalid stored file path"));
    }
    Ok(path)
}
// A staged filesystem change must survive cancellation of the async request.
// Each synchronous rename and its guard registration run together on the blocking
// pool. If the join handle/result is dropped, the returned guard restores files.
struct StagedDeletion {
    moved: Vec<(PathBuf, PathBuf)>,
    committed: bool,
}
impl Drop for StagedDeletion {
    fn drop(&mut self) {
        if self.committed {
            return;
        }
        for (source, trash) in self.moved.iter().rev() {
            if let Err(err) = std::fs::rename(trash, source) {
                tracing::error!(path=%source.display(),staged=%trash.display(),%err,"Cannot restore cancelled file deletion");
            }
        }
    }
}
async fn stage_deletions(root: PathBuf, sources: Vec<PathBuf>) -> Result<StagedDeletion, AppError> {
    tokio::task::spawn_blocking(move || {
        let mut staged = StagedDeletion {
            moved: Vec::with_capacity(sources.len()),
            committed: false,
        };
        for source in sources {
            match std::fs::canonicalize(&source) {
                Ok(real) if real.starts_with(&root) => {
                    if !std::fs::metadata(&real)?.is_file() {
                        return Err(AppError::bad_request("Stored upload is not a regular file"));
                    }
                    let trash = source.with_file_name(format!(".delete-{}", Uuid::new_v4()));
                    std::fs::rename(&source, &trash)?;
                    staged.moved.push((source, trash));
                }
                Ok(_) => {
                    return Err(AppError::new(
                        1005,
                        "Stored file path escapes the upload directory",
                    ));
                }
                Err(err) if err.kind() == io::ErrorKind::NotFound => {}
                Err(err) => return Err(err.into()),
            }
        }
        Ok(staged)
    })
    .await
    .map_err(|_| AppError::new(1200, "Cannot stage file deletion"))?
}
fn validate_delete_ids(ids: &[i64]) -> Result<(), AppError> {
    if ids.is_empty()
        || ids.len() > 500
        || ids.iter().any(|id| *id <= 0)
        || ids.iter().collect::<HashSet<_>>().len() != ids.len()
    {
        return Err(AppError::bad_request(
            "Provide 1 to 500 unique positive file IDs",
        ));
    }
    Ok(())
}
async fn delete_files(
    State(state): State<AppState>,
    headers: HeaderMap,
    Json(req): Json<DeleteRequest>,
) -> ApiResult {
    validate_delete_ids(&req.ids)?;
    let requested_count = req.ids.len();
    let mut tx = state.db.begin_with("BEGIN IMMEDIATE").await?;
    let mut query = QueryBuilder::<Sqlite>::new(
        "SELECT id,src FROM file WHERE deleted_at IS NULL AND user_id = ",
    );
    query.push_bind(state.owner_id).push(" AND id IN (");
    let mut values = query.separated(",");
    for id in req.ids {
        values.push_bind(id);
    }
    values.push_unseparated(")");
    let rows = query.build().fetch_all(&mut *tx).await?;
    if rows.len() != requested_count {
        return Err(AppError::new(
            1005,
            "A file does not exist or belongs to another user",
        ));
    }
    // The write lock precedes the reference read, so another configuration
    // transaction cannot introduce a reference between this check and deletion.
    let references = upload_references(&mut tx, &headers).await?;
    let blocked: Vec<Value> = rows.iter().filter_map(|row| {
        let src = row.get::<&str,_>("src");
        let usages = references.get(&upload_reference_key(src, &headers)?)?;
        Some(json!({"id":row.get::<i64,_>("id"),"referenceCount":usages.len(),"references":usages}))
    }).collect();
    if !blocked.is_empty() {
        return Ok(Json(
            json!({"code":1310,"msg":"File is in use. Replace active references or permanently remove its recycle bin entries before deleting it.","data":{"files":blocked}}),
        ));
    }
    let root = tokio::fs::canonicalize(&state.config.uploads_dir).await?;
    let sources = rows
        .iter()
        .map(|row| upload_relative(row.get::<&str, _>("src")).map(|relative| root.join(relative)))
        .collect::<Result<Vec<_>, _>>()?;
    let mut staged = stage_deletions(root, sources).await?;
    let timestamp = Utc::now().to_rfc3339();
    for row in &rows {
        let changed = sqlx::query("UPDATE file SET deleted_at = ?,updated_at = ? WHERE id = ? AND user_id = ? AND deleted_at IS NULL")
            .bind(&timestamp).bind(&timestamp).bind(row.get::<i64,_>("id")).bind(state.owner_id).execute(&mut *tx).await?.rows_affected();
        if changed != 1 {
            return Err(AppError::new(1200, "File changed during deletion"));
        }
    }
    // Complete the commit/cleanup as one owned operation. A disconnected client
    // cannot cancel after COMMIT succeeds but before the guard is disarmed.
    tokio::spawn(async move {
        tx.commit().await?;
        staged.committed = true;
        for (_,trash) in &staged.moved {
            if let Err(err) = tokio::fs::remove_file(trash).await {
                tracing::warn!(path=%trash.display(),%err,"Unable to remove deleted upload; hidden file retained");
            }
        }
        Ok::<(),AppError>(())
    }).await.map_err(|_|AppError::new(1200,"File deletion failed"))??;
    success()
}

/// Sampling is triggered only by requests. CPU/memory share one snapshot and
/// concurrent requests share one refresh. There are no idle timers or process scans.
pub struct MonitorState {
    system: Mutex<System>,
    snapshot: RwLock<Option<(Instant, Value)>>,
    refreshing: AtomicBool,
    ready: Notify,
    disks: RwLock<Option<(Instant, Value)>>,
    disk_gate: tokio::sync::Mutex<()>,
    upload_jobs: Arc<tokio::sync::Semaphore>,
    favicon_jobs: Arc<tokio::sync::Semaphore>,
}
pub fn new_monitor() -> MonitorState {
    MonitorState {
        system: Mutex::new(System::new()),
        snapshot: RwLock::new(None),
        refreshing: AtomicBool::new(false),
        ready: Notify::new(),
        disks: RwLock::new(None),
        disk_gate: tokio::sync::Mutex::new(()),
        upload_jobs: Arc::new(tokio::sync::Semaphore::new(4)),
        favicon_jobs: Arc::new(tokio::sync::Semaphore::new(2)),
    }
}
fn sample_system(monitor: &MonitorState) -> Value {
    let mut system = monitor.system.lock().unwrap_or_else(|p| p.into_inner());
    if system.cpus().is_empty() {
        system.refresh_cpu_all();
        // CPU percentage needs two readings; this runs on the blocking pool once.
        std::thread::sleep(sysinfo::MINIMUM_CPU_UPDATE_INTERVAL);
    }
    system.refresh_cpu_usage();
    system.refresh_memory();
    let total = system.total_memory();
    let used = system.used_memory();
    json!({"sampledAt":Utc::now().to_rfc3339(), "scope":{"source":"operating-system","cpu":"OS-visible logical CPUs","memory":"OS-visible memory","disk":"mounted filesystem","containerQuotaApplied":false},"cpuInfo": {"coreCount":System::physical_core_count().unwrap_or(system.cpus().len()),
        "cpuNum": system.cpus().len(), "model":system.cpus().first().map(|cpu|cpu.brand()).unwrap_or(""),
        "usages":[system.global_cpu_usage()]},
        "memoryInfo":{"total":total,"used":used,"free":system.free_memory(),"available":system.available_memory(),
            "usedPercent":if total == 0 {0.0} else {used as f64 / total as f64 * 100.0}}})
}
fn fresh_snapshot(monitor: &MonitorState, ttl: Duration) -> Option<Value> {
    monitor
        .snapshot
        .read()
        .unwrap_or_else(|p| p.into_inner())
        .as_ref()
        .filter(|(at, _)| at.elapsed() < ttl)
        .map(|(_, value)| value.clone())
}
async fn monitor_snapshot(state: &AppState) -> Result<Value, AppError> {
    let ttl = Duration::from_secs(state.config.monitor_interval_secs.max(1));
    if let Some(value) = fresh_snapshot(&state.monitor, ttl) {
        return Ok(value);
    }
    let notified = state.monitor.ready.notified();
    tokio::pin!(notified);
    notified.as_mut().enable();
    // Another request may have completed its refresh while this one registered.
    if let Some(value) = fresh_snapshot(&state.monitor, ttl) {
        return Ok(value);
    }
    if !state.monitor.refreshing.swap(true, Ordering::AcqRel) {
        let monitor = state.monitor.clone();
        tokio::spawn(async move {
            let worker = monitor.clone();
            match tokio::task::spawn_blocking(move || sample_system(&worker)).await {
                Ok(value) => {
                    *monitor.snapshot.write().unwrap_or_else(|p| p.into_inner()) =
                        Some((Instant::now(), value));
                }
                Err(err) => tracing::error!(%err,"System sampler failed"),
            }
            monitor.refreshing.store(false, Ordering::Release);
            monitor.ready.notify_waiters();
        });
    }
    // Every expired read asynchronously awaits the same refresh. Returning an old
    // snapshot here would let browser caches extend stale data by another cycle.
    tokio::time::timeout(Duration::from_secs(2), notified)
        .await
        .map_err(|_| AppError::new(1200, "System monitor refresh timed out; retry shortly"))?;
    fresh_snapshot(&state.monitor, ttl)
        .ok_or_else(|| AppError::new(1200, "System monitor is warming up; retry shortly"))
}
#[derive(Default, Deserialize)]
#[serde(rename_all = "camelCase")]
struct MonitorBatchRequest {
    #[serde(default)]
    disk_paths: Vec<String>,
}

async fn monitor_batch(
    State(state): State<AppState>,
    Json(request): Json<MonitorBatchRequest>,
) -> ApiResult {
    if request.disk_paths.len() > 16 {
        return Err(AppError::bad_request("At most 16 disk paths are allowed"));
    }
    for path in &request.disk_paths {
        if path.len() > 4096 || !Path::new(path).is_absolute() {
            return Err(AppError::bad_request("An absolute disk path is required"));
        }
    }
    let mut snapshot = monitor_snapshot(&state).await?;
    let mut results = serde_json::Map::new();
    if !request.disk_paths.is_empty() {
        let disks = cached_disks(&state).await;
        for path in request.disk_paths {
            if results.contains_key(&path) {
                continue;
            }
            let result = match &disks {
                Ok(disks) => match tokio::fs::canonicalize(&path).await {
                    Ok(canonical) => match disk_for_path(disks, &canonical) {
                        Some(disk) => json!({"data": disk}),
                        None => json!({"error": "No disk found for path"}),
                    },
                    Err(_) => json!({"error": "Disk path is unavailable"}),
                },
                Err(_) => json!({"error": "Disk monitor failed"}),
            };
            results.insert(path, result);
        }
    }
    snapshot["disks"] = Value::Object(results);
    ok(snapshot)
}
fn disk_for_path(disks: &Value, canonical: &Path) -> Option<Value> {
    disks
        .as_array()
        .into_iter()
        .flatten()
        .filter(|entry| {
            entry["mountpoint"]
                .as_str()
                .is_some_and(|mount| canonical.starts_with(mount))
        })
        .max_by_key(|entry| entry["mountpoint"].as_str().map_or(0, str::len))
        .cloned()
}
fn disk_values(disks: &Disks) -> Vec<Value> {
    disks.list().iter().map(|disk| {
        let total = disk.total_space();
        let free = disk.available_space();
        let used = total.saturating_sub(free);
        json!({"mountpoint":disk.mount_point().to_string_lossy(), "device":disk.name().to_string_lossy(),
            "fstype":disk.file_system().to_string_lossy(), "total":total, "free":free, "used":used,
            "usedPercent":if total == 0 {0.0} else {used as f64 / total as f64 * 100.0}})
    }).collect()
}
async fn cached_disks(state: &AppState) -> Result<Value, AppError> {
    let ttl = Duration::from_secs(state.config.monitor_interval_secs.max(1));
    let cached = || {
        state
            .monitor
            .disks
            .read()
            .unwrap_or_else(|p| p.into_inner())
            .as_ref()
            .filter(|(at, _)| at.elapsed() < ttl)
            .map(|(_, v)| v.clone())
    };
    if let Some(data) = cached() {
        return Ok(data);
    }
    let _guard = state.monitor.disk_gate.lock().await;
    if let Some(data) = cached() {
        return Ok(data);
    }
    let values = tokio::task::spawn_blocking(|| disk_values(&Disks::new_with_refreshed_list()))
        .await
        .map_err(|_| AppError::new(1200, "Disk monitor failed"))?;
    let value = json!(values);
    *state
        .monitor
        .disks
        .write()
        .unwrap_or_else(|p| p.into_inner()) = Some((Instant::now(), value.clone()));
    Ok(value)
}
async fn mountpoints(State(state): State<AppState>) -> ApiResult {
    ok(cached_disks(&state).await?)
}

// Validate at the actual DNS-resolution boundary, so a hostname cannot rebind
// from an approved address to a forbidden address between validation and connect.
#[derive(Debug)]
struct SafeResolver {
    allow_private: bool,
}
impl Resolve for SafeResolver {
    fn resolve(&self, name: Name) -> Resolving {
        let host = name.as_str().to_owned();
        let allow_private = self.allow_private;
        Box::pin(async move {
            let addresses: Vec<SocketAddr> = tokio::net::lookup_host((host.as_str(), 0))
                .await?
                .filter(|address| permitted_address(address.ip(), allow_private))
                .collect();
            if addresses.is_empty() {
                return Err(Box::new(io::Error::new(
                    io::ErrorKind::PermissionDenied,
                    "Favicon address is not permitted",
                ))
                    as Box<dyn std::error::Error + Send + Sync>);
            }
            Ok(Box::new(addresses.into_iter()) as Addrs)
        })
    }
}
fn permitted_address(address: IpAddr, allow_private: bool) -> bool {
    match address {
        IpAddr::V4(ip) => {
            let [a, b, c, d] = ip.octets();
            if ip.is_loopback()
                || ip.is_link_local()
                || ip.is_unspecified()
                || ip.is_multicast()
                || ip.is_broadcast()
                || a == 0
                || a >= 240
                || (a == 100 && (64..=127).contains(&b))
                || (a == 192 && b == 0 && (c == 0 || c == 2))
                || (a == 192 && b == 88 && c == 99)
                || (a == 198 && (b == 18 || b == 19 || (b == 51 && c == 100)))
                || (a == 203 && b == 0 && c == 113)
                || [a, b, c, d] == [168, 63, 129, 16]
            {
                return false;
            }
            allow_private || !ip.is_private()
        }
        IpAddr::V6(ip) => {
            if let Some(v4) = ip.to_ipv4_mapped() {
                return permitted_address(IpAddr::V4(v4), allow_private);
            }
            if ip.is_loopback()
                || ip.is_unspecified()
                || ip.is_multicast()
                || ip.is_unicast_link_local()
            {
                return false;
            }
            // Deny cloud metadata even with the explicit NAS/private-network opt-in.
            let segments = ip.segments();
            if segments == [0xfd00, 0x0ec2, 0, 0, 0, 0, 0, 0x0254] {
                return false;
            }
            if ip.is_unique_local() {
                return allow_private;
            }
            // Only global unicast space, excluding documentation and tunnel ranges.
            (segments[0] & 0xe000) == 0x2000
                && segments[0] != 0x2002
                && !(segments[0] == 0x2001 && (segments[1] == 0 || segments[1] == 0x0db8))
        }
    }
}
pub fn http_client(allow_private: bool) -> Result<reqwest::Client, reqwest::Error> {
    reqwest::Client::builder()
        .user_agent(concat!("Sun-Panel-Rust/", env!("CARGO_PKG_VERSION")))
        .timeout(Duration::from_secs(10))
        .connect_timeout(Duration::from_secs(3))
        .pool_max_idle_per_host(2)
        .pool_idle_timeout(Duration::from_secs(30))
        .no_proxy()
        .redirect(reqwest::redirect::Policy::none())
        .dns_resolver(Arc::new(SafeResolver { allow_private }))
        .build()
}
fn validate_url(url: &Url, allow_private: bool) -> Result<(), AppError> {
    if !["http", "https"].contains(&url.scheme())
        || !url.username().is_empty()
        || url.password().is_some()
        || url.host_str().is_none()
    {
        return Err(AppError::bad_request(
            "Only HTTP(S) URLs without credentials are allowed",
        ));
    }
    if url.as_str().len() > 4096 {
        return Err(AppError::bad_request("URL is too long"));
    }
    // Literal IPs bypass DNS, so enforce the same policy here.
    if let Some(host) = url.host_str()
        && let Ok(ip) = host.trim_matches(['[', ']']).parse::<IpAddr>()
        && !permitted_address(ip, allow_private)
    {
        return Err(AppError::bad_request("Favicon address is not permitted"));
    }
    Ok(())
}
async fn download(
    state: &AppState,
    mut url: Url,
    limit: usize,
) -> Result<(Url, Vec<u8>), AppError> {
    for _ in 0..=4 {
        validate_url(&url, state.config.allow_private_favicon)?;
        let mut response = state.http.get(url.clone()).send().await?;
        if response.status().is_redirection() {
            let location = response
                .headers()
                .get(reqwest::header::LOCATION)
                .and_then(|h| h.to_str().ok())
                .ok_or_else(|| AppError::bad_request("Invalid favicon redirect"))?;
            url = url
                .join(location)
                .map_err(|_| AppError::bad_request("Invalid favicon redirect URL"))?;
            continue;
        }
        if !response.status().is_success() {
            return Err(AppError::bad_request(
                "Remote site returned an unsuccessful status",
            ));
        }
        if response
            .content_length()
            .is_some_and(|size| size > limit as u64)
        {
            return Err(AppError::bad_request("Remote content exceeds size limit"));
        }
        let mut bytes = Vec::new();
        while let Some(chunk) = response.chunk().await? {
            if bytes.len().saturating_add(chunk.len()) > limit {
                return Err(AppError::bad_request("Remote content exceeds size limit"));
            }
            bytes.extend_from_slice(&chunk);
        }
        return Ok((url, bytes));
    }
    Err(AppError::bad_request("Too many favicon redirects"))
}
#[derive(Deserialize)]
struct FaviconRequest {
    url: String,
}
fn favicon_candidate(page: &[u8], base: &Url, allow_private: bool) -> Option<Url> {
    // Keep the non-Send HTML tree entirely outside the asynchronous network path.
    let document = Html::parse_document(&String::from_utf8_lossy(page));
    let selector = Selector::parse("link[href][rel]").expect("constant favicon selector");
    document.select(&selector).find_map(|element| {
        let rel = element.value().attr("rel")?;
        if !rel.split_ascii_whitespace().any(|word| {
            word.eq_ignore_ascii_case("icon") || word.eq_ignore_ascii_case("apple-touch-icon")
        }) {
            return None;
        }
        let url = base.join(element.value().attr("href")?).ok()?;
        if url.path().to_ascii_lowercase().ends_with(".svg")
            || validate_url(&url, allow_private).is_err()
        {
            return None;
        }
        Some(url)
    })
}
async fn favicon(State(state): State<AppState>, Json(req): Json<FaviconRequest>) -> ApiResult {
    let lease = Arc::new(
        state
            .monitor
            .favicon_jobs
            .clone()
            .try_acquire_owned()
            .map_err(|_| {
                AppError::bad_request("Too many concurrent favicon requests; retry shortly")
            })?,
    );
    let url = Url::parse(&req.url).map_err(|_| AppError::bad_request("Invalid website URL"))?;
    let (page_url, page) = download(&state, url, 1024 * 1024).await?;
    let image = if image_extension(&page).is_some() {
        page
    } else {
        let candidate = favicon_candidate(&page, &page_url, state.config.allow_private_favicon);
        let candidate = match candidate {
            Some(url) => url,
            None => page_url
                .join("/favicon.ico")
                .map_err(|_| AppError::bad_request("Cannot resolve favicon URL"))?,
        };
        download(&state, candidate, 1024 * 1024).await?.1
    };
    let src = save_upload(
        &state,
        state.owner_id,
        page_url.host_str().unwrap_or("favicon"),
        &image,
        &lease,
    )
    .await?;
    ok(json!({"iconUrl":src,"src":src}))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn upload_references_normalize_real_paths_without_matching_remote_substrings() {
        let mut headers = HeaderMap::new();
        headers.insert("host", "panel.local".parse().unwrap());
        headers.insert("origin", "https://panel.local".parse().unwrap());
        for reference in [
            "./uploads/a/image.png?size=1",
            "/uploads/a//image.png",
            "uploads/a/%69mage.png",
            "https://panel.local/uploads/a/image.png",
            "//panel.local/uploads/a/image.png",
        ] {
            assert_eq!(
                upload_reference_key(reference, &headers).as_deref(),
                Some("a/image.png"),
                "{reference}"
            );
        }
        for reference in [
            "/uploads/a/../../secret",
            "/other/uploads/a/image.png",
            "https://remote.local/uploads/a/image.png",
            "http://panel.local/uploads/a/image.png",
            "/uploads/a/%00.png",
            "a label containing /uploads/a/image.png",
            "javascript:/uploads/a/image.png",
        ] {
            assert_eq!(
                upload_reference_key(reference, &headers),
                None,
                "{reference}"
            );
        }
    }
    #[test]
    fn address_policy_blocks_metadata_loopback_and_private_by_default() {
        for address in [
            "127.0.0.1",
            "169.254.169.254",
            "10.0.0.1",
            "192.168.1.1",
            "100.100.100.200",
            "::1",
            "fe80::1",
            "fc00::1",
            "::ffff:127.0.0.1",
        ] {
            assert!(
                !permitted_address(address.parse().unwrap(), false),
                "{address}"
            );
        }
        for address in [
            "127.0.0.1",
            "169.254.169.254",
            "100.100.100.200",
            "::1",
            "fe80::1",
        ] {
            assert!(
                !permitted_address(address.parse().unwrap(), true),
                "{address}"
            );
        }
        assert!(permitted_address("192.168.1.2".parse().unwrap(), true));
        assert!(permitted_address("8.8.8.8".parse().unwrap(), false));
        assert!(permitted_address(
            "2606:4700:4700::1111".parse().unwrap(),
            false
        ));
        assert!(permitted_address("fd10::1".parse().unwrap(), true));
        for address in [
            "192.0.2.1",
            "198.18.1.2",
            "203.0.113.1",
            "168.63.129.16",
            "fd00:ec2::254",
            "64:ff9b::7f00:1",
            "2001:db8::1",
            "2002:7f00:1::",
        ] {
            assert!(
                !permitted_address(address.parse().unwrap(), true),
                "{address}"
            );
        }
    }
    #[test]
    fn favicon_links_resolve_relative_paths_and_skip_active_protocols() {
        let base = Url::parse("https://example.com/nested/page").unwrap();
        let html = br#"<link rel="icon" href="data:image/svg+xml,bad"><link rel="icon" href="/icon.svg"><link rel="shortcut icon" href="../assets/icon.png?size=32">"#;
        assert_eq!(
            favicon_candidate(html, &base, false).unwrap().as_str(),
            "https://example.com/assets/icon.png?size=32"
        );
        assert!(
            favicon_candidate(
                br#"<link rel="icon" href="http://127.0.0.1/secret">"#,
                &base,
                true
            )
            .is_none()
        );
    }
    #[tokio::test]
    async fn dns_resolution_rejects_localhost_before_connecting() {
        let resolver = SafeResolver {
            allow_private: true,
        };
        assert!(
            resolver
                .resolve("localhost".parse::<Name>().unwrap())
                .await
                .is_err()
        );
    }
    #[test]
    fn uploads_do_not_trust_names_or_active_content() {
        assert_eq!(clean_filename("../../my.png"), "my.png");
        assert!(upload_relative("./uploads/2026/10/test.png").is_ok());
        assert!(upload_relative("./uploads/../secret").is_err());
        assert!(upload_relative("/etc/passwd").is_err());
        assert!(active_content(b"<svg onload='alert(1)'/>", "image.png"));
        assert!(active_content(b"arbitrary", "image.svg"));
        assert_eq!(image_extension(b"<html>fake.png</html>"), None);
        for src in [
            "./uploads/../../etc/passwd",
            "/uploads/a/../private",
            "/uploads//",
            "./uploads/a/../../b",
            "uploads/",
        ] {
            assert!(upload_relative(src).is_err(), "{src}");
        }
        assert!(active_content(b"  \xef\xbb\xbf<svg>", "image.svg"));
        assert!(active_content(b"   <!DOCTYPE HTML>", "image.png"));
        assert!(active_content(b"arbitrary", "IMG.SvG"));
        assert_eq!(image_extension(b"GIF89a0000000"), Some("gif"));
        assert_eq!(
            image_extension(b"\x89PNG\r\n\x1a\n0000000000000000"),
            Some("png")
        );
    }
    #[tokio::test]
    async fn database_failure_rolls_back_an_uploaded_file() {
        let temporary = tempfile::tempdir().unwrap();
        let mut config = crate::config::Config::from_env().unwrap();
        config.uploads_dir = temporary.path().join("uploads");
        let state = AppState {
            db: sqlx::SqlitePool::connect("sqlite::memory:").await.unwrap(),
            config: Arc::new(config),
            http: http_client(false).unwrap(),
            monitor: Arc::new(new_monitor()),
            owner_id: 1,
        };
        // The database intentionally has no file table. Disk I/O succeeds first,
        // and a failed database insert must remove the generated image again.
        let lease = Arc::new(
            state
                .monitor
                .upload_jobs
                .clone()
                .try_acquire_owned()
                .unwrap(),
        );
        assert!(
            save_upload(
                &state,
                1,
                "test.png",
                b"\x89PNG\r\n\x1a\n0000000000000000",
                &lease,
            )
            .await
            .is_err()
        );
        fn files_in(directory: &Path) -> usize {
            std::fs::read_dir(directory)
                .unwrap()
                .map(|entry| {
                    let path = entry.unwrap().path();
                    if path.is_dir() { files_in(&path) } else { 1 }
                })
                .sum()
        }
        assert_eq!(files_in(&state.config.uploads_dir), 0);
    }
    #[tokio::test]
    async fn cancelled_upload_finishes_consistently_and_retains_its_concurrency_permit() {
        let temporary = tempfile::tempdir().unwrap();
        let mut config = crate::config::Config::from_env().unwrap();
        config.uploads_dir = temporary.path().join("uploads");
        let pool = sqlx::sqlite::SqlitePoolOptions::new()
            .max_connections(1)
            .connect("sqlite::memory:")
            .await
            .unwrap();
        sqlx::query("CREATE TABLE file(id INTEGER PRIMARY KEY,created_at TEXT,updated_at TEXT,src TEXT,user_id INTEGER,file_name TEXT,method INTEGER,ext TEXT)").execute(&pool).await.unwrap();
        let state = AppState {
            db: pool,
            config: Arc::new(config),
            http: http_client(false).unwrap(),
            monitor: Arc::new(new_monitor()),
            owner_id: 1,
        };
        // Occupy the only DB connection so the owned upload pauses after its
        // final rename but before INSERT can commit.
        let connection = state.db.acquire().await.unwrap();
        let lease = Arc::new(
            state
                .monitor
                .upload_jobs
                .clone()
                .try_acquire_owned()
                .unwrap(),
        );
        let worker = state.clone();
        let task = tokio::spawn(async move {
            save_upload(
                &worker,
                1,
                "test.png",
                b"\x89PNG\r\n\x1a\n0000000000000000",
                &lease,
            )
            .await
        });
        fn final_image(directory: &Path) -> Option<PathBuf> {
            let entries = std::fs::read_dir(directory).ok()?;
            for entry in entries.flatten() {
                let path = entry.path();
                if path.is_dir()
                    && let Some(image) = final_image(&path)
                {
                    return Some(image);
                } else if path.extension().is_some_and(|extension| extension == "png") {
                    return Some(path);
                }
            }
            None
        }
        let image = tokio::time::timeout(Duration::from_secs(2), async {
            loop {
                if let Some(path) = final_image(&state.config.uploads_dir) {
                    return path;
                }
                tokio::time::sleep(Duration::from_millis(2)).await;
            }
        })
        .await
        .unwrap();
        task.abort();
        assert!(task.await.unwrap_err().is_cancelled());
        assert_eq!(
            state.monitor.upload_jobs.available_permits(),
            3,
            "cancel must not free a live owned operation's permit"
        );
        assert!(image.exists());
        drop(connection);
        tokio::time::timeout(Duration::from_secs(2), async {
            loop {
                if state.monitor.upload_jobs.available_permits() == 4 {
                    break;
                }
                tokio::time::sleep(Duration::from_millis(2)).await;
            }
        })
        .await
        .unwrap();
        let src: String = sqlx::query_scalar("SELECT src FROM file WHERE user_id=1")
            .fetch_one(&state.db)
            .await
            .unwrap();
        assert_eq!(
            state
                .config
                .uploads_dir
                .join(upload_relative(&src).unwrap()),
            image
        );
        assert_eq!(
            std::fs::read(&image).unwrap(),
            b"\x89PNG\r\n\x1a\n0000000000000000"
        );
        assert_eq!(
            sqlx::query_scalar::<_, i64>("SELECT count(*) FROM file")
                .fetch_one(&state.db)
                .await
                .unwrap(),
            1
        );
        state.db.close().await;
    }
    #[test]
    fn deletion_rejects_partial_or_invalid_id_requests() {
        for ids in [vec![], vec![0], vec![-1], vec![1, 1], vec![1; 501]] {
            assert!(validate_delete_ids(&ids).is_err());
        }
        assert!(validate_delete_ids(&[1, 2]).is_ok());
    }
    #[tokio::test]
    async fn cancelled_delete_restores_original_path_and_rolls_back_database() {
        let temporary = tempfile::tempdir().unwrap();
        let root = std::fs::canonicalize(temporary.path()).unwrap();
        let source = root.join("image.png");
        std::fs::write(&source, b"original-image").unwrap();
        let pool = sqlx::sqlite::SqlitePoolOptions::new()
            .max_connections(1)
            .connect("sqlite::memory:")
            .await
            .unwrap();
        sqlx::query("CREATE TABLE file(id INTEGER PRIMARY KEY,deleted_at TEXT)")
            .execute(&pool)
            .await
            .unwrap();
        sqlx::query("INSERT INTO file(id) VALUES(1)")
            .execute(&pool)
            .await
            .unwrap();
        let (ready_tx, ready_rx) = tokio::sync::oneshot::channel();
        let worker_pool = pool.clone();
        let worker_source = source.clone();
        let task = tokio::spawn(async move {
            let mut tx = worker_pool.begin().await.unwrap();
            sqlx::query("UPDATE file SET deleted_at='staged' WHERE id=1")
                .execute(&mut *tx)
                .await
                .unwrap();
            let staged = stage_deletions(root, vec![worker_source]).await.unwrap();
            ready_tx.send(()).unwrap();
            std::future::pending::<()>().await;
            drop(staged);
            tx.rollback().await.unwrap();
        });
        ready_rx.await.unwrap();
        assert!(!source.exists());
        task.abort();
        assert!(task.await.unwrap_err().is_cancelled());
        assert_eq!(std::fs::read(&source).unwrap(), b"original-image");
        let deleted: Option<String> = sqlx::query_scalar("SELECT deleted_at FROM file WHERE id=1")
            .fetch_one(&pool)
            .await
            .unwrap();
        assert!(deleted.is_none());
        pool.close().await;
    }
    #[tokio::test]
    async fn failed_staging_restores_previously_moved_files() {
        let temporary = tempfile::tempdir().unwrap();
        let root = std::fs::canonicalize(temporary.path()).unwrap();
        let source = root.join("image.png");
        std::fs::write(&source, b"original-image").unwrap();
        let error = stage_deletions(root.clone(), vec![source.clone(), root]).await;
        assert!(error.is_err());
        assert_eq!(std::fs::read(source).unwrap(), b"original-image");
    }
    #[tokio::test]
    async fn expired_monitor_reads_await_and_share_one_fresh_snapshot() {
        let config = crate::config::Config::from_env().unwrap();
        let state = AppState {
            db: sqlx::SqlitePool::connect("sqlite::memory:").await.unwrap(),
            config: Arc::new(config),
            http: http_client(false).unwrap(),
            monitor: Arc::new(new_monitor()),
            owner_id: 1,
        };
        *state.monitor.snapshot.write().unwrap() = Some((
            Instant::now() - Duration::from_secs(1000),
            json!({"cpuInfo":{"usages":[999.0]},"memoryInfo":{"total":0}}),
        ));
        let (first, second) = tokio::join!(monitor_snapshot(&state), monitor_snapshot(&state));
        let first = first.unwrap();
        assert_eq!(first, second.unwrap());
        assert!(first["cpuInfo"]["usages"][0].as_f64().unwrap() <= 100.0);
        assert!(first["memoryInfo"]["total"].as_u64().unwrap() > 0);
        assert!(fresh_snapshot(&state.monitor, Duration::from_secs(1)).is_some());
        state.db.close().await;
    }
    #[test]
    fn sampling_reports_real_cpu_and_memory_without_scanning_processes() {
        if !sysinfo::IS_SUPPORTED_SYSTEM {
            return;
        }
        let monitor = new_monitor();
        let value = sample_system(&monitor);
        assert!(value["cpuInfo"]["cpuNum"].as_u64().unwrap() > 0);
        let usage = value["cpuInfo"]["usages"][0].as_f64().unwrap();
        assert!(usage.is_finite() && (0.0..=100.0).contains(&usage));
        assert!(value["memoryInfo"]["total"].as_u64().unwrap() > 0);
        assert!(monitor.system.lock().unwrap().processes().is_empty());
        assert!(monitor.disks.read().unwrap().is_none());
    }
    #[test]
    fn url_policy_applies_to_literals_and_protocols() {
        for url in [
            "file:///etc/passwd",
            "http://127.0.0.1/icon.png",
            "http://[::1]/",
            "http://2130706433/",
            "http://0x7f000001/",
            "http://0177.0.0.1/",
            "http://[::ffff:127.0.0.1]/",
            "http://user:pass@example.com/",
        ] {
            assert!(
                validate_url(&Url::parse(url).unwrap(), false).is_err(),
                "{url}"
            );
        }
        assert!(validate_url(&Url::parse("https://example.com/icon.png").unwrap(), false).is_ok());
    }
}
