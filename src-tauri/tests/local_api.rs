use pixoma_studio::{LocalApi, LocalRequest, Store};
use serde_json::json;
use std::path::PathBuf;

async fn request(
    api: &LocalApi,
    method: &str,
    path: &str,
    body: serde_json::Value,
) -> pixoma_studio::Result<serde_json::Value> {
    api.request(
        "account-a",
        LocalRequest {
            method: method.to_owned(),
            path: path.to_owned(),
            body: Some(body),
        },
    )
    .await
}

#[tokio::test]
async fn library_tree_preserves_groups_nested_categories_and_pagination()
-> Result<(), Box<dyn std::error::Error>> {
    let root = PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../.local-cache");
    std::fs::create_dir_all(&root)?;
    let directory = tempfile::Builder::new()
        .prefix("local-api-")
        .tempdir_in(root)?;
    let store = Store::open(&directory.path().join("studio.sqlite")).await?;
    let api = LocalApi::new(store.clone());
    let session = store.create_session("account-a", "session").await?;
    let first = store
        .create_text_asset("account-a", &session.id, "初稿.md", "正文", "first")
        .await?;
    let second = store
        .create_text_asset("account-a", &session.id, "终稿.md", "更新正文", "second")
        .await?;
    let parent = store.create_category("account-a", "", "文稿", "").await?;
    let parent_id = parent["id"].as_str().ok_or("分类标识缺失")?;
    let child = store
        .create_category("account-a", "", "终稿", parent_id)
        .await?;
    let page = store
        .library_assets("account-a", &Default::default())
        .await?;
    let items = page["items"].as_array().ok_or("资产列表缺失")?;
    let first_id = items
        .iter()
        .find(|item| item["asset_id"] == first.id)
        .ok_or("初稿缺失")?["id"]
        .as_str()
        .ok_or("项目资产标识缺失")?;
    let second_id = items
        .iter()
        .find(|item| item["asset_id"] == second.id)
        .ok_or("终稿缺失")?["id"]
        .as_str()
        .ok_or("项目资产标识缺失")?;
    store
        .patch_project_asset("account-a", first_id, &json!({ "category_id": parent_id }))
        .await?;
    store
        .patch_project_asset(
            "account-a",
            second_id,
            &json!({ "category_id": child["id"] }),
        )
        .await?;
    for mode in ["asset", "session", "category", "format", "rating", "tag"] {
        let path = format!("/api/v1/studio/library/tree?project_id=&mode={mode}&limit=1");
        let tree = request(&api, "GET", &path, json!(null)).await?;
        let node = &tree["nodes"][0];
        if mode == "asset" {
            assert_eq!(node["kind"], "asset");
            let cursor = tree["next_cursor"].as_str().ok_or("分页标识缺失")?;
            let next =
                request(&api, "GET", &format!("{path}&cursor={cursor}"), json!(null)).await?;
            assert_ne!(next["nodes"][0]["id"], node["id"]);
        } else {
            assert_eq!(node["kind"], "group");
            assert_eq!(node["count"], 2);
            let parent_id = node["id"].as_str().ok_or("分组标识缺失")?;
            let children = request(
                &api,
                "GET",
                &format!(
                    "/api/v1/studio/library/tree?project_id=&mode={mode}&parent_id={parent_id}"
                ),
                json!(null),
            )
            .await?;
            assert_eq!(children["nodes"].as_array().ok_or("分组内容缺失")?.len(), 2);
        }
    }
    let direct = request(
        &api,
        "GET",
        &format!("/api/v1/studio/library/assets?project_id=&category_id=direct:{parent_id}"),
        json!(null),
    )
    .await?;
    assert_eq!(direct["total"], 1);
    assert_eq!(direct["items"][0]["id"], first_id);
    let untagged = request(
        &api,
        "GET",
        "/api/v1/studio/library/assets?project_id=&tag_ids=untagged",
        json!(null),
    )
    .await?;
    assert_eq!(untagged["total"], 2);
    assert!(request(&api, "GET", "/api/v1/studio/library/assets?added_from=2026-10-19T00%3A00%3A00Z&added_to=2026-10-18T00%3A00%3A00Z", json!(null)).await.is_err());
    store.close().await;
    Ok(())
}

