use pixoma_studio::{AgentRuntime, LocalApi, LocalRequest, PlatformClient, Store};
use serde_json::{Value, json};

struct TestCredentials(Vec<keyring::Entry>);

impl Drop for TestCredentials {
    fn drop(&mut self) {
        for entry in &self.0 {
            match entry.delete_credential() {
                Ok(()) | Err(keyring::Error::NoEntry) => {}
                Err(error) => panic!("删除工作流检查凭据失败：{error}"),
            }
        }
    }
}

#[tokio::test]
#[ignore = "需要真实平台、模型、MCP 和 ComfyUI 计算节点"]
async fn real_workflow_executes_with_local_image_bytes() -> Result<(), Box<dyn std::error::Error>> {
    let platform = PlatformClient::new(&std::env::var("PIXOMA_TEST_PLATFORM_URL")?)?;
    let password = std::env::var("PIXOMA_TEST_STUDIO_PASSWORD")?;
    platform.request_auth("POST", "login", Some(json!({
        "username": std::env::var("PIXOMA_TEST_STUDIO_USERNAME")?, "password": password, "remember_me": false,
    }))).await?;
    platform
        .request_auth(
            "POST",
            "password",
            Some(json!({
                "current_password": password, "new_password": uuid::Uuid::new_v4().to_string(),
            })),
        )
        .await?;
    let account = platform.account_scope().await?;
    let workflow_id = std::env::var("PIXOMA_TEST_WORKFLOW_ID")?;
    let root = std::path::PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../.local-cache");
    std::fs::create_dir_all(&root)?;
    let directory = tempfile::Builder::new()
        .prefix("workflow-execution-")
        .tempdir_in(root)?;
    let database = directory.path().join("studio.sqlite");
    let mut store = Store::open(&database).await?;
    let api = LocalApi::new(store.clone());
    api.request(
        &account,
        LocalRequest {
            method: "POST".to_owned(),
            path: "/api/v1/studio-operations/models".to_owned(),
            body: Some(serde_json::from_str(&std::env::var(
                "PIXOMA_TEST_MODEL_CONFIG",
            )?)?),
        },
    )
    .await?;
    api.request(
        &account,
        LocalRequest {
            method: "POST".to_owned(),
            path: "/api/v1/studio-operations/connectors".to_owned(),
            body: Some(json!({
                "name": "真实图片处理", "url": std::env::var("PIXOMA_TEST_MCP_URL")?,
                "credential": std::env::var("PIXOMA_TEST_MCP_TOKEN")?, "enabled": true, "policy": "auto",
            })),
        },
    )
    .await?;
    let connection = sqlx::SqlitePool::connect(&format!("sqlite://{}", database.display())).await?;
    let references: Vec<String> = sqlx::query_scalar("SELECT credential_ref FROM model_configs WHERE account_id=? UNION SELECT credential_ref FROM connectors WHERE account_id=?")
        .bind(&account).bind(&account).fetch_all(&connection).await?;
    let _credentials = TestCredentials(
        references
            .iter()
            .map(|reference| keyring::Entry::new("com.pixoma.studio", reference))
            .collect::<Result<_, _>>()?,
    );
    let (session, task, response, asset_id) = Box::pin(check_workflow_outputs(
        &mut store,
        &connection,
        &account,
        &workflow_id,
        &database,
    ))
    .await?;
    Box::pin(check_workflow_cancellation(
        &store,
        &connection,
        &account,
        &workflow_id,
        &response,
        &asset_id,
    ))
    .await?;
    let runtime = AgentRuntime::new(store.clone());
    Box::pin(check_expert_workflows(
        &store,
        &runtime,
        &connection,
        &account,
        &workflow_id,
        &response,
    ))
    .await?;
    Box::pin(check_unknown_submission(
        &store,
        &runtime,
        &connection,
        &account,
        &workflow_id,
        &response,
    ))
    .await?;
    connection.close().await;
    store.close().await;
    let reopened = Store::open(&database).await?;
    assert_eq!(reopened.recover_interrupted_runs().await?, 0);
    assert_eq!(
        reopened.session_detail(&account, &session).await?["workflow_executions"][0]["task_id"],
        task
    );
    assert_eq!(
        reopened
            .list_session_assets(&account, &session)
            .await?
            .len(),
        3
    );
    reopened.close().await;
    platform.request_auth("POST", "logout", None).await?;
    Ok(())
}

