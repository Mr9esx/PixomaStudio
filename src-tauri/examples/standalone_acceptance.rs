use pixoma_studio::{Store, compute_router};
use serde_json::{Value, json};
use std::{path::PathBuf, time::Duration};
use tokio::process::{Child, Command};

fn launch_agent(
    binary: &PathBuf,
    root: &std::path::Path,
    address: &str,
    token: &str,
    node: &str,
) -> std::io::Result<Child> {
    Command::new(binary)
        .env("CONTROL_PLANE_URL", address)
        .env("AGENT_TOKEN", token)
        .env("EDGE_ID", node)
        .env("EDGE_DATA_DIR", root.join("agent"))
        .env_remove("HTTP_PROXY")
        .env_remove("HTTPS_PROXY")
        .env_remove("ALL_PROXY")
        .env_remove("http_proxy")
        .env_remove("https_proxy")
        .env_remove("all_proxy")
        .kill_on_drop(true)
        .spawn()
}

#[tokio::main]
async fn main() -> Result<(), Box<dyn std::error::Error>> {
    let mut args = std::env::args_os().skip(1);
    let workflow_path = PathBuf::from(args.next().ok_or("需要工作流文件路径")?);
    let root = PathBuf::from(args.next().ok_or("需要验收目录")?);
    let binary = PathBuf::from(args.next().ok_or("需要 Agent 可执行文件路径")?);
    let database = root.join("studio.sqlite");
    if database.exists() {
        return Err("验收目录已经存在数据库，请指定新目录".into());
    }
    let graph: Value = serde_json::from_slice(&std::fs::read(&workflow_path)?)?;
    let mut store = Store::open(&database).await?;
    let node = store
        .create_compute_node(json!({
            "name": "Linux-ComfyUI", "location": "remote", "enabled": true,
            "comfyui_base_url": "http://192.168.31.131:8188"
        }))
        .await?;
    let node_id = node["node"]["id"].as_str().ok_or("节点标识缺失")?;
    let token = node["agent_token"].as_str().ok_or("节点凭据缺失")?;
    let listener = tokio::net::TcpListener::bind("127.0.0.1:18083").await?;
    let router = compute_router(store.clone());
    let mut server = tokio::spawn(async move { axum::serve(listener, router).await });
    let mut agent = launch_agent(&binary, &root, "http://127.0.0.1:18083", token, node_id)?;
    for index in 0..120 {
        if let Some(status) = agent.try_wait()? {
            return Err(format!("Agent 提前结束：{status}").into());
        }
        if store.get_compute_node(node_id).await?["healthy"] == true {
            break;
        }
        if index == 119 {
            return Err("等待 Agent 接入超时".into());
        }
        tokio::time::sleep(Duration::from_millis(500)).await;
    }
    let definition = json!({
        "type": "comfyui", "inputs": [{ "key": "prompt", "type": "string", "required": true }],
        "input_schema": { "type": "object", "properties": { "prompt": { "type": "string", "minLength": 1 } }, "required": ["prompt"], "additionalProperties": false },
        "outputs": [{ "key": "image", "type": "image", "media_type": "image/png" }],
        "execution": { "node_id": node_id, "workflow_filename": workflow_path.file_name().ok_or("工作流文件名称缺失")?.to_str().ok_or("文件名称编码无效")?,
            "workflow": graph, "input_bindings": [{ "key": "prompt", "node_id": "40", "field_path": "text" }],
            "output_bindings": [{ "key": "image", "node_id": "80", "index": 0 }] }
    });
    let workflow = store.create_workflow(json!({ "name": "MiniMax Klein9B 文生图", "category": "image", "enabled": true, "draft": definition })).await?;
    let workflow_id = workflow["id"].as_str().ok_or("工作流标识缺失")?;
    let version = store
        .create_workflow_version(
            workflow_id,
            json!({ "expected_revision": 1, "version": "1.0.0" }),
        )
        .await?;
    let request = json!({ "request_id": "klein9b-acceptance", "workflow_id": workflow_id,
        "version_id": version["id"], "inputs": { "prompt": graph["40"]["inputs"]["text"] } });
    let execution = store.create_local_execution(request.clone()).await?;
    let id = execution["id"].as_str().ok_or("执行标识缺失")?.to_owned();
    assert_eq!(store.create_local_execution(request).await?["id"], id);
    println!("{}", json!({ "execution_id": id, "phase": "等待节点执行" }));
    let deadline = tokio::time::Instant::now() + Duration::from_secs(3600);
    let mut restarted = false;
    let mut original_prompt = Value::Null;
    let mut original_attempt = Value::Null;
    let mut last_status = String::new();
    let result = loop {
        if tokio::time::Instant::now() > deadline {
            return Err("等待实际生成超时，原执行记录已保留".into());
        }
        if let Some(status) = agent.try_wait()? {
            return Err(format!("Agent 提前结束：{status}").into());
        }
        let state = store.get_local_execution(&id).await?;
        let status = state["status"].as_str().ok_or("执行状态缺失")?;
        if status != last_status {
            println!(
                "{}",
                json!({ "execution_id": id, "status": status, "prompt_id": state["prompt_id"] })
            );
            last_status = status.to_owned();
        }
        if status == "running" && !restarted {
            original_prompt = state["prompt_id"].clone();
            original_attempt = state["attempt_id"].clone();
            agent.kill().await?;
            server.abort();
            let _ = server.await;
            store.close().await;
            store = Store::open(&database).await?;
            let listener = tokio::net::TcpListener::bind("127.0.0.1:18083").await?;
            let router = compute_router(store.clone());
            server = tokio::spawn(async move { axum::serve(listener, router).await });
            agent = launch_agent(&binary, &root, "http://127.0.0.1:18083", token, node_id)?;
            restarted = true;
            println!(
                "{}",
                json!({ "phase": "已重新启动 Rust 服务与 Go Agent", "prompt_id": original_prompt, "attempt_id": original_attempt })
            );
        }
        if status == "succeeded" {
            break state;
        }
        if matches!(status, "failed" | "cancelled" | "submission_unknown") {
            return Err(format!("执行未成功：{state}").into());
        }
        tokio::time::sleep(Duration::from_millis(250)).await;
    };
    assert!(restarted);
    assert_eq!(result["prompt_id"], original_prompt);
    assert_eq!(result["attempt_id"], original_attempt);
    let output = &result["outputs"][0];
    let asset = output["asset_id"].as_str().ok_or("资产标识缺失")?;
    let asset_version = output["asset_version_id"].as_str().ok_or("资产版本缺失")?;
    let bytes = store.read_asset(asset, asset_version).await?;
    let image = image::load_from_memory(&bytes)?;
    assert_eq!((image.width(), image.height()), (1024, 1024));
    let client = reqwest::Client::builder().no_proxy().build()?;
    let history: Value = client
        .get(format!(
            "http://192.168.31.131:8188/history/{}",
            original_prompt.as_str().ok_or("ComfyUI 编号缺失")?
        ))
        .send()
        .await?
        .error_for_status()?
        .json()
        .await?;
    let remote_file =
        &history[original_prompt.as_str().ok_or("ComfyUI 编号缺失")?]["outputs"]["80"]["images"][0];
    let remote_bytes = client
        .get("http://192.168.31.131:8188/view")
        .query(&[
            (
                "filename",
                remote_file["filename"].as_str().ok_or("输出文件名称缺失")?,
            ),
            (
                "subfolder",
                remote_file["subfolder"].as_str().ok_or("输出目录缺失")?,
            ),
            ("type", remote_file["type"].as_str().ok_or("输出位置缺失")?),
        ])
        .send()
        .await?
        .error_for_status()?
        .bytes()
        .await?;
    assert_eq!(bytes.as_slice(), remote_bytes.as_ref());
    std::fs::write(root.join("result.png"), &bytes)?;
    std::fs::write(
        root.join("result.json"),
        serde_json::to_vec_pretty(&json!({
            "execution": result, "workflow_id": workflow_id, "version_id": version["id"],
            "service_and_agent_restarted": restarted, "remote_bytes_equal": true,
            "width": image.width(), "height": image.height()
        }))?,
    )?;
    agent.kill().await?;
    server.abort();
    let _ = server.await;
    store.close().await;
    let reopened = Store::open(&database).await?;
    assert_eq!(reopened.read_asset(asset, asset_version).await?, bytes);
    assert_eq!(
        reopened.get_local_execution(&id).await?["outputs"]
            .as_array()
            .ok_or("输出列表缺失")?
            .len(),
        1
    );
    reopened.close().await;
    println!(
        "{}",
        json!({ "status": "验收通过", "result": root.join("result.json"), "image": root.join("result.png") })
    );
    Ok(())
}
