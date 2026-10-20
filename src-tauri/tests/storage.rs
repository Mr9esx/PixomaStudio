use pixoma_studio::{LocalApi, LocalRequest, Store};
use serde_json::json;
use std::path::PathBuf;

fn database_directory() -> Result<tempfile::TempDir, std::io::Error> {
    let root = PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../.local-cache");
    std::fs::create_dir_all(&root)?;
    tempfile::Builder::new().prefix("storage-").tempdir_in(root)
}

#[tokio::test]
async fn projects_and_sessions_survive_reopening_and_enforce_account_scope()
-> Result<(), Box<dyn std::error::Error>> {
    let directory = database_directory()?;
    let path = directory.path().join("studio.sqlite");
    let store = Store::open(&path).await?;
    let project = store.create_project("account-a", "项目一").await?;
    let session = store.create_session("account-a", "request-a").await?;
    store
        .move_session("account-a", &session.id, Some(&project.id))
        .await?;
    assert!(store.get_session("account-b", &session.id).await.is_err());
    assert!(
        store
            .move_session("account-b", &session.id, None)
            .await
            .is_err()
    );
    assert!(store.list_projects("account-b").await?.is_empty());
    assert_eq!(
        store
            .get_session("account-a", &session.id)
            .await?
            .project_id,
        Some(project.id.clone())
    );
    store.close().await;

    let reopened = Store::open(&path).await?;
    assert_eq!(reopened.list_projects("account-a").await?[0].name, "项目一");
    assert_eq!(
        reopened
            .get_session("account-a", &session.id)
            .await?
            .project_id,
        Some(project.id)
    );
    reopened.close().await;
    Ok(())
}

#[tokio::test]
async fn repeated_session_requests_are_idempotent_and_project_delete_keeps_sessions()
-> Result<(), Box<dyn std::error::Error>> {
    let directory = database_directory()?;
    let store = Store::open(&directory.path().join("studio.sqlite")).await?;
    let project = store.create_project("account-a", "项目").await?;
    let session = store.create_session("account-a", "request-a").await?;
    let repeated = store.create_session("account-a", "request-a").await?;
    assert_eq!(session.id, repeated.id);
    store
        .move_session("account-a", &session.id, Some(&project.id))
        .await?;
    assert!(
        store
            .delete_project("account-b", &project.id)
            .await
            .is_err()
    );
    store.delete_project("account-a", &project.id).await?;
    assert_eq!(
        store
            .get_session("account-a", &session.id)
            .await?
            .project_id,
        None
    );
    assert_eq!(
        store
            .list_sessions("account-a", None, false, 30, 0)
            .await?
            .len(),
        1
    );
    store.close().await;
    Ok(())
}

#[tokio::test]
async fn invalid_names_and_cross_account_project_references_are_rejected()
-> Result<(), Box<dyn std::error::Error>> {
    let directory = database_directory()?;
    let store = Store::open(&directory.path().join("studio.sqlite")).await?;
    assert!(store.create_project("account-a", "  ").await.is_err());
    assert!(store.create_project("", "项目").await.is_err());
    let project = store
        .create_project("account-b", "另一个账号的项目")
        .await?;
    let session = store.create_session("account-a", "request-a").await?;
    assert!(
        store
            .move_session("account-a", &session.id, Some(&project.id))
            .await
            .is_err()
    );
    assert_eq!(
        store
            .get_session("account-a", &session.id)
            .await?
            .project_id,
        None
    );
    store.close().await;
    Ok(())
}

#[tokio::test]
async fn clearing_sessions_requires_confirmation_and_retains_library_assets()
-> Result<(), Box<dyn std::error::Error>> {
    let directory = database_directory()?;
    let path = directory.path().join("studio.sqlite");
    let store = Store::open(&path).await?;
    let api = LocalApi::new(store.clone());
    let project = store.create_project("account-a", "保留项目").await?;
    let session = store.create_session("account-a", "clear-a").await?;
    let other = store.create_session("account-b", "clear-b").await?;
    store
        .move_session("account-a", &session.id, Some(&project.id))
        .await?;
    let asset = store
        .create_text_asset(
            "account-a",
            &session.id,
            "保留文档",
            "保留内容",
            "clear-asset",
        )
        .await?;
    let mut placement = api
        .request(
            "account-a",
            LocalRequest {
                method: "GET".to_owned(),
                path: format!("/api/v1/studio/library/assets?project_id={}", project.id),
                body: None,
            },
        )
        .await?;
    for confirmation in [None, Some(json!({ "confirmation": "" }))] {
        assert!(
            api.request(
                "account-a",
                LocalRequest {
                    method: "DELETE".to_owned(),
                    path: "/api/v1/studio/sessions".to_owned(),
                    body: confirmation,
                }
            )
            .await
            .is_err()
        );
        assert!(store.get_session("account-a", &session.id).await.is_ok());
    }
    api.request(
        "account-a",
        LocalRequest {
            method: "DELETE".to_owned(),
            path: "/api/v1/studio/sessions".to_owned(),
            body: Some(json!({ "confirmation": "确认清空" })),
        },
    )
    .await?;
    assert!(
        store
            .list_sessions("account-a", None, false, 100, 0)
            .await?
            .is_empty()
    );
    assert!(store.get_session("account-b", &other.id).await.is_ok());
    assert_eq!(store.list_projects("account-a").await?[0].id, project.id);
    assert_eq!(
        store
            .read_asset("account-a", &asset.id, &asset.versions[0].id)
            .await?,
        "保留内容".as_bytes()
    );
    let retained = api
        .request(
            "account-a",
            LocalRequest {
                method: "GET".to_owned(),
                path: format!("/api/v1/studio/library/assets?project_id={}", project.id),
                body: None,
            },
        )
        .await?;
    placement["items"][0]["usages"][0]["session_available"] = json!(false);
    placement["items"][0]["asset"]["usages"][0]["session_available"] = json!(false);
    assert_eq!(placement, retained);
    store.close().await;
    let reopened = Store::open(&path).await?;
    assert!(
        reopened
            .list_sessions("account-a", None, false, 100, 0)
            .await?
            .is_empty()
    );
    assert_eq!(
        reopened
            .get_asset("account-a", &asset.id)
            .await?
            .versions
            .len(),
        1
    );
    reopened.close().await;
    Ok(())
}
