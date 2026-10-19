//! HTTP regressions for local workspace scope and all-or-nothing writes.
//! A second test-only state represents archived data belonging to another owner.
use axum::{Router, body::Body, http::Request};
use http_body_util::BodyExt;
use serde_json::{Value, json};
use sun_panel_rust::{app, config::Config, initialize, state::AppState};
use tempfile::TempDir;
use tower::ServiceExt;

struct Fixture {
    _directory: TempDir,
    state: AppState,
    router: Router,
    other_router: Router,
    admin: String,
    other: String,
    admin_id: i64,
    other_id: i64,
}

async fn request(router: &Router, path: &str, value: Value) -> Value {
    let builder = Request::builder()
        .method("POST")
        .uri(path)
        .header("content-type", "application/json");
    let response = router
        .clone()
        .oneshot(builder.body(Body::from(value.to_string())).unwrap())
        .await
        .unwrap();
    let status = response.status();
    let bytes = response.into_body().collect().await.unwrap().to_bytes();
    assert!(
        status.is_success(),
        "{path}: {status}: {}",
        String::from_utf8_lossy(&bytes)
    );
    serde_json::from_slice(&bytes).unwrap()
}

async fn navigation(router: &Router) -> Value {
    let value = request(router, "/api/panel/navigation", json!({})).await;
    assert_ok(&value);
    assert!(value["revision"].is_i64());
    value
}

fn group_item_ids(navigation: &Value, group_id: i64) -> Vec<i64> {
    navigation["data"]["list"]
        .as_array()
        .unwrap()
        .iter()
        .find(|group| group["id"] == group_id)
        .unwrap()["items"]
        .as_array()
        .unwrap()
        .iter()
        .map(|item| item["id"].as_i64().unwrap())
        .collect()
}

fn assert_ok(value: &Value) {
    assert_eq!(value["code"], 0, "{value}");
}

#[tokio::test]
async fn direct_moves_persist_cross_group_positions_same_group_positions_and_empty_groups() {
    let f = Fixture::new().await;
    let source = f.group(&f.admin, "Source").await;
    let target = f.group(&f.admin, "Target").await;
    let empty = f.group(&f.admin, "Empty").await;
    let first = f.item(&f.admin, source, "First").await;
    let second = f.item(&f.admin, source, "Second").await;
    let third = f.item(&f.admin, source, "Third").await;
    let fourth = f.item(&f.admin, target, "Fourth").await;
    let fifth = f.item(&f.admin, target, "Fifth").await;
    let mut revision = navigation(&f.router).await["revision"].as_i64().unwrap();

    for (destination, index, expected_source, expected_target) in [
        (target, 1, vec![second, third], vec![fourth, first, fifth]),
        (target, 0, vec![second, third], vec![first, fourth, fifth]),
        (target, 2, vec![second, third], vec![fourth, fifth, first]),
    ] {
        let moved = request(
            &f.router,
            "/api/panel/navigation/move",
            json!({
                "itemId":first,"targetGroupId":destination,"targetIndex":index,"revision":revision
            }),
        )
        .await;
        assert_ok(&moved);
        assert_eq!(moved["revision"], moved["data"]["revision"]);
        assert!(moved["revision"].as_i64().unwrap() > revision);
        revision = moved["revision"].as_i64().unwrap();
        let snapshot = navigation(&f.router).await;
        assert_eq!(snapshot["revision"], revision);
        assert_eq!(group_item_ids(&snapshot, source), expected_source);
        assert_eq!(group_item_ids(&snapshot, target), expected_target);
        for group in snapshot["data"]["list"].as_array().unwrap() {
            for (index, item) in group["items"].as_array().unwrap().iter().enumerate() {
                assert_eq!(item["itemIconGroupId"], group["id"]);
                assert_eq!(item["sort"], index as i64);
            }
        }
    }
    let unchanged = navigation(&f.router).await;
    let no_op = request(
        &f.router,
        "/api/panel/navigation/move",
        json!({
            "itemId":first,"targetGroupId":target,"targetIndex":2,"revision":revision
        }),
    )
    .await;
    assert_ok(&no_op);
    assert_eq!(
        no_op["revision"], revision,
        "a no-op must not create a conflict in another page"
    );
    assert_eq!(navigation(&f.router).await, unchanged);

    let moved = request(
        &f.router,
        "/api/panel/navigation/move",
        json!({
            "itemId":first,"targetGroupId":empty,"targetIndex":0,"revision":revision
        }),
    )
    .await;
    assert_ok(&moved);
    let snapshot = navigation(&f.router).await;
    assert_eq!(group_item_ids(&snapshot, source), [second, third]);
    assert_eq!(group_item_ids(&snapshot, target), [fourth, fifth]);
    assert_eq!(group_item_ids(&snapshot, empty), [first]);
}

#[tokio::test]
async fn direct_moves_reject_foreign_deleted_missing_and_invalid_targets_without_changes() {
    let f = Fixture::new().await;
    let source = f.group(&f.admin, "Source").await;
    let target = f.group(&f.admin, "Target").await;
    let archived = f.group(&f.admin, "Deleted").await;
    let foreign = f.group(&f.other, "Foreign").await;
    let item = f.item(&f.admin, source, "First").await;
    let foreign_item = f.item(&f.other, foreign, "Foreign item").await;
    let deleted_item = f.item(&f.admin, source, "Deleted item").await;
    sqlx::query("UPDATE item_icon_group SET deleted_at=CURRENT_TIMESTAMP WHERE id=?")
        .bind(archived)
        .execute(&f.state.db)
        .await
        .unwrap();
    sqlx::query("UPDATE item_icon SET deleted_at=CURRENT_TIMESTAMP WHERE id=?")
        .bind(deleted_item)
        .execute(&f.state.db)
        .await
        .unwrap();
    let before = navigation(&f.router).await;
    let other_before = navigation(&f.other_router).await;
    let revision = before["revision"].as_i64().unwrap();
    for (moving, destination, index, expected_code) in [
        (item, target, -1, 1400),
        (item, target, 1, 1400),
        (item, source, 1, 1400),
        (item, target, i64::MAX, 1400),
        (0, target, 0, 1400),
        (item, 0, 0, 1400),
        (i64::MAX, target, 0, 1005),
        (foreign_item, target, 0, 1005),
        (deleted_item, target, 0, 1005),
        (item, foreign, 0, 1005),
        (item, archived, 0, 1005),
        (item, i64::MAX, 0, 1005),
    ] {
        let rejected = request(
            &f.router,
            "/api/panel/navigation/move",
            json!({
                "itemId":moving,"targetGroupId":destination,"targetIndex":index,"revision":revision
            }),
        )
        .await;
        assert_eq!(rejected["code"], expected_code, "{rejected}");
        assert_eq!(navigation(&f.router).await, before);
        assert_eq!(navigation(&f.other_router).await, other_before);
    }
}

