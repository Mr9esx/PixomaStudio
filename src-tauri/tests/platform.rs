use pixoma_studio::PlatformClient;
use sha2::Digest;

static PLATFORM_CREDENTIAL_LOCK: tokio::sync::Mutex<()> = tokio::sync::Mutex::const_new(());

struct TestPlatformCredential(keyring::Entry);

impl Drop for TestPlatformCredential {
    fn drop(&mut self) {
        match self.0.delete_credential() {
            Ok(()) | Err(keyring::Error::NoEntry) => {}
            Err(error) => panic!("删除测试平台凭据失败：{error}"),
        }
    }
}

#[test]
fn platform_addresses_reject_credentials_and_non_https_remote_hosts() {
    assert!(PlatformClient::new("file:///Users/example/platform").is_err());
    assert!(PlatformClient::new("https://user:password@example.com").is_err());
    assert!(PlatformClient::new("http://example.com").is_err());
    assert!(PlatformClient::new("https://example.com?token=secret").is_err());
    assert!(PlatformClient::new("http://127.0.0.1:30808").is_ok());
    assert!(PlatformClient::new("https://example.com").is_ok());
}

#[tokio::test]
#[ignore = "需要配置 PIXOMA_TEST_PLATFORM_URL 并连接真实平台"]
async fn platform_registration_uses_a_real_configured_service()
-> Result<(), Box<dyn std::error::Error>> {
    let _lock = PLATFORM_CREDENTIAL_LOCK.lock().await;
    let url = std::env::var("PIXOMA_TEST_PLATFORM_URL")?;
    let reference = format!(
        "platform-{:x}",
        sha2::Sha256::digest(url::Url::parse(&url)?.as_str())
    );
    let _credential = TestPlatformCredential(keyring::Entry::new("com.pixoma.studio", &reference)?);
    let client = PlatformClient::new(&url)?;
    let registration = client.request_auth("GET", "registration", None).await?;
    assert_eq!(registration["enabled"], true);
    let username = format!("desktop-{}", uuid::Uuid::new_v4());
    let password = uuid::Uuid::new_v4().to_string();
    let next_password = uuid::Uuid::new_v4().to_string();
    let registered = client
        .request_auth(
            "POST",
            "register",
            Some(serde_json::json!({
                "username": username, "password": password, "nickname": "桌面联调", "email": ""
            })),
        )
        .await?;
    let scope = client.account_scope().await?;
    assert_eq!(
        client.request_auth("GET", "me", None).await?["id"],
        registered["id"]
    );
    let profile = client
        .request_auth(
            "PATCH",
            "profile",
            Some(serde_json::json!({ "nickname": "桌面验证" })),
        )
        .await?;
    assert_eq!(profile["nickname"], "桌面验证");
    client
        .request_auth(
            "POST",
            "password",
            Some(
                serde_json::json!({ "current_password": password, "new_password": next_password }),
            ),
        )
        .await?;
    client.request_auth("POST", "logout", None).await?;
    assert!(client.account_scope().await.is_err());
    client.request_auth("POST", "login", Some(serde_json::json!({ "username": username, "password": next_password, "remember_me": true }))).await?;
    assert_eq!(client.account_scope().await?, scope);
    drop(client);
    let restored = PlatformClient::new(&url)?;
    assert_eq!(restored.account_scope().await?, scope);
    let catalog = restored
        .request_catalog("/api/v1/studio/skill-market")
        .await?;
    let skill = catalog
        .as_array()
        .and_then(|items| items.first())
        .ok_or("真实平台没有可供验证的技能")?;
    let skill_id = skill["id"].as_str().ok_or("官方技能标识缺失")?;
    let detail = restored
        .request_catalog(&format!("/api/v1/studio/skill-market/{skill_id}"))
        .await
        .map_err(|error| format!("读取真实官方技能详情失败：{error}"))?;
    assert_eq!(detail["id"], skill_id);
    let root = std::path::PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../.local-cache");
    std::fs::create_dir_all(&root)?;
    let directory = tempfile::Builder::new()
        .prefix("official-skill-")
        .tempdir_in(root)?;
    let database = directory.path().join("studio.sqlite");
    let store = pixoma_studio::Store::open(&database).await?;
    let selected = serde_json::json!({ "skill_ids": [skill_id], "request_id": uuid::Uuid::new_v4().to_string() });
    restored
        .prepare_skills(&store, &selected)
        .await
        .map_err(|error| format!("保存官方技能版本失败：{error}"))?;
    restored.prepare_skills(&store, &selected).await?;
    store.close().await;
    let reopened = pixoma_studio::Store::open(&database).await?;
    let api = pixoma_studio::LocalApi::new(reopened.clone());
    let saved = api
        .request(
            &scope,
            pixoma_studio::LocalRequest {
                method: "GET".to_owned(),
                path: format!("/api/v1/studio/skills/{skill_id}"),
                body: None,
            },
        )
        .await?;
    assert_eq!(saved["source"], "official");
    assert_eq!(saved["name"], skill["name"]);
    assert!(
        saved["source_url"]
            .as_str()
            .ok_or("官方来源地址缺失")?
            .starts_with(&url)
    );
    assert!(
        !saved["prompt"]
            .as_str()
            .ok_or("官方技能内容缺失")?
            .is_empty()
    );
    let versions = api
        .request(
            &scope,
            pixoma_studio::LocalRequest {
                method: "GET".to_owned(),
                path: format!("/api/v1/studio/skills/{skill_id}/versions"),
                body: None,
            },
        )
        .await?;
    assert_eq!(versions.as_array().ok_or("官方版本列表缺失")?.len(), 1);
    assert!(
        api.request(
            &scope,
            pixoma_studio::LocalRequest {
                method: "PATCH".to_owned(),
                path: format!("/api/v1/studio/skills/{skill_id}/enabled"),
                body: Some(serde_json::json!({ "enabled": false })),
            }
        )
        .await
        .is_err()
    );
    reopened.close().await;
    restored.request_auth("POST", "logout", None).await?;
    assert!(restored.account_scope().await.is_err());
    Ok(())
}

