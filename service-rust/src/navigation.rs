//! Navigation and configuration for the local workspace. Ownership checks share
//! the same transaction as every write.
use std::collections::{HashMap, HashSet};

use axum::{
    Json, Router,
    extract::State,
    http::HeaderMap,
    routing::{get, post},
};
use serde::{Deserialize, Serialize};
use serde_json::{Value, json};
use sqlx::{Row, SqliteConnection, sqlite::SqliteRow};

use crate::{
    error::{ApiResult, AppError, list, ok},
    state::AppState,
};

const MAX_BATCH: usize = 1000;
const MAX_IMPORT_ITEMS: usize = 10_000;
const MAX_CONFIG_BYTES: usize = 262_144;
// Dynamic SQL below interpolates only this constant; all request data uses bind parameters.
const NOW: &str = "strftime('%Y-%m-%dT%H:%M:%fZ','now')";

pub fn routes() -> Router<AppState> {
    Router::new()
        .route("/api/panel/itemIcon/edit", post(edit_item))
        .route("/api/panel/itemIcon/deletes", post(delete_items))
        .route("/api/panel/itemIconGroup/edit", post(edit_group))
        .route("/api/panel/itemIconGroup/getList", post(get_groups))
        .route("/api/panel/itemIconGroup/deletes", post(delete_groups))
        .route("/api/panel/itemIconGroup/saveSort", post(sort_groups))
        .route("/api/panel/userConfig/get", post(get_user_config))
        .route("/api/panel/userConfig/set", post(set_user_config))
        .route(
            "/api/system/moduleConfig/getByName",
            post(get_module_config),
        )
        .route("/api/system/moduleConfig/save", post(save_module_config))
        .route(
            "/api/panel/navigation",
            get(get_navigation).post(get_navigation),
        )
        .route("/api/panel/navigation/move", post(move_item))
        .route("/api/panel/navigation/trash/list", post(list_trash))
        .route("/api/panel/navigation/trash/restore", post(restore_trash))
        .route("/api/panel/navigation/trash/purge", post(purge_trash))
        .route(
            "/api/panel/navigation/export",
            get(export_navigation).post(export_navigation),
        )
        .route("/api/panel/navigation/import/preview", post(preview_import))
        .route("/api/panel/navigation/import", post(import_navigation))
}

#[derive(Debug, Default, Clone, Deserialize, Serialize)]
#[serde(rename_all = "camelCase", default)]
struct Icon {
    item_type: i64,
    src: String,
    text: String,
    background_color: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    style: Option<i64>,
    #[serde(skip_serializing_if = "Option::is_none")]
    text_color: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    hide_description: Option<bool>,
    #[serde(skip_serializing_if = "Option::is_none")]
    hide_title: Option<bool>,
}

#[derive(Debug, Default, Clone, Deserialize)]
#[serde(rename_all = "camelCase", default)]
struct ItemInput {
    id: i64,
    icon: Option<Icon>,
    title: String,
    url: String,
    description: String,
    open_method: i64,
    sort: Option<i64>,
    item_icon_group_id: i64,
}

#[derive(Debug, Default, Clone, Deserialize)]
#[serde(rename_all = "camelCase", default)]
struct GroupInput {
    id: i64,
    icon: String,
    title: String,
    description: String,
    sort: Option<i64>,
}

#[derive(Deserialize)]
struct IdsInput {
    ids: Vec<i64>,
    #[serde(default)]
    revision: Option<i64>,
}

#[derive(Deserialize)]
struct SortItem {
    id: i64,
    sort: i64,
}

#[derive(Deserialize)]
#[serde(rename_all = "camelCase")]
struct SortInput {
    sort_items: Vec<SortItem>,
    #[serde(default)]
    revision: Option<i64>,
}

#[derive(Deserialize)]
#[serde(rename_all = "camelCase")]
struct TrashInput {
    kind: String,
    id: i64,
    #[serde(default)]
    target_group_id: Option<i64>,
    #[serde(default)]
    target_index: Option<i64>,
    #[serde(default)]
    deletion_id: Option<String>,
    #[serde(default)]
    revision: Option<i64>,
}

#[derive(Deserialize)]
#[serde(rename_all = "camelCase")]
struct MoveItemInput {
    item_id: i64,
    target_group_id: i64,
    // Index after removing the moving item from its original list.
    target_index: i64,
    #[serde(default)]
    revision: Option<i64>,
}

#[derive(Deserialize)]
#[serde(rename_all = "camelCase")]
struct UserConfigInput {
    #[serde(default)]
    revision: Option<i64>,
    #[serde(default, deserialize_with = "present_value")]
    panel: Option<Value>,
    #[serde(default, deserialize_with = "present_value")]
    search_engine: Option<Value>,
}

// Distinguish an explicitly supplied JSON null (clear this field) from an
// omitted field (preserve its saved value).
fn present_value<'de, D: serde::Deserializer<'de>>(
    deserializer: D,
) -> Result<Option<Value>, D::Error> {
    Value::deserialize(deserializer).map(Some)
}

#[derive(Deserialize)]
struct ModuleInput {
    #[serde(default)]
    revision: Option<i64>,
    name: String,
    #[serde(default)]
    value: Value,
}

fn limit_text(value: &str, field: &str, max: usize) -> Result<(), AppError> {
    if value.chars().count() > max || value.contains('\0') {
        return Err(AppError::bad_request(format!(
            "{field} exceeds {max} characters or contains NUL"
        )));
    }
    Ok(())
}

fn validate_url(value: &str, field: &str, optional: bool, upload: bool) -> Result<(), AppError> {
    limit_text(value, field, 2000)?;
    if optional && value.is_empty() {
        return Ok(());
    }
    if value.chars().any(char::is_control) {
        return Err(AppError::bad_request(format!(
            "{field} contains control characters"
        )));
    }
    // Existing Go uploads are returned both with and without the leading slash.
    if upload {
        let relative = value
            .strip_prefix("./")
            .or_else(|| value.strip_prefix('/'))
            .unwrap_or(value);
        if relative.starts_with("uploads/")
            && !relative.contains(['\\', '?', '#', '%'])
            && relative
                .split('/')
                .all(|part| !matches!(part, "." | ".." | ""))
        {
            return Ok(());
        }
    }
    let url = reqwest::Url::parse(value).map_err(|_| {
        AppError::bad_request(format!("{field} must be an absolute HTTP or HTTPS URL"))
    })?;
    if !matches!(url.scheme(), "http" | "https")
        || url.host_str().is_none()
        || !url.username().is_empty()
        || url.password().is_some()
        || value.chars().any(char::is_control)
    {
        return Err(AppError::bad_request(format!(
            "{field} must be an HTTP or HTTPS URL without credentials"
        )));
    }
    Ok(())
}

fn validate_sort(sort: i64) -> Result<(), AppError> {
    if !(-1_000_000_000..=1_000_000_000).contains(&sort) {
        return Err(AppError::bad_request("sort is outside the supported range"));
    }
    Ok(())
}

