//! Recycle bin HTTP regressions use isolated temporary SQLite databases.
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
}

async fn post(router: &Router, path: &str, data: Value) -> Value {
    let response = router
        .clone()
        .oneshot(
            Request::post(path)
                .header("content-type", "application/json")
                .body(Body::from(data.to_string()))
                .unwrap(),
        )
        .await
        .unwrap();
    assert!(
        response.status().is_success(),
        "{path}: {}",
        response.status()
    );
    serde_json::from_slice(&response.into_body().collect().await.unwrap().to_bytes()).unwrap()
}

fn assert_ok(value: &Value) {
    assert_eq!(value["code"], 0, "{value}");
}

impl Fixture {
    async fn new() -> Self {
        let directory = TempDir::new().unwrap();
        let root = directory.path();
        let state = initialize(Config {
            bind: "127.0.0.1:0".into(),
            database_path: root.join("database.db"),
            uploads_dir: root.join("uploads"),
            web_dir: root.join("web"),
            max_upload_bytes: 1024 * 1024,
            monitor_interval_secs: 5,
            allow_private_favicon: false,
            worker_threads: 1,
            database_connections: 2,
        })
        .await
        .unwrap();
        let router = app(state.clone());
        Self {
            _directory: directory,
            state,
            router,
        }
    }

    async fn call(&self, path: &str, data: Value) -> Value {
        post(&self.router, path, data).await
    }

    async fn group(&self, title: &str) -> i64 {
        let result = self
            .call(
                "/api/panel/itemIconGroup/edit",
                json!({"title":title,"sort":3}),
            )
            .await;
        assert_ok(&result);
        result["data"]["id"].as_i64().unwrap()
    }

    async fn item(&self, group: i64, title: &str) -> i64 {
        let result = self.call("/api/panel/itemIcon/edit", json!({"title":title,"url":"https://example.com","openMethod":2,"itemIconGroupId":group,"sort":7,"icon":{"itemType":1,"text":"A"}})).await;
        assert_ok(&result);
        result["data"]["id"].as_i64().unwrap()
    }

    async fn trash(&self) -> Value {
        let result = self
            .call("/api/panel/navigation/trash/list", json!({}))
            .await;
        assert_ok(&result);
        assert_eq!(result["revision"], result["data"]["revision"]);
        result
    }

    async fn navigation(&self) -> Value {
        let result = self.call("/api/panel/navigation", json!({})).await;
        assert_ok(&result);
        result
    }
}

#[tokio::test]
async fn trash_items_round_trip_with_original_ids_contents_sort_and_revision() {
    let f = Fixture::new().await;
    let group = f.group("Apps").await;
    let item = f.item(group, "NAS").await;
    let before = f.navigation().await;
    let deleted = f
        .call(
            "/api/panel/itemIcon/deletes",
            json!({"ids":[item],"revision":before["revision"]}),
        )
        .await;
    assert_ok(&deleted);
    assert_eq!(deleted["revision"], deleted["data"]["revision"]);
    assert!(deleted["revision"].as_i64().unwrap() > before["revision"].as_i64().unwrap());
    assert_eq!(deleted["data"]["trash"][0]["kind"], "item");
    assert!(
        f.navigation().await["data"]["list"][0]["items"]
            .as_array()
            .unwrap()
            .is_empty()
    );
    let trash = f.trash().await;
    assert_eq!(trash["data"]["count"], 1);
    let entry = &trash["data"]["list"][0];
    assert_eq!(entry["id"], item);
    assert_eq!(entry["title"], "NAS");
    assert_eq!(entry["groupId"], group);
    assert_eq!(entry["groupTitle"], "Apps");
    assert_eq!(entry["requiresTargetGroup"], false);
    assert!(entry["deletedAt"].is_string());
    assert_eq!(
        entry["deletionId"],
        deleted["data"]["trash"][0]["deletionId"]
    );
    let restored = f.call("/api/panel/navigation/trash/restore", json!({"kind":"item","id":item,"deletionId":entry["deletionId"],"revision":trash["revision"]})).await;
    assert_ok(&restored);
    assert_eq!(restored["revision"], restored["data"]["revision"]);
    let after = f.navigation().await;
    let old = &before["data"]["list"][0]["items"][0];
    let new = &after["data"]["list"][0]["items"][0];
    for key in [
        "id",
        "createTime",
        "icon",
        "title",
        "url",
        "openMethod",
        "sort",
        "itemIconGroupId",
    ] {
        assert_eq!(new[key], old[key], "{key}");
    }
    assert_eq!(f.trash().await["data"]["count"], 0);
    assert_eq!(
        f.call(
            "/api/panel/navigation/trash/restore",
            json!({"kind":"item","id":item})
        )
        .await["code"],
        1005
    );
    assert_eq!(
        f.call(
            "/api/panel/navigation/trash/purge",
            json!({"kind":"item","id":item})
        )
        .await["code"],
        1005
    );
    assert_eq!(f.navigation().await, after);
}

