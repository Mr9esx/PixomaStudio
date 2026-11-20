use pixoma_studio::Store;
use serde_json::json;
use std::path::PathBuf;

fn directory() -> Result<tempfile::TempDir, std::io::Error> {
    let root = PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../.local-cache");
    std::fs::create_dir_all(&root)?;
    tempfile::Builder::new()
        .prefix("compute-nodes-")
        .tempdir_in(root)
}

#[tokio::test]
async fn connection_settings_persist_and_report_a_required_restart() -> Result<(), Box<dyn std::error::Error>> {
    let directory = directory()?;
    let path = directory.path().join("studio.sqlite");
    let store = Store::open(&path).await?;
    assert_eq!(store.compute_connection_settings().await?["remote_access"], false);
    let saved = store.save_compute_connection(json!({ "listen_address": "0.0.0.0:18093", "public_url": "http://192.168.31.10:18093" })).await?;
    assert_eq!(saved["restart_required"], true);
    assert_eq!(saved["local_url"], "http://127.0.0.1:18083");
    assert!(store.save_compute_connection(json!({ "listen_address": "0.0.0.0:0", "public_url": "" })).await.is_err());
    assert!(store.save_compute_connection(json!({ "listen_address": "127.0.0.1:18083", "public_url": "file:///etc/passwd" })).await.is_err());
    store.close().await;
    let store = Store::open(&path).await?;
    let settings = store.compute_connection_settings().await?;
    assert_eq!(settings["listen_address"], "0.0.0.0:18093");
    assert_eq!(settings["remote_access"], true);
    assert_eq!(settings["restart_required"], false);
    assert_eq!(settings["public_url"], "http://192.168.31.10:18093");
    store.close().await;
    Ok(())
}

#[tokio::test]
async fn node_observation_exposes_empty_history_and_real_execution_statistics()
-> Result<(), Box<dyn std::error::Error>> {
    use pixoma_studio::{LocalApi, LocalRequest};
    let directory = directory()?;
    let store = Store::open(&directory.path().join("studio.sqlite")).await?;
    let created = store.create_compute_node(json!({ "name": "观察节点", "location": "local", "enabled": true, "comfyui_base_url": "http://127.0.0.1:8188" })).await?;
    let id = created["node"]["id"].as_str().ok_or("节点缺失")?;
    let api = LocalApi::new(store.clone());
    let request = |suffix: &str| LocalRequest { method: "GET".into(), path: format!("/api/v1/studio/compute-nodes/{id}/{suffix}"), body: None };
    let metrics = api.request(request("metrics?window=1h")).await?;
    assert_eq!(metrics["series"], json!([]));
    assert_eq!(metrics["latest"], serde_json::Value::Null);
    assert!(api.request(request("metrics?window=unsupported")).await.is_err());
    let stats = api.request(request("observation")).await?;
    assert_eq!(stats["stats"]["task_count"], 0);
    assert_eq!(stats["stats"]["success_rate"], serde_json::Value::Null);
    let mut definition = super::workflow_definitions::comfy_definition();
    definition["execution"]["node_id"] = json!(id);
    let workflow = store.create_workflow(json!({ "name": "观察工作流", "category": "image", "enabled": true, "draft": definition })).await?;
    let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await?;
    let address = listener.local_addr()?;
    let shutdown = tokio_util::sync::CancellationToken::new();
    let stop = shutdown.clone();
    let router = pixoma_studio::compute_router(store.clone());
    let server = tokio::spawn(async move { axum::serve(listener, router).with_graceful_shutdown(stop.cancelled_owned()).await });
    let response = reqwest::Client::new().post(format!("http://{address}/agent/v1/presence"))
        .bearer_auth(created["agent_token"].as_str().ok_or("凭据缺失")?)
        .json(&json!({ "protocol_version": 1, "agent_version": "0.1.0", "agent_instance_id": uuid::Uuid::new_v4(), "started_id": uuid::Uuid::new_v4(), "configuration_revision": 1, "comfyui": { "ready": true, "object_info": {} } })).send().await?;
    assert_eq!(response.status(), 200);
    let execution = store.create_local_execution(json!({ "request_id": uuid::Uuid::new_v4().to_string(), "workflow_id": workflow["id"], "draft_revision": 1, "inputs": { "prompt": "测试" } })).await?;
    store.cancel_local_execution(execution["id"].as_str().ok_or("执行缺失")?, false).await?;
    let stats = api.request(request("observation")).await?;
    assert_eq!(stats["stats"]["task_count"], 1);
    assert_eq!(stats["executions"][0]["status"], "cancelled");
    assert_eq!(stats["workflows"][0]["id"], workflow["id"]);
    shutdown.cancel();
    server.await??;
    store.close().await;
    Ok(())
}