#[tokio::test]
async fn direct_moves_use_revision_cas_and_only_one_concurrent_page_can_commit() {
    let f = Fixture::new().await;
    let source = f.group(&f.admin, "Source").await;
    let target = f.group(&f.admin, "Target").await;
    let first = f.item(&f.admin, source, "First").await;
    let second = f.item(&f.admin, source, "Second").await;
    let before = navigation(&f.router).await;
    let revision = before["revision"].as_i64().unwrap();
    let missing = request(
        &f.router,
        "/api/panel/navigation/move",
        json!({
            "itemId":first,"targetGroupId":target,"targetIndex":0
        }),
    )
    .await;
    assert_eq!(missing["code"], 1409);
    assert_eq!(missing["currentRevision"], revision);
    assert_eq!(navigation(&f.router).await, before);
    let first_move =
        json!({"itemId":first,"targetGroupId":target,"targetIndex":0,"revision":revision});
    let second_move =
        json!({"itemId":second,"targetGroupId":target,"targetIndex":0,"revision":revision});
    let (one, two) = tokio::join!(
        request(&f.router, "/api/panel/navigation/move", first_move.clone()),
        request(&f.router, "/api/panel/navigation/move", second_move),
    );
    let codes = [one["code"].as_i64().unwrap(), two["code"].as_i64().unwrap()];
    assert!(codes == [0, 1409] || codes == [1409, 0], "{one}; {two}");
    let after = navigation(&f.router).await;
    assert_eq!(group_item_ids(&after, source).len(), 1);
    assert_eq!(group_item_ids(&after, target).len(), 1);
    let rejected = request(&f.router, "/api/panel/navigation/move", first_move).await;
    assert_eq!(rejected["code"], 1409);
    assert_eq!(rejected["currentRevision"], after["revision"]);
    assert_eq!(navigation(&f.router).await, after);
}

#[tokio::test]
async fn direct_moves_roll_back_source_target_and_revision_when_target_write_fails() {
    let f = Fixture::new().await;
    let source = f.group(&f.admin, "Source").await;
    let target = f.group(&f.admin, "Target").await;
    let first = f.item(&f.admin, source, "First").await;
    f.item(&f.admin, source, "Second").await;
    f.item(&f.admin, target, "Target item").await;
    let before = navigation(&f.router).await;
    let revision = before["revision"].as_i64().unwrap();
    // The source item's sort has already changed by the time this target write fails.
    sqlx::query(sqlx::AssertSqlSafe(format!(
        "CREATE TRIGGER fail_target_move BEFORE UPDATE ON item_icon WHEN OLD.id={first} AND NEW.item_icon_group_id={target} BEGIN SELECT RAISE(ABORT,'injected move failure'); END"
    ))).execute(&f.state.db).await.unwrap();
    let failed = request(
        &f.router,
        "/api/panel/navigation/move",
        json!({
            "itemId":first,"targetGroupId":target,"targetIndex":1,"revision":revision
        }),
    )
    .await;
    assert_eq!(failed["code"], 1200);
    assert_eq!(
        navigation(&f.router).await,
        before,
        "every sort, membership and revision write must roll back"
    );
}

#[tokio::test]
async fn direct_moves_and_navigation_share_stable_sort_then_id_order() {
    let f = Fixture::new().await;
    let group = f.group(&f.admin, "Source").await;
    let first = f.item(&f.admin, group, "First").await;
    let second = f.item(&f.admin, group, "Second").await;
    let third = f.item(&f.admin, group, "Third").await;
    sqlx::query("UPDATE item_icon SET created_at='2000-01-01' WHERE id=?")
        .bind(third)
        .execute(&f.state.db)
        .await
        .unwrap();
    let before = navigation(&f.router).await;
    assert_eq!(group_item_ids(&before, group), [first, second, third]);
    let moved = request(
        &f.router,
        "/api/panel/navigation/move",
        json!({
            "itemId":second,"targetGroupId":group,"targetIndex":2,"revision":before["revision"]
        }),
    )
    .await;
    assert_ok(&moved);
    assert_eq!(
        group_item_ids(&navigation(&f.router).await, group),
        [first, third, second]
    );
}

impl Fixture {
    async fn new() -> Self {
        let directory = TempDir::new().unwrap();
        let root = directory.path();
        let config = Config {
            bind: "127.0.0.1:0".into(),
            database_path: root.join("database.db"),
            uploads_dir: root.join("uploads"),
            web_dir: root.join("web"),
            max_upload_bytes: 1024 * 1024,
            monitor_interval_secs: 5,
            allow_private_favicon: false,
            worker_threads: 1,
            database_connections: 2,
        };
        let state = initialize(config).await.unwrap();
        let router = app(state.clone());
        let admin_id = state.owner_id;
        let other_id = sqlx::query("INSERT INTO user(username,password,name,status,role) VALUES('archived-user','preserved-hash','Archived',1,2)")
            .execute(&state.db).await.unwrap().last_insert_rowid();
        let other_state = AppState {
            owner_id: other_id,
            ..state.clone()
        };
        let other_router = app(other_state);
        let admin = "local".to_owned();
        let other = "archived".to_owned();
        Self {
            _directory: directory,
            state,
            router,
            other_router,
            admin,
            other,
            admin_id,
            other_id,
        }
    }

    async fn post(&self, path: &str, token: &str, mut value: Value) -> Value {
        if path == "/api/panel/navigation/import" && value.get("revision").is_none() {
            let user = if token == self.other {
                self.other_id
            } else {
                self.admin_id
            };
            let revision: Option<i64> =
                sqlx::query_scalar("SELECT revision FROM rust_navigation_revision WHERE user_id=?")
                    .bind(user)
                    .fetch_optional(&self.state.db)
                    .await
                    .unwrap();
            value["revision"] = json!(revision.unwrap_or(0));
        }
        let router = if token == self.other {
            &self.other_router
        } else {
            &self.router
        };
        request(router, path, value).await
    }

    async fn group(&self, token: &str, title: &str) -> i64 {
        let response = self
            .post(
                "/api/panel/itemIconGroup/edit",
                token,
                json!({"title":title,"icon":"mdi:folder","sort":0}),
            )
            .await;
        assert_ok(&response);
        response["data"]["id"].as_i64().unwrap()
    }

