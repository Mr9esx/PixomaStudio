use super::*;
use crate::{LocalApi, LocalRequest, Store, local_api::text};
use serde_json::json;
use uuid::Uuid;

#[tokio::test]
#[ignore = "需要真实平台地址和专用账号"]
async fn real_personal_publications_preserve_versions_and_request_identity()
-> std::result::Result<(), Box<dyn std::error::Error>> {
    let client = PlatformClient::new(&std::env::var("PIXOMA_TEST_PLATFORM_URL")?)?;
    let password = std::env::var("PIXOMA_TEST_STUDIO_PASSWORD")?;
    client.request_auth("POST", "login", Some(json!({
        "username": std::env::var("PIXOMA_TEST_STUDIO_USERNAME")?, "password": password, "remember_me": false,
    }))).await?;
    client
        .request_auth(
            "POST",
            "password",
            Some(json!({
                "current_password": password, "new_password": uuid::Uuid::new_v4().to_string(),
            })),
        )
        .await?;
    let account = client.account_scope().await?;
    let root = std::path::PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../.local-cache");
    std::fs::create_dir_all(&root)?;
    let directory = tempfile::Builder::new()
        .prefix("personal-publications-")
        .tempdir_in(root)?;
    let database = directory.path().join("studio.sqlite");
    let store = Store::open(&database).await?;
    let api = LocalApi::new(store.clone());
    let mut input = json!({
        "name": "publish-review", "description": "检查创作资料", "prompt": "\n 保留资料编号。\n", "enabled": true,
        "version": "1.0.0", "notes": "原始版本说明",
    });
    let first = api
        .request(
            &account,
            LocalRequest {
                method: "POST".to_owned(),
                path: "/api/v1/studio/skills".to_owned(),
                body: Some(input.clone()),
            },
        )
        .await?;
    let id = text(&first, "id")?;
    input["version"] = json!("1.1.0");
    input["prompt"] = json!("保留资料编号和出处。");
    input["updated_at"] = first["updated_at"].clone();
    let second = store.save_skill(&account, Some(id), input.clone()).await?;
    let request = json!({ "request_id": uuid::Uuid::new_v4().to_string(), "version": "1.1.0" });
    let command = LocalRequest {
        method: "POST".to_owned(),
        path: format!("/api/v1/studio/skills/{id}/publish"),
        body: Some(request.clone()),
    };
    let (one, two) = tokio::join!(
        client.asset_publication_request(&store, &command),
        client.change_asset_publication(&store, "skill", id, "publish", request.clone()),
    );
    assert_eq!(
        one?.ok_or("桌面发布操作没有返回结果")?["published_version"],
        "1.1.0"
    );
    assert_eq!(two?["id"], id);
    let sync = store
        .asset_sync(&account, "skill", id)
        .await?
        .ok_or("同步记录缺失")?;
    let remote = text(&sync, "remote_id")?;
    assert_ne!(remote, id);
    assert_eq!(
        store.skill_version(&account, id, "1.0.0").await?["prompt"],
        "\n 保留资料编号。\n"
    );
    let remote_versions = client
        .request(
            Method::GET,
            &format!("/api/v1/studio/skills/{remote}/versions"),
            None,
        )
        .await?;
    assert_eq!(remote_versions.as_array().ok_or("平台版本缺失")?.len(), 2);
    assert_eq!(
        client
            .request(
                Method::GET,
                &format!("/api/v1/studio/skills/{remote}/versions/1.0.0"),
                None
            )
            .await?["prompt"],
        "保留资料编号。"
    );
    assert_eq!(
        client
            .asset_publication_history(&store, "skill", id)
            .await?
            .as_array()
            .ok_or("发布记录缺失")?
            .len(),
        1
    );
    let mut mismatch = request.clone();
    mismatch["version"] = json!("1.0.0");
    assert!(
        client
            .change_asset_publication(&store, "skill", id, "publish", mismatch)
            .await
            .is_err()
    );
    assert!(
        store
            .publication_history("another-account", "skill", id)
            .await
            .is_err()
    );
    input["version"] = json!("1.2.0");
    input["prompt"] = json!("保留资料编号、出处和日期。");
    input["updated_at"] = second["updated_at"].clone();
    let saved = store.save_skill(&account, Some(id), input).await?;
    assert_eq!(saved["published_version"], "1.1.0");
    assert_eq!(
        store.get_skill(&account, id).await?["published_version"],
        "1.1.0"
    );
    let published = store.get_skill(&account, id).await?;
    assert_eq!(published["version"], "1.2.0");
    assert_eq!(published["published"], true);
    drop(api);
    store.close().await;
    let store = Store::open(&database).await?;
    assert_eq!(
        client
            .change_asset_publication(&store, "skill", id, "publish", request)
            .await?["published_version"],
        "1.1.0"
    );
    client
        .change_asset_publication(
            &store,
            "skill",
            id,
            "publish",
            json!({
                "request_id": uuid::Uuid::new_v4().to_string(), "version": "1.0.0",
            }),
        )
        .await?;
    assert_eq!(
        store.get_skill(&account, id).await?["published_version"],
        "1.0.0"
    );

    let models = client
        .request(Method::GET, "/api/v1/studio/models", None)
        .await?;
    let model = models
        .as_array()
        .and_then(|models| {
            models
                .iter()
                .find(|model| model["capabilities"]["tools"] == true)
        })
        .ok_or("缺少平台工具模型")?;
    let mut configuration = json!({ "name": "发布模型", "api_key": "", "enabled": true, "agent_enabled": true, "default": true });
    for field in [
        "protocol",
        "base_url",
        "model",
        "limits",
        "thinking",
        "capabilities",
    ] {
        configuration[field] = model[field].clone();
    }
    let local_model = store.save_model(&account, None, configuration).await?;
    let image = include_bytes!("../../../public/images/pixoma-ghost.png");
    let avatar = store.upload_asset(&account, json!({
        "file_name": "头像.png", "project_id": "", "request_id": "publication-avatar", "purpose": "expert_avatar",
    }), image).await?;
    let expert = store.save_expert(&account, None, json!({
        "schema_version": 2, "name": "发布资料检查", "description": "检查资料", "model_id": local_model["id"],
        "prompt_parts": [{ "type": "skill", "skill_id": id, "version": "1.0.0", "name": "" }],
        "avatar_asset_id": avatar["id"], "avatar_version_id": avatar["versions"][0]["id"],
        "examples": [], "skill_refs": [], "workflow_refs": [], "sop": [], "version": "1.0.0", "enabled": true,
    })).await?;
    let expert_id = text(&expert, "id")?;
    client
        .change_asset_publication(
            &store,
            "expert",
            expert_id,
            "publish",
            json!({
                "request_id": uuid::Uuid::new_v4().to_string(), "version": "1.0.0",
            }),
        )
        .await?;
    let expert_sync = store
        .asset_sync(&account, "expert", expert_id)
        .await?
        .ok_or("专家同步记录缺失")?;
    let remote_expert = text(&expert_sync, "remote_id")?;
    let platform_expert = client
        .request(
            Method::GET,
            &format!("/api/v1/studio/experts/{remote_expert}"),
            None,
        )
        .await?;
    assert_eq!(platform_expert["skill_refs"][0]["skill_id"], remote);
    assert_eq!(platform_expert["model_id"], model["id"]);
    assert_eq!(
        store.get_expert(&account, expert_id).await?["model_id"],
        local_model["id"]
    );
    assert_eq!(
        store.expert_version(&account, expert_id, "1.0.0").await?["prompt_parts"][0]["name"],
        ""
    );
    assert_ne!(platform_expert["avatar_asset_id"], avatar["id"]);
    assert_eq!(
        client
            .expert_avatar_response(remote_expert, "1.0.0", "GET")
            .await?
            .body,
        image
    );
    client
        .change_asset_publication(
            &store,
            "expert",
            expert_id,
            "unpublish",
            json!({ "request_id": uuid::Uuid::new_v4().to_string() }),
        )
        .await?;
    client
        .change_asset_publication(
            &store,
            "skill",
            id,
            "unpublish",
            json!({ "request_id": uuid::Uuid::new_v4().to_string() }),
        )
        .await?;
    assert_eq!(store.get_skill(&account, id).await?["published"], false);
    assert_eq!(
        store.get_expert(&account, expert_id).await?["published"],
        false
    );
    assert_eq!(
        client
            .asset_publication_history(&store, "skill", id)
            .await?
            .as_array()
            .ok_or("发布记录缺失")?
            .len(),
        3
    );
    assert_eq!(
        client
            .asset_publication_history(&store, "expert", expert_id)
            .await?
            .as_array()
            .ok_or("发布记录缺失")?
            .len(),
        2
    );
    store.close().await;
    Box::pin(check_unknown_publication_results(
        &client,
        &account,
        directory.path(),
    ))
    .await?;
    client.request_auth("POST", "logout", None).await?;
    Ok(())
}