struct TestModelCredentials(Vec<keyring::Entry>);

impl Drop for TestModelCredentials {
    fn drop(&mut self) {
        for entry in &self.0 {
            match entry.delete_credential() {
                Ok(()) | Err(keyring::Error::NoEntry) => {}
                Err(error) => panic!("删除测试模型凭据失败：{error}"),
            }
        }
    }
}

#[tokio::test]
#[ignore = "需要真实平台地址、模型配置和专用账号的环境变量"]
async fn official_experts_bind_the_local_default_model_and_keep_the_session_selection()
-> Result<(), Box<dyn std::error::Error>> {
    let _lock = PLATFORM_CREDENTIAL_LOCK.lock().await;
    let url = std::env::var("PIXOMA_TEST_PLATFORM_URL")?;
    let mut configuration: serde_json::Value =
        serde_json::from_str(&std::env::var("PIXOMA_TEST_MODEL_CONFIG")?)?;
    let reference = format!(
        "platform-{:x}",
        sha2::Sha256::digest(url::Url::parse(&url)?.as_str())
    );
    let _platform_credential =
        TestPlatformCredential(keyring::Entry::new("com.pixoma.studio", &reference)?);
    let client = PlatformClient::new(&url)?;
    let password = std::env::var("PIXOMA_TEST_STUDIO_PASSWORD")?;
    client.request_auth("POST", "login", Some(serde_json::json!({ "username": std::env::var("PIXOMA_TEST_STUDIO_USERNAME")?, "password": password, "remember_me": false }))).await?;
    client.request_auth("POST", "password", Some(serde_json::json!({ "current_password": password, "new_password": uuid::Uuid::new_v4().to_string() }))).await?;
    let account = client.account_scope().await?;
    let catalog = client
        .request_catalog("/api/v1/studio/expert-market")
        .await?;
    let expert = catalog
        .as_array()
        .and_then(|items| items.first())
        .ok_or("真实平台没有可供验证的专家")?;
    let expert_id = expert["id"].as_str().ok_or("官方专家标识缺失")?;
    let root = std::path::PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../.local-cache");
    std::fs::create_dir_all(&root)?;
    let directory = tempfile::Builder::new()
        .prefix("official-expert-")
        .tempdir_in(root)?;
    let database = directory.path().join("studio.sqlite");
    let mut store = pixoma_studio::Store::open(&database).await?;
    let mut credentials = TestModelCredentials(Vec::new());
    let api = pixoma_studio::LocalApi::new(store.clone());
    let first = api
        .request(
            &account,
            pixoma_studio::LocalRequest {
                method: "POST".to_owned(),
                path: "/api/v1/studio-operations/models".to_owned(),
                body: Some(configuration.clone()),
            },
        )
        .await?;
    let first_id = first["id"].as_str().ok_or("本地模型标识缺失")?;
    let connection = sqlx::SqlitePool::connect(&format!("sqlite://{}", database.display())).await?;
    let reference: String =
        sqlx::query_scalar("SELECT credential_ref FROM model_configs WHERE account_id=? AND id=?")
            .bind(&account)
            .bind(first_id)
            .fetch_one(&connection)
            .await?;
    credentials
        .0
        .push(keyring::Entry::new("com.pixoma.studio", &reference)?);
    connection.close().await;
    let marker = uuid::Uuid::new_v4().to_string();
    let input = serde_json::json!({ "request_id": "official-expert", "text": format!("当前只记住本次会话编号 {marker}，仅回复完整编号，不需要执行创作步骤。"),
        "expert_id": expert_id, "expert_version": expert["version"], "permission_mode": "auto_approve" });
    client.prepare_capabilities(&store, &input).await?;
    client.prepare_capabilities(&store, &input).await?;
    let queued = store.submit_message(&account, input).await?;
    let session = queued["session"]["id"].as_str().ok_or("专家会话标识缺失")?;
    assert_eq!(queued["run"]["model_config_id"], first_id);
    assert_eq!(queued["session"]["expert_id"], expert_id);
    let cached = api
        .request(
            &account,
            pixoma_studio::LocalRequest {
                method: "GET".to_owned(),
                path: format!("/api/v1/studio/experts/{expert_id}"),
                body: None,
            },
        )
        .await?;
    assert_eq!(cached["source"], "official");
    assert!(
        cached["source_url"]
            .as_str()
            .ok_or("专家来源地址缺失")?
            .starts_with(&url)
    );
    assert_ne!(cached["model_id"], first_id);
    drop(api);
    store.close().await;
    store = pixoma_studio::Store::open(&database).await?;
    let outcome = pixoma_studio::AgentRuntime::new(store.clone())
        .execute(
            &account,
            queued["run"]["id"].as_str().ok_or("专家运行缺失")?,
        )
        .await?;
    assert_eq!(
        outcome["status"], "succeeded",
        "{}",
        outcome["error_message"]
    );
    let api = pixoma_studio::LocalApi::new(store.clone());
    let mut changed = configuration.clone();
    changed["base_url"] = serde_json::json!("http://127.0.0.1:1/v1");
    changed["model"] = serde_json::json!("changed-model");
    changed["limits"]["max_output_tokens"] = serde_json::json!(64);
    changed["capabilities"]["tools"] = serde_json::json!(false);
    api.request(
        &account,
        pixoma_studio::LocalRequest {
            method: "PATCH".to_owned(),
            path: format!("/api/v1/studio-operations/models/{first_id}"),
            body: Some(changed),
        },
    )
    .await?;
    let connection = sqlx::SqlitePool::connect(&format!("sqlite://{}", database.display())).await?;
    let reference: String =
        sqlx::query_scalar("SELECT credential_ref FROM model_configs WHERE account_id=? AND id=?")
            .bind(&account)
            .bind(first_id)
            .fetch_one(&connection)
            .await?;
    credentials
        .0
        .push(keyring::Entry::new("com.pixoma.studio", &reference)?);
    connection.close().await;
    configuration["name"] = serde_json::json!("第二项本地默认配置");
    let second = api
        .request(
            &account,
            pixoma_studio::LocalRequest {
                method: "POST".to_owned(),
                path: "/api/v1/studio-operations/models".to_owned(),
                body: Some(configuration),
            },
        )
        .await?;
    let second_id = second["id"].as_str().ok_or("第二项本地模型标识缺失")?;
    let connection = sqlx::SqlitePool::connect(&format!("sqlite://{}", database.display())).await?;
    let reference: String =
        sqlx::query_scalar("SELECT credential_ref FROM model_configs WHERE account_id=? AND id=?")
            .bind(&account)
            .bind(second_id)
            .fetch_one(&connection)
            .await?;
    credentials
        .0
        .push(keyring::Entry::new("com.pixoma.studio", &reference)?);
    connection.close().await;
    let continued = serde_json::json!({ "session_id": session, "request_id": "official-continued", "text": "刚才的会话编号是什么？仅回复完整编号。", "permission_mode": "auto_approve" });
    client.prepare_capabilities(&store, &continued).await?;
    let continued = store.submit_message(&account, continued).await?;
    assert_eq!(continued["run"]["model_config_id"], first_id);
    let connection = sqlx::SqlitePool::connect(&format!("sqlite://{}", database.display())).await?;
    let snapshots: Vec<String> = sqlx::query_scalar("SELECT model_snapshot FROM runs WHERE account_id=? AND session_id=? ORDER BY created_at,id")
        .bind(&account).bind(session).fetch_all(&connection).await?;
    connection.close().await;
    assert_eq!(snapshots.len(), 2);
    assert_eq!(
        snapshots[0], snapshots[1],
        "专家会话需要保留创建时的完整模型配置"
    );
    let run_id = continued["run"]["id"].as_str().ok_or("连续消息运行缺失")?;
    let outcome = pixoma_studio::AgentRuntime::new(store.clone())
        .execute(&account, run_id)
        .await?;
    assert_eq!(
        outcome["status"], "succeeded",
        "{}",
        outcome["error_message"]
    );
    let events = store.run_events_after(&account, run_id, 0).await?;
    let output: String = events
        .iter()
        .filter(|event| event["type"] == "TEXT_MESSAGE_CONTENT")
        .filter_map(|event| event["payload"]["delta"].as_str())
        .collect();
    assert!(output.contains(&marker));
    let input = serde_json::json!({ "request_id": "new-official-session", "text": "仅回复新会话已就绪，不需要执行创作步骤。", "expert_id": expert_id, "expert_version": expert["version"], "permission_mode": "auto_approve" });
    client.prepare_capabilities(&store, &input).await?;
    let fresh = store.submit_message(&account, input).await?;
    assert_eq!(fresh["run"]["model_config_id"], second_id);
    let outcome = pixoma_studio::AgentRuntime::new(store.clone())
        .execute(
            &account,
            fresh["run"]["id"].as_str().ok_or("新会话运行缺失")?,
        )
        .await?;
    assert_eq!(
        outcome["status"], "succeeded",
        "{}",
        outcome["error_message"]
    );
    store.clear_sessions(&account).await?;
    assert!(matches!(
        credentials.0[0].get_password(),
        Err(keyring::Error::NoEntry)
    ));
    assert!(credentials.0[1].get_password().is_ok());
    assert!(credentials.0[2].get_password().is_ok());
    store.close().await;
    client.request_auth("POST", "logout", None).await?;
    Ok(())
}
