use pixoma_studio::Store;
use serde_json::{Value, json};
use std::path::PathBuf;

#[tokio::test]
async fn executions_keep_one_attempt_and_verified_output_after_reopening()
-> Result<(), Box<dyn std::error::Error>> {
    let root = PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../.local-cache");
    std::fs::create_dir_all(&root)?;
    let directory = tempfile::Builder::new()
        .prefix("executions-")
        .tempdir_in(root)?;
    let path = directory.path().join("studio.sqlite");
    let store = Store::open(&path).await?;
    let node = store
        .create_compute_node(json!({
            "name": "执行节点", "location": "local", "enabled": true,
            "comfyui_base_url": "http://127.0.0.1:8188"
        }))
        .await?;
    let token = node["agent_token"].as_str().ok_or("节点凭据缺失")?;
    let mut definition = super::workflow_definitions::comfy_definition();
    definition["execution"]["node_id"] = node["node"]["id"].clone();
    let workflow = store
        .create_workflow(json!({
            "name": "Klein9B", "category": "image", "enabled": true, "draft": definition
        }))
        .await?;
    let workflow_id = workflow["id"].as_str().ok_or("工作流标识缺失")?;
    let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await?;
    let base = format!("http://{}", listener.local_addr()?);
    let router = pixoma_studio::compute_router(store.clone());
    let server = tokio::spawn(async move { axum::serve(listener, router).await });
    let client = reqwest::Client::builder().no_proxy().build()?;
    let presence = json!({
        "protocol_version": 1, "agent_version": "0.1.0",
        "agent_instance_id": uuid::Uuid::new_v4().to_string(),
        "started_id": uuid::Uuid::new_v4().to_string(), "configuration_revision": 1,
        "comfyui": { "ready": true, "object_info": { "SaveImage": {} } }
    });
    client
        .post(format!("{base}/agent/v1/presence"))
        .bearer_auth(token)
        .json(&presence)
        .send()
        .await?
        .error_for_status()?;
    let source = store
        .create_asset_bytes(
            None,
            "工作流.json",
            include_bytes!("data/klein9b-api.json"),
            "user",
            "input-format-check",
        )
        .await?;
    let mut image_definition = super::workflow_definitions::comfy_definition();
    image_definition["execution"]["node_id"] = node["node"]["id"].clone();
    image_definition["inputs"][0]["type"] = json!("image");
    image_definition["input_schema"]["properties"]["prompt"] = json!({
        "type": "object", "properties": { "asset_id": { "type": "string" }, "asset_version_id": { "type": "string" } },
        "required": ["asset_id", "asset_version_id"], "additionalProperties": false
    });
    let image_workflow = store
        .create_workflow(json!({
            "name": "输入格式检查", "category": "image", "enabled": true, "draft": image_definition
        }))
        .await?;
    let format_error = store
        .create_local_execution(json!({
            "request_id": "invalid-image-input", "workflow_id": image_workflow["id"], "draft_revision": 1,
            "inputs": { "prompt": { "asset_id": source.id, "asset_version_id": source.versions[0].id } }
        }))
        .await
        .expect_err("JSON 文件不能作为图片输入");
    assert!(format_error.to_string().contains("prompt"));
    let request = json!({
        "request_id": "generate-one", "workflow_id": workflow_id, "draft_revision": 1,
        "inputs": { "prompt": "一只橘猫坐在窗边" }
    });
    let execution = store.create_local_execution(request.clone()).await?;
    let id = execution["id"].as_str().ok_or("执行标识缺失")?.to_owned();
    assert_eq!(
        store.create_local_execution(request.clone()).await?["id"],
        id
    );
    let mut changed = request;
    changed["inputs"]["prompt"] = json!("另一幅画面");
    assert!(store.create_local_execution(changed).await.is_err());
    let cancelled = store
        .create_local_execution(json!({
            "request_id": "cancel-before-claim", "workflow_id": workflow_id,
            "draft_revision": 1, "inputs": { "prompt": "取消前的输入" }
        }))
        .await?;
    let cancelled_id = cancelled["id"].as_str().ok_or("执行标识缺失")?;
    let cancelled = store.cancel_local_execution(cancelled_id, false).await?;
    assert_eq!(cancelled["status"], "cancelled");
    assert!(cancelled["attempt_id"].is_null());
    let claim_url = format!("{base}/agent/v1/jobs/claim");
    let (first, second) = tokio::join!(
        client.get(&claim_url).bearer_auth(token).send(),
        client.get(&claim_url).bearer_auth(token).send()
    );
    let first: Value = first?.error_for_status()?.json().await?;
    let second: Value = second?.error_for_status()?.json().await?;
    assert_eq!(first, second);
    let attempt = first["attempt_id"].as_str().ok_or("尝试标识缺失")?;
    let package: Value = client
        .get(format!(
            "{base}{}",
            first["package_url"].as_str().ok_or("任务包地址缺失")?
        ))
        .bearer_auth(token)
        .send()
        .await?
        .error_for_status()?
        .json()
        .await?;
    assert_eq!(
        package["workflow"]["40"]["inputs"]["text"],
        "一只橘猫坐在窗边"
    );
    let other_node = store
        .create_compute_node(json!({
            "name": "其他节点", "location": "local", "enabled": true,
            "comfyui_base_url": "http://127.0.0.1:8188"
        }))
        .await?;
    assert_eq!(
        client
            .get(format!("{base}/agent/v1/executions/{id}/package"))
            .bearer_auth(other_node["agent_token"].as_str().ok_or("节点凭据缺失")?)
            .send()
            .await?
            .status(),
        403
    );
    let event_url = format!("{base}/agent/v1/executions/{id}/events");
    for (index, kind, payload) in [
        (
            1,
            "accepted",
            json!({ "content_digest": package["content_digest"] }),
        ),
        (
            2,
            "submission_started",
            json!({ "content_digest": package["content_digest"] }),
        ),
        (
            3,
            "submitted",
            json!({ "prompt_id": "persisted-prompt-id" }),
        ),
    ] {
        let event =
            json!({ "attempt_id": attempt, "sequence": index, "kind": kind, "payload": payload });
        client
            .post(&event_url)
            .bearer_auth(token)
            .json(&event)
            .send()
            .await?
            .error_for_status()?;
        client
            .post(&event_url)
            .bearer_auth(token)
            .json(&event)
            .send()
            .await?
            .error_for_status()?;
    }
    let conflicting = json!({ "attempt_id": attempt, "sequence": 3, "kind": "submitted", "payload": { "prompt_id": "different" } });
    let stopped = store.cancel_local_execution(&id, true).await?;
    assert_eq!(stopped["status"], "running");
    assert_eq!(stopped["waiting"], false);
    assert_eq!(stopped["cancel_requested"], false);
    assert_eq!(
        client
            .post(&event_url)
            .bearer_auth(token)
            .json(&conflicting)
            .send()
            .await?
            .status(),
        409
    );
    let complete_url = format!("{base}/agent/v1/executions/{id}/complete");
    assert_eq!(
        client
            .post(&complete_url)
            .bearer_auth(token)
            .json(&json!({ "attempt_id": attempt, "status": "succeeded", "outputs": [] }))
            .send()
            .await?
            .status(),
        409
    );
    let bytes = include_bytes!("../../public/images/pixoma-ghost.png");
    use sha2::{Digest, Sha256};
    let hash = format!("{:x}", Sha256::digest(bytes));
    let output_url = format!("{base}/agent/v1/executions/{id}/outputs/image-0");
    let upload = |sha256: &str| {
        client
            .put(&output_url)
            .bearer_auth(token)
            .header("x-attempt-id", attempt)
            .header("x-output-field", "image")
            .header("x-output-index", "0")
            .header("x-content-sha256", sha256)
            .header("content-type", "image/png")
            .body(bytes.to_vec())
    };
    assert_eq!(upload(&"0".repeat(64)).send().await?.status(), 400);
    assert_eq!(
        store.get_local_execution(&id).await?["outputs"]
            .as_array()
            .ok_or("输出列表缺失")?
            .len(),
        0
    );
    let output: Value = upload(&hash)
        .send()
        .await?
        .error_for_status()?
        .json()
        .await?;
    assert_eq!(
        output,
        upload(&hash)
            .send()
            .await?
            .error_for_status()?
            .json::<Value>()
            .await?
    );
    let complete = json!({ "attempt_id": attempt, "status": "succeeded", "outputs": [{ "output_id": "image-0", "sha256": hash }] });
    client
        .post(&complete_url)
        .bearer_auth(token)
        .json(&complete)
        .send()
        .await?
        .error_for_status()?;
    client
        .post(&complete_url)
        .bearer_auth(token)
        .json(&complete)
        .send()
        .await?
        .error_for_status()?;
    assert_eq!(
        client
            .get(claim_url)
            .bearer_auth(token)
            .send()
            .await?
            .status(),
        204
    );
    server.abort();
    let _ = server.await;
    let token = token.to_owned();
    let attempt = attempt.to_owned();
    store.close().await;
    let reopened = Store::open(&path).await?;
    let node_id = node["node"]["id"].as_str().ok_or("节点标识缺失")?;
    let node_after_restart = reopened.get_compute_node(node_id).await?;
    assert_eq!(node_after_restart["healthy"], false);
    assert_eq!(node_after_restart["connected"], false);
    let result = reopened.get_local_execution(&id).await?;
    assert_eq!(result["status"], "succeeded");
    assert_eq!(result["attempt_id"], attempt);
    assert_eq!(result["prompt_id"], "persisted-prompt-id");
    assert_eq!(result["outputs"].as_array().ok_or("输出列表缺失")?.len(), 1);
    let saved = &result["outputs"][0];
    assert_eq!(
        reopened
            .read_asset(
                saved["asset_id"].as_str().ok_or("资产标识缺失")?,
                saved["asset_version_id"].as_str().ok_or("资产版本缺失")?
            )
            .await?,
        bytes
    );
    assert!(reopened.authenticate_compute_node(&token).await.is_ok());
    reopened.close().await;
    Ok(())
}