fn validate_item(item: &ItemInput, require_group: bool) -> Result<(), AppError> {
    if item.id < 0 || (require_group && item.item_icon_group_id <= 0) {
        return Err(AppError::bad_request(
            "A valid group and non-negative item ID are required",
        ));
    }
    if item.title.trim().is_empty() {
        return Err(AppError::bad_request("title is required"));
    }
    limit_text(&item.title, "title", 50)?;
    limit_text(&item.description, "description", 1000)?;
    validate_url(&item.url, "url", false, false)?;
    if !matches!(item.open_method, 1..=3) {
        return Err(AppError::bad_request("openMethod must be 1, 2 or 3"));
    }
    if let Some(sort) = item.sort {
        validate_sort(sort)?;
    }
    if let Some(icon) = &item.icon {
        if !matches!(icon.item_type, 0..=3) {
            return Err(AppError::bad_request("Unsupported icon itemType"));
        }
        limit_text(&icon.text, "icon.text", 255)?;
        limit_text(&icon.background_color, "icon.backgroundColor", 100)?;
        if icon.style.is_some_and(|style| !matches!(style, 0 | 1)) {
            return Err(AppError::bad_request("icon.style must be 0 or 1"));
        }
        if let Some(color) = &icon.text_color {
            limit_text(color, "icon.textColor", 100)?;
        }
        validate_url(&icon.src, "icon.src", true, true)?;
    }
    Ok(())
}

fn validate_group(group: &GroupInput) -> Result<(), AppError> {
    if group.id < 0 || group.title.trim().is_empty() {
        return Err(AppError::bad_request(
            "A group title and non-negative ID are required",
        ));
    }
    limit_text(&group.title, "title", 50)?;
    limit_text(&group.icon, "icon", 255)?;
    limit_text(&group.description, "description", 1000)?;
    if let Some(sort) = group.sort {
        validate_sort(sort)?;
    }
    Ok(())
}

fn validate_ids(ids: &[i64]) -> Result<(), AppError> {
    if ids.is_empty()
        || ids.len() > MAX_BATCH
        || ids.iter().any(|id| *id <= 0)
        || ids.iter().collect::<HashSet<_>>().len() != ids.len()
    {
        return Err(AppError::bad_request(
            "Provide 1 to 1000 unique positive IDs",
        ));
    }
    Ok(())
}

fn validate_json(value: &Value, field: &str) -> Result<String, AppError> {
    if !value.is_object() && !value.is_null() {
        return Err(AppError::bad_request(format!(
            "{field} must be an object or null"
        )));
    }
    fn depth(value: &Value, level: usize) -> bool {
        level <= 32
            && match value {
                Value::Array(items) => items.iter().all(|v| depth(v, level + 1)),
                Value::Object(items) => items.values().all(|v| depth(v, level + 1)),
                _ => true,
            }
    }
    let encoded = value.to_string();
    if encoded.len() > MAX_CONFIG_BYTES || !depth(value, 0) {
        return Err(AppError::bad_request(format!(
            "{field} exceeds the configuration size or nesting limit"
        )));
    }
    Ok(encoded)
}

fn string_column(row: &SqliteRow, name: &str) -> String {
    row.try_get::<Option<String>, _>(name)
        .ok()
        .flatten()
        .unwrap_or_default()
}

fn number_column(row: &SqliteRow, name: &str) -> i64 {
    row.try_get::<Option<i64>, _>(name)
        .ok()
        .flatten()
        .unwrap_or_default()
}

fn item_json(row: &SqliteRow) -> Value {
    json!({
        "id": number_column(row, "id"),
        "createTime": string_column(row, "created_at"),
        "updateTime": string_column(row, "updated_at"),
        "icon": serde_json::from_str::<Value>(&string_column(row, "icon_json")).unwrap_or(Value::Null),
        "title": string_column(row, "title"), "url": string_column(row, "url"),
        "description": string_column(row, "description"),
        "openMethod": number_column(row, "open_method"), "sort": number_column(row, "sort"),
        "itemIconGroupId": number_column(row, "item_icon_group_id"), "userId": number_column(row, "user_id")
    })
}

fn group_json(row: &SqliteRow) -> Value {
    json!({
        "id": number_column(row, "id"),
        "createTime": string_column(row, "created_at"), "updateTime": string_column(row, "updated_at"),
        "icon": string_column(row, "icon"), "title": string_column(row, "title"),
        "description": string_column(row, "description"), "sort": number_column(row, "sort"),
        "userId": number_column(row, "user_id")
    })
}

async fn ensure_group(
    conn: &mut SqliteConnection,
    user_id: i64,
    group_id: i64,
) -> Result<(), AppError> {
    let exists: i64 = sqlx::query_scalar(
        "SELECT count(*) FROM item_icon_group WHERE id=? AND user_id=? AND deleted_at IS NULL",
    )
    .bind(group_id)
    .bind(user_id)
    .fetch_one(conn)
    .await?;
    if exists != 1 {
        return Err(AppError::new(
            1005,
            "Group does not exist or belongs to another user",
        ));
    }
    Ok(())
}

fn item_upload_sources(item: &ItemInput) -> Vec<String> {
    item.icon
        .as_ref()
        .map(|icon| vec![icon.src.clone()])
        .unwrap_or_default()
}

// Unmanaged image paths remain usable. A managed image that was deleted cannot
// be reintroduced after the deletion transaction checked its references.
async fn check_retired_uploads(
    conn: &mut SqliteConnection,
    headers: &HeaderMap,
    sources: &[String],
) -> Result<(), AppError> {
    let requested: HashSet<String> = sources
        .iter()
        .filter_map(|src| crate::system::upload_reference_key(src, headers))
        .collect();
    if requested.is_empty() {
        return Ok(());
    }
    let rows = sqlx::query("SELECT src FROM file WHERE deleted_at IS NOT NULL")
        .fetch_all(&mut *conn)
        .await?;
    let retired: HashSet<String> = rows
        .iter()
        .filter_map(|row| crate::system::upload_reference_key(&string_column(row, "src"), headers))
        .filter(|key| requested.contains(key))
        .collect();
    if retired.is_empty() {
        return Ok(());
    }
    // Old manually imported records can reuse a path; a live record takes precedence.
    let rows = sqlx::query("SELECT src FROM file WHERE deleted_at IS NULL")
        .fetch_all(conn)
        .await?;
    let live: HashSet<String> = rows
        .iter()
        .filter_map(|row| crate::system::upload_reference_key(&string_column(row, "src"), headers))
        .collect();
    if retired.iter().any(|key| !live.contains(key)) {
        return Err(AppError::new(
            1311,
            "An image was deleted. Select a replacement before saving.",
        ));
    }
    Ok(())
}

async fn insert_item(
    conn: &mut SqliteConnection,
    user_id: i64,
    item: &ItemInput,
) -> Result<i64, AppError> {
    let sql = format!(
        "INSERT INTO item_icon(created_at,updated_at,icon_json,title,url,lan_url,description,open_method,sort,item_icon_group_id,user_id) VALUES({NOW},{NOW},?,?,?,?,?,?,?,?,?)"
    );
    Ok(sqlx::query(sqlx::AssertSqlSafe(sql.as_str()))
        .bind(serde_json::to_string(&item.icon).map_err(|_| AppError::bad_request("Invalid icon"))?)
        .bind(item.title.trim())
        .bind(&item.url)
        .bind("")
        .bind(&item.description)
        .bind(item.open_method)
        .bind(item.sort.unwrap_or(9999))
        .bind(item.item_icon_group_id)
        .bind(user_id)
        .execute(conn)
        .await?
        .last_insert_rowid())
}

async fn insert_group(
    conn: &mut SqliteConnection,
    user_id: i64,
    group: &GroupInput,
) -> Result<i64, AppError> {
    let sql = format!(
        "INSERT INTO item_icon_group(created_at,updated_at,icon,title,description,sort,user_id) VALUES({NOW},{NOW},?,?,?,?,?)"
    );
    Ok(sqlx::query(sqlx::AssertSqlSafe(sql.as_str()))
        .bind(&group.icon)
        .bind(group.title.trim())
        .bind(&group.description)
        .bind(group.sort.unwrap_or(0))
        .bind(user_id)
        .execute(conn)
        .await?
        .last_insert_rowid())
}

