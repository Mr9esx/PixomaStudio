use pixoma_studio::{AgentRuntime, LocalApi, LocalRequest, Store};
use serde_json::{Value, json};
use std::path::PathBuf;

struct TestCredential(keyring::Entry);

impl Drop for TestCredential {
    fn drop(&mut self) {
        self.0.delete_credential().expect("删除测试模型凭据失败");
    }
}

async fn overview(store: &Store, session: &str) -> Result<Value, Box<dyn std::error::Error>> {
    Ok(LocalApi::new(store.clone())
        .request(
            "real-compaction-check",
            LocalRequest {
                method: "GET".to_owned(),
                path: format!("/api/v1/studio/sessions/{session}/context"),
                body: None,
            },
        )
        .await?)
}

#[tokio::test]
#[ignore = "需要 PIXOMA_TEST_MODEL_CONFIG 并连接真实模型服务"]
async fn real_model_compacts_selected_material_and_reuses_the_saved_summary()
-> Result<(), Box<dyn std::error::Error>> {
    let mut configuration: Value =
        serde_json::from_str(&std::env::var("PIXOMA_TEST_MODEL_CONFIG")?)?;
    configuration["limits"] = json!({ "context_window_tokens": 12000, "max_input_tokens": 3000, "max_output_tokens": 1200 });
    configuration["capabilities"]["tools"] = json!(false);
    configuration["capabilities"]["vision"] = json!(true);
    let root = PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../.local-cache");
    std::fs::create_dir_all(&root)?;
    let directory = tempfile::Builder::new()
        .prefix("real-compaction-")
        .tempdir_in(root)?;
    let database = directory.path().join("studio.sqlite");
    let mut store = Store::open(&database).await?;
    let model = LocalApi::new(store.clone())
        .request(
            "real-compaction-check",
            LocalRequest {
                method: "POST".to_owned(),
                path: "/api/v1/studio-operations/models".to_owned(),
                body: Some(configuration),
            },
        )
        .await?;
    let model_id = model["id"].as_str().ok_or("模型标识缺失")?;
    let connection = sqlx::SqlitePool::connect(&format!("sqlite://{}", database.display())).await?;
    let reference: String =
        sqlx::query_scalar("SELECT credential_ref FROM model_configs WHERE account_id=? AND id=?")
            .bind("real-compaction-check")
            .bind(model_id)
            .fetch_one(&connection)
            .await?;
    let _credential = TestCredential(keyring::Entry::new("com.pixoma.studio", &reference)?);
    connection.close().await;
    let session = store
        .create_session("real-compaction-check", "material")
        .await?;
    let expected = uuid::Uuid::new_v4().to_string();
    let retained = uuid::Uuid::new_v4().to_string();
    let material = format!(
        "审核编号：{expected}\n{}\n备用编号：{retained}",
        "这是一份参考资料。湖边有一条道路，道路旁有树木，远处可以看见山峰。\n".repeat(160)
    );
    let asset = store
        .create_text_asset(
            "real-compaction-check",
            &session.id,
            "较长资料",
            &material,
            "material",
        )
        .await?;
    let turn = store.submit_message("real-compaction-check", json!({
        "session_id": session.id, "request_id": "compaction-first", "text": "请根据所附资料仅回复审核编号。", "locale": "zh", "permission_mode": "auto_approve", "model_config_id": model_id,
        "parts": [{ "type": "asset_ref", "asset_id": asset.id, "asset_version_id": asset.versions[0].id, "name": asset.name }],
    })).await?;
    let id = turn["run"]["id"].as_str().ok_or("运行标识缺失")?;
    let outcome = AgentRuntime::new(store.clone())
        .execute("real-compaction-check", id)
        .await?;
    assert_eq!(
        outcome["status"], "succeeded",
        "{}",
        outcome["error_message"]
    );
    let text = store
        .run_events_after("real-compaction-check", id, 0)
        .await?
        .iter()
        .filter(|event| event["type"] == "TEXT_MESSAGE_CONTENT")
        .filter_map(|event| event["payload"]["delta"].as_str())
        .collect::<String>();
    assert!(text.contains(&expected), "压缩后的资料未保留审核编号");
    let first = overview(&store, &session.id).await?;
    let compactions = first["compactions"].as_u64().ok_or("压缩次数缺失")?;
    assert!(compactions > 0, "超过输入限制的资料没有经过压缩");
    store.close().await;
    store = Store::open(&database).await?;
    let followup = store.submit_message("real-compaction-check", json!({
        "session_id": session.id, "request_id": "compaction-followup", "text": "同一份资料里的备用编号是什么？仅回复备用编号。", "locale": "zh", "permission_mode": "auto_approve", "model_config_id": model_id,
    })).await?;
    let followup_id = followup["run"]["id"].as_str().ok_or("运行标识缺失")?;
    let outcome = AgentRuntime::new(store.clone())
        .execute("real-compaction-check", followup_id)
        .await?;
    assert_eq!(
        outcome["status"], "succeeded",
        "{}",
        outcome["error_message"]
    );
    let text = store
        .run_events_after("real-compaction-check", followup_id, 0)
        .await?
        .iter()
        .filter(|event| event["type"] == "TEXT_MESSAGE_CONTENT")
        .filter_map(|event| event["payload"]["delta"].as_str())
        .collect::<String>();
    assert!(
        text.contains(&retained),
        "重新打开数据库后未保留摘要中的备用编号"
    );
    let final_view = overview(&store, &session.id).await?;
    assert_eq!(
        final_view["compactions"], compactions,
        "已经保存的资料摘要被重复生成"
    );
    let requests = LocalApi::new(store.clone())
        .request(
            "real-compaction-check",
            LocalRequest {
                method: "GET".to_owned(),
                path: format!("/api/v1/studio/sessions/{}/context/requests", session.id),
                body: None,
            },
        )
        .await?;
    let requests = requests["requests"].as_array().ok_or("请求记录缺失")?;
    assert!(
        requests
            .iter()
            .any(|request| request["purpose"] == "compaction")
    );
    for request in requests {
        assert!(
            request["projected_tokens"]
                .as_u64()
                .is_some_and(|tokens| tokens <= 3000)
        );
        assert!(
            request["input_tokens"]
                .as_u64()
                .is_some_and(|tokens| tokens <= 3000)
        );
    }
    let oversized = "请完整保留这段用户输入。".repeat(1000);
    let rejected = store
        .queue_run(
            "real-compaction-check",
            &session.id,
            "oversized-current",
            model_id,
            &oversized,
            "auto_approve",
        )
        .await?;
    let rejected_id = rejected["run"]["id"].as_str().ok_or("运行标识缺失")?;
    let outcome = AgentRuntime::new(store.clone())
        .execute("real-compaction-check", rejected_id)
        .await?;
    assert_eq!(outcome["status"], "failed");
    let after_rejection = overview(&store, &session.id).await?;
    assert_eq!(
        after_rejection["steps"], final_view["steps"],
        "超长当前消息触发了额外的模型请求"
    );
    let connection = sqlx::SqlitePool::connect(&format!("sqlite://{}", database.display())).await?;
    let saved: String = sqlx::query_scalar("SELECT json_extract(document,'$.content[0].text') FROM messages WHERE account_id=? AND id=?")
        .bind("real-compaction-check").bind(outcome["trigger_message_id"].as_str().ok_or("消息标识缺失")?)
        .fetch_one(&connection).await?;
    assert_eq!(saved, oversized);
    connection.close().await;
    assert_eq!(
        store
            .read_asset("real-compaction-check", &asset.id, &asset.versions[0].id)
            .await?,
        material.as_bytes()
    );
    store.close().await;
    Ok(())
}