    async fn item(&self, token: &str, group: i64, title: &str) -> i64 {
        let response = self.post("/api/panel/itemIcon/edit", token, json!({"title":title,"url":"https://example.com","openMethod":2,"itemIconGroupId":group,"sort":7,"icon":{"itemType":1,"text":"APP"}})).await;
        assert_ok(&response);
        response["data"]["id"].as_i64().unwrap()
    }

    async fn live_count(&self, table: &str, user: i64) -> i64 {
        // Table identifiers only come from the static test source.
        sqlx::query_scalar(sqlx::AssertSqlSafe(format!(
            "SELECT count(*) FROM {table} WHERE user_id=? AND deleted_at IS NULL"
        )))
        .bind(user)
        .fetch_one(&self.state.db)
        .await
        .unwrap()
    }
}

#[tokio::test]
async fn mutations_reject_foreign_records_and_roll_back_entire_batches() {
    let f = Fixture::new().await;
    let first_group = f.group(&f.admin, "First").await;
    let second_group = f.group(&f.admin, "Second").await;
    let foreign_group = f.group(&f.other, "Private").await;
    let first = f.item(&f.admin, first_group, "First item").await;
    let second = f.item(&f.admin, second_group, "Second item").await;
    let foreign = f.item(&f.other, foreign_group, "Private item").await;

    let result = f.post("/api/panel/itemIcon/edit", &f.admin, json!({"id":foreign,"title":"Stolen","url":"https://example.com","openMethod":2,"itemIconGroupId":first_group})).await;
    assert_eq!(result["code"], 1005);
    let result = f
        .post(
            "/api/panel/itemIconGroup/edit",
            &f.admin,
            json!({"id":foreign_group,"title":"Stolen group"}),
        )
        .await;
    assert_eq!(result["code"], 1005);
    let result = f.post("/api/panel/itemIcon/edit", &f.admin, json!({"id":first,"title":"Move","url":"https://example.com","openMethod":2,"itemIconGroupId":foreign_group})).await;
    assert_eq!(result["code"], 1005);
    let result = f
        .post(
            "/api/panel/itemIcon/deletes",
            &f.admin,
            json!({"ids":[first,foreign]}),
        )
        .await;
    assert_eq!(result["code"], 1005);
    assert_eq!(f.live_count("item_icon", f.admin_id).await, 2);
    assert_eq!(f.live_count("item_icon", f.other_id).await, 1);

    let result = f
        .post(
            "/api/panel/itemIconGroup/saveSort",
            &f.admin,
            json!({"sortItems":[{"id":first_group,"sort":123},{"id":foreign_group,"sort":456}]}),
        )
        .await;
    assert_eq!(result["code"], 1005);
    let sort: i64 = sqlx::query_scalar("SELECT sort FROM item_icon_group WHERE id=?")
        .bind(first_group)
        .fetch_one(&f.state.db)
        .await
        .unwrap();
    assert_eq!(sort, 0);

    let result = f
        .post(
            "/api/panel/itemIconGroup/deletes",
            &f.admin,
            json!({"ids":[first_group,foreign_group]}),
        )
        .await;
    assert_eq!(result["code"], 1005);
    assert_eq!(f.live_count("item_icon_group", f.admin_id).await, 2);
    let result = f
        .post(
            "/api/panel/itemIconGroup/deletes",
            &f.admin,
            json!({"ids":[first_group,second_group]}),
        )
        .await;
    assert_eq!(result["code"], 1201);
    assert_eq!(f.live_count("item_icon_group", f.admin_id).await, 2);

    let result = f
        .post(
            "/api/panel/itemIconGroup/deletes",
            &f.admin,
            json!({"ids":[first_group]}),
        )
        .await;
    assert_ok(&result);
    assert_eq!(f.live_count("item_icon", f.admin_id).await, 1);
    assert_eq!(f.live_count("item_icon", f.other_id).await, 1);
    let result = f.post("/api/panel/navigation", &f.admin, json!({})).await;
    assert_ok(&result);
    let groups = result["data"]["list"].as_array().unwrap();
    assert_eq!(groups.len(), 1);
    assert_eq!(groups[0]["id"], second_group);
    assert_eq!(groups[0]["items"][0]["id"], second);
    assert_eq!(groups[0]["items"][0]["sort"], 7);
}

#[tokio::test]
async fn anonymous_configuration_upserts_preserve_workspace_and_archived_data() {
    let f = Fixture::new().await;
    let initial = f
        .post("/api/panel/userConfig/get", &f.admin, json!({}))
        .await;
    assert_ok(&initial);
    assert_eq!(initial["data"]["panel"], json!({}));
    assert_eq!(initial["data"]["searchEngine"], json!({}));
    for (token, label, attempted_owner) in [
        (&f.admin, "Admin", f.other_id),
        (&f.other, "Other", f.admin_id),
    ] {
        let result = f.post("/api/panel/userConfig/set", token, json!({"revision":0,"userId":attempted_owner,"panel":{"logoText":label},"searchEngine":{"provider":label}})).await;
        assert_ok(&result);
        let result = f
            .post(
                "/api/system/moduleConfig/save",
                token,
                json!({"revision":0,"userId":attempted_owner,"name":"clock","value":{"label":label}}),
            )
            .await;
        assert_ok(&result);
    }
    for (token, label, actual_owner) in [
        (&f.admin, "Admin", f.admin_id),
        (&f.other, "Other", f.other_id),
    ] {
        let result = f.post("/api/panel/userConfig/get", token, json!({})).await;
        assert_ok(&result);
        assert_eq!(result["data"]["userId"], actual_owner);
        assert_eq!(result["data"]["panel"]["logoText"], label);
        assert_eq!(result["data"]["searchEngine"]["provider"], label);
        let result = f
            .post(
                "/api/system/moduleConfig/getByName",
                token,
                json!({"name":"clock"}),
            )
            .await;
        assert_ok(&result);
        assert_eq!(result["data"]["label"], label);
    }
    let result = f
        .post(
            "/api/panel/userConfig/set",
            &f.admin,
            json!({"revision":1,"panel":{"logoText":"Updated"}}),
        )
        .await;
    assert_ok(&result);
    let result = f
        .post("/api/panel/userConfig/get", &f.admin, json!({}))
        .await;
    assert_eq!(
        result["data"]["searchEngine"]["provider"], "Admin",
        "Saving panel style must retain the search configuration"
    );
    let result = f
        .post(
            "/api/panel/userConfig/set",
            &f.admin,
            json!({"revision":2,"searchEngine":{"provider":"Updated search"}}),
        )
        .await;
    assert_ok(&result);
    let result = f
        .post("/api/panel/userConfig/get", &f.admin, json!({}))
        .await;
    assert_eq!(
        result["data"]["panel"]["logoText"], "Updated",
        "Saving search configuration must retain the panel style"
    );
    assert_eq!(result["data"]["searchEngine"]["provider"], "Updated search");
    let result = f
        .post(
            "/api/system/moduleConfig/save",
            &f.admin,
            json!({"revision":1,"name":"clock","value":{"label":"Updated"}}),
        )
        .await;
    assert_ok(&result);
    let config_count: i64 = sqlx::query_scalar("SELECT count(*) FROM user_config")
        .fetch_one(&f.state.db)
        .await
        .unwrap();
    let module_count: i64 =
        sqlx::query_scalar("SELECT count(*) FROM module_config WHERE name='clock'")
            .fetch_one(&f.state.db)
            .await
            .unwrap();
    assert_eq!(config_count, 2);
    assert_eq!(module_count, 2);
    let result = f
        .post(
            "/api/system/moduleConfig/getByName",
            &f.other,
            json!({"name":"clock"}),
        )
        .await;
    assert_eq!(result["data"]["label"], "Other");
    let result = f
        .post(
            "/api/system/moduleConfig/getByName",
            &f.other,
            json!({"name":"unknown"}),
        )
        .await;
    assert_ok(&result);
    assert!(result["data"].is_null());
    let result = f
        .post(
            "/api/panel/userConfig/set",
            &f.admin,
            json!({"panel":[],"searchEngine":{}}),
        )
        .await;
    assert_eq!(result["code"], 1400);
}

