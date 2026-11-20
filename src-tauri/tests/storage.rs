use pixoma_studio::{LocalApi, LocalRequest, Store};
use serde_json::json;
use std::path::PathBuf;

fn database_directory() -> Result<tempfile::TempDir, std::io::Error> {
    let root = PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../.local-cache");
    std::fs::create_dir_all(&root)?;
    tempfile::Builder::new().prefix("storage-").tempdir_in(root)
}

#[tokio::test]
async fn library_preferences_use_single_application_record()
-> Result<(), Box<dyn std::error::Error>> {
    use sqlx::Connection;

    let directory = database_directory()?;
    let path = directory.path().join("studio.sqlite");
    let store = Store::open(&path).await?;
    let mut connection = sqlx::SqliteConnection::connect_with(
        &sqlx::sqlite::SqliteConnectOptions::new().filename(&path),
    )
    .await?;
    let columns: Vec<String> =
        sqlx::query_scalar("SELECT name FROM pragma_table_info('preferences')")
            .fetch_all(&mut connection)
            .await?;
    assert!(!columns.iter().any(|column| column == "account_id"));
    let api = LocalApi::new(store.clone());
    for mode in ["category", "session"] {
        api.request(
            LocalRequest {
                method: "PUT".to_owned(),
                path: "/api/v1/studio/library/preferences".to_owned(),
                body: Some(json!({ "tree_mode": mode, "last_project_id": "" })),
            },
        )
        .await?;
    }
    let count: i64 = sqlx::query_scalar("SELECT count(*) FROM preferences")
        .fetch_one(&mut connection)
        .await?;
    assert_eq!(count, 1);
    connection.close().await?;
    store.close().await;
    let reopened = Store::open(&path).await?;
    let saved = LocalApi::new(reopened.clone())
        .request(
            LocalRequest {
                method: "GET".to_owned(),
                path: "/api/v1/studio/library/preferences".to_owned(),
                body: None,
            },
        )
        .await?;
    assert_eq!(saved["tree_mode"], "session");
    reopened.close().await;
    Ok(())
}

#[tokio::test]
async fn projects_can_be_saved_without_an_account_column() -> Result<(), Box<dyn std::error::Error>>
{
    use sqlx::Connection;

    let directory = database_directory()?;
    let path = directory.path().join("studio.sqlite");
    let store = Store::open(&path).await?;
    let mut connection = sqlx::SqliteConnection::connect_with(
        &sqlx::sqlite::SqliteConnectOptions::new().filename(&path),
    )
    .await?;
    sqlx::query("INSERT INTO projects(id,name,created_at,updated_at) VALUES('project-one','角色与分镜','2026-10-29T00:00:00Z','2026-10-29T00:00:00Z')")
        .execute(&mut connection)
        .await?;
    let name: String = sqlx::query_scalar("SELECT name FROM projects WHERE id='project-one'")
        .fetch_one(&mut connection)
        .await?;
    assert_eq!(name, "角色与分镜");
    connection.close().await?;
    store.close().await;
    Ok(())
}

#[tokio::test]
async fn sessions_can_be_saved_without_an_account_column() -> Result<(), Box<dyn std::error::Error>>
{
    use sqlx::Connection;

    let directory = database_directory()?;
    let path = directory.path().join("studio.sqlite");
    let store = Store::open(&path).await?;
    let mut connection = sqlx::SqliteConnection::connect_with(
        &sqlx::sqlite::SqliteConnectOptions::new().filename(&path),
    )
    .await?;
    sqlx::query("INSERT INTO sessions(id,request_id,title,created_at,updated_at) VALUES('session-one','request-one','工作流计算','2026-10-29T00:00:00Z','2026-10-29T00:00:00Z')")
        .execute(&mut connection)
        .await?;
    let title: String = sqlx::query_scalar("SELECT title FROM sessions WHERE id='session-one'")
        .fetch_one(&mut connection)
        .await?;
    assert_eq!(title, "工作流计算");
    connection.close().await?;
    store.close().await;
    Ok(())
}