async fn check_workflow_outputs(
    store: &mut Store,
    connection: &sqlx::SqlitePool,
    account: &str,
    workflow_id: &str,
    database: &std::path::Path,
) -> Result<(String, Value, Value, String), Box<dyn std::error::Error>> {
    let input = json!({
        "request_id": uuid::Uuid::new_v4().to_string(), "permission_mode": "full_access",
        "text": format!("请立即调用真实平台工作流 {workflow_id} 的 run_workflow 工具获取输入表单，参数严格使用 case_id={workflow_id}、inputs={{}}。App 会在真正提交之前显示输入卡片，我会在卡片中选择图片。不要调用其他工具，不要自行填写任何输入。"),
    });
    let queued = store.submit_message(account, input).await?;
    let run = queued["run"]["id"].as_str().ok_or("运行标识缺失")?;
    let session = queued["session"]["id"].as_str().ok_or("会话标识缺失")?;
    let runtime = AgentRuntime::new(store.clone());
    let waiting = runtime.execute(account, run).await?;
    assert_eq!(
        waiting["status"], "waiting_clarification",
        "{}",
        waiting["error_message"]
    );
    let mut bytes = std::io::Cursor::new(Vec::new());
    image::DynamicImage::ImageRgb8(image::RgbImage::from_pixel(
        12,
        9,
        image::Rgb([20, 60, 100]),
    ))
    .write_to(&mut bytes, image::ImageFormat::Png)?;
    let asset = store
        .create_asset_bytes(
            account,
            Some(session),
            "输入图片.png",
            bytes.get_ref(),
            "user",
            &uuid::Uuid::new_v4().to_string(),
        )
        .await?;
    let pending = store.session_detail(account, session).await?;
    let response = json!({
        "interruptId": pending["pending_clarifications"][0]["id"], "status": "resolved",
        "payload": { "inputs": { "image": { "asset_id": asset.id, "asset_version_id": asset.versions[0].id } } },
    });
    assert!(
        store
            .resolve_run_interrupt("other-account", session, &response)
            .await
            .is_err()
    );
    store
        .resolve_run_interrupt(account, session, &response)
        .await?;
    let mut running = Box::pin(runtime.execute(account, run));
    let recorded_task = tokio::select! {
        outcome = &mut running => return Err(format!("工作流在读取运行中记录之前结束：{}", outcome?).into()),
        task = tokio::time::timeout(std::time::Duration::from_secs(15), async {
            loop {
                let task: Option<String> = sqlx::query_scalar("SELECT json_extract(document,'$.task_id') FROM workflow_executions WHERE account_id=? AND run_id=? AND coalesce(json_extract(document,'$.task_id'),'')<>''")
                    .bind(account).bind(run).fetch_optional(connection).await?;
                if let Some(task) = task { return Ok::<_, sqlx::Error>(task); }
                tokio::time::sleep(std::time::Duration::from_millis(5)).await;
            }
        }) => task??,
    };
    drop(running);
    drop(runtime);
    assert_eq!(
        store.get_run(account, run).await?["status"],
        "waiting_workflow"
    );
    store.close().await;
    *store = Store::open(database).await?;
    assert_eq!(store.recover_interrupted_runs().await?, 1);
    assert_eq!(store.get_run(account, run).await?["status"], "queued");
    assert!(store.claim_run(account, run).await?);
    store.close().await;
    *store = Store::open(database).await?;
    assert_eq!(store.recover_interrupted_runs().await?, 1);
    assert_eq!(store.get_run(account, run).await?["status"], "queued");
    let runtime = AgentRuntime::new(store.clone());
    let (outcome, observed) = tokio::time::timeout(std::time::Duration::from_secs(120), async {
        tokio::join!(runtime.execute(account, run), async {
            let mut saved_count = 0;
            loop {
                let (first, second) = tokio::join!(
                    store.session_detail(account, session),
                    store.session_detail(account, session),
                );
                first?;
                second?;
                let document: String = sqlx::query_scalar(
                    "SELECT document FROM workflow_executions WHERE account_id=? AND run_id=?",
                )
                .bind(account)
                .bind(run)
                .fetch_one(connection)
                .await?;
                let document: Value = serde_json::from_str(&document)?;
                let count = document["outputs"]
                    .as_array()
                    .ok_or(pixoma_studio::Error::State("保存的输出列表缺失"))?
                    .len();
                assert!(count >= saved_count);
                saved_count = count;
                let node: String = sqlx::query_scalar(
                    "SELECT document FROM flow_nodes WHERE account_id=? AND id=?",
                )
                .bind(account)
                .bind(
                    document["operation_node_id"]
                        .as_str()
                        .ok_or(pixoma_studio::Error::State("工作流节点缺失"))?,
                )
                .fetch_one(connection)
                .await?;
                let node: Value = serde_json::from_str(&node)?;
                assert!(
                    node["outputs"]
                        .as_array()
                        .ok_or(pixoma_studio::Error::State("节点输出缺失"))?
                        .len()
                        >= saved_count
                );
                if !matches!(
                    store.get_run(account, run).await?["status"].as_str(),
                    Some("queued" | "running" | "waiting_workflow")
                ) {
                    return Ok::<_, pixoma_studio::Error>(());
                }
                tokio::time::sleep(std::time::Duration::from_millis(5)).await;
            }
        })
    })
    .await?;
    observed?;
    let outcome = outcome?;
    assert_eq!(
        outcome["status"], "succeeded",
        "{}",
        outcome["error_message"]
    );
    let detail = store.session_detail(account, session).await?;
    let executions = detail["workflow_executions"]
        .as_array()
        .ok_or("工作流执行记录缺失")?;
    assert_eq!(executions.len(), 1);
    assert_eq!(executions[0]["status"], "succeeded");
    assert_eq!(executions[0]["task_id"], recorded_task);
    assert!(
        !executions[0]["task_id"]
            .as_str()
            .ok_or("远程任务标识缺失")?
            .is_empty()
    );
    let outputs = store.list_session_assets(account, session).await?;
    assert_eq!(outputs.len(), 3);
    assert_eq!(
        executions[0]["outputs"]
            .as_array()
            .ok_or("任务输出缺失")?
            .len(),
        2
    );
    let inverted = executions[0]["outputs"]
        .as_array()
        .ok_or("任务输出缺失")?
        .iter()
        .find(|output| output["key"] == "out-0")
        .ok_or("反色输出缺失")?;
    let output = outputs
        .iter()
        .find(|asset| asset.id == inverted["asset_id"])
        .ok_or("计算结果资产缺失")?;
    let computed = store
        .read_asset(account, &output.id, &output.versions[0].id)
        .await?;
    let decoded = image::load_from_memory(&computed)?.to_rgb8();
    assert_eq!(decoded.dimensions(), (12, 9));
    assert_eq!(*decoded.get_pixel(0, 0), image::Rgb([235, 195, 155]));
    let original = executions[0]["outputs"]
        .as_array()
        .ok_or("任务输出缺失")?
        .iter()
        .find(|output| output["key"] == "out-1")
        .ok_or("原图输出缺失")?;
    let original_bytes = store
        .read_asset(
            account,
            original["asset_id"].as_str().ok_or("原图资产缺失")?,
            original["asset_version_id"]
                .as_str()
                .ok_or("原图版本缺失")?,
        )
        .await?;
    assert_eq!(
        *image::load_from_memory(&original_bytes)?
            .to_rgb8()
            .get_pixel(0, 0),
        image::Rgb([20, 60, 100])
    );
    assert!(
        detail["flow"]["nodes"]
            .as_array()
            .ok_or("Flow 节点缺失")?
            .iter()
            .any(|node| node["outputs"][0]["asset_id"] == output.id)
    );
    assert!(
        detail["flow"]["edges"]
            .as_array()
            .ok_or("Flow 连线缺失")?
            .iter()
            .any(|edge| edge["target_input_key"] == "image")
    );
    let task = executions[0]["task_id"].clone();
    Ok((session.to_owned(), task, response, asset.id))
}