#[tokio::test]
async fn trash_group_restores_only_its_own_deletion_batch() {
    let f = Fixture::new().await;
    let group = f.group("Deleted group").await;
    f.group("Keep").await;
    let independent = f.item(group, "Deleted before group").await;
    let cascade = f.item(group, "Active with group").await;
    assert_ok(
        &f.call("/api/panel/itemIcon/deletes", json!({"ids":[independent]}))
            .await,
    );
    let deleted = f
        .call("/api/panel/itemIconGroup/deletes", json!({"ids":[group]}))
        .await;
    assert_ok(&deleted);
    let trash = f.trash().await;
    assert_eq!(
        trash["data"]["count"], 2,
        "cascade child is nested under its group"
    );
    let entries = trash["data"]["list"].as_array().unwrap();
    let group_entry = entries
        .iter()
        .find(|entry| entry["kind"] == "group")
        .unwrap();
    assert_eq!(group_entry["id"], group);
    assert_eq!(group_entry["itemCount"], 1);
    assert_eq!(
        group_entry["deletionId"],
        deleted["data"]["trash"][0]["deletionId"]
    );
    assert!(
        entries
            .iter()
            .any(|entry| entry["id"] == independent && entry["requiresTargetGroup"] == true)
    );
    assert_ok(
        &f.call(
            "/api/panel/navigation/trash/restore",
            json!({"kind":"group","id":group}),
        )
        .await,
    );
    let navigation = f.navigation().await;
    let restored_group = navigation["data"]["list"]
        .as_array()
        .unwrap()
        .iter()
        .find(|entry| entry["id"] == group)
        .unwrap();
    assert_eq!(restored_group["items"].as_array().unwrap().len(), 1);
    assert_eq!(restored_group["items"][0]["id"], cascade);
    let trash = f.trash().await;
    assert_eq!(trash["data"]["count"], 1);
    assert_eq!(trash["data"]["list"][0]["id"], independent);
    assert_eq!(trash["data"]["list"][0]["requiresTargetGroup"], false);
    assert_ok(
        &f.call(
            "/api/panel/navigation/trash/restore",
            json!({"kind":"item","id":independent}),
        )
        .await,
    );
}