async fn edit_item(
    State(state): State<AppState>,
    headers: HeaderMap,
    Json(item): Json<ItemInput>,
) -> ApiResult {
    validate_item(&item, true)?;
    let mut tx = state.db.begin_with("BEGIN IMMEDIATE").await?;
    ensure_group(&mut tx, state.owner_id, item.item_icon_group_id).await?;
    check_retired_uploads(&mut tx, &headers, &item_upload_sources(&item)).await?;
    let id = if item.id == 0 {
        insert_item(&mut tx, state.owner_id, &item).await?
    } else {
        let sql = format!(
            "UPDATE item_icon SET icon_json=?,title=?,url=?,lan_url=?,description=?,open_method=?,sort=COALESCE(?,sort),item_icon_group_id=?,updated_at={NOW} WHERE id=? AND user_id=? AND deleted_at IS NULL"
        );
        let changed = sqlx::query(sqlx::AssertSqlSafe(sql.as_str()))
            .bind(
                serde_json::to_string(&item.icon)
                    .map_err(|_| AppError::bad_request("Invalid icon"))?,
            )
            .bind(item.title.trim())
            .bind(&item.url)
            .bind("")
            .bind(&item.description)
            .bind(item.open_method)
            .bind(item.sort)
            .bind(item.item_icon_group_id)
            .bind(item.id)
            .bind(state.owner_id)
            .execute(&mut *tx)
            .await?
            .rows_affected();
        if changed != 1 {
            return Err(AppError::new(
                1005,
                "Item does not exist or belongs to another user",
            ));
        }
        item.id
    };
    let row = sqlx::query("SELECT * FROM item_icon WHERE id=?")
        .bind(id)
        .fetch_one(&mut *tx)
        .await?;
    tx.commit().await?;
    Ok(ok(item_json(&row)))
}

async fn delete_items(State(state): State<AppState>, Json(input): Json<IdsInput>) -> ApiResult {
    validate_ids(&input.ids)?;
    let mut tx = state.db.begin_with("BEGIN IMMEDIATE").await?;
    let revision = navigation_revision(&mut tx, state.owner_id).await?;
    if input.revision.is_some_and(|expected| expected != revision) {
        return Ok(revision_conflict(revision));
    }
    let deletion_id = uuid::Uuid::new_v4().to_string();
    let trash: Vec<Value> = input
        .ids
        .iter()
        .map(|id| json!({"kind":"item","id":id,"deletionId":deletion_id}))
        .collect();
    let sql = format!(
        "UPDATE item_icon SET deleted_at={NOW},updated_at={NOW},trash_batch=? WHERE id=? AND user_id=? AND deleted_at IS NULL"
    );
    for id in input.ids {
        if sqlx::query(sqlx::AssertSqlSafe(sql.as_str()))
            .bind(&deletion_id)
            .bind(id)
            .bind(state.owner_id)
            .execute(&mut *tx)
            .await?
            .rows_affected()
            != 1
        {
            return Err(AppError::new(
                1005,
                "Item does not exist or belongs to another user",
            ));
        }
    }
    let revision = navigation_revision(&mut tx, state.owner_id).await?;
    tx.commit().await?;
    Ok(revision_result(
        json!({"trash":trash,"revision":revision}),
        revision,
    ))
}

async fn ordered_item_ids(
    conn: &mut SqliteConnection,
    user_id: i64,
    group_id: i64,
) -> Result<Vec<i64>, AppError> {
    Ok(sqlx::query_scalar(
        "SELECT id FROM item_icon WHERE user_id=? AND item_icon_group_id=? AND deleted_at IS NULL ORDER BY sort,id",
    )
    .bind(user_id)
    .bind(group_id)
    .fetch_all(conn)
    .await?)
}

async fn write_item_order(
    conn: &mut SqliteConnection,
    user_id: i64,
    group_id: i64,
    items: &[i64],
) -> Result<(), AppError> {
    let sql = format!(
        "UPDATE item_icon SET sort=?,item_icon_group_id=?,updated_at={NOW} WHERE id=? AND user_id=? AND deleted_at IS NULL AND (sort<>? OR item_icon_group_id<>?)"
    );
    for (index, id) in items.iter().enumerate() {
        let sort = i64::try_from(index)
            .map_err(|_| AppError::bad_request("Too many items in the target group"))?;
        sqlx::query(sqlx::AssertSqlSafe(sql.as_str()))
            .bind(sort)
            .bind(group_id)
            .bind(id)
            .bind(user_id)
            .bind(sort)
            .bind(group_id)
            .execute(&mut *conn)
            .await?;
    }
    Ok(())
}

async fn move_item(State(state): State<AppState>, Json(input): Json<MoveItemInput>) -> ApiResult {
    if input.item_id <= 0 || input.target_group_id <= 0 || input.target_index < 0 {
        return Err(AppError::bad_request(
            "A positive item ID, target group ID and non-negative target index are required",
        ));
    }
    let mut tx = state.db.begin_with("BEGIN IMMEDIATE").await?;
    let revision = navigation_revision(&mut tx, state.owner_id).await?;
    if input.revision != Some(revision) {
        tx.rollback().await?;
        return Ok(revision_conflict(revision));
    }
    let source_group: Option<i64> = sqlx::query_scalar(
        "SELECT item_icon_group_id FROM item_icon WHERE id=? AND user_id=? AND deleted_at IS NULL",
    )
    .bind(input.item_id)
    .bind(state.owner_id)
    .fetch_optional(&mut *tx)
    .await?;
    let source_group = source_group
        .ok_or_else(|| AppError::new(1005, "Item does not exist or belongs to another user"))?;
    ensure_group(&mut tx, state.owner_id, source_group).await?;
    ensure_group(&mut tx, state.owner_id, input.target_group_id).await?;
    let mut source = ordered_item_ids(&mut tx, state.owner_id, source_group).await?;
    let source_index = source
        .iter()
        .position(|id| *id == input.item_id)
        .ok_or_else(|| AppError::new(1005, "Item is not in its source group"))?;
    source.remove(source_index);
    let target_index = usize::try_from(input.target_index)
        .map_err(|_| AppError::bad_request("Target index is outside the supported range"))?;
    if source_group == input.target_group_id {
        if target_index > source.len() {
            return Err(AppError::bad_request("Target index exceeds the group size"));
        }
        if source_index == target_index {
            tx.rollback().await?;
            return Ok(revision_result(json!({"revision":revision}), revision));
        }
        source.insert(target_index, input.item_id);
        write_item_order(&mut tx, state.owner_id, source_group, &source).await?;
    } else {
        let mut target = ordered_item_ids(&mut tx, state.owner_id, input.target_group_id).await?;
        if target_index > target.len() {
            return Err(AppError::bad_request("Target index exceeds the group size"));
        }
        target.insert(target_index, input.item_id);
        write_item_order(&mut tx, state.owner_id, source_group, &source).await?;
        write_item_order(&mut tx, state.owner_id, input.target_group_id, &target).await?;
    }
    let revision = navigation_revision(&mut tx, state.owner_id).await?;
    tx.commit().await?;
    Ok(revision_result(json!({"revision":revision}), revision))
}