#[tokio::test]
async fn node_credentials_rotate_and_configuration_survives_reopening()
-> Result<(), Box<dyn std::error::Error>> {
    let directory = directory()?;
    let path = directory.path().join("studio.sqlite");
    let store = Store::open(&path).await?;
    let created = store
        .create_compute_node(json!({
            "name": "本机 ComfyUI", "location": "local", "enabled": true,
            "comfyui_base_url": "http://127.0.0.1:8188"
        }))
        .await?;
    let id = created["node"]["id"].as_str().ok_or("节点标识缺失")?;
    let token = created["agent_token"].as_str().ok_or("节点凭据缺失")?;
    assert_eq!(store.authenticate_compute_node(token).await?, id);
    assert!(
        store
            .authenticate_compute_node("invalid-token")
            .await
            .is_err()
    );
    let node = store.get_compute_node(id).await?;
    assert_eq!(node["healthy"], false);
    assert!(node.get("credential_hash").is_none());
    assert!(!node.to_string().contains(token));
    store
        .update_compute_node(
            id,
            json!({ "expected_revision": 1, "name": "工作站 ComfyUI", "capabilities": ["图片", "视频"], "hardware": { "cpu_model": "设备信息", "cpu_cores": 8, "ram_bytes": 17179869184u64, "gpus": [] } }),
        )
        .await?;
    assert!(
        store
            .update_compute_node(id, json!({ "expected_revision": 1, "enabled": false }))
            .await
            .is_err()
    );
    let rotated = store.rotate_compute_node_credential(id).await?;
    let replacement = rotated["agent_token"].as_str().ok_or("节点凭据缺失")?;
    assert!(store.authenticate_compute_node(token).await.is_err());
    assert_eq!(store.authenticate_compute_node(replacement).await?, id);
    let id = id.to_owned();
    let replacement = replacement.to_owned();
    store.close().await;
    let reopened = Store::open(&path).await?;
    let saved = reopened.get_compute_node(&id).await?;
    assert_eq!(saved["name"], "工作站 ComfyUI");
    assert_eq!(saved["capabilities"], json!(["图片", "视频"]));
    assert_eq!(saved["hardware"]["cpu_cores"], 8);
    assert_eq!(saved["comfyui_base_url"], "http://127.0.0.1:8188");
    assert_eq!(reopened.authenticate_compute_node(&replacement).await?, id);
    let saved_credential = pixoma_studio::LocalApi::new(reopened.clone()).request(pixoma_studio::LocalRequest {
        method: "GET".into(), path: format!("/api/v1/studio/compute-nodes/{id}/credential"), body: None,
    }).await?;
    assert_eq!(saved_credential["agent_token"], replacement);
    assert_eq!(
        reopened
            .list_compute_nodes()
            .await?
            .as_array()
            .ok_or("节点列表缺失")?
            .len(),
        1
    );
    reopened.delete_compute_node(&id).await?;
    assert!(
        reopened
            .authenticate_compute_node(&replacement)
            .await
            .is_err()
    );
    reopened.close().await;
    Ok(())
}