#[tokio::test]
async fn library_copy_versions_duplicates_and_batch_tags_use_owned_records()
-> Result<(), Box<dyn std::error::Error>> {
    let root = PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../.local-cache");
    std::fs::create_dir_all(&root)?;
    let directory = tempfile::Builder::new()
        .prefix("local-api-")
        .tempdir_in(root)?;
    let store = Store::open(&directory.path().join("studio.sqlite")).await?;
    let api = LocalApi::new(store.clone());
    let project = store.create_project("account-a", "发布").await?;
    let session = store.create_session("account-a", "session").await?;
    let asset = store
        .create_text_asset("account-a", &session.id, "内容.md", "正文", "asset")
        .await?;
    let source = store
        .library_assets("account-a", &Default::default())
        .await?;
    let source_id = source["items"][0]["id"]
        .as_str()
        .ok_or("项目资产标识缺失")?;
    let copy = request(
        &api,
        "POST",
        &format!("/api/v1/studio/library/assets/{source_id}/projects"),
        json!({ "project_id": project.id, "asset_version_id": asset.versions[0].id }),
    )
    .await?;
    let copy_id = copy["id"].as_str().ok_or("复制资产标识缺失")?;
    assert_eq!(copy["copy_source"]["project_asset_id"], source_id);
    let updated = store
        .update_text_asset("account-a", &asset.id, "更新正文", "update")
        .await?;
    let latest = updated
        .versions
        .iter()
        .find(|version| version.version == 2)
        .ok_or("新版本缺失")?;
    let copy = request(
        &api,
        "PATCH",
        &format!("/api/v1/studio/library/assets/{copy_id}/version"),
        json!({ "asset_version_id": latest.id }),
    )
    .await?;
    assert_eq!(copy["asset_version_id"], latest.id);
    assert_eq!(
        store.project_asset("account-a", source_id).await?["asset_version_id"],
        asset.versions[0].id
    );
    let duplicate = store
        .create_text_asset("account-a", &session.id, "重复.md", "更新正文", "duplicate")
        .await?;
    let duplicate_page = request(
        &api,
        "GET",
        &format!("/api/v1/studio/library/assets/{copy_id}/duplicates"),
        json!(null),
    )
    .await?;
    assert_eq!(duplicate_page["items"][0]["asset_id"], duplicate.id);
    assert!(
        request(
            &api,
            "PATCH",
            &format!("/api/v1/studio/library/assets/{copy_id}/version"),
            json!({ "asset_version_id": duplicate.versions[0].id })
        )
        .await
        .is_err()
    );
    let first = store.create_tag("account-a", "原标签").await?;
    let second = store.create_tag("account-a", "新增标签").await?;
    request(
        &api,
        "PUT",
        &format!("/api/v1/studio/library/assets/{copy_id}/tags"),
        json!({ "tag_ids": [first["id"]] }),
    )
    .await?;
    for action in ["tags", "tags_remove"] {
        let result = request(
            &api,
            "POST",
            "/api/v1/studio/library/assets/batch",
            json!({ "project_asset_ids": [copy_id], "action": action, "tag_ids": [second["id"]] }),
        )
        .await?;
        assert_eq!(result["results"][0]["success"], true);
        let tags = store.project_asset("account-a", copy_id).await?["tags"]
            .as_array()
            .ok_or("标签列表缺失")?
            .clone();
        assert_eq!(tags.len(), if action == "tags" { 2 } else { 1 });
    }
    assert_eq!(
        store.project_asset("account-a", copy_id).await?["tags"][0]["id"],
        first["id"]
    );
    let projects = request(&api, "GET", "/api/v1/studio/library/projects", json!(null)).await?;
    assert!(
        projects
            .as_array()
            .ok_or("项目列表缺失")?
            .iter()
            .any(|item| item["id"] == project.id && item["asset_count"] == 1)
    );
    let formats = request(
        &api,
        "GET",
        &format!("/api/v1/studio/library/formats?project_id={}", project.id),
        json!(null),
    )
    .await?;
    assert!(
        formats
            .as_array()
            .ok_or("格式列表缺失")?
            .contains(&json!("md"))
    );
    store.close().await;
    Ok(())
}