#[tokio::test]
async fn removed_import_formats_never_modify_the_current_navigation() {
    let f = Fixture::new().await;
    let group = f.group(&f.admin, "Current apps").await;
    f.item(&f.admin, group, "Keep this app").await;
    let before = request(&f.router, "/api/panel/navigation/export", json!({})).await;
    let mut documents = vec![
        json!({"appName":"Sun-Panel-Config","version":1,"icons":[]}),
        json!({"appName":"Sun-Panel-Navigation","schemaVersion":2,"groups":[]}),
        json!({"appName":"Sun-Panel-Navigation","schemaVersion":1,"groups":[],"replace":true}),
        json!({"appName":"Sun-Panel-Navigation","schemaVersion":1,"groups":[{"title":"Old","children":[]}]}),
        json!({"appName":"Sun-Panel-Navigation","schemaVersion":1,"groups":[{"title":"Old","items":[{"title":"Old address","url":"http://192.168.1.1","lanUrl":"http://192.168.1.2","openMethod":2}]}]}),
    ];
    let preview = request(
        &f.router,
        "/api/panel/navigation/import/preview",
        before["data"].clone(),
    )
    .await;
    for document in &mut documents {
        document["revision"] = preview["data"]["revision"].clone();
        for endpoint in [
            "/api/panel/navigation/import/preview",
            "/api/panel/navigation/import",
        ] {
            let rejected = request(&f.router, endpoint, document.clone()).await;
            assert_eq!(rejected["code"], 1400, "{document}: {rejected}");
        }
    }
    assert_eq!(
        request(&f.router, "/api/panel/navigation/export", json!({})).await,
        before
    );
}

#[tokio::test]
async fn native_imports_are_transactional_round_trip_and_preserve_other_owners() {
    let f = Fixture::new().await;
    let initial_group = f.group(&f.admin, "Existing").await;
    let initial_item = f.item(&f.admin, initial_group, "Existing item").await;
    let foreign_group = f.group(&f.other, "Private").await;
    f.item(&f.other, foreign_group, "Private item").await;

    let mut document = json!({"appName":"Sun-Panel-Navigation","schemaVersion":1,"scope":"navigation","mode":"replace","groups":[{"title":"Imported","items":[
        {"title":"Valid","url":"https://example.com","openMethod":2},
        {"title":"Invalid","url":"javascript:alert(1)","openMethod":2}
    ]}]});
    let result = f
        .post("/api/panel/navigation/import", &f.admin, document.clone())
        .await;
    assert_eq!(result["code"], 1400);
    assert_eq!(f.live_count("item_icon_group", f.admin_id).await, 1);
    assert_eq!(f.live_count("item_icon", f.admin_id).await, 1);

    // Exercise an actual database failure after replace and the first insert,
    // rather than only relying on pre-validation to avoid partial writes.
    sqlx::query("CREATE TRIGGER test_import_failure BEFORE INSERT ON item_icon WHEN NEW.title='Database failure' BEGIN SELECT RAISE(ABORT, 'injected test failure'); END")
        .execute(&f.state.db).await.unwrap();
    document["groups"][0]["items"][1]["title"] = json!("Database failure");
    document["groups"][0]["items"][1]["url"] = json!("https://example.org");
    let result = f
        .post("/api/panel/navigation/import", &f.admin, document)
        .await;
    assert_eq!(result["code"], 1200);
    assert_eq!(f.live_count("item_icon_group", f.admin_id).await, 1);
    assert_eq!(f.live_count("item_icon", f.admin_id).await, 1);
    let original_live: i64 =
        sqlx::query_scalar("SELECT count(*) FROM item_icon WHERE id=? AND deleted_at IS NULL")
            .bind(initial_item)
            .fetch_one(&f.state.db)
            .await
            .unwrap();
    assert_eq!(original_live, 1);
    sqlx::query("DROP TRIGGER test_import_failure")
        .execute(&f.state.db)
        .await
        .unwrap();

    let exported = f
        .post("/api/panel/navigation/export", &f.admin, json!({}))
        .await;
    assert_ok(&exported);
    assert_eq!(exported["data"]["scope"], "navigation");
    let mut exported_document = exported["data"].clone();
    exported_document["mode"] = json!("replace");
    let result = f
        .post("/api/panel/navigation/import", &f.admin, exported_document)
        .await;
    assert_ok(&result);
    assert_eq!(result["data"]["groups"], 1);
    assert_eq!(result["data"]["items"], 1);
    let result = f.post("/api/panel/navigation", &f.admin, json!({})).await;
    assert_ok(&result);
    assert_eq!(result["data"]["list"][0]["title"], "Existing");
    assert_eq!(
        result["data"]["list"][0]["items"][0]["title"],
        "Existing item"
    );
    assert_ne!(result["data"]["list"][0]["id"], initial_group);

    assert_eq!(f.live_count("item_icon_group", f.other_id).await, 1);
    assert_eq!(f.live_count("item_icon", f.other_id).await, 1);
    let result = f.post("/api/panel/navigation", &f.other, json!({})).await;
    assert_eq!(result["data"]["list"][0]["title"], "Private");
    assert_eq!(
        result["data"]["list"][0]["items"][0]["title"],
        "Private item"
    );
}