async fn check_workflow_cancellation(
    store: &Store,
    connection: &sqlx::SqlitePool,
    account: &str,
    workflow_id: &str,
    response: &Value,
    asset_id: &str,
) -> Result<(), Box<dyn std::error::Error>> {
    let runtime = AgentRuntime::new(store.clone());
    let next = store.submit_message(account, json!({
        "request_id": uuid::Uuid::new_v4().to_string(), "permission_mode": "full_access",
        "text": format!("请立即调用真实平台工作流 {workflow_id} 的 run_workflow 工具获取输入表单，参数严格使用 case_id={workflow_id}、inputs={{}}。App 会在真正提交之前显示输入卡片，我会在卡片中选择图片。不要调用其他工具，不要自行填写任何输入。"),
    })).await?;
    let next_run = next["run"]["id"].as_str().ok_or("第二次运行标识缺失")?;
    let next_session = next["session"]["id"].as_str().ok_or("第二个会话标识缺失")?;
    let next_waiting = runtime.execute(account, next_run).await?;
    assert_eq!(
        next_waiting["status"], "waiting_clarification",
        "{}",
        next_waiting["error_message"]
    );
    let next_pending = store.session_detail(account, next_session).await?;
    let mut next_response = response.clone();
    next_response["interruptId"] = next_pending["pending_clarifications"][0]["id"].clone();
    store
        .resolve_run_interrupt(account, next_session, &next_response)
        .await?;
    let mut next_running = Box::pin(runtime.execute(account, next_run));
    let next_task = tokio::select! {
        outcome = &mut next_running => return Err(format!("第二个工作流在取消前结束：{}", outcome?).into()),
        task = tokio::time::timeout(std::time::Duration::from_secs(15), async {
            loop {
                let task: Option<String> = sqlx::query_scalar("SELECT json_extract(document,'$.task_id') FROM workflow_executions WHERE account_id=? AND run_id=? AND coalesce(json_extract(document,'$.task_id'),'')<>''")
                    .bind(account).bind(next_run).fetch_optional(connection).await?;
                if let Some(task) = task { return Ok::<_, sqlx::Error>(task); }
                tokio::time::sleep(std::time::Duration::from_millis(5)).await;
            }
        }) => task??,
    };
    runtime.cancel(account, next_run).await?;
    assert_eq!(next_running.await?["status"], "cancelled");
    let next_detail = store.session_detail(account, next_session).await?;
    assert!(
        next_detail["assets"]
            .as_array()
            .ok_or("会话资产缺失")?
            .iter()
            .any(|reference| reference["id"] == asset_id)
    );
    let remote = next_detail["workflow_executions"]
        .as_array()
        .ok_or("工作流记录缺失")?
        .iter()
        .find(|execution| execution["task_id"] == next_task)
        .ok_or("取消后的远程任务记录缺失")?;
    assert_ne!(remote["status"], "cancelled");
    let continued = store.submit_message(account, json!({
        "session_id": next_session, "request_id": uuid::Uuid::new_v4().to_string(), "permission_mode": "full_access",
        "text": "我已结束本地等待，请只回复已收到，不要调用工具。",
    })).await?;
    let continued_run = continued["run"]["id"].as_str().ok_or("后续运行标识缺失")?;
    let continued_outcome = runtime.execute(account, continued_run).await?;
    assert_eq!(
        continued_outcome["status"], "succeeded",
        "{}",
        continued_outcome["error_message"]
    );
    let completed_remote = tokio::time::timeout(std::time::Duration::from_secs(30), async {
        loop {
            let detail = store.session_detail(account, next_session).await?;
            let execution = &detail["workflow_executions"][0];
            if execution["status"] != "submitted" {
                return Ok::<_, pixoma_studio::Error>(execution.clone());
            }
            tokio::time::sleep(std::time::Duration::from_millis(100)).await;
        }
    })
    .await??;
    assert_eq!(completed_remote["status"], "succeeded");
    assert_eq!(
        store.get_run(account, next_run).await?["status"],
        "cancelled"
    );
    Ok(())
}