async fn edit_group(State(state): State<AppState>, Json(group): Json<GroupInput>) -> ApiResult {
    validate_group(&group)?;
    let mut tx = state.db.begin_with("BEGIN IMMEDIATE").await?;
    let id = if group.id == 0 {
        insert_group(&mut tx, state.owner_id, &group).await?
    } else {
        let sql = format!(
            "UPDATE item_icon_group SET icon=?,title=?,description=?,sort=COALESCE(?,sort),updated_at={NOW} WHERE id=? AND user_id=? AND deleted_at IS NULL"
        );
        if sqlx::query(sqlx::AssertSqlSafe(sql.as_str()))
            .bind(&group.icon)
            .bind(group.title.trim())
            .bind(&group.description)
            .bind(group.sort)
            .bind(group.id)
            .bind(state.owner_id)
            .execute(&mut *tx)
            .await?
            .rows_affected()
            != 1
        {
            return Err(AppError::new(
                1005,
                "Group does not exist or belongs to another user",
            ));
        }
        group.id
    };
    let row = sqlx::query("SELECT * FROM item_icon_group WHERE id=?")
        .bind(id)
        .fetch_one(&mut *tx)
        .await?;
    tx.commit().await?;
    Ok(ok(group_json(&row)))
}

async fn groups_in_transaction(
    conn: &mut SqliteConnection,
    user_id: i64,
) -> Result<Vec<Value>, AppError> {
    let rows = sqlx::query("SELECT * FROM item_icon_group WHERE user_id=? AND deleted_at IS NULL ORDER BY sort,created_at,id")
        .bind(user_id).fetch_all(&mut *conn).await?;
    Ok(rows.iter().map(group_json).collect())
}

async fn get_groups(State(state): State<AppState>) -> ApiResult {
    let mut connection = state.db.acquire().await?;
    let groups = groups_in_transaction(&mut connection, state.owner_id).await?;
    Ok(list(json!(groups), 0))
}

async fn delete_groups(State(state): State<AppState>, Json(input): Json<IdsInput>) -> ApiResult {
    validate_ids(&input.ids)?;
    let mut tx = state.db.begin_with("BEGIN IMMEDIATE").await?;
    let revision = navigation_revision(&mut tx, state.owner_id).await?;
    if input.revision.is_some_and(|expected| expected != revision) {
        return Ok(revision_conflict(revision));
    }
    for id in &input.ids {
        ensure_group(&mut tx, state.owner_id, *id).await?;
    }
    let count: i64 = sqlx::query_scalar(
        "SELECT count(*) FROM item_icon_group WHERE user_id=? AND deleted_at IS NULL",
    )
    .bind(state.owner_id)
    .fetch_one(&mut *tx)
    .await?;
    if count <= input.ids.len() as i64 {
        return Err(AppError::new(1201, "At least one group must be retained"));
    }
    let group_sql = format!(
        "UPDATE item_icon_group SET deleted_at={NOW},updated_at={NOW},trash_batch=? WHERE id=? AND user_id=? AND deleted_at IS NULL"
    );
    let item_sql = format!(
        "UPDATE item_icon SET deleted_at={NOW},updated_at={NOW},trash_batch=? WHERE item_icon_group_id=? AND user_id=? AND deleted_at IS NULL"
    );
    let deletion_id = uuid::Uuid::new_v4().to_string();
    let trash: Vec<Value> = input
        .ids
        .iter()
        .map(|id| json!({"kind":"group","id":id,"deletionId":deletion_id}))
        .collect();
    for id in input.ids {
        sqlx::query(sqlx::AssertSqlSafe(group_sql.as_str()))
            .bind(&deletion_id)
            .bind(id)
            .bind(state.owner_id)
            .execute(&mut *tx)
            .await?;
        sqlx::query(sqlx::AssertSqlSafe(item_sql.as_str()))
            .bind(&deletion_id)
            .bind(id)
            .bind(state.owner_id)
            .execute(&mut *tx)
            .await?;
    }
    let revision = navigation_revision(&mut tx, state.owner_id).await?;
    tx.commit().await?;
    Ok(revision_result(
        json!({"trash":trash,"revision":revision}),
        revision,
    ))
}

async fn sort_groups(State(state): State<AppState>, Json(input): Json<SortInput>) -> ApiResult {
    validate_ids(&input.sort_items.iter().map(|i| i.id).collect::<Vec<_>>())?;
    for item in &input.sort_items {
        validate_sort(item.sort)?;
    }
    let mut tx = state.db.begin_with("BEGIN IMMEDIATE").await?;
    let revision = navigation_revision(&mut tx, state.owner_id).await?;
    if input.revision.is_some_and(|expected| expected != revision) {
        return Ok(revision_conflict(revision));
    }
    let sql = format!(
        "UPDATE item_icon_group SET sort=?,updated_at={NOW} WHERE id=? AND user_id=? AND deleted_at IS NULL"
    );
    for item in input.sort_items {
        if sqlx::query(sqlx::AssertSqlSafe(sql.as_str()))
            .bind(item.sort)
            .bind(item.id)
            .bind(state.owner_id)
            .execute(&mut *tx)
            .await?
            .rows_affected()
            != 1
        {
            return Err(AppError::new(
                1005,
                "Group does not exist or belongs to another user",
            ));
        }
    }
    let revision = navigation_revision(&mut tx, state.owner_id).await?;
    tx.commit().await?;
    Ok(revision_result(json!({"revision":revision}), revision))
}

async fn list_trash(State(state): State<AppState>) -> ApiResult {
    let mut tx = state.db.begin().await?;
    // Only a known matching batch is grouped under its deleted parent. Legacy
    // deletions have no reliable cascade history and remain separate entries.
    let rows = sqlx::query(
        "WITH trash AS (
            SELECT 'group' AS kind,g.id,g.title,g.deleted_at,g.trash_batch,
                NULL AS group_id,NULL AS group_title,0 AS requires_target_group,
                (SELECT count(*) FROM item_icon i WHERE i.user_id=g.user_id
                    AND i.item_icon_group_id=g.id AND i.deleted_at IS NOT NULL
                    AND g.trash_batch IS NOT NULL AND i.trash_batch=g.trash_batch) AS item_count
            FROM item_icon_group g WHERE g.user_id=? AND g.deleted_at IS NOT NULL
            UNION ALL
            SELECT 'item',i.id,i.title,i.deleted_at,i.trash_batch,i.item_icon_group_id,g.title,
                CASE WHEN g.id IS NULL OR g.deleted_at IS NOT NULL THEN 1 ELSE 0 END,0
            FROM item_icon i LEFT JOIN item_icon_group g ON g.id=i.item_icon_group_id AND g.user_id=i.user_id
            WHERE i.user_id=? AND i.deleted_at IS NOT NULL
                AND NOT (g.deleted_at IS NOT NULL AND g.trash_batch IS NOT NULL
                    AND i.trash_batch IS NOT NULL AND i.trash_batch=g.trash_batch)
        ) SELECT * FROM trash ORDER BY julianday(deleted_at) DESC,kind,id DESC",
    )
    .bind(state.owner_id)
    .bind(state.owner_id)
    .fetch_all(&mut *tx)
    .await?;
    let entries: Vec<Value> = rows
        .iter()
        .map(|row| {
            let kind = string_column(row, "kind");
            let mut entry = json!({
                "kind":kind,"id":number_column(row,"id"),"title":string_column(row,"title"),
                "deletedAt":string_column(row,"deleted_at"),"itemCount":number_column(row,"item_count")
            });
            let batch = string_column(row, "trash_batch");
            if !batch.is_empty() {
                entry["deletionId"] = json!(batch);
            }
            if kind == "item" {
                entry["groupId"] = json!(number_column(row, "group_id"));
                entry["groupTitle"] = json!(string_column(row, "group_title"));
                entry["requiresTargetGroup"] = json!(number_column(row, "requires_target_group") != 0);
            }
            entry
        })
        .collect();
    let revision = navigation_revision(&mut tx, state.owner_id).await?;
    tx.commit().await?;
    Ok(revision_result(
        json!({"count":entries.len(),"list":entries,"revision":revision}),
        revision,
    ))
}