#[tokio::test]
async fn optimistic_configuration_writes_reject_stale_missing_and_concurrent_revisions() {
    let f = Fixture::new().await;
    let path = "/api/panel/userConfig/set";
    let missing = request(
        &f.router,
        path,
        json!({"panel":{"logoText":"must not save"}}),
    )
    .await;
    assert_eq!(missing["code"], 1409);
    assert_eq!(missing["currentRevision"], 0);
    let (first, second) = tokio::join!(
        request(
            &f.router,
            path,
            json!({"revision":0,"panel":{"logoText":"First"}})
        ),
        request(
            &f.router,
            path,
            json!({"revision":0,"panel":{"logoText":"Second"}})
        ),
    );
    assert!(
        (first["code"] == 0 && second["code"] == 1409)
            || (first["code"] == 1409 && second["code"] == 0),
        "{first}, {second}"
    );
    let saved = request(&f.router, "/api/panel/userConfig/get", json!({})).await;
    assert_eq!(saved["revision"], 1);
    assert_eq!(saved["data"]["revision"], 1);
    let partial = request(
        &f.router,
        path,
        json!({"revision":1,"searchEngine":{"provider":"Bing"}}),
    )
    .await;
    assert_ok(&partial);
    assert_eq!(partial["revision"], 2);
    let saved = request(&f.router, "/api/panel/userConfig/get", json!({})).await;
    assert!(
        !saved["data"]["panel"]["logoText"]
            .as_str()
            .unwrap()
            .is_empty()
    );
    assert_eq!(saved["data"]["searchEngine"]["provider"], "Bing");

    let path = "/api/system/moduleConfig/save";
    let missing = request(&f.router, path, json!({"name":"clock","value":{}})).await;
    assert_eq!(missing["code"], 1409);
    let (first, second) = tokio::join!(
        request(
            &f.router,
            path,
            json!({"revision":0,"name":"clock","value":{"label":"First"}})
        ),
        request(
            &f.router,
            path,
            json!({"revision":0,"name":"clock","value":{"label":"Second"}})
        ),
    );
    assert!(
        (first["code"] == 0 && second["code"] == 1409)
            || (first["code"] == 1409 && second["code"] == 0),
        "{first}, {second}"
    );
    let saved = request(
        &f.router,
        "/api/system/moduleConfig/getByName",
        json!({"name":"clock"}),
    )
    .await;
    assert_eq!(saved["revision"], 1);
    assert!(
        saved["data"].get("revision").is_none(),
        "Module data retains its original shape"
    );
    let clear = request(
        &f.router,
        "/api/panel/userConfig/set",
        json!({"revision":2,"panel":null}),
    )
    .await;
    assert_ok(&clear);
    let saved = request(&f.router, "/api/panel/userConfig/get", json!({})).await;
    assert!(saved["data"]["panel"].is_null());
    assert_eq!(saved["data"]["searchEngine"]["provider"], "Bing");
}

#[tokio::test]
async fn reviewed_imports_preserve_fields_and_merge_idempotently() {
    let f = Fixture::new().await;
    let mut document = json!({"appName":"Sun-Panel-Navigation","schemaVersion":1,"groups":[{
        "title":"Storage","icon":"mdi:nas","description":"Local services","sort":0,
        "items":[{"title":"NAS","url":"http://192.168.1.5:5000","description":"Files","openMethod":2,"sort":0,"icon":{"itemType":1,"text":"NAS"}}]
    }]});
    for round in 0..2 {
        let preview = request(
            &f.router,
            "/api/panel/navigation/import/preview",
            document.clone(),
        )
        .await;
        assert_ok(&preview);
        assert_eq!(preview["data"]["mode"], "merge");
        assert_eq!(preview["data"]["newItems"], if round == 0 { 1 } else { 0 });
        assert_eq!(
            preview["data"]["duplicateItems"],
            if round == 0 { 0 } else { 1 }
        );
        assert!(preview["data"]["conflicts"].as_array().unwrap().is_empty());
        document["revision"] = preview["data"]["revision"].clone();
        assert_ok(&request(&f.router, "/api/panel/navigation/import", document.clone()).await);
        assert_eq!(f.live_count("item_icon_group", f.admin_id).await, 1);
        assert_eq!(f.live_count("item_icon", f.admin_id).await, 1);
    }
    let export = request(&f.router, "/api/panel/navigation/export", json!({})).await;
    let group = &export["data"]["groups"][0];
    assert_eq!(group["icon"], "mdi:nas");
    assert_eq!(group["description"], "Local services");
    assert_eq!(group["sort"], 0);
    assert_eq!(group["items"][0]["sort"], 0);
    assert_eq!(group["items"][0]["url"], "http://192.168.1.5:5000");
    assert!(group["items"][0].get("lanUrl").is_none());
    let legacy_column: String =
        sqlx::query_scalar("SELECT lan_url FROM item_icon WHERE user_id=? AND deleted_at IS NULL")
            .bind(f.admin_id)
            .fetch_one(&f.state.db)
            .await
            .unwrap();
    assert!(legacy_column.is_empty());

    document["groups"][0]["items"][0]["description"] = json!("Updated description");
    let preview = request(
        &f.router,
        "/api/panel/navigation/import/preview",
        document.clone(),
    )
    .await;
    assert_eq!(preview["data"]["updatedItems"], 1);
    document["revision"] = preview["data"]["revision"].clone();
    assert_ok(&request(&f.router, "/api/panel/navigation/import", document.clone()).await);
    assert_eq!(f.live_count("item_icon", f.admin_id).await, 1);
    document["mode"] = json!("append");
    let preview = request(
        &f.router,
        "/api/panel/navigation/import/preview",
        document.clone(),
    )
    .await;
    document["revision"] = preview["data"]["revision"].clone();
    assert_ok(&request(&f.router, "/api/panel/navigation/import", document).await);
    assert_eq!(f.live_count("item_icon_group", f.admin_id).await, 2);
}