async fn check_expert_workflows(
    store: &Store,
    runtime: &AgentRuntime,
    connection: &sqlx::SqlitePool,
    account: &str,
    workflow_id: &str,
    response: &Value,
) -> Result<(), Box<dyn std::error::Error>> {
    let model_id: String = sqlx::query_scalar(
        "SELECT id FROM model_configs WHERE account_id=? AND json_extract(document,'$.default')=1",
    )
    .bind(account)
    .fetch_one(connection)
    .await?;
    let expert = store.save_expert(account, None, json!({
        "schema_version": 2, "name": "图片处理", "description": "执行图片工作流并确认结果", "model_id": model_id,
        "prompt_parts": [{ "type": "text", "text": format!("严格通过专家工具管理任务。用户明确要求开始处理时，先用 update_expert_task create_task 创建任务，再用 start_step 为 process 步骤保存本轮真实 authorization_message_id 和 inputs={{}}。每次模型输出只调用一个专家控制工具。随后立即调用工作流 {workflow_id} 的 run_workflow，参数严格使用 case_id={workflow_id}、inputs={{}}。App 会显示图片输入卡片，等待用户选择。处理完成后使用工具返回的 workflow_execution_id 及资产版本，在 process 步骤展示 result_review 卡片。用户 accept 以后才保存步骤和任务完成状态。") }],
        "avatar_asset_id": "", "avatar_version_id": "", "examples": [], "skill_refs": [], "workflow_refs": [],
        "sop": [{ "id": "process", "title": "处理图片", "instruction_parts": [{ "type": "text", "text": "执行真实工作流，保存处理结果并等待用户确认。" }] }],
        "version": "1.0.0", "enabled": true,
    })).await?;
    let expert_turn = store.submit_message(account, json!({
        "request_id": uuid::Uuid::new_v4().to_string(), "permission_mode": "full_access",
        "text": "现在开始处理图片，授权开始 process 步骤。请按顺序执行：调用 update_expert_task create_task；调用 start_step 并使用本轮真实 authorization_message_id；调用 run_workflow 且 inputs={}，显示工作流图片输入卡片。每次只调用一个工具，不要提前查询 MCP 工作流。",
        "expert_id": expert["id"], "expert_version": "1.0.0",
    })).await?;
    let expert_run = expert_turn["run"]["id"]
        .as_str()
        .ok_or("专家运行标识缺失")?;
    let expert_session = expert_turn["session"]["id"]
        .as_str()
        .ok_or("专家会话标识缺失")?;
    let expert_waiting = runtime.execute(account, expert_run).await?;
    assert_eq!(
        expert_waiting["status"], "waiting_clarification",
        "{}",
        expert_waiting["error_message"]
    );
    let expert_pending = store.session_detail(account, expert_session).await?;
    let attempt = expert_pending["expert_tasks"][0]["steps"][0]["latest_attempt_id"].clone();
    let mut expert_response = response.clone();
    expert_response["interruptId"] = expert_pending["pending_clarifications"][0]["id"].clone();
    store
        .resolve_run_interrupt(account, expert_session, &expert_response)
        .await?;
    let expert_outcome = runtime.execute(account, expert_run).await?;
    assert_eq!(
        expert_outcome["status"], "succeeded",
        "{}",
        expert_outcome["error_message"]
    );
    let expert_detail = store.session_detail(account, expert_session).await?;
    let execution = &expert_detail["workflow_executions"][0];
    assert_eq!(execution["expert_step_attempt_id"], attempt);
    assert_eq!(
        execution["expert_task_id"],
        expert_detail["expert_tasks"][0]["id"]
    );
    let stored_attempt: String =
        sqlx::query_scalar("SELECT document FROM expert_step_attempts WHERE account_id=? AND id=?")
            .bind(account)
            .bind(attempt.as_str().ok_or("专家处理记录缺失")?)
            .fetch_one(connection)
            .await?;
    let stored_attempt: Value = serde_json::from_str(&stored_attempt)?;
    assert!(
        stored_attempt["result"]["workflow_execution_ids"]
            .as_array()
            .ok_or("专家工作流结果缺失")?
            .contains(&execution["id"])
    );
    let expert_assets = stored_attempt["result"]["assets"]
        .as_array()
        .ok_or("专家资产结果缺失")?;
    for output in execution["outputs"]
        .as_array()
        .ok_or("专家工作流输出缺失")?
    {
        assert!(expert_assets.contains(
            &json!({ "asset_id": output["asset_id"], "asset_version_id": output["asset_version_id"] })
        ));
    }
    for reference in expert_assets {
        store
            .read_asset(
                account,
                reference["asset_id"].as_str().ok_or("结果资产标识缺失")?,
                reference["asset_version_id"]
                    .as_str()
                    .ok_or("结果资产版本缺失")?,
            )
            .await?;
    }
    let card = expert_detail["expert_cards"]
        .as_array()
        .ok_or("专家卡片缺失")?
        .iter()
        .find(|card| card["status"] == "pending");
    if card.is_none() {
        let calls: Vec<(String, String, Option<String>)> = sqlx::query_as("SELECT call,status,result FROM tool_calls WHERE account_id=? AND run_id=? ORDER BY turn,part")
            .bind(account).bind(expert_run).fetch_all(connection).await?;
        eprintln!("专家工具记录：{}", serde_json::to_string_pretty(&calls)?);
        eprintln!(
            "专家消息记录：{}",
            serde_json::to_string_pretty(&expert_detail["messages"])?
        );
        eprintln!(
            "专家任务记录：{}",
            serde_json::to_string_pretty(&expert_detail["expert_tasks"])?
        );
        eprintln!(
            "专家处理记录：{}",
            serde_json::to_string_pretty(&stored_attempt)?
        );
        return Err("工作流结果卡片缺失".into());
    }
    let card = card.ok_or("工作流结果卡片缺失")?;
    assert_eq!(card["expert_card"]["mode"], "result_review");
    let accepted = store
        .resolve_expert_card_response(
            account,
            card["id"].as_str().ok_or("结果卡片标识缺失")?,
            json!({ "response_id": uuid::Uuid::new_v4().to_string(), "action": "accept" }),
        )
        .await?;
    let accepted_outcome = runtime
        .execute(account, accepted["id"].as_str().ok_or("确认运行标识缺失")?)
        .await?;
    assert_eq!(
        accepted_outcome["status"], "succeeded",
        "{}",
        accepted_outcome["error_message"]
    );
    assert_eq!(
        store.session_detail(account, expert_session).await?["expert_tasks"][0]["status"],
        "completed"
    );
    let failed_workflow = std::env::var("PIXOMA_TEST_FAILED_WORKFLOW_ID")?;
    let failed_turn = store.submit_message(account, json!({
        "request_id": uuid::Uuid::new_v4().to_string(), "permission_mode": "full_access",
        "text": format!("现在开始处理图片，授权 process 步骤。本次指定工作流 {failed_workflow}。请依次调用 update_expert_task create_task、start_step，再调用 run_workflow，参数严格使用 case_id={failed_workflow}、inputs={{}}。App 会显示图片输入卡片。每次只调用一个工具。"),
        "expert_id": expert["id"], "expert_version": "1.0.0",
    })).await?;
    let failed_run = failed_turn["run"]["id"]
        .as_str()
        .ok_or("失败检查运行标识缺失")?;
    let failed_session = failed_turn["session"]["id"]
        .as_str()
        .ok_or("失败检查会话标识缺失")?;
    let failed_waiting = Box::pin(runtime.execute(account, failed_run)).await?;
    assert_eq!(
        failed_waiting["status"], "waiting_clarification",
        "{}",
        failed_waiting["error_message"]
    );
    let failed_pending = store.session_detail(account, failed_session).await?;
    assert_eq!(
        failed_pending["pending_clarifications"][0]["metadata"]["workflow"]["id"],
        failed_workflow
    );
    let mut failed_response = response.clone();
    failed_response["interruptId"] = failed_pending["pending_clarifications"][0]["id"].clone();
    store
        .resolve_run_interrupt(account, failed_session, &failed_response)
        .await?;
    let failed_outcome = tokio::time::timeout(
        std::time::Duration::from_secs(90),
        Box::pin(runtime.execute(account, failed_run)),
    )
    .await??;
    assert_eq!(failed_outcome["status"], "failed");
    let failed_detail = store.session_detail(account, failed_session).await?;
    assert_eq!(failed_detail["workflow_executions"][0]["status"], "failed");
    assert!(
        !failed_detail["workflow_executions"][0]["error_message"]
            .as_str()
            .ok_or("远程错误缺失")?
            .is_empty()
    );
    assert_eq!(
        failed_detail["expert_tasks"][0]["steps"][0]["status"],
        "failed"
    );
    assert_eq!(failed_detail["expert_tasks"][0]["status"], "active");
    let failed_attempt: String =
        sqlx::query_scalar("SELECT document FROM expert_step_attempts WHERE account_id=? AND id=?")
            .bind(account)
            .bind(
                failed_detail["expert_tasks"][0]["steps"][0]["latest_attempt_id"]
                    .as_str()
                    .ok_or("失败处理记录缺失")?,
            )
            .fetch_one(connection)
            .await?;
    let failed_attempt: Value = serde_json::from_str(&failed_attempt)?;
    assert_eq!(failed_attempt["status"], "failed");
    assert!(failed_attempt["completed_at"].as_str().is_some());
    Ok(())
}