async fn check_unknown_publication_results(
    client: &PlatformClient,
    account: &str,
    root: &std::path::Path,
) -> std::result::Result<(), Box<dyn std::error::Error>> {
    let database = root.join("unknown.sqlite");
    let store = Store::open(&database).await?;
    let input = json!({
        "name": "publish-package", "description": "保存技能包", "enabled": true, "version": "1.0.0", "notes": "检查完整技能包",
        "files": [
            { "path": "SKILL.md", "content": "---\nname: publish-package\ndescription: 保存技能包\n---\n读取 references/code.txt。" },
            { "path": "references", "content": "", "directory": true },
            { "path": "references/code.txt", "content": "PX73916" },
            { "path": "resources/data.bin", "content": "AAECA/8=", "binary": true },
        ],
    });
    let asset = store.save_skill(account, None, input).await?;
    let id = text(&asset, "id")?;
    let before = client
        .request(Method::GET, "/api/v1/studio/skills", None)
        .await?
        .as_array()
        .ok_or("平台技能列表缺失")?
        .len();
    sqlx::query("CREATE TRIGGER reject_mapping BEFORE UPDATE ON asset_syncs WHEN json_type(NEW.document,'$.remote_id')='text' BEGIN SELECT RAISE(ABORT,'检查同步对象保存中断'); END")
        .execute(&store.pool).await?;
    let request = json!({ "request_id": Uuid::new_v4().to_string(), "version": "1.0.0" });
    assert!(matches!(
        client
            .change_asset_publication(&store, "skill", id, "publish", request.clone())
            .await,
        Err(Error::Database(_))
    ));
    let pending = store
        .asset_sync(account, "skill", id)
        .await?
        .ok_or("同步记录缺失")?;
    assert_eq!(pending["creation_state"], "creating");
    assert!(pending["remote_id"].is_null());
    assert_eq!(
        client
            .request(Method::GET, "/api/v1/studio/skills", None)
            .await?
            .as_array()
            .ok_or("平台技能列表缺失")?
            .len(),
        before + 1
    );
    assert!(
        client
            .change_asset_publication(&store, "skill", id, "publish", request.clone())
            .await
            .is_err()
    );
    assert_eq!(
        client
            .request(Method::GET, "/api/v1/studio/skills", None)
            .await?
            .as_array()
            .ok_or("平台技能列表缺失")?
            .len(),
        before + 1
    );
    sqlx::query("DROP TRIGGER reject_mapping")
        .execute(&store.pool)
        .await?;
    store.close().await;
    let store = Store::open(&database).await?;
    let result = client
        .change_asset_publication(&store, "skill", id, "publish", request)
        .await?;
    assert_eq!(result["published"], true);
    assert_eq!(
        client
            .request(Method::GET, "/api/v1/studio/skills", None)
            .await?
            .as_array()
            .ok_or("平台技能列表缺失")?
            .len(),
        before + 1
    );
    let sync = store
        .asset_sync(account, "skill", id)
        .await?
        .ok_or("同步对象缺失")?;
    let remote = text(&sync, "remote_id")?;
    let received = client
        .request(
            Method::GET,
            &format!("/api/v1/studio/skills/{remote}/versions/1.0.0"),
            None,
        )
        .await?;
    let files = received["files"].as_array().ok_or("平台技能文件缺失")?;
    assert!(
        files
            .iter()
            .any(|file| file["path"] == "references/code.txt" && file["content"] == "PX73916")
    );
    assert!(files.iter().any(|file| file["path"] == "resources/data.bin"
        && file["content"] == "AAECA/8="
        && file["binary"] == true));
    sqlx::query("CREATE TRIGGER reject_publication_receipt BEFORE UPDATE ON asset_publication_requests WHEN json_type(NEW.document,'$.response')='object' BEGIN SELECT RAISE(ABORT,'检查发布结果保存中断'); END")
        .execute(&store.pool).await?;
    let request = json!({ "request_id": Uuid::new_v4().to_string() });
    assert!(matches!(
        client
            .change_asset_publication(&store, "skill", id, "unpublish", request.clone())
            .await,
        Err(Error::Database(_))
    ));
    assert_eq!(
        client
            .request(
                Method::GET,
                &format!("/api/v1/studio/skills/{remote}"),
                None
            )
            .await?["published"],
        false
    );
    assert!(
        client
            .change_asset_publication(&store, "skill", id, "unpublish", request.clone())
            .await
            .is_err()
    );
    assert_eq!(
        client
            .request(
                Method::GET,
                &format!("/api/v1/studio/skills/{remote}/publications"),
                None
            )
            .await?
            .as_array()
            .ok_or("平台发布记录缺失")?
            .len(),
        2
    );
    sqlx::query("DROP TRIGGER reject_publication_receipt")
        .execute(&store.pool)
        .await?;
    store.close().await;
    let store = Store::open(&database).await?;
    assert_eq!(
        client
            .change_asset_publication(&store, "skill", id, "unpublish", request)
            .await?["published"],
        false
    );
    assert_eq!(
        store
            .publication_history(account, "skill", id)
            .await?
            .as_array()
            .ok_or("本地发布记录缺失")?
            .len(),
        2
    );
    store.close().await;
    Ok(())
}