#[tokio::test]
async fn import_previews_report_ambiguity_and_reject_changes_after_review() {
    let f = Fixture::new().await;
    let mut document = json!({"appName":"Sun-Panel-Navigation","schemaVersion":1,"mode":"replace","groups":[{
        "title":"Imported","items":[{"title":"App","url":"http://192.168.1.2","openMethod":2}]
    }]});
    let missing = request(&f.router, "/api/panel/navigation/import", document.clone()).await;
    assert_eq!(missing["code"], 1409);
    let preview = request(
        &f.router,
        "/api/panel/navigation/import/preview",
        document.clone(),
    )
    .await;
    document["revision"] = preview["data"]["revision"].clone();
    f.group(&f.admin, "Concurrent edit").await;
    let stale = request(&f.router, "/api/panel/navigation/import", document.clone()).await;
    assert_eq!(stale["code"], 1409);
    assert_eq!(f.live_count("item_icon_group", f.admin_id).await, 1);
    assert_eq!(f.live_count("item_icon", f.admin_id).await, 0);
    f.group(&f.admin, "Ambiguous").await;
    f.group(&f.admin, "Ambiguous").await;
    document["mode"] = json!("merge");
    document["groups"][0]["title"] = json!("Ambiguous");
    let preview = request(
        &f.router,
        "/api/panel/navigation/import/preview",
        document.clone(),
    )
    .await;
    assert!(!preview["data"]["conflicts"].as_array().unwrap().is_empty());
    document["revision"] = preview["data"]["revision"].clone();
    let rejected = request(&f.router, "/api/panel/navigation/import", document).await;
    assert_eq!(rejected["code"], 1400);
    assert_eq!(f.live_count("item_icon_group", f.admin_id).await, 3);
}

#[tokio::test]
async fn single_address_migration_archives_original_values_and_retains_zero_sort() {
    let pool = sqlx::sqlite::SqlitePoolOptions::new()
        .max_connections(1)
        .connect("sqlite::memory:")
        .await
        .unwrap();
    for migration in [
        include_str!("../migrations/0001_legacy_compatible.sql"),
        include_str!("../migrations/0002_config_uniqueness.sql"),
        include_str!("../migrations/0003_read_indexes.sql"),
        include_str!("../migrations/0004_single_user_workspace.sql"),
    ] {
        sqlx::raw_sql(migration).execute(&pool).await.unwrap();
    }
    sqlx::query("INSERT INTO item_icon(id,title,url,lan_url,sort,item_icon_group_id,user_id) VALUES(7,'NAS','https://public.example.com','http://192.168.1.7:5000',0,1,5),(8,'Router','http://192.168.1.1','',0,1,5)")
        .execute(&pool).await.unwrap();
    sqlx::raw_sql(include_str!(
        "../migrations/0005_single_address_and_revisions.sql"
    ))
    .execute(&pool)
    .await
    .unwrap();
    let chosen: (String, String, i64) =
        sqlx::query_as("SELECT url,lan_url,sort FROM item_icon WHERE id=7")
            .fetch_one(&pool)
            .await
            .unwrap();
    assert_eq!(chosen, ("http://192.168.1.7:5000".into(), "".into(), 0));
    let archived: (String, String) = sqlx::query_as(
        "SELECT previous_url,previous_lan_url FROM rust_legacy_item_addresses WHERE item_id=7",
    )
    .fetch_one(&pool)
    .await
    .unwrap();
    assert_eq!(
        archived,
        (
            "https://public.example.com".into(),
            "http://192.168.1.7:5000".into()
        )
    );
    let retained: String = sqlx::query_scalar("SELECT url FROM item_icon WHERE id=8")
        .fetch_one(&pool)
        .await
        .unwrap();
    assert_eq!(retained, "http://192.168.1.1");
}

#[tokio::test]
async fn individual_icon_styles_round_trip_and_participate_in_import_deduplication() {
    let f = Fixture::new().await;
    let group = f.group(&f.admin, "Styled apps").await;
    let item = f.item(&f.admin, group, "NAS").await;
    let before = request(&f.router, "/api/panel/navigation/export", json!({})).await;
    let icon = &before["data"]["groups"][0]["items"][0]["icon"];
    for field in ["style", "textColor", "hideDescription", "hideTitle"] {
        assert!(
            icon.get(field).is_none(),
            "missing overrides must remain absent"
        );
    }

    let icon = json!({"itemType":2,"src":"uploads/nas.png","text":"NAS","backgroundColor":"#112233","style":0,"textColor":"#fedcba","hideDescription":false,"hideTitle":true});
    let mut edit = json!({"id":item,"title":"NAS","url":"https://example.com","openMethod":2,"itemIconGroupId":group,"sort":7,"icon":icon});
    assert_ok(&request(&f.router, "/api/panel/itemIcon/edit", edit.clone()).await);
    let exported = request(&f.router, "/api/panel/navigation/export", json!({})).await;
    assert_eq!(exported["data"]["groups"][0]["items"][0]["icon"], icon);

    let mut document = exported["data"].clone();
    let preview = request(
        &f.router,
        "/api/panel/navigation/import/preview",
        document.clone(),
    )
    .await;
    assert_ok(&preview);
    assert_eq!(preview["data"]["duplicateItems"], 1);
    document["groups"][0]["items"][0]["icon"]["style"] = json!(1);
    let preview = request(
        &f.router,
        "/api/panel/navigation/import/preview",
        document.clone(),
    )
    .await;
    assert_ok(&preview);
    assert_eq!(preview["data"]["updatedItems"], 1);
    assert_eq!(preview["data"]["duplicateItems"], 0);
    document["revision"] = preview["data"]["revision"].clone();
    assert_ok(&request(&f.router, "/api/panel/navigation/import", document.clone()).await);
    let exported = request(&f.router, "/api/panel/navigation/export", json!({})).await;
    assert_eq!(
        exported["data"]["groups"][0]["items"][0]["icon"],
        document["groups"][0]["items"][0]["icon"]
    );
    let preview = request(
        &f.router,
        "/api/panel/navigation/import/preview",
        document.clone(),
    )
    .await;
    assert_eq!(preview["data"]["duplicateItems"], 1);

    for style in [-1, 2] {
        edit["icon"]["style"] = json!(style);
        assert_eq!(
            request(&f.router, "/api/panel/itemIcon/edit", edit.clone()).await["code"],
            1400
        );
        document["groups"][0]["items"][0]["icon"]["style"] = json!(style);
        assert_eq!(
            request(
                &f.router,
                "/api/panel/navigation/import/preview",
                document.clone()
            )
            .await["code"],
            1400
        );
    }
    edit["icon"]["style"] = json!(0);
    edit["icon"]["textColor"] = json!("x".repeat(101));
    assert_eq!(
        request(&f.router, "/api/panel/itemIcon/edit", edit).await["code"],
        1400
    );
    document["groups"][0]["items"][0]["icon"]["style"] = json!(1);
    document["groups"][0]["items"][0]["icon"]["textColor"] = json!("x".repeat(101));
    assert_eq!(
        request(&f.router, "/api/panel/navigation/import/preview", document).await["code"],
        1400
    );
}