#[tokio::test]
async fn trash_purge_group_keeps_independent_items_recoverable_into_a_valid_target() {
    let f = Fixture::new().await;
    let group = f.group("Delete").await;
    let target = f.group("Target").await;
    let independent = f.item(group, "Independent").await;
    let cascade = f.item(group, "Cascade").await;
    for id in [independent, cascade] {
        sqlx::query("INSERT INTO rust_legacy_item_addresses(item_id,user_id,previous_url,previous_lan_url) VALUES(?,?,'old','old')")
            .bind(id).bind(f.state.owner_id).execute(&f.state.db).await.unwrap();
    }
    assert_ok(
        &f.call("/api/panel/itemIcon/deletes", json!({"ids":[independent]}))
            .await,
    );
    assert_ok(
        &f.call("/api/panel/itemIconGroup/deletes", json!({"ids":[group]}))
            .await,
    );
    assert_ok(
        &f.call(
            "/api/panel/navigation/trash/purge",
            json!({"kind":"group","id":group}),
        )
        .await,
    );
    assert_eq!(
        sqlx::query_scalar::<_, i64>("SELECT count(*) FROM item_icon WHERE id=?")
            .bind(cascade)
            .fetch_one(&f.state.db)
            .await
            .unwrap(),
        0
    );
    assert_eq!(
        sqlx::query_scalar::<_, i64>(
            "SELECT count(*) FROM rust_legacy_item_addresses WHERE item_id=?"
        )
        .bind(cascade)
        .fetch_one(&f.state.db)
        .await
        .unwrap(),
        0
    );
    let trash = f.trash().await;
    assert_eq!(trash["data"]["count"], 1);
    assert_eq!(trash["data"]["list"][0]["id"], independent);
    assert_eq!(trash["data"]["list"][0]["requiresTargetGroup"], true);
    assert_eq!(
        f.call(
            "/api/panel/navigation/trash/restore",
            json!({"kind":"item","id":independent})
        )
        .await["code"],
        1005
    );
    assert_eq!(f.trash().await, trash);
    assert_ok(
        &f.call(
            "/api/panel/navigation/trash/restore",
            json!({"kind":"item","id":independent,"targetGroupId":target}),
        )
        .await,
    );
    assert_eq!(
        f.navigation().await["data"]["list"][0]["items"][0]["id"],
        independent
    );
    assert_ok(
        &f.call("/api/panel/itemIcon/deletes", json!({"ids":[independent]}))
            .await,
    );
    assert_ok(
        &f.call(
            "/api/panel/navigation/trash/purge",
            json!({"kind":"item","id":independent}),
        )
        .await,
    );
    assert_eq!(f.trash().await["data"]["count"], 0);
    assert_eq!(
        sqlx::query_scalar::<_, i64>(
            "SELECT count(*) FROM rust_legacy_item_addresses WHERE item_id=?"
        )
        .bind(independent)
        .fetch_one(&f.state.db)
        .await
        .unwrap(),
        0
    );
}

#[tokio::test]
async fn trash_is_owner_scoped_and_invalid_operations_never_change_either_workspace() {
    let f = Fixture::new().await;
    let group = f.group("Local").await;
    let item = f.item(group, "Local deleted").await;
    assert_ok(
        &f.call("/api/panel/itemIcon/deletes", json!({"ids":[item]}))
            .await,
    );
    let active = f.item(group, "Active").await;
    let foreign_id = sqlx::query("INSERT INTO user(username,password) VALUES('archived','hash')")
        .execute(&f.state.db)
        .await
        .unwrap()
        .last_insert_rowid();
    let foreign_router = app(AppState {
        owner_id: foreign_id,
        ..f.state.clone()
    });
    let foreign_group = post(
        &foreign_router,
        "/api/panel/itemIconGroup/edit",
        json!({"title":"Foreign"}),
    )
    .await["data"]["id"]
        .as_i64()
        .unwrap();
    let foreign_item = post(&foreign_router, "/api/panel/itemIcon/edit", json!({"title":"Foreign item","url":"https://example.com","openMethod":2,"itemIconGroupId":foreign_group})).await["data"]["id"].as_i64().unwrap();
    assert_ok(
        &post(
            &foreign_router,
            "/api/panel/itemIcon/deletes",
            json!({"ids":[foreign_item]}),
        )
        .await,
    );
    let local_trash = f.trash().await;
    let foreign_trash = post(
        &foreign_router,
        "/api/panel/navigation/trash/list",
        json!({}),
    )
    .await;
    assert_eq!(local_trash["data"]["count"], 1);
    assert_eq!(foreign_trash["data"]["count"], 1);
    for action in ["restore", "purge"] {
        let path = format!("/api/panel/navigation/trash/{action}");
        for body in [
            json!({"kind":"item","id":foreign_item}),
            json!({"kind":"group","id":foreign_group}),
            json!({"kind":"item","id":i64::MAX}),
        ] {
            assert_eq!(f.call(&path, body).await["code"], 1005);
        }
        for body in [
            json!({"kind":"unknown","id":item}),
            json!({"kind":"item","id":0}),
            json!({"kind":"item","id":item,"targetGroupId":0}),
        ] {
            assert_eq!(f.call(&path, body).await["code"], 1400);
        }
    }
    assert_eq!(
        f.call(
            "/api/panel/navigation/trash/restore",
            json!({"kind":"item","id":item,"targetGroupId":foreign_group})
        )
        .await["code"],
        1005
    );
    // Batch ownership is checked atomically; the local item remains active.
    let before = f.navigation().await;
    assert_eq!(
        f.call(
            "/api/panel/itemIcon/deletes",
            json!({"ids":[active,foreign_item]})
        )
        .await["code"],
        1005
    );
    assert_eq!(f.navigation().await, before);
    assert_eq!(f.trash().await, local_trash);
    assert_eq!(
        post(
            &foreign_router,
            "/api/panel/navigation/trash/list",
            json!({})
        )
        .await,
        foreign_trash
    );
}

