use pixoma_studio::Store;
use serde_json::{Value, json};
use std::{path::PathBuf, process::{Command, Stdio}, time::Duration};

#[tokio::main]
async fn main() -> Result<(), Box<dyn std::error::Error>> {
    let root = PathBuf::from(std::env::var_os("PIXOMA_STUDIO_DATA_DIR").ok_or("需要验收数据目录")?);
    let workflow_path = PathBuf::from(std::env::args_os().nth(1).ok_or("需要 H3 工作流路径")?);
    let binary = PathBuf::from(std::env::args_os().nth(2).ok_or("需要本机 Agent 路径")?);
    let manifest_path = root.join("h3-manifest.json");
    if manifest_path.exists() { return Err("H3 验收记录已经存在，检查原进程与记录后继续".into()); }
    let store = Store::open(&root.join("studio-standalone.sqlite")).await?;
    let registration = store.create_compute_node(json!({
        "name":"H3 茶园验收节点", "description":"连接自建 ComfyUI 的独立验收 Agent",
        "location":"remote", "enabled":true, "comfyui_base_url":"http://192.168.31.131:8188"
    })).await?;
    let node_id = registration["node"]["id"].as_str().ok_or("节点标识缺失")?;
    let mut definition: Value = serde_json::from_str(&std::fs::read_to_string(workflow_path)?)?;
    definition["execution"]["node_id"] = json!(node_id);
    let workflow = store.create_workflow(json!({
        "name":"H3 山间茶事", "category":"视频生成", "tags":["H3","茶园"],
        "enabled":true, "draft":definition
    })).await?;
    let workflow_id = workflow["id"].as_str().ok_or("工作流标识缺失")?;
    let version = store.create_workflow_version(workflow_id, json!({
        "expected_revision":workflow["revision"], "version":"1.0.0", "notes":"自建 H3 真实视频生成"
    })).await?;
    let version_id = version["id"].as_str().ok_or("工作流版本缺失")?;
    store.set_default_workflow_version(workflow_id, version_id).await?;
    let mut manifest = json!({"node_id":node_id,"workflow_id":workflow_id,"version_id":version_id,"status":"waiting_agent"});
    std::fs::write(&manifest_path, serde_json::to_vec_pretty(&manifest)?)?;
    println!("{}", manifest);
    let log = std::fs::OpenOptions::new().create(true).append(true).open(root.join("h3-agent.log"))?;
    let mut agent = Command::new(binary)
        .env("CONTROL_PLANE_URL","http://127.0.0.1:19429")
        .env("EDGE_ID",node_id)
        .env("AGENT_TOKEN",registration["agent_token"].as_str().ok_or("节点凭据缺失")?)
        .env("EDGE_DATA_DIR",root.join("h3-agent"))
        .env_remove("HTTP_PROXY").env_remove("HTTPS_PROXY").env_remove("ALL_PROXY")
        .env_remove("http_proxy").env_remove("https_proxy").env_remove("all_proxy")
        .stdout(Stdio::null()).stderr(Stdio::from(log)).spawn()?;
    let deadline = tokio::time::Instant::now() + Duration::from_secs(1200);
    loop {
        if let Some(status) = agent.try_wait()? { return Err(format!("验收 Agent 已退出：{status}").into()); }
        if store.get_compute_node(node_id).await?["healthy"] == true { break; }
        if tokio::time::Instant::now() >= deadline { return Err("等待 Studio 节点监听超时".into()); }
        tokio::time::sleep(Duration::from_secs(3)).await;
    }
    let inputs = json!({
        "prompt":definition["execution"]["workflow"]["6"]["inputs"]["prompt"],
        "width":608,"height":352,"length":124,"seed":2026103102
    });
    let execution = store.create_local_execution(json!({
        "request_id":"real-h3-tea-20261031-02", "workflow_id":workflow_id,
        "version_id":version_id, "inputs":inputs,
        "session_id":"fa73025d-eaec-4a14-a307-06d46a329d3e"
    })).await?;
    let execution_id = execution["id"].as_str().ok_or("执行标识缺失")?;
    manifest["execution_id"] = json!(execution_id);
    let mut last_status = String::new();
    loop {
        let current = store.get_local_execution(execution_id).await?;
        let status = current["status"].as_str().ok_or("执行状态缺失")?;
        if status != last_status {
            last_status = status.to_owned();
            manifest["status"] = json!(status);
            std::fs::write(&manifest_path, serde_json::to_vec_pretty(&manifest)?)?;
            println!("{}",json!({"execution_id":execution_id,"status":status}));
        }
        if status == "succeeded" {
            manifest["outputs"] = current["outputs"].clone();
            std::fs::write(&manifest_path, serde_json::to_vec_pretty(&manifest)?)?;
            println!("{}",manifest);
            break;
        }
        if ["failed","cancelled","submission_unknown"].contains(&status) {
            return Err(format!("H3 正式执行未成功：{}",current["error"]).into());
        }
        tokio::time::sleep(Duration::from_secs(2)).await;
    }
    loop {
        if let Some(status) = agent.try_wait()? { return Err(format!("验收 Agent 已退出：{status}").into()); }
        tokio::time::sleep(Duration::from_secs(10)).await;
    }
}