#[tokio::test]
async fn individual_style_migration_preserves_content_trash_monitor_cards_and_revisions() {
    let pool = sqlx::sqlite::SqlitePoolOptions::new()
        .max_connections(1)
        .connect("sqlite::memory:")
        .await
        .unwrap();
    for migration in [
        include_str!("../migrations/0001_legacy_compatible.sql"),
        include_str!("../migrations/0002_config_uniqueness.sql"),
        include_str!("../migrations/0003_read_indexes.sql"),
        include_str!("../migrations/0004_single_user_workspace.sql"),
        include_str!("../migrations/0005_single_address_and_revisions.sql"),
        include_str!("../migrations/0006_navigation_trash.sql"),
    ] {
        sqlx::raw_sql(migration).execute(&pool).await.unwrap();
    }
    sqlx::query("INSERT INTO user_config(user_id,panel_json,search_engine_json,revision) VALUES(11,?,'{\"provider\":\"keep\"}',6)")
        .bind(json!({"iconStyle":0,"iconTextColor":"#abcdef","iconTextInfoHideDescription":true,"iconTextIconHideTitle":false,"backgroundImageSrc":"uploads/wallpaper.png","logoText":"Keep"}).to_string())
        .execute(&pool).await.unwrap();
    let original = json!({"itemType":2,"src":"uploads/nas.png","text":"NAS","backgroundColor":"#123456","extra":"keep"});
    let overrides = json!({"itemType":1,"text":"Trash","style":1,"textColor":"#111111","hideDescription":false,"hideTitle":true});
    for (id, owner, icon, deleted) in [
        (1, 11, original.clone(), None),
        (2, 11, overrides.clone(), Some("removed")),
        (3, 22, Value::Null, None),
        (4, 11, json!({}), None),
    ] {
        sqlx::query("INSERT INTO item_icon(id,user_id,item_icon_group_id,title,url,icon_json,deleted_at,trash_batch) VALUES(?,?,8,'Keep','http://192.168.1.2:5000',?,?, 'keep-batch')")
            .bind(id).bind(owner).bind(icon.to_string()).bind(deleted).execute(&pool).await.unwrap();
    }
    // Unexpected legacy payloads must not be rewritten into invented icon data.
    sqlx::query("INSERT INTO item_icon(id,user_id,item_icon_group_id,icon_json) VALUES(5,11,8,'broken-json')")
        .execute(&pool).await.unwrap();
    let monitors = json!({"list":[
        {"monitorType":"disk","description":"Keep disk","extendParam":{"path":"/media","color":"#987654","progressColor":"#fff"}},
        {"monitorType":"cpu","extendParam":{"style":1,"color":"#eee"}},
        {"monitorType":"memory"},null,true,["keep"]
    ],"extra":"keep"});
    sqlx::query("INSERT INTO module_config(id,user_id,name,value_json,revision) VALUES(1,11,'systemMonitor',?,9),(2,22,'systemMonitor','{\"list\":[{\"monitorType\":\"cpu\"}]}',2),(3,11,'clock','{\"keep\":true}',4)")
        .bind(monitors.to_string()).execute(&pool).await.unwrap();
    let navigation_revision: i64 =
        sqlx::query_scalar("SELECT revision FROM rust_navigation_revision WHERE user_id=11")
            .fetch_one(&pool)
            .await
            .unwrap();
    sqlx::raw_sql(include_str!(
        "../migrations/0007_individual_icon_styles.sql"
    ))
    .execute(&pool)
    .await
    .unwrap();

    let icon_json: String = sqlx::query_scalar("SELECT icon_json FROM item_icon WHERE id=1")
        .fetch_one(&pool)
        .await
        .unwrap();
    let mut expected = original;
    expected["style"] = json!(0);
    expected["textColor"] = json!("#abcdef");
    expected["hideDescription"] = json!(true);
    expected["hideTitle"] = json!(false);
    assert_eq!(serde_json::from_str::<Value>(&icon_json).unwrap(), expected);
    let trash: (String, String, String, String) =
        sqlx::query_as("SELECT icon_json,url,deleted_at,trash_batch FROM item_icon WHERE id=2")
            .fetch_one(&pool)
            .await
            .unwrap();
    assert_eq!(serde_json::from_str::<Value>(&trash.0).unwrap(), overrides);
    assert_eq!(
        (&trash.1, &trash.2, &trash.3),
        (
            &"http://192.168.1.2:5000".to_owned(),
            &"removed".to_owned(),
            &"keep-batch".to_owned()
        )
    );
    let defaults: String = sqlx::query_scalar("SELECT icon_json FROM item_icon WHERE id=3")
        .fetch_one(&pool)
        .await
        .unwrap();
    assert_eq!(
        serde_json::from_str::<Value>(&defaults).unwrap(),
        json!({"style":1,"textColor":"#ffffff","hideDescription":false,"hideTitle":false})
    );
    let empty_icon: String = sqlx::query_scalar("SELECT icon_json FROM item_icon WHERE id=4")
        .fetch_one(&pool)
        .await
        .unwrap();
    assert_eq!(
        serde_json::from_str::<Value>(&empty_icon).unwrap(),
        json!({"style":0,"textColor":"#abcdef","hideDescription":true,"hideTitle":false})
    );
    let malformed: String = sqlx::query_scalar("SELECT icon_json FROM item_icon WHERE id=5")
        .fetch_one(&pool)
        .await
        .unwrap();
    assert_eq!(malformed, "broken-json");
    let panel: (String, String, i64) = sqlx::query_as(
        "SELECT panel_json,search_engine_json,revision FROM user_config WHERE user_id=11",
    )
    .fetch_one(&pool)
    .await
    .unwrap();
    assert_eq!(
        serde_json::from_str::<Value>(&panel.0).unwrap(),
        json!({"backgroundImageSrc":"uploads/wallpaper.png","logoText":"Keep"})
    );
    assert_eq!(
        serde_json::from_str::<Value>(&panel.1).unwrap(),
        json!({"provider":"keep"})
    );
    assert_eq!(panel.2, 7);
    let module: (String, i64) =
        sqlx::query_as("SELECT value_json,revision FROM module_config WHERE id=1")
            .fetch_one(&pool)
            .await
            .unwrap();
    let mut expected = monitors;
    expected["list"][0]["extendParam"]["style"] = json!(0);
    expected["list"][2]["extendParam"] = json!({"style":0});
    assert_eq!(serde_json::from_str::<Value>(&module.0).unwrap(), expected);
    assert_eq!(module.1, 10);
    let defaults: (String, i64) =
        sqlx::query_as("SELECT value_json,revision FROM module_config WHERE id=2")
            .fetch_one(&pool)
            .await
            .unwrap();
    assert_eq!(
        serde_json::from_str::<Value>(&defaults.0).unwrap(),
        json!({"list":[{"monitorType":"cpu","extendParam":{"style":1}}]})
    );
    assert_eq!(defaults.1, 3);
    let unchanged: (String, i64) =
        sqlx::query_as("SELECT value_json,revision FROM module_config WHERE id=3")
            .fetch_one(&pool)
            .await
            .unwrap();
    assert_eq!(unchanged, ("{\"keep\":true}".into(), 4));
    let updated_revision: i64 =
        sqlx::query_scalar("SELECT revision FROM rust_navigation_revision WHERE user_id=11")
            .fetch_one(&pool)
            .await
            .unwrap();
    assert!(updated_revision > navigation_revision);
}