#[tokio::test]
async fn trash_cascade_restore_and_purge_roll_back_all_rows_and_revision_on_failure() {
    let f = Fixture::new().await;
    let group = f.group("Delete").await;
    f.group("Keep").await;
    let child = f.item(group, "Child").await;
    assert_ok(
        &f.call("/api/panel/itemIconGroup/deletes", json!({"ids":[group]}))
            .await,
    );
    let trash = f.trash().await;
    let navigation = f.navigation().await;
    sqlx::query("CREATE TRIGGER fail_trash_restore BEFORE UPDATE ON item_icon WHEN NEW.deleted_at IS NULL BEGIN SELECT RAISE(ABORT,'injected restore failure'); END").execute(&f.state.db).await.unwrap();
    assert_eq!(
        f.call(
            "/api/panel/navigation/trash/restore",
            json!({"kind":"group","id":group})
        )
        .await["code"],
        1200
    );
    assert_eq!(f.trash().await, trash);
    assert_eq!(f.navigation().await, navigation);
    sqlx::query("DROP TRIGGER fail_trash_restore")
        .execute(&f.state.db)
        .await
        .unwrap();
    sqlx::query("CREATE TRIGGER fail_trash_purge BEFORE DELETE ON item_icon_group BEGIN SELECT RAISE(ABORT,'injected purge failure'); END").execute(&f.state.db).await.unwrap();
    assert_eq!(
        f.call(
            "/api/panel/navigation/trash/purge",
            json!({"kind":"group","id":group})
        )
        .await["code"],
        1200
    );
    assert_eq!(f.trash().await, trash);
    assert_eq!(f.navigation().await, navigation);
    assert_eq!(
        sqlx::query_scalar::<_, i64>("SELECT count(*) FROM item_icon WHERE id=?")
            .bind(child)
            .fetch_one(&f.state.db)
            .await
            .unwrap(),
        1
    );
}