fn trash_table(input: &TrashInput) -> Result<&'static str, AppError> {
    if input.id <= 0
        || input.target_group_id.is_some_and(|id| id <= 0)
        || input.target_index.is_some_and(|index| index < 0)
    {
        return Err(AppError::bad_request("Positive IDs are required"));
    }
    match input.kind.as_str() {
        "item" => Ok("item_icon"),
        "group" if input.target_group_id.is_none() && input.target_index.is_none() => {
            Ok("item_icon_group")
        }
        "group" => Err(AppError::bad_request(
            "A group cannot be restored into another group",
        )),
        _ => Err(AppError::bad_request("kind must be item or group")),
    }
}

async fn deleted_record(
    conn: &mut SqliteConnection,
    user_id: i64,
    input: &TrashInput,
    table: &'static str,
) -> Result<SqliteRow, AppError> {
    // The table name is selected exclusively by trash_table.
    sqlx::query(sqlx::AssertSqlSafe(format!(
        "SELECT * FROM {table} WHERE id=? AND user_id=? AND deleted_at IS NOT NULL"
    )))
    .bind(input.id)
    .bind(user_id)
    .fetch_optional(conn)
    .await?
    .ok_or_else(|| {
        AppError::new(
            1005,
            "Deleted entry does not exist or belongs to another user",
        )
    })
}

fn trash_deletion_conflict(
    input: &TrashInput,
    row: &SqliteRow,
    revision: i64,
) -> Option<Json<Value>> {
    input.deletion_id.as_ref().filter(|expected| **expected != string_column(row,"trash_batch")).map(|_| {
        Json(json!({"code":1409,"msg":"This entry was deleted again. Refresh the recycle bin before continuing.","currentRevision":revision}))
    })
}

fn row_icon_source(row: &SqliteRow) -> Option<String> {
    serde_json::from_str::<Value>(&string_column(row, "icon_json"))
        .ok()?
        .get("src")?
        .as_str()
        .map(str::to_owned)
}

async fn restore_trash(
    State(state): State<AppState>,
    headers: HeaderMap,
    Json(input): Json<TrashInput>,
) -> ApiResult {
    let table = trash_table(&input)?;
    let mut tx = state.db.begin_with("BEGIN IMMEDIATE").await?;
    let revision = navigation_revision(&mut tx, state.owner_id).await?;
    if input.revision.is_some_and(|expected| expected != revision) {
        return Ok(revision_conflict(revision));
    }
    let row = deleted_record(&mut tx, state.owner_id, &input, table).await?;
    if let Some(conflict) = trash_deletion_conflict(&input, &row, revision) {
        return Ok(conflict);
    }
    if input.kind == "item" {
        let group_id = input
            .target_group_id
            .unwrap_or_else(|| number_column(&row, "item_icon_group_id"));
        ensure_group(&mut tx, state.owner_id, group_id).await?;
        let restored_order = if let Some(index) = input.target_index {
            let mut order = ordered_item_ids(&mut tx, state.owner_id, group_id).await?;
            let index = usize::try_from(index).map_err(|_| {
                AppError::bad_request("Target index is outside the supported range")
            })?;
            if index > order.len() {
                return Err(AppError::bad_request("Target index exceeds the group size"));
            }
            order.insert(index, input.id);
            Some(order)
        } else {
            None
        };
        let sources: Vec<String> = row_icon_source(&row).into_iter().collect();
        check_retired_uploads(&mut tx, &headers, &sources).await?;
        let sql = format!(
            "UPDATE item_icon SET deleted_at=NULL,trash_batch=NULL,item_icon_group_id=?,updated_at={NOW} WHERE id=? AND user_id=?"
        );
        sqlx::query(sqlx::AssertSqlSafe(sql.as_str()))
            .bind(group_id)
            .bind(input.id)
            .bind(state.owner_id)
            .execute(&mut *tx)
            .await?;
        if let Some(order) = restored_order {
            write_item_order(&mut tx, state.owner_id, group_id, &order).await?;
        }
    } else {
        let batch = string_column(&row, "trash_batch");
        let children = sqlx::query("SELECT icon_json FROM item_icon WHERE user_id=? AND item_icon_group_id=? AND deleted_at IS NOT NULL AND trash_batch=?")
            .bind(state.owner_id).bind(input.id).bind(&batch).fetch_all(&mut *tx).await?;
        let mut sources: Vec<String> = children.iter().filter_map(row_icon_source).collect();
        sources.push(string_column(&row, "icon"));
        check_retired_uploads(&mut tx, &headers, &sources).await?;
        let sql = format!(
            "UPDATE item_icon_group SET deleted_at=NULL,trash_batch=NULL,updated_at={NOW} WHERE id=? AND user_id=?"
        );
        sqlx::query(sqlx::AssertSqlSafe(sql.as_str()))
            .bind(input.id)
            .bind(state.owner_id)
            .execute(&mut *tx)
            .await?;
        // Empty/NULL legacy batches never match. Independently deleted items
        // retain their own batch and must be restored explicitly.
        if !batch.is_empty() {
            let sql = format!(
                "UPDATE item_icon SET deleted_at=NULL,trash_batch=NULL,updated_at={NOW} WHERE user_id=? AND item_icon_group_id=? AND deleted_at IS NOT NULL AND trash_batch=?"
            );
            sqlx::query(sqlx::AssertSqlSafe(sql.as_str()))
                .bind(state.owner_id)
                .bind(input.id)
                .bind(batch)
                .execute(&mut *tx)
                .await?;
        }
    }
    let revision = navigation_revision(&mut tx, state.owner_id).await?;
    tx.commit().await?;
    Ok(revision_result(json!({"revision":revision}), revision))
}

async fn purge_trash(State(state): State<AppState>, Json(input): Json<TrashInput>) -> ApiResult {
    let table = trash_table(&input)?;
    let mut tx = state.db.begin_with("BEGIN IMMEDIATE").await?;
    let revision = navigation_revision(&mut tx, state.owner_id).await?;
    if input.revision.is_some_and(|expected| expected != revision) {
        return Ok(revision_conflict(revision));
    }
    let row = deleted_record(&mut tx, state.owner_id, &input, table).await?;
    if let Some(conflict) = trash_deletion_conflict(&input, &row, revision) {
        return Ok(conflict);
    }
    if input.kind == "group" {
        let batch = string_column(&row, "trash_batch");
        if !batch.is_empty() {
            sqlx::query("DELETE FROM rust_legacy_item_addresses WHERE user_id=? AND item_id IN (SELECT id FROM item_icon WHERE user_id=? AND item_icon_group_id=? AND deleted_at IS NOT NULL AND trash_batch=?)")
                .bind(state.owner_id).bind(state.owner_id).bind(input.id).bind(&batch).execute(&mut *tx).await?;
            sqlx::query("DELETE FROM item_icon WHERE user_id=? AND item_icon_group_id=? AND deleted_at IS NOT NULL AND trash_batch=?")
                .bind(state.owner_id).bind(input.id).bind(&batch).execute(&mut *tx).await?;
        }
    } else {
        sqlx::query("DELETE FROM rust_legacy_item_addresses WHERE item_id=? AND user_id=?")
            .bind(input.id)
            .bind(state.owner_id)
            .execute(&mut *tx)
            .await?;
    }
    sqlx::query(sqlx::AssertSqlSafe(format!(
        "DELETE FROM {table} WHERE id=? AND user_id=? AND deleted_at IS NOT NULL"
    )))
    .bind(input.id)
    .bind(state.owner_id)
    .execute(&mut *tx)
    .await?;
    let revision = navigation_revision(&mut tx, state.owner_id).await?;
    tx.commit().await?;
    Ok(revision_result(json!({"revision":revision}), revision))
}