#[tokio::test]
async fn projects_and_sessions_survive_reopening() -> Result<(), Box<dyn std::error::Error>> {
    let directory = database_directory()?;
    let path = directory.path().join("studio.sqlite");
    let store = Store::open(&path).await?;
    let project = store.create_project("项目一").await?;
    let session = store.create_session("request-a").await?;
    store.move_session(&session.id, Some(&project.id)).await?;
    assert!(store.get_session("missing-session").await.is_err());
    assert!(store.move_session("missing-session", None).await.is_err());
    assert_eq!(store.list_projects().await?[0].id, project.id);
    assert_eq!(
        store.get_session(&session.id).await?.project_id,
        Some(project.id.clone())
    );
    store.close().await;

    let reopened = Store::open(&path).await?;
    assert_eq!(reopened.list_projects().await?[0].name, "项目一");
    assert_eq!(
        reopened.get_session(&session.id).await?.project_id,
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
    let project = store.create_project("项目").await?;
    let session = store.create_session("request-a").await?;
    let repeated = store.create_session("request-a").await?;
    assert_eq!(session.id, repeated.id);
    store.move_session(&session.id, Some(&project.id)).await?;
    store.delete_project(&project.id).await?;
    assert_eq!(store.get_session(&session.id).await?.project_id, None);
    assert_eq!(store.list_sessions(None, false, 30, 0).await?.len(), 1);
    store.close().await;
    Ok(())
}

#[tokio::test]
async fn invalid_names_and_missing_project_references_are_rejected()
-> Result<(), Box<dyn std::error::Error>> {
    let directory = database_directory()?;
    let store = Store::open(&directory.path().join("studio.sqlite")).await?;
    assert!(store.create_project("  ").await.is_err());
    let session = store.create_session("request-a").await?;
    assert!(
        store
            .move_session(&session.id, Some("missing-project"))
            .await
            .is_err()
    );
    assert_eq!(store.get_session(&session.id).await?.project_id, None);
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
    let project = store.create_project("保留项目").await?;
    let session = store.create_session("clear-a").await?;
    let other = store.create_session("clear-b").await?;
    store.move_session(&session.id, Some(&project.id)).await?;
    let asset = store
        .create_text_asset(&session.id, "保留文档", "保留内容", "clear-asset")
        .await?;
    let mut placement = api
        .request(
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
                LocalRequest {
                    method: "DELETE".to_owned(),
                    path: "/api/v1/studio/sessions".to_owned(),
                    body: confirmation,
                }
            )
            .await
            .is_err()
        );
        assert!(store.get_session(&session.id).await.is_ok());
    }
    api.request(
        LocalRequest {
            method: "DELETE".to_owned(),
            path: "/api/v1/studio/sessions".to_owned(),
            body: Some(json!({ "confirmation": "确认清空" })),
        },
    )
    .await?;
    assert!(store.list_sessions(None, false, 100, 0).await?.is_empty());
    assert!(store.get_session(&other.id).await.is_err());
    assert_eq!(store.list_projects().await?[0].id, project.id);
    assert_eq!(
        store.read_asset(&asset.id, &asset.versions[0].id).await?,
        "保留内容".as_bytes()
    );
    let retained = api
        .request(
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
            .list_sessions(None, false, 100, 0)
            .await?
            .is_empty()
    );
    assert_eq!(reopened.get_asset(&asset.id).await?.versions.len(), 1);
    reopened.close().await;
    Ok(())
}
#[tokio::test]
async fn runtime_tables_use_only_local_business_identifiers()
-> Result<(), Box<dyn std::error::Error>> {
    let root = std::path::PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../.local-cache");
    std::fs::create_dir_all(&root)?;
    let directory = tempfile::Builder::new().prefix("runtime-schema-").tempdir_in(root)?;
    let database = directory.path().join("studio.sqlite");
    let store = pixoma_studio::Store::open(&database).await?;
    let pool = sqlx::SqlitePool::connect(&format!("sqlite://{}", database.display())).await?;
    let columns: Vec<(String, String)> = sqlx::query_as(
        "SELECT m.name,p.name FROM sqlite_master m JOIN pragma_table_info(m.name) p WHERE m.type='table' AND p.name='account_id' ORDER BY m.name",
    ).fetch_all(&pool).await?;
    assert!(columns.is_empty(), "业务表仍包含账号列：{columns:?}");
    let indexes: Vec<String> = sqlx::query_scalar(
        "SELECT name FROM sqlite_master WHERE type='index' AND (name LIKE '%account%' OR sql LIKE '%account_id%') ORDER BY name",
    ).fetch_all(&pool).await?;
    assert!(indexes.is_empty(), "业务索引仍包含账号：{indexes:?}");
    pool.close().await;
    store.close().await;
    Ok(())
}