async fn check_unknown_submission(
    store: &Store,
    runtime: &AgentRuntime,
    connection: &sqlx::SqlitePool,
    account: &str,
    workflow_id: &str,
    response: &Value,
) -> Result<(), Box<dyn std::error::Error>> {
    let unknown_turn = store.submit_message(account, json!({
        "request_id": uuid::Uuid::new_v4().to_string(), "permission_mode": "full_access",
        "text": format!("请调用工作流 {workflow_id} 的 run_workflow，参数严格使用 case_id={workflow_id}、inputs={{}}，等待我填写图片输入卡片。不要调用其他工具。"),
    })).await?;
    let unknown_run = unknown_turn["run"]["id"]
        .as_str()
        .ok_or("提交检查运行标识缺失")?;
    let unknown_session = unknown_turn["session"]["id"]
        .as_str()
        .ok_or("提交检查会话标识缺失")?;
    assert_eq!(
        Box::pin(runtime.execute(account, unknown_run)).await?["status"],
        "waiting_clarification"
    );
    let unknown_pending = store.session_detail(account, unknown_session).await?;
    let mut unknown_response = response.clone();
    unknown_response["interruptId"] = unknown_pending["pending_clarifications"][0]["id"].clone();
    store
        .resolve_run_interrupt(account, unknown_session, &unknown_response)
        .await?;
    sqlx::query("CREATE TRIGGER workflow_task_persistence_failure BEFORE UPDATE OF document ON workflow_executions WHEN coalesce(json_extract(OLD.document,'$.task_id'),'')='' AND coalesce(json_extract(NEW.document,'$.task_id'),'')<>'' BEGIN SELECT RAISE(ABORT,'任务编号写入失败检查'); END")
        .execute(connection).await?;
    let unknown_outcome = Box::pin(runtime.execute(account, unknown_run)).await?;
    sqlx::query("DROP TRIGGER workflow_task_persistence_failure")
        .execute(connection)
        .await?;
    assert_eq!(unknown_outcome["status"], "failed");
    let unknown_detail = store.session_detail(account, unknown_session).await?;
    let unknown_execution = &unknown_detail["workflow_executions"][0];
    assert_eq!(unknown_execution["submission_state"], "result_unknown");
    assert_eq!(unknown_execution["task_id"], "");
    let candidates = unknown_execution["reconciliation"]["tasks"]
        .as_array()
        .ok_or("真实任务核对结果缺失")?;
    assert_eq!(candidates.len(), 4);
    assert!(candidates.iter().all(|task| {
        task["case_id"].as_u64().map(|id| id.to_string()).as_deref() == Some(workflow_id)
    }));
    assert_eq!(unknown_outcome["error_code"], "tool_result_unknown");
    assert_eq!(
        Box::pin(runtime.execute(account, unknown_run)).await?["status"],
        "failed"
    );
    assert!(
        store
            .reconcile_workflow_submission(
                "other-account",
                unknown_execution["id"].as_str().ok_or("执行标识缺失")?
            )
            .await
            .is_err()
    );
    let repeated = LocalApi::new(store.clone())
        .request(
            account,
            LocalRequest {
                method: "POST".to_owned(),
                path: format!(
                    "/api/v1/studio/workflow-executions/{}/reconcile",
                    unknown_execution["id"].as_str().ok_or("执行标识缺失")?
                ),
                body: None,
            },
        )
        .await?;
    assert_eq!(
        repeated["reconciliation"]["tasks"]
            .as_array()
            .ok_or("重复核对任务缺失")?
            .len(),
        4
    );
    assert_eq!(repeated["task_id"], "");
    Ok(())
}