/// Navigation uses two set-based reads, independent of group count.
/// A transaction gives export and the home screen a consistent snapshot.
async fn navigation_tree(state: &AppState, user_id: i64) -> Result<Vec<Value>, AppError> {
    let mut tx = state.db.begin().await?;
    let groups = navigation_tree_in_transaction(&mut tx, user_id).await?;
    tx.commit().await?;
    Ok(groups)
}

async fn navigation_tree_in_transaction(
    conn: &mut SqliteConnection,
    user_id: i64,
) -> Result<Vec<Value>, AppError> {
    let mut groups = groups_in_transaction(conn, user_id).await?;
    let rows = sqlx::query(
        "SELECT * FROM item_icon WHERE user_id=? AND deleted_at IS NULL ORDER BY sort,id",
    )
    .bind(user_id)
    .fetch_all(conn)
    .await?;
    let mut by_group: HashMap<i64, Vec<Value>> = HashMap::new();
    for row in &rows {
        by_group
            .entry(number_column(row, "item_icon_group_id"))
            .or_default()
            .push(item_json(row));
    }
    for group in &mut groups {
        let id = group["id"].as_i64().unwrap_or_default();
        group["items"] = json!(by_group.remove(&id).unwrap_or_default());
    }
    Ok(groups)
}

async fn get_navigation(State(state): State<AppState>) -> ApiResult {
    let mut tx = state.db.begin().await?;
    let revision = navigation_revision(&mut tx, state.owner_id).await?;
    let groups = navigation_tree_in_transaction(&mut tx, state.owner_id).await?;
    tx.commit().await?;
    Ok(revision_result(json!({"list":groups,"count":0}), revision))
}

fn revision_result(data: Value, revision: i64) -> Json<Value> {
    Json(json!({"code":0,"msg":"OK","data":data,"revision":revision}))
}

fn revision_conflict(revision: i64) -> Json<Value> {
    Json(
        json!({"code":1409,"msg":"Configuration changed. Refresh before saving again.","currentRevision":revision}),
    )
}

async fn get_user_config(State(state): State<AppState>) -> ApiResult {
    let row = sqlx::query(
        "SELECT panel_json,search_engine_json,revision FROM user_config WHERE user_id=?",
    )
    .bind(state.owner_id)
    .fetch_optional(&state.db)
    .await?;
    let (panel, search, revision) = row
        .map(|row| {
            (
                serde_json::from_str::<Value>(&string_column(&row, "panel_json"))
                    .unwrap_or(Value::Null),
                serde_json::from_str::<Value>(&string_column(&row, "search_engine_json"))
                    .unwrap_or(Value::Null),
                number_column(&row, "revision"),
            )
        })
        .unwrap_or((json!({}), json!({}), 0));
    Ok(revision_result(
        json!({"userId":state.owner_id,"panel":panel,"searchEngine":search,"revision":revision}),
        revision,
    ))
}

async fn set_user_config(
    State(state): State<AppState>,
    headers: HeaderMap,
    Json(input): Json<UserConfigInput>,
) -> ApiResult {
    let panel = input
        .panel
        .as_ref()
        .map(|v| validate_json(v, "panel"))
        .transpose()?;
    let search = input
        .search_engine
        .as_ref()
        .map(|v| validate_json(v, "searchEngine"))
        .transpose()?;
    let mut tx = state.db.begin_with("BEGIN IMMEDIATE").await?;
    if let Some(panel) = &input.panel {
        check_retired_uploads(
            &mut tx,
            &headers,
            &crate::system::panel_upload_sources(panel),
        )
        .await?;
    }
    // Acquire SQLite's writer lock before reading. Concurrent clients never upgrade a stale snapshot.
    sqlx::query("INSERT OR IGNORE INTO user_config(user_id,panel_json,search_engine_json) VALUES(?,'{}','{}')")
        .bind(state.owner_id)
        .execute(&mut *tx)
        .await?;
    let changed = sqlx::query("UPDATE user_config SET panel_json=COALESCE(?,panel_json),search_engine_json=COALESCE(?,search_engine_json),revision=revision+1 WHERE user_id=? AND revision=?")
        .bind(panel).bind(search).bind(state.owner_id).bind(input.revision.unwrap_or(-1))
        .execute(&mut *tx).await?.rows_affected();
    let revision: i64 = sqlx::query_scalar("SELECT revision FROM user_config WHERE user_id=?")
        .bind(state.owner_id)
        .fetch_one(&mut *tx)
        .await?;
    if changed != 1 {
        tx.rollback().await?;
        return Ok(revision_conflict(revision));
    }
    tx.commit().await?;
    Ok(revision_result(json!({"revision":revision}), revision))
}

fn validate_module_name(name: &str) -> Result<(), AppError> {
    limit_text(name, "name", 255)?;
    if name.trim().is_empty() {
        return Err(AppError::bad_request("Module name is required"));
    }
    Ok(())
}

async fn get_module_config(
    State(state): State<AppState>,
    Json(input): Json<ModuleInput>,
) -> ApiResult {
    validate_module_name(&input.name)?;
    let row = sqlx::query(
        "SELECT value_json,revision,deleted_at FROM module_config WHERE user_id=? AND name=?",
    )
    .bind(state.owner_id)
    .bind(input.name.trim())
    .fetch_optional(&state.db)
    .await?;
    let (value, revision) = row
        .map(|row| {
            let deleted = row
                .try_get::<Option<String>, _>("deleted_at")
                .ok()
                .flatten()
                .is_some();
            (
                if deleted {
                    Value::Null
                } else {
                    serde_json::from_str(&string_column(&row, "value_json")).unwrap_or(Value::Null)
                },
                number_column(&row, "revision"),
            )
        })
        .unwrap_or((Value::Null, 0));
    Ok(revision_result(value, revision))
}

async fn save_module_config(
    State(state): State<AppState>,
    Json(input): Json<ModuleInput>,
) -> ApiResult {
    validate_module_name(&input.name)?;
    let value = validate_json(&input.value, "value")?;
    let mut tx = state.db.begin_with("BEGIN IMMEDIATE").await?;
    sqlx::query("INSERT OR IGNORE INTO module_config(user_id,name) VALUES(?,?)")
        .bind(state.owner_id)
        .bind(input.name.trim())
        .execute(&mut *tx)
        .await?;
    let sql = format!(
        "UPDATE module_config SET value_json=?,revision=revision+1,updated_at={NOW},deleted_at=NULL WHERE user_id=? AND name=? AND revision=?"
    );
    let changed = sqlx::query(sqlx::AssertSqlSafe(sql.as_str()))
        .bind(value)
        .bind(state.owner_id)
        .bind(input.name.trim())
        .bind(input.revision.unwrap_or(-1))
        .execute(&mut *tx)
        .await?
        .rows_affected();
    let revision: i64 =
        sqlx::query_scalar("SELECT revision FROM module_config WHERE user_id=? AND name=?")
            .bind(state.owner_id)
            .bind(input.name.trim())
            .fetch_one(&mut *tx)
            .await?;
    if changed != 1 {
        tx.rollback().await?;
        return Ok(revision_conflict(revision));
    }
    tx.commit().await?;
    Ok(revision_result(json!({"revision":revision}), revision))
}

async fn export_navigation(State(state): State<AppState>) -> ApiResult {
    let groups = navigation_tree(&state, state.owner_id).await?;
    // This deliberately describes navigation only: uploaded bytes, secrets,
    // users and instance settings are outside this format.
    Ok(ok(
        json!({ "appName": "Sun-Panel-Navigation", "schemaVersion": 1, "scope": "navigation", "groups": groups }),
    ))
}