#[tokio::test]
async fn deleted_managed_images_cannot_be_reintroduced_but_unmanaged_paths_remain_importable() {
    let f = Fixture::new().await;
    let group = f.group(&f.admin, "Apps").await;
    sqlx::query("INSERT INTO file(src,user_id,file_name,deleted_at) VALUES('./uploads/deleted.png',?,'deleted.png',CURRENT_TIMESTAMP)")
        .bind(f.other_id).execute(&f.state.db).await.unwrap();
    let item = json!({"title":"NAS","url":"http://192.168.1.7","openMethod":2,"itemIconGroupId":group,"icon":{"itemType":2,"src":"uploads/deleted.png"}});
    let rejected = request(&f.router, "/api/panel/itemIcon/edit", item.clone()).await;
    assert_eq!(rejected["code"], 1311);
    assert_eq!(f.live_count("item_icon", f.admin_id).await, 0);
    let rejected = request(
        &f.router,
        "/api/panel/userConfig/set",
        json!({"revision":0,"panel":{"backgroundImageSrc":"/uploads/deleted.png?v=1"}}),
    )
    .await;
    assert_eq!(rejected["code"], 1311);
    let mut document = json!({"appName":"Sun-Panel-Navigation","schemaVersion":1,"mode":"replace","groups":[{"title":"Imported","items":[item]}]});
    let preview = request(
        &f.router,
        "/api/panel/navigation/import/preview",
        document.clone(),
    )
    .await;
    document["revision"] = preview["data"]["revision"].clone();
    let rejected = request(&f.router, "/api/panel/navigation/import", document.clone()).await;
    assert_eq!(rejected["code"], 1311);
    assert_eq!(f.live_count("item_icon_group", f.admin_id).await, 1);
    document["groups"][0]["items"][0]["icon"]["src"] = json!("uploads/unmanaged.png");
    assert_ok(&request(&f.router, "/api/panel/navigation/import", document).await);
    assert_eq!(f.live_count("item_icon_group", f.admin_id).await, 1);
    assert_eq!(f.live_count("item_icon", f.admin_id).await, 1);
}

#[tokio::test]
async fn empty_navigation_snapshots_round_trip_and_replace_clears_only_the_local_workspace() {
    let f = Fixture::new().await;
    let exported = request(&f.router, "/api/panel/navigation/export", json!({})).await;
    assert_ok(&exported);
    assert_eq!(exported["data"]["groups"], json!([]));
    let mut empty_document = exported["data"].clone();
    for mode in ["merge", "append"] {
        empty_document["mode"] = json!(mode);
        let preview = request(
            &f.router,
            "/api/panel/navigation/import/preview",
            empty_document.clone(),
        )
        .await;
        assert_ok(&preview);
        assert_eq!(preview["data"]["groups"], 0);
        assert_eq!(preview["data"]["items"], 0);
        empty_document["revision"] = preview["data"]["revision"].clone();
        assert_ok(
            &request(
                &f.router,
                "/api/panel/navigation/import",
                empty_document.clone(),
            )
            .await,
        );
        let exported = request(&f.router, "/api/panel/navigation/export", json!({})).await;
        assert_eq!(exported["data"]["groups"], json!([]));
    }
    let group = f.group(&f.admin, "Existing").await;
    f.item(&f.admin, group, "Existing item").await;
    let other_group = f.group(&f.other, "Archived").await;
    f.item(&f.other, other_group, "Archived item").await;
    for mode in ["merge", "append"] {
        empty_document["mode"] = json!(mode);
        let preview = request(
            &f.router,
            "/api/panel/navigation/import/preview",
            empty_document.clone(),
        )
        .await;
        empty_document["revision"] = preview["data"]["revision"].clone();
        assert_ok(
            &request(
                &f.router,
                "/api/panel/navigation/import",
                empty_document.clone(),
            )
            .await,
        );
        assert_eq!(f.live_count("item_icon_group", f.admin_id).await, 1);
        assert_eq!(f.live_count("item_icon", f.admin_id).await, 1);
    }
    empty_document["mode"] = json!("replace");
    let preview = request(
        &f.router,
        "/api/panel/navigation/import/preview",
        empty_document.clone(),
    )
    .await;
    assert_eq!(preview["data"]["removedGroups"], 1);
    assert_eq!(preview["data"]["removedItems"], 1);
    empty_document["revision"] = preview["data"]["revision"].clone();
    assert_ok(&request(&f.router, "/api/panel/navigation/import", empty_document).await);
    assert_eq!(f.live_count("item_icon_group", f.admin_id).await, 0);
    assert_eq!(f.live_count("item_icon", f.admin_id).await, 0);
    assert_eq!(f.live_count("item_icon_group", f.other_id).await, 1);
    assert_eq!(f.live_count("item_icon", f.other_id).await, 1);
    let navigation = request(&f.router, "/api/panel/navigation", json!({})).await;
    assert_eq!(navigation["data"]["list"], json!([]));
    let exported = request(&f.router, "/api/panel/navigation/export", json!({})).await;
    assert_eq!(
        exported["data"]["groups"],
        json!([]),
        "reads must not recreate groups after an explicit reset"
    );
    f.group(&f.admin, "New group after reset").await;
    assert_eq!(f.live_count("item_icon_group", f.admin_id).await, 1);
}