#[tokio::test]
async fn referenced_nodes_require_disabling_and_invalid_addresses_are_rejected()
-> Result<(), Box<dyn std::error::Error>> {
    let directory = directory()?;
    let store = Store::open(&directory.path().join("studio.sqlite")).await?;
    for address in [
        "file:///etc/passwd",
        "http://user:password@localhost:8188",
        "http://localhost:8188/?token=secret",
    ] {
        assert!(
            store
                .create_compute_node(
                    json!({ "name": "无效连接", "location": "remote", "comfyui_base_url": address })
                )
                .await
                .is_err()
        );
    }
    let created = store
        .create_compute_node(json!({ "name": "工作流节点", "location": "remote", "enabled": true, "comfyui_base_url": "http://127.0.0.1:8188" }))
        .await?;
    let id = created["node"]["id"].as_str().ok_or("节点标识缺失")?;
    let mut definition = super::workflow_definitions::comfy_definition();
    definition["execution"]["node_id"] = json!(id);
    let workflow = store
        .create_workflow(json!({ "name": "节点引用", "category": "image", "draft": definition }))
        .await?;
    assert!(store.delete_compute_node(id).await.is_err());
    let disabled = store
        .update_compute_node(id, json!({ "expected_revision": 1, "enabled": false }))
        .await?;
    assert_eq!(disabled["enabled"], false);
    assert_eq!(disabled["healthy"], false);
    store
        .delete_workflow(workflow["id"].as_str().ok_or("工作流标识缺失")?)
        .await?;
    store.delete_compute_node(id).await?;
    store.close().await;
    Ok(())
}

#[tokio::test]
async fn real_http_presence_checks_credentials_protocol_and_agent_ownership()
-> Result<(), Box<dyn std::error::Error>> {
    let directory = directory()?;
    let store = Store::open(&directory.path().join("studio.sqlite")).await?;
    let node = store
        .create_compute_node(json!({ "name": "协议检查节点", "location": "local", "enabled": true, "comfyui_base_url": "http://127.0.0.1:8188" }))
        .await?;
    let id = node["node"]["id"].as_str().ok_or("节点标识缺失")?;
    let token = node["agent_token"].as_str().ok_or("节点凭据缺失")?;
    let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await?;
    let url = format!("http://{}/agent/v1/presence", listener.local_addr()?);
    let router = pixoma_studio::compute_router(store.clone());
    let shutdown = tokio_util::sync::CancellationToken::new();
    let stop = shutdown.clone();
    let server = tokio::spawn(async move {
        axum::serve(listener, router)
            .with_graceful_shutdown(stop.cancelled_owned())
            .await
    });
    let client = reqwest::Client::builder()
        .timeout(std::time::Duration::from_secs(10))
        .build()?;
    let body = json!({
        "protocol_version": 1, "agent_version": env!("CARGO_PKG_VERSION"),
        "agent_instance_id": uuid::Uuid::new_v4().to_string(), "started_id": uuid::Uuid::new_v4().to_string(),
        "configuration_revision": 1, "comfyui": { "ready": false, "error": "" }
    });
    assert_eq!(client.post(&url).json(&body).send().await?.status(), 401);
    let response = client
        .post(&url)
        .bearer_auth(token)
        .json(&body)
        .send()
        .await?;
    assert_eq!(response.status(), 200);
    assert_eq!(response.json::<serde_json::Value>().await?["node_id"], id);
    let saved = store.get_compute_node(id).await?;
    assert_eq!(saved["connected"], true);
    assert_eq!(saved["healthy"], false);
    let mut other = body.clone();
    other["agent_instance_id"] = json!(uuid::Uuid::new_v4().to_string());
    assert_eq!(
        client
            .post(&url)
            .bearer_auth(token)
            .json(&other)
            .send()
            .await?
            .status(),
        409
    );
    let mut unknown = body.clone();
    unknown["protocol_version"] = json!(2);
    assert_eq!(
        client
            .post(&url)
            .bearer_auth(token)
            .json(&unknown)
            .send()
            .await?
            .status(),
        400
    );
    store.rotate_compute_node_credential(id).await?;
    assert_eq!(
        client
            .post(&url)
            .bearer_auth(token)
            .json(&body)
            .send()
            .await?
            .status(),
        401
    );
    shutdown.cancel();
    server.await??;
    store.close().await;
    Ok(())
}
