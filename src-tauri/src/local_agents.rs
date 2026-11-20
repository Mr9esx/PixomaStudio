use crate::{Error, Result, Store};
use sqlx::Row;
use std::{collections::{HashMap, HashSet}, path::PathBuf};
use tokio::process::{Child, Command};

struct LocalAgent {
    generation: i64,
    child: Option<Child>,
}

pub(crate) struct LocalAgentService {
    stop: tokio_util::sync::CancellationToken,
    task: std::sync::Mutex<Option<tokio::task::JoinHandle<Result<()>>>>,
}

pub(crate) fn start(store: Store) -> LocalAgentService {
    let stop = tokio_util::sync::CancellationToken::new();
    let cancellation = stop.clone();
    LocalAgentService { stop, task: std::sync::Mutex::new(Some(tokio::spawn(run(store, cancellation)))) }
}

impl LocalAgentService {
    pub(crate) fn stop(&self) -> Result<()> {
        self.stop.cancel();
        let task = self.task.lock().map_err(|_| Error::State("本机 Agent 服务状态无法读取"))?.take();
        if let Some(task) = task {
            tauri::async_runtime::block_on(async move { task.await.map_err(|_| Error::State("等待本机 Agent 服务失败"))? })?;
        }
        Ok(())
    }
}

impl Drop for LocalAgentService {
    fn drop(&mut self) {
        self.stop.cancel();
        if let Some(task) = self.task.get_mut().expect("本机 Agent 服务状态无法读取").take() { task.abort(); }
    }
}

fn binary_path() -> Result<PathBuf> {
    if cfg!(debug_assertions) { return Ok(PathBuf::from(env!("PIXOMA_EDGE_AGENT_PATH"))); }
    Ok(std::env::current_exe()?.parent().ok_or(Error::State("应用程序目录不存在"))?.join(if cfg!(windows) { "edge-agent.exe" } else { "edge-agent" }))
}

fn launch(store: &Store, id: &str, token: &str) -> Result<Child> {
    if token.is_empty() { return Err(Error::Invalid("本机 Agent 缺少凭据，重新生成节点凭据")); }
    let directory = store.root.join("nodes").join(id);
    if directory.is_symlink() { return Err(Error::Invalid("节点目录不能使用符号链接")); }
    std::fs::create_dir_all(&directory)?;
    let log = std::fs::OpenOptions::new().create(true).append(true).open(directory.join("agent.log"))?;
    Ok(Command::new(binary_path()?)
        .env("CONTROL_PLANE_URL", crate::compute_nodes::local_url(store.compute_address))
        .env("AGENT_TOKEN", token)
        .env("EDGE_ID", id)
        .env("EDGE_DATA_DIR", &directory)
        .env_remove("COMFYUI_BASE_URL")
        .env_remove("HTTP_PROXY").env_remove("HTTPS_PROXY").env_remove("ALL_PROXY")
        .env_remove("http_proxy").env_remove("https_proxy").env_remove("all_proxy")
        .stdout(log.try_clone()?).stderr(log).kill_on_drop(true).spawn()?)
}

async fn run(store: Store, stop: tokio_util::sync::CancellationToken) -> Result<()> {
    let mut agents: HashMap<String, LocalAgent> = HashMap::new();
    let mut interval = tokio::time::interval(std::time::Duration::from_secs(1));
    loop {
        tokio::select! { _ = stop.cancelled() => break, _ = interval.tick() => {} }
        if store.compute_service_error()?.is_some() { continue; }
        let rows = sqlx::query("SELECT id,agent_token,local_agent_enabled,local_agent_generation FROM compute_nodes WHERE location='local'").fetch_all(&store.pool).await?;
        let present: HashSet<String> = rows.iter().map(|row| row.get("id")).collect();
        let removed: Vec<String> = agents.keys().filter(|id| !present.contains(*id)).cloned().collect();
        for id in removed {
            if let Some(mut agent) = agents.remove(&id) && let Some(child) = &mut agent.child && child.try_wait()?.is_none() { child.kill().await?; }
        }
        for row in rows {
            let id: String = row.get("id");
            let generation: i64 = row.get("local_agent_generation");
            let enabled: bool = row.get("local_agent_enabled");
            if agents.get(&id).is_some_and(|agent| agent.generation != generation || !enabled)
                && let Some(mut agent) = agents.remove(&id)
                && let Some(child) = &mut agent.child && child.try_wait()?.is_none() { child.kill().await?; }
            if !enabled { continue; }
            if let Some(agent) = agents.get_mut(&id) {
                if let Some(child) = &mut agent.child && let Some(status) = child.try_wait()? {
                    let error = format!("本机 Agent 已退出（{status}），检查节点日志并重新启动");
                    sqlx::query("UPDATE compute_nodes SET local_agent_error=?,last_seen_at=NULL,reported_revision=NULL,comfyui_ready=0 WHERE id=?").bind(error).bind(&id).execute(&store.pool).await?;
                    agent.child = None;
                }
                continue;
            }
            let token: String = row.get("agent_token");
            let child = match launch(&store, &id, &token) {
                Ok(child) => Some(child),
                Err(error) => {
                    sqlx::query("UPDATE compute_nodes SET local_agent_error=? WHERE id=?").bind(error.to_string()).bind(&id).execute(&store.pool).await?;
                    None
                }
            };
            agents.insert(id, LocalAgent { generation, child });
        }
    }
    for (_, mut agent) in agents {
        if let Some(child) = &mut agent.child && child.try_wait()?.is_none() { child.kill().await?; }
    }
    Ok(())
}