#[tokio::test]
async fn trash_and_sort_revisions_and_deletion_ids_reject_stale_or_concurrent_undo() {
    let f = Fixture::new().await;
    let group = f.group("Apps").await;
    let item = f.item(group, "App").await;
    let old_revision = f.navigation().await["revision"].clone();
    let deletion = f
        .call(
            "/api/panel/itemIcon/deletes",
            json!({"ids":[item],"revision":old_revision}),
        )
        .await;
    assert_ok(&deletion);
    let deletion_id = deletion["data"]["trash"][0]["deletionId"].clone();
    for (path, body) in [
        (
            "/api/panel/navigation/trash/restore",
            json!({"kind":"item","id":item,"revision":old_revision}),
        ),
        (
            "/api/panel/navigation/trash/purge",
            json!({"kind":"item","id":item,"revision":old_revision}),
        ),
        (
            "/api/panel/itemIconGroup/saveSort",
            json!({"sortItems":[{"id":group,"sort":9}],"revision":old_revision}),
        ),
        (
            "/api/panel/itemIconGroup/deletes",
            json!({"ids":[group],"revision":old_revision}),
        ),
        (
            "/api/panel/itemIcon/deletes",
            json!({"ids":[item],"revision":old_revision}),
        ),
    ] {
        let rejected = f.call(path, body).await;
        assert_eq!(rejected["code"], 1409, "{rejected}");
        assert_eq!(rejected["currentRevision"], deletion["revision"]);
    }
    let body =
        json!({"kind":"item","id":item,"revision":deletion["revision"],"deletionId":deletion_id});
    let (one, two) = tokio::join!(
        f.call("/api/panel/navigation/trash/restore", body.clone()),
        f.call("/api/panel/navigation/trash/restore", body)
    );
    assert!(
        (one["code"] == 0 && two["code"] == 1409) || (two["code"] == 0 && one["code"] == 1409),
        "{one}; {two}"
    );
    assert_ok(
        &f.call("/api/panel/itemIcon/deletes", json!({"ids":[item]}))
            .await,
    );
    let before = f.trash().await;
    for action in ["restore", "purge"] {
        assert_eq!(
            f.call(
                &format!("/api/panel/navigation/trash/{action}"),
                json!({"kind":"item","id":item,"deletionId":deletion_id})
            )
            .await["code"],
            1409
        );
    }
    assert_eq!(f.trash().await, before);
    let sorted = f
        .call(
            "/api/panel/itemIconGroup/saveSort",
            json!({"sortItems":[{"id":group,"sort":9}],"revision":before["revision"]}),
        )
        .await;
    assert_ok(&sorted);
    assert_eq!(sorted["revision"], sorted["data"]["revision"]);
}

#[tokio::test]
async fn trash_batches_survive_database_close_and_application_reinitialization() {
    let f = Fixture::new().await;
    let group = f.group("Delete").await;
    f.group("Keep").await;
    let item = f.item(group, "Child").await;
    assert_ok(
        &f.call("/api/panel/itemIconGroup/deletes", json!({"ids":[group]}))
            .await,
    );
    let before = f.trash().await;
    let config = f.state.config.as_ref().clone();
    f.state.db.close().await;
    let reopened = initialize(config).await.unwrap();
    let router = app(reopened.clone());
    assert_eq!(
        post(&router, "/api/panel/navigation/trash/list", json!({})).await,
        before
    );
    assert_ok(
        &post(
            &router,
            "/api/panel/navigation/trash/restore",
            json!({"kind":"group","id":group,"deletionId":before["data"]["list"][0]["deletionId"]}),
        )
        .await,
    );
    let navigation = post(&router, "/api/panel/navigation", json!({})).await;
    assert!(
        navigation["data"]["list"]
            .as_array()
            .unwrap()
            .iter()
            .any(|entry| entry["id"] == group && entry["items"][0]["id"] == item)
    );
    reopened.db.close().await;
}

#[tokio::test]
async fn trash_legacy_deletions_are_individual_and_never_guess_a_group_cascade() {
    let f = Fixture::new().await;
    let group = f.group("Legacy").await;
    let item = f.item(group, "Legacy deleted child").await;
    sqlx::query("UPDATE item_icon_group SET deleted_at='2020-01-01' WHERE id=?")
        .bind(group)
        .execute(&f.state.db)
        .await
        .unwrap();
    sqlx::query("UPDATE item_icon SET deleted_at='2020-01-01' WHERE id=?")
        .bind(item)
        .execute(&f.state.db)
        .await
        .unwrap();
    let before = f.trash().await;
    assert_eq!(before["data"]["count"], 2);
    assert!(
        before["data"]["list"]
            .as_array()
            .unwrap()
            .iter()
            .any(|entry| entry["kind"] == "group" && entry["itemCount"] == 0)
    );
    assert_ok(
        &f.call(
            "/api/panel/navigation/trash/restore",
            json!({"kind":"group","id":group}),
        )
        .await,
    );
    assert!(
        f.navigation().await["data"]["list"][0]["items"]
            .as_array()
            .unwrap()
            .is_empty()
    );
    assert_eq!(f.trash().await["data"]["count"], 1);
    assert_ok(
        &f.call(
            "/api/panel/navigation/trash/restore",
            json!({"kind":"item","id":item}),
        )
        .await,
    );
}