type ImportedGroups = Vec<(GroupInput, Vec<ItemInput>)>;

#[derive(Debug, Clone, Copy, PartialEq, Eq, Deserialize, Serialize, Default)]
#[serde(rename_all = "lowercase")]
enum ImportMode {
    Append,
    #[default]
    Merge,
    Replace,
}

fn parse_import(value: &Value) -> Result<(ImportedGroups, ImportMode), AppError> {
    if value.get("appName").and_then(Value::as_str) != Some("Sun-Panel-Navigation")
        || value.get("schemaVersion").and_then(Value::as_u64) != Some(1)
        || ["version", "icons", "replace"]
            .iter()
            .any(|field| value.get(field).is_some())
    {
        return Err(AppError::bad_request(
            "Only Sun-Panel-Navigation schema version 1 is supported",
        ));
    }
    let groups = value
        .get("groups")
        .and_then(Value::as_array)
        .ok_or_else(|| AppError::bad_request("Navigation groups must be an array"))?;
    if groups.len() > MAX_BATCH {
        return Err(AppError::bad_request("Import supports at most 1000 groups"));
    }
    let mut result = Vec::with_capacity(groups.len());
    let mut count = 0;
    for value in groups {
        if value.get("children").is_some() {
            return Err(AppError::bad_request("Use the current group items format"));
        }
        let mut group: GroupInput = serde_json::from_value(value.clone())
            .map_err(|_| AppError::bad_request("Invalid imported group"))?;
        group.id = 0;
        group.title = group.title.trim().to_owned();
        validate_group(&group)?;
        let children = value
            .get("items")
            .and_then(Value::as_array)
            .ok_or_else(|| AppError::bad_request("Each group requires an items array"))?;
        count += children.len();
        if count > MAX_IMPORT_ITEMS {
            return Err(AppError::bad_request("Import supports at most 10000 items"));
        }
        let mut items = Vec::with_capacity(children.len());
        for value in children {
            let mut item: ItemInput = serde_json::from_value(value.clone())
                .map_err(|_| AppError::bad_request("Invalid imported item"))?;
            if value.get("lanUrl").is_some() {
                return Err(AppError::bad_request("Each item supports only one url"));
            }
            item.id = 0;
            item.title = item.title.trim().to_owned();
            item.item_icon_group_id = 0;
            validate_item(&item, false)?;
            items.push(item);
        }
        result.push((group, items));
    }
    let mode = value
        .get("mode")
        .map(|mode| {
            serde_json::from_value(mode.clone())
                .map_err(|_| AppError::bad_request("mode must be append, merge or replace"))
        })
        .transpose()?
        .unwrap_or_default();
    Ok((result, mode))
}

#[derive(Default, Serialize)]
#[serde(rename_all = "camelCase")]
struct ImportStats {
    groups: usize,
    items: usize,
    new_groups: usize,
    updated_groups: usize,
    duplicate_groups: usize,
    new_items: usize,
    updated_items: usize,
    duplicate_items: usize,
    removed_groups: usize,
    removed_items: usize,
    conflicts: Vec<String>,
    revision: i64,
    mode: ImportMode,
}

struct PlannedItem {
    value: ItemInput,
    target_id: Option<i64>,
    update: bool,
}
struct PlannedGroup {
    value: GroupInput,
    target_id: Option<i64>,
    update: bool,
    items: Vec<PlannedItem>,
}

fn same_group(row: &SqliteRow, group: &GroupInput) -> bool {
    string_column(row, "title") == group.title
        && string_column(row, "icon") == group.icon
        && string_column(row, "description") == group.description
        && number_column(row, "sort") == group.sort.unwrap_or(0)
}
fn same_item(row: &SqliteRow, item: &ItemInput) -> bool {
    string_column(row, "title") == item.title
        && string_column(row, "description") == item.description
        && number_column(row, "open_method") == item.open_method
        && number_column(row, "sort") == item.sort.unwrap_or(9999)
        && serde_json::from_str::<Option<Icon>>(&string_column(row, "icon_json"))
            .ok()
            .flatten()
            .map(|icon| serde_json::to_value(icon).unwrap_or(Value::Null))
            == item
                .icon
                .as_ref()
                .map(|icon| serde_json::to_value(icon).unwrap_or(Value::Null))
}

async fn navigation_revision(conn: &mut SqliteConnection, user_id: i64) -> Result<i64, AppError> {
    Ok(
        sqlx::query_scalar("SELECT revision FROM rust_navigation_revision WHERE user_id=?")
            .bind(user_id)
            .fetch_optional(conn)
            .await?
            .unwrap_or(0),
    )
}

async fn import_plan(
    conn: &mut SqliteConnection,
    user_id: i64,
    groups: ImportedGroups,
    mode: ImportMode,
) -> Result<(Vec<PlannedGroup>, ImportStats), AppError> {
    let existing_groups =
        sqlx::query("SELECT * FROM item_icon_group WHERE user_id=? AND deleted_at IS NULL")
            .bind(user_id)
            .fetch_all(&mut *conn)
            .await?;
    let existing_items =
        sqlx::query("SELECT * FROM item_icon WHERE user_id=? AND deleted_at IS NULL")
            .bind(user_id)
            .fetch_all(&mut *conn)
            .await?;
    let mut groups_by_title: HashMap<String, Vec<&SqliteRow>> = HashMap::new();
    for group in &existing_groups {
        groups_by_title
            .entry(string_column(group, "title").trim().to_owned())
            .or_default()
            .push(group);
    }
    let mut items_by_group_url: HashMap<(i64, String), Vec<&SqliteRow>> = HashMap::new();
    for item in &existing_items {
        items_by_group_url
            .entry((
                number_column(item, "item_icon_group_id"),
                string_column(item, "url"),
            ))
            .or_default()
            .push(item);
    }
    let mut stats = ImportStats {
        groups: groups.len(),
        mode,
        revision: navigation_revision(conn, user_id).await?,
        ..Default::default()
    };
    if mode == ImportMode::Replace {
        stats.removed_groups = existing_groups.len();
        stats.removed_items = existing_items.len();
    }
    let mut seen_groups = HashSet::new();
    let mut plan = Vec::with_capacity(groups.len());
    for (group, items) in groups {
        let matches = groups_by_title
            .get(&group.title)
            .map(Vec::as_slice)
            .unwrap_or(&[]);
        if mode == ImportMode::Merge
            && (!seen_groups.insert(group.title.clone()) || matches.len() > 1)
        {
            stats.conflicts.push(format!(
                "Group '{}' is ambiguous; rename duplicate groups or choose append/replace",
                group.title
            ));
        }
        let previous = if mode == ImportMode::Merge && matches.len() == 1 {
            Some(matches[0])
        } else {
            None
        };
        let target_id = previous.map(|row| number_column(row, "id"));
        let update = previous.is_some_and(|row| !same_group(row, &group));
        if target_id.is_none() {
            stats.new_groups += 1;
        } else if update {
            stats.updated_groups += 1;
        } else {
            stats.duplicate_groups += 1;
        }
        stats.items += items.len();
        let mut planned_items = Vec::with_capacity(items.len());
        let mut seen_urls = HashSet::new();
        for item in items {
            let matches = target_id
                .and_then(|id| items_by_group_url.get(&(id, item.url.clone())))
                .map(Vec::as_slice)
                .unwrap_or(&[]);
            if mode == ImportMode::Merge
                && (!seen_urls.insert(item.url.clone()) || matches.len() > 1)
            {
                stats.conflicts.push(format!("URL '{}' in group '{}' is ambiguous; choose append/replace or remove duplicate URLs",item.url,group.title));
            }
            let previous = if matches.len() == 1 {
                Some(matches[0])
            } else {
                None
            };
            let target_id = previous.map(|row| number_column(row, "id"));
            let update = previous.is_some_and(|row| !same_item(row, &item));
            if target_id.is_none() {
                stats.new_items += 1;
            } else if update {
                stats.updated_items += 1;
            } else {
                stats.duplicate_items += 1;
            }
            planned_items.push(PlannedItem {
                value: item,
                target_id,
                update,
            });
        }
        plan.push(PlannedGroup {
            value: group,
            target_id,
            update,
            items: planned_items,
        });
    }
    Ok((plan, stats))
}