#[tokio::test]
async fn library_category_routes_reject_cycles_and_preserve_assets_when_deleted()
-> Result<(), Box<dyn std::error::Error>> {
    let root = PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../.local-cache");
    std::fs::create_dir_all(&root)?;
    let directory = tempfile::Builder::new()
        .prefix("local-api-")
        .tempdir_in(root)?;
    let store = Store::open(&directory.path().join("studio.sqlite")).await?;
    let api = LocalApi::new(store.clone());
    let project = store.create_project("account-a", "文档").await?;
    let session = store.create_session("account-a", "session").await?;
    store
        .move_session("account-a", &session.id, Some(&project.id))
        .await?;
    store
        .create_text_asset("account-a", &session.id, "正文.md", "正文", "asset")
        .await?;
    let parent = request(
        &api,
        "POST",
        "/api/v1/studio/library/categories",
        json!({ "project_id": project.id, "name": "文稿", "parent_id": "" }),
    )
    .await?;
    let child = request(
        &api,
        "POST",
        "/api/v1/studio/library/categories",
        json!({ "project_id": project.id, "name": "初稿", "parent_id": parent["id"] }),
    )
    .await?;
    let parent_id = parent["id"].as_str().ok_or("分类标识缺失")?;
    let child_id = child["id"].as_str().ok_or("分类标识缺失")?;
    assert!(
        request(
            &api,
            "PATCH",
            &format!("/api/v1/studio/library/categories/{parent_id}"),
            json!({ "parent_id": child_id })
        )
        .await
        .is_err()
    );
    let page = request(
        &api,
        "GET",
        &format!("/api/v1/studio/library/assets?project_id={}", project.id),
        json!(null),
    )
    .await?;
    let placement = page["items"][0]["id"].as_str().ok_or("项目资产标识缺失")?;
    request(
        &api,
        "PATCH",
        &format!("/api/v1/studio/library/assets/{placement}"),
        json!({ "category_id": child_id }),
    )
    .await?;
    let filtered = request(
        &api,
        "GET",
        &format!(
            "/api/v1/studio/library/assets?project_id={}&category_id={parent_id}",
            project.id
        ),
        json!(null),
    )
    .await?;
    assert_eq!(filtered["total"], 1);
    request(
        &api,
        "DELETE",
        &format!("/api/v1/studio/library/categories/{child_id}"),
        json!(null),
    )
    .await?;
    assert_eq!(
        store.project_asset("account-a", placement).await?["category_id"],
        parent_id
    );
    let categories = request(
        &api,
        "GET",
        &format!(
            "/api/v1/studio/library/categories?project_id={}",
            project.id
        ),
        json!(null),
    )
    .await?;
    assert_eq!(categories.as_array().ok_or("分类列表无效")?.len(), 1);
    let preferences = json!({ "tree_mode": "category", "last_project_id": project.id });
    request(
        &api,
        "PUT",
        "/api/v1/studio/library/preferences",
        preferences.clone(),
    )
    .await?;
    store.close().await;
    let reopened = Store::open(&directory.path().join("studio.sqlite")).await?;
    assert_eq!(
        request(
            &LocalApi::new(reopened.clone()),
            "GET",
            "/api/v1/studio/library/preferences",
            json!(null)
        )
        .await?,
        preferences
    );
    reopened.close().await;
    Ok(())
}

#[tokio::test]
async fn project_and_session_routes_operate_on_the_real_database()
-> Result<(), Box<dyn std::error::Error>> {
    let root = PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../.local-cache");
    std::fs::create_dir_all(&root)?;
    let directory = tempfile::Builder::new()
        .prefix("local-api-")
        .tempdir_in(root)?;
    let store = Store::open(&directory.path().join("studio.sqlite")).await?;
    let api = LocalApi::new(store.clone());
    let project = api
        .request(
            "account-a",
            LocalRequest {
                method: "POST".to_owned(),
                path: "/api/v1/studio/projects".to_owned(),
                body: Some(json!({ "name": "项目甲" })),
            },
        )
        .await?;
    let project_id = project["id"].as_str().ok_or("项目标识缺失")?;
    let session = api
        .request(
            "account-a",
            LocalRequest {
                method: "POST".to_owned(),
                path: "/api/v1/studio/sessions".to_owned(),
                body: Some(json!({ "request_id": "create-a" })),
            },
        )
        .await?;
    let id = session["id"].as_str().ok_or("会话标识缺失")?;
    api.request(
        "account-a",
        LocalRequest {
            method: "PATCH".to_owned(),
            path: format!("/api/v1/studio/sessions/{id}/project"),
            body: Some(json!({ "project_id": project_id })),
        },
    )
    .await?;
    let list = api
        .request(
            "account-a",
            LocalRequest {
                method: "GET".to_owned(),
                path: format!("/api/v1/studio/sessions?project_id={project_id}&limit=30"),
                body: None,
            },
        )
        .await?;
    assert_eq!(list[0]["id"], id);
    assert_eq!(
        store
            .get_session("account-a", id)
            .await?
            .project_id
            .as_deref(),
        Some(project_id)
    );
    assert!(
        api.request(
            "account-b",
            LocalRequest {
                method: "GET".to_owned(),
                path: format!("/api/v1/studio/sessions/{id}"),
                body: None,
            }
        )
        .await
        .is_err()
    );
    store.close().await;
    Ok(())
}

#[tokio::test]
async fn unknown_routes_and_invalid_parameters_fail_without_creating_records()
-> Result<(), Box<dyn std::error::Error>> {
    let root = PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../.local-cache");
    std::fs::create_dir_all(&root)?;
    let directory = tempfile::Builder::new()
        .prefix("local-api-")
        .tempdir_in(root)?;
    let store = Store::open(&directory.path().join("studio.sqlite")).await?;
    let api = LocalApi::new(store.clone());
    assert!(
        api.request(
            "account-a",
            LocalRequest {
                method: "GET".to_owned(),
                path: "/api/v1/studio/sessions?limit=invalid".to_owned(),
                body: None
            }
        )
        .await
        .is_err()
    );
    assert!(
        api.request(
            "account-a",
            LocalRequest {
                method: "POST".to_owned(),
                path: "/api/v1/studio/unknown".to_owned(),
                body: None
            }
        )
        .await
        .is_err()
    );
    assert!(
        api.request(
            "account-a",
            LocalRequest {
                method: "GET".to_owned(),
                path: "https://example.com/api/v1/studio/projects".to_owned(),
                body: None
            }
        )
        .await
        .is_err()
    );
    assert!(store.list_projects("account-a").await?.is_empty());
    store.close().await;
    Ok(())
}
