use pixoma_studio::{LibraryQuery, Store};
use serde_json::json;
use std::path::PathBuf;

#[tokio::test]
async fn library_filters_tags_and_versions_persist_without_changing_other_accounts()
-> Result<(), Box<dyn std::error::Error>> {
    let root = PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../.local-cache");
    std::fs::create_dir_all(&root)?;
    let directory = tempfile::Builder::new()
        .prefix("library-")
        .tempdir_in(root)?;
    let store = Store::open(&directory.path().join("studio.sqlite")).await?;
    let project = store.create_project("account-a", "素材").await?;
    let session = store.create_session("account-a", "create-session").await?;
    store
        .move_session("account-a", &session.id, Some(&project.id))
        .await?;
    let asset = store
        .create_text_asset("account-a", &session.id, "脚本.md", "正文", "create-asset")
        .await?;
    let query = LibraryQuery {
        project_id: project.id.clone(),
        ..Default::default()
    };
    let page = store.library_assets("account-a", &query).await?;
    assert_eq!(page["total"], 1);
    let project_asset_id = page["items"][0]["id"].as_str().ok_or("项目资产标识缺失")?;
    let tag = store.create_tag("account-a", "待审阅").await?;
    let tag_id = tag["id"].as_str().ok_or("标签标识缺失")?;
    let category = store
        .create_category("account-a", &project.id, "文档", "")
        .await?;
    store
        .patch_project_asset(
            "account-a",
            project_asset_id,
            &json!({ "rating": 4, "category_id": category["id"] }),
        )
        .await?;
    store
        .set_project_asset_tags("account-a", project_asset_id, &[tag_id.to_owned()])
        .await?;
    let filtered = store
        .library_assets(
            "account-a",
            &LibraryQuery {
                project_id: project.id.clone(),
                rating: Some(4),
                tag_ids: Some(tag_id.to_owned()),
                ..Default::default()
            },
        )
        .await?;
    assert_eq!(filtered["total"], 1);
    assert_eq!(filtered["items"][0]["asset_id"], asset.id);
    assert!(
        store
            .patch_project_asset("account-b", project_asset_id, &json!({ "rating": 1 }))
            .await
            .is_err()
    );
    assert!(
        store
            .patch_project_asset("account-a", project_asset_id, &json!({ "rating": 9 }))
            .await
            .is_err()
    );
    assert!(
        store
            .patch_project_asset("account-a", project_asset_id, &json!({ "unknown": true }))
            .await
            .is_err()
    );
    let other = store
        .library_assets("account-b", &LibraryQuery::default())
        .await?;
    assert_eq!(other["total"], 0);
    store.delete_project("account-a", &project.id).await?;
    let unassigned = store
        .library_assets("account-a", &LibraryQuery::default())
        .await?;
    assert_eq!(unassigned["total"], 1);
    assert_eq!(unassigned["items"][0]["category_id"], "");
    store.close().await;
    Ok(())
}

#[tokio::test]
async fn project_delete_merges_existing_unassigned_placements_without_losing_tags()
-> Result<(), Box<dyn std::error::Error>> {
    let root = PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../.local-cache");
    std::fs::create_dir_all(&root)?;
    let directory = tempfile::Builder::new()
        .prefix("library-")
        .tempdir_in(root)?;
    let store = Store::open(&directory.path().join("studio.sqlite")).await?;
    let project = store.create_project("account-a", "项目").await?;
    let session = store.create_session("account-a", "create-session").await?;
    store
        .move_session("account-a", &session.id, Some(&project.id))
        .await?;
    let asset = store
        .create_text_asset("account-a", &session.id, "文档.md", "正文", "create-asset")
        .await?;
    store
        .place_asset("account-a", "", &asset.id, &asset.versions[0].id)
        .await?;
    let page = store
        .library_assets(
            "account-a",
            &LibraryQuery {
                project_id: project.id.clone(),
                ..Default::default()
            },
        )
        .await?;
    let id = page["items"][0]["id"].as_str().ok_or("项目资产标识缺失")?;
    let tag = store.create_tag("account-a", "保留").await?;
    store
        .set_project_asset_tags(
            "account-a",
            id,
            &[tag["id"].as_str().ok_or("标签标识缺失")?.to_owned()],
        )
        .await?;
    store.delete_project("account-a", &project.id).await?;
    let remaining = store
        .library_assets("account-a", &LibraryQuery::default())
        .await?;
    assert_eq!(remaining["total"], 1);
    assert_eq!(remaining["items"][0]["tags"][0]["name"], "保留");
    assert_eq!(
        store
            .read_asset("account-a", &asset.id, &asset.versions[0].id)
            .await?,
        "正文".as_bytes()
    );
    store.close().await;
    Ok(())
}