#[tokio::test]
#[ignore = "需要真实平台、模型、MCP 和专用账号的环境变量"]
async fn real_workflow_missing_inputs_are_saved_before_remote_submission()
-> Result<(), Box<dyn std::error::Error>> {
    let platform = PlatformClient::new(&std::env::var("PIXOMA_TEST_PLATFORM_URL")?)?;
    let password = std::env::var("PIXOMA_TEST_STUDIO_PASSWORD")?;
    platform.request_auth("POST", "login", Some(json!({
        "username": std::env::var("PIXOMA_TEST_STUDIO_USERNAME")?, "password": password, "remember_me": false,
    }))).await?;
    platform
        .request_auth(
            "POST",
            "password",
            Some(json!({
                "current_password": password, "new_password": uuid::Uuid::new_v4().to_string(),
            })),
        )
        .await?;
    let account = platform.account_scope().await?;
    let catalog = platform
        .request_catalog("/api/v1/studio/workflow-market")
        .await?;
    let workflow = catalog
        .as_array()
        .ok_or("工作流目录缺失")?
        .iter()
        .find(|workflow| workflow["id"] == "5")
        .ok_or("真实平台缺少文生视频工作流")?;
    let workflow_id = workflow["id"].as_str().ok_or("工作流标识缺失")?;
    let detail = platform
        .request_catalog(&format!("/api/v1/studio/workflow-market/{workflow_id}"))
        .await?;
    assert!(
        detail["input_fields"]
            .as_array()
            .ok_or("真实工作流字段缺失")?
            .iter()
            .any(|field| field["required"] == true)
    );
    let root = std::path::PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../.local-cache");
    std::fs::create_dir_all(&root)?;
    let directory = tempfile::Builder::new()
        .prefix("workflow-input-")
        .tempdir_in(root)?;
    let database = directory.path().join("studio.sqlite");
    let store = Store::open(&database).await?;
    let api = LocalApi::new(store.clone());
    let configuration: Value = serde_json::from_str(&std::env::var("PIXOMA_TEST_MODEL_CONFIG")?)?;
    api.request(
        &account,
        LocalRequest {
            method: "POST".to_owned(),
            path: "/api/v1/studio-operations/models".to_owned(),
            body: Some(configuration),
        },
    )
    .await?;
    let connector = api
        .request(
            &account,
            LocalRequest {
                method: "POST".to_owned(),
                path: "/api/v1/studio-operations/connectors".to_owned(),
                body: Some(json!({
                    "name": "真实平台工作流", "url": std::env::var("PIXOMA_TEST_MCP_URL")?,
                    "credential": std::env::var("PIXOMA_TEST_MCP_TOKEN")?, "enabled": true, "policy": "auto",
                })),
            },
        )
        .await?;
    let connection = sqlx::SqlitePool::connect(&format!("sqlite://{}", database.display())).await?;
    let references: Vec<String> = sqlx::query_scalar("SELECT credential_ref FROM model_configs WHERE account_id=? UNION SELECT credential_ref FROM connectors WHERE account_id=?")
        .bind(&account).bind(&account).fetch_all(&connection).await?;
    let _credentials = TestCredentials(
        references
            .iter()
            .map(|reference| keyring::Entry::new("com.pixoma.studio", reference))
            .collect::<Result<_, _>>()?,
    );
    connection.close().await;
    let input = json!({
        "request_id": uuid::Uuid::new_v4().to_string(), "permission_mode": "full_access",
        "text": format!("请调用真实平台工作流 {workflow_id} 的 run_workflow 工具获取输入表单，参数严格使用 case_id={workflow_id}、inputs={{}}。不要自行填写任何输入，等待我填写表单。"),
        "parts": [{ "type": "workflow_ref", "workflow_id": workflow_id, "version_id": detail["version_id"] }],
    });
    platform.prepare_capabilities(&store, &input).await?;
    let queued = store.submit_message(&account, input.clone()).await?;
    let run_id = queued["run"]["id"].as_str().ok_or("运行标识缺失")?;
    let session_id = queued["session"]["id"].as_str().ok_or("会话标识缺失")?;
    let outcome = AgentRuntime::new(store.clone())
        .execute(&account, run_id)
        .await?;
    assert_eq!(
        outcome["status"], "waiting_clarification",
        "{}",
        outcome["error_message"]
    );
    let saved = store.session_detail(&account, session_id).await?;
    let pending = saved["pending_clarifications"]
        .as_array()
        .ok_or("输入表单缺失")?;
    assert_eq!(pending.len(), 1);
    assert_eq!(pending[0]["reason"], "workflow_input");
    assert_eq!(pending[0]["metadata"]["workflow"]["id"], workflow_id);
    assert_eq!(
        pending[0]["metadata"]["workflow"]["input_schema"],
        detail["input_schema"]
    );
    assert!(
        saved["workflow_executions"]
            .as_array()
            .ok_or("工作流记录缺失")?
            .is_empty()
    );
    store.close().await;
    let reopened = Store::open(&database).await?;
    assert_eq!(reopened.recover_interrupted_runs().await?, 0);
    assert_eq!(
        reopened.session_detail(&account, session_id).await?["pending_clarifications"],
        saved["pending_clarifications"]
    );
    let interrupt_id = pending[0]["id"].as_str().ok_or("输入表单标识缺失")?;
    let cancel = json!({ "interruptId": interrupt_id, "status": "cancelled" });
    assert!(
        reopened
            .resolve_run_interrupt("other-account", session_id, &cancel)
            .await
            .is_err()
    );
    assert!(
        reopened
            .resolve_run_interrupt(
                &account,
                session_id,
                &json!({
                    "interruptId": interrupt_id, "status": "resolved", "payload": { "inputs": {} },
                })
            )
            .await
            .is_err()
    );
    let (first, repeated) = tokio::join!(
        reopened.resolve_run_interrupt(&account, session_id, &cancel),
        reopened.resolve_run_interrupt(&account, session_id, &cancel),
    );
    assert_eq!(first?["id"], repeated?["id"]);
    let outcome = AgentRuntime::new(reopened.clone())
        .execute(&account, run_id)
        .await?;
    assert_eq!(outcome["status"], "cancelled");
    assert_eq!(
        reopened.get_run(&account, run_id).await?["status"],
        "cancelled"
    );
    assert!(
        reopened.session_detail(&account, session_id).await?["pending_clarifications"]
            .as_array()
            .ok_or("输入表单缺失")?
            .is_empty()
    );
    let mut next = input;
    next["request_id"] = json!(uuid::Uuid::new_v4().to_string());
    let next = reopened.submit_message(&account, next).await?;
    let next_run = next["run"]["id"].as_str().ok_or("运行标识缺失")?;
    let next_session = next["session"]["id"].as_str().ok_or("会话标识缺失")?;
    let runtime = AgentRuntime::new(reopened.clone());
    assert_eq!(
        runtime.execute(&account, next_run).await?["status"],
        "waiting_clarification"
    );
    let next_detail = reopened.session_detail(&account, next_session).await?;
    let next_input = next_detail["pending_clarifications"][0]["id"]
        .as_str()
        .ok_or("输入卡片缺失")?;
    LocalApi::new(reopened.clone())
        .request(&account, LocalRequest {
            method: "PATCH".to_owned(), path: format!("/api/v1/studio-operations/connectors/{}", connector["id"].as_str().ok_or("连接器标识缺失")?),
            body: Some(json!({ "name": "真实平台工作流", "url": std::env::var("PIXOMA_TEST_MCP_URL")?, "credential": "", "enabled": true, "policy": "approval" })),
        })
        .await?;
    let submitted_inputs = json!({ "提示词": "海边的日出", "视频分辨率": "0.4", "视频长度": "1" });
    reopened
        .resolve_run_interrupt(
            &account,
            next_session,
            &json!({
                "interruptId": next_input, "status": "resolved", "payload": { "inputs": submitted_inputs },
            }),
        )
        .await?;
    assert_eq!(
        runtime.execute(&account, next_run).await?["status"],
        "waiting_approval"
    );
    let approved_detail = reopened.session_detail(&account, next_session).await?;
    let approval = &approved_detail["pending_approvals"][0];
    assert_eq!(
        approval["metadata"]["arguments"]["inputs"],
        submitted_inputs
    );
    reopened
        .resolve_run_interrupt(
            &account,
            next_session,
            &json!({
                "interruptId": approval["id"], "status": "resolved", "payload": true,
            }),
        )
        .await?;
    let connection = sqlx::SqlitePool::connect(&format!("sqlite://{}", database.display())).await?;
    let arguments: String = sqlx::query_scalar(
        "SELECT approved_arguments FROM tool_calls WHERE account_id=? AND run_id=? AND id=?",
    )
    .bind(&account)
    .bind(next_run)
    .bind(
        approval["metadata"]["toolCallId"]
            .as_str()
            .ok_or("工具调用标识缺失")?,
    )
    .fetch_one(&connection)
    .await?;
    assert_eq!(
        serde_json::from_str::<Value>(&arguments)?["inputs"],
        submitted_inputs
    );
    connection.close().await;
    runtime.cancel(&account, next_run).await?;
    reopened.close().await;
    platform.request_auth("POST", "logout", None).await?;
    Ok(())
}