async fn preview_import(State(state): State<AppState>, Json(value): Json<Value>) -> ApiResult {
    let (groups, mode) = parse_import(&value)?;
    let mut tx = state.db.begin().await?;
    let (_, stats) = import_plan(&mut tx, state.owner_id, groups, mode).await?;
    tx.commit().await?;
    Ok(ok(stats))
}

async fn import_navigation(
    State(state): State<AppState>,
    headers: HeaderMap,
    Json(value): Json<Value>,
) -> ApiResult {
    let (groups, mode) = parse_import(&value)?;
    let mut tx = state.db.begin_with("BEGIN IMMEDIATE").await?;
    // Serialize the writer before reading the preview revision and matching records.
    sqlx::query("INSERT OR IGNORE INTO rust_navigation_revision(user_id) VALUES(?)")
        .bind(state.owner_id)
        .execute(&mut *tx)
        .await?;
    let (plan, mut stats) = import_plan(&mut tx, state.owner_id, groups, mode).await?;
    if value.get("revision").and_then(Value::as_i64) != Some(stats.revision) {
        tx.rollback().await?;
        return Ok(Json(
            json!({"code":1409,"msg":"Navigation changed. Preview the import again.","currentRevision":stats.revision}),
        ));
    }
    if !stats.conflicts.is_empty() {
        tx.rollback().await?;
        return Ok(Json(
            json!({"code":1400,"msg":"Import contains ambiguous matches. Review conflicts or choose another mode.","data":stats}),
        ));
    }
    let sources = plan
        .iter()
        .flat_map(|group| {
            group
                .items
                .iter()
                .flat_map(|item| item_upload_sources(&item.value))
        })
        .collect::<Vec<_>>();
    check_retired_uploads(&mut tx, &headers, &sources).await?;
    if mode == ImportMode::Replace {
        for table in ["item_icon", "item_icon_group"] {
            let sql = format!(
                "UPDATE {table} SET deleted_at={NOW},updated_at={NOW} WHERE user_id=? AND deleted_at IS NULL"
            );
            sqlx::query(sqlx::AssertSqlSafe(sql.as_str()))
                .bind(state.owner_id)
                .execute(&mut *tx)
                .await?;
        }
    }
    for group in plan {
        let group_id = if let Some(id) = group.target_id {
            if group.update {
                let sql = format!(
                    "UPDATE item_icon_group SET icon=?,title=?,description=?,sort=?,updated_at={NOW} WHERE id=? AND user_id=? AND deleted_at IS NULL"
                );
                sqlx::query(sqlx::AssertSqlSafe(sql.as_str()))
                    .bind(&group.value.icon)
                    .bind(&group.value.title)
                    .bind(&group.value.description)
                    .bind(group.value.sort.unwrap_or(0))
                    .bind(id)
                    .bind(state.owner_id)
                    .execute(&mut *tx)
                    .await?;
            }
            id
        } else {
            insert_group(&mut tx, state.owner_id, &group.value).await?
        };
        for planned in group.items {
            let mut item = planned.value;
            item.item_icon_group_id = group_id;
            if let Some(id) = planned.target_id {
                if planned.update {
                    let sql = format!(
                        "UPDATE item_icon SET icon_json=?,title=?,url=?,lan_url='',description=?,open_method=?,sort=?,updated_at={NOW} WHERE id=? AND user_id=? AND deleted_at IS NULL"
                    );
                    sqlx::query(sqlx::AssertSqlSafe(sql.as_str()))
                        .bind(
                            serde_json::to_string(&item.icon)
                                .map_err(|_| AppError::bad_request("Invalid icon"))?,
                        )
                        .bind(&item.title)
                        .bind(&item.url)
                        .bind(&item.description)
                        .bind(item.open_method)
                        .bind(item.sort.unwrap_or(9999))
                        .bind(id)
                        .bind(state.owner_id)
                        .execute(&mut *tx)
                        .await?;
                }
            } else {
                insert_item(&mut tx, state.owner_id, &item).await?;
            }
        }
    }
    stats.revision = navigation_revision(&mut tx, state.owner_id).await?;
    tx.commit().await?;
    let mut data =
        serde_json::to_value(stats).map_err(|_| AppError::bad_request("Invalid import result"))?;
    data["replaced"] = json!(mode == ImportMode::Replace);
    data["scope"] = json!("navigation");
    Ok(ok(data))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn rejects_executable_urls_and_unsafe_upload_paths() {
        for url in [
            "javascript:alert(1)",
            "data:text/html,x",
            "file:///etc/passwd",
            "//example.com",
            "http://user:secret@example.com",
            "uploads/../secret",
            "/uploads/%2e%2e/secret",
        ] {
            assert!(validate_url(url, "url", false, true).is_err(), "{url}");
        }
        for url in [
            "https://example.com/a?q=1",
            "http://192.168.1.1:8080",
            "/uploads/2026/10/image.png",
            "uploads/image.png",
            "./uploads/image.png",
        ] {
            assert!(validate_url(url, "url", false, true).is_ok(), "{url}");
        }
    }

    #[test]
    fn validates_batches_and_explicit_zero_sort() {
        assert!(validate_ids(&[]).is_err());
        assert!(validate_ids(&[1, 1]).is_err());
        assert!(validate_ids(&[0]).is_err());
        assert!(validate_ids(&[1, 2]).is_ok());
        assert!(validate_sort(0).is_ok());
        assert!(validate_sort(i64::MAX).is_err());
    }

    #[test]
    fn accepts_native_navigation_and_rejects_legacy_or_partial_formats() {
        let value = json!({"appName":"Sun-Panel-Navigation", "schemaVersion":1, "groups":[{"title":"Apps", "items":[{"title":"NAS", "url":"http://192.168.1.1", "openMethod":2}]}]});
        let (groups, mode) = parse_import(&value).unwrap();
        assert_eq!(groups.len(), 1);
        assert_eq!(groups[0].1.len(), 1);
        assert_eq!(mode, ImportMode::Merge);
        for invalid in [
            json!({"appName":"Sun-Panel-Config", "version":1, "icons":[]}),
            json!({"appName":"Sun-Panel-Navigation", "schemaVersion":2, "groups":[]}),
            json!({"appName":"Sun-Panel-Navigation", "schemaVersion":1, "groups":[], "replace":true}),
            json!({"appName":"Sun-Panel-Navigation", "schemaVersion":1, "groups":[{"title":"Apps", "children":[]}]}),
            json!({"appName":"Sun-Panel-Navigation", "schemaVersion":1, "groups":[{"title":"Apps", "items":[{"title":"NAS","url":"http://192.168.1.1","lanUrl":"http://192.168.1.2","openMethod":2}]}]}),
        ] {
            assert!(parse_import(&invalid).is_err(), "{invalid}");
        }
        let mut invalid = value;
        invalid["groups"][0]["items"][0]["url"] = json!("javascript:alert(1)");
        assert!(parse_import(&invalid).is_err());
    }
}