#[tokio::test]
async fn trash_restoration_rejects_retired_managed_images_atomically() {
    let f = Fixture::new().await;
    let group = f.group("Delete").await;
    f.group("Keep").await;
    let child = f.item(group, "Child").await;
    sqlx::query("UPDATE item_icon SET icon_json=? WHERE id=?")
        .bind(json!({"itemType":2,"src":"/uploads/retired.png"}).to_string())
        .bind(child)
        .execute(&f.state.db)
        .await
        .unwrap();
    assert_ok(
        &f.call("/api/panel/itemIconGroup/deletes", json!({"ids":[group]}))
            .await,
    );
    sqlx::query("INSERT INTO file(src,user_id,deleted_at) VALUES('uploads/retired.png',?,CURRENT_TIMESTAMP)").bind(f.state.owner_id).execute(&f.state.db).await.unwrap();
    let before = f.trash().await;
    assert_eq!(
        f.call(
            "/api/panel/navigation/trash/restore",
            json!({"kind":"group","id":group})
        )
        .await["code"],
        1311
    );
    assert_eq!(f.trash().await, before);
    let target = f.navigation().await["data"]["list"][0]["id"].clone();
    assert_eq!(
        f.call(
            "/api/panel/navigation/trash/restore",
            json!({"kind":"item","id":child,"targetGroupId":target})
        )
        .await["code"],
        1311
    );
    assert_eq!(f.trash().await, before);
}

#[tokio::test]
async fn trash_restore_target_index_recovers_original_position_after_sibling_moves_and_rolls_back_sort_failure()
 {
    let f = Fixture::new().await;
    let group = f.group("Apps").await;
    let first = f.item(group, "First").await;
    let second = f.item(group, "Deleted second").await;
    let third = f.item(group, "Third").await;
    assert_ok(
        &f.call("/api/panel/itemIcon/deletes", json!({"ids":[second]}))
            .await,
    );
    for target_index in [1, 0] {
        let revision = f.navigation().await["revision"].clone();
        assert_ok(&f.call("/api/panel/navigation/move", json!({"itemId":first,"targetGroupId":group,"targetIndex":target_index,"revision":revision})).await);
    }
    let before = f.trash().await;
    let navigation = f.navigation().await;
    for index in [-1, 3, i64::MAX] {
        assert_eq!(
            f.call(
                "/api/panel/navigation/trash/restore",
                json!({"kind":"item","id":second,"targetIndex":index})
            )
            .await["code"],
            1400
        );
        assert_eq!(f.trash().await, before);
        assert_eq!(f.navigation().await, navigation);
    }
    sqlx::query(sqlx::AssertSqlSafe(format!("CREATE TRIGGER fail_restored_order BEFORE UPDATE ON item_icon WHEN OLD.id={third} AND NEW.sort=2 BEGIN SELECT RAISE(ABORT,'injected restored sort failure'); END")))
        .execute(&f.state.db).await.unwrap();
    assert_eq!(
        f.call(
            "/api/panel/navigation/trash/restore",
            json!({"kind":"item","id":second,"targetIndex":1})
        )
        .await["code"],
        1200
    );
    assert_eq!(f.trash().await, before);
    assert_eq!(f.navigation().await, navigation);
    sqlx::query("DROP TRIGGER fail_restored_order")
        .execute(&f.state.db)
        .await
        .unwrap();
    assert_ok(&f.call("/api/panel/navigation/trash/restore", json!({"kind":"item","id":second,"targetGroupId":group,"targetIndex":1,"revision":before["revision"]})).await);
    let after = f.navigation().await;
    let items = after["data"]["list"][0]["items"].as_array().unwrap();
    assert_eq!(
        items
            .iter()
            .map(|item| item["id"].as_i64().unwrap())
            .collect::<Vec<_>>(),
        [first, second, third]
    );
    assert_eq!(
        items
            .iter()
            .map(|item| item["sort"].as_i64().unwrap())
            .collect::<Vec<_>>(),
        [0, 1, 2]
    );
}
