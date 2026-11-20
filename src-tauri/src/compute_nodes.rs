use crate::{
    Error, Result, Store,
    assets::digest,
    error::{required, timestamp},
};
use serde::Deserialize;
use serde_json::{Value, json};
use sqlx::{Row, sqlite::SqliteRow};
use time::{OffsetDateTime, format_description::well_known::Rfc3339};
use uuid::Uuid;

pub(crate) fn listen_address(configured: &str) -> Result<std::net::SocketAddr> {
    std::env::var_os("PIXOMA_STUDIO_AGENT_LISTEN")
        .map(|value| value.into_string().map_err(|_| Error::Invalid("计算节点监听地址编码无效")))
        .transpose()?
        .unwrap_or_else(|| configured.to_owned())
        .parse()
        .map_err(|_| Error::Invalid("计算节点监听地址无效"))
}

pub(crate) fn local_url(address: std::net::SocketAddr) -> String {
    let host = if address.ip().is_unspecified() {
        if address.is_ipv4() { "127.0.0.1".to_owned() } else { "[::1]".to_owned() }
    } else if address.is_ipv6() {
        format!("[{}]", address.ip())
    } else {
        address.ip().to_string()
    };
    format!("http://{host}:{}", address.port())
}

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct CreateNode {
    name: String,
    #[serde(default)]
    description: String,
    location: String,
    #[serde(default)]
    enabled: bool,
    comfyui_base_url: String,
    #[serde(default)]
    capabilities: Vec<String>,
}

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct UpdateNode {
    expected_revision: i64,
    name: Option<String>,
    description: Option<String>,
    enabled: Option<bool>,
    comfyui_base_url: Option<String>,
    capabilities: Option<Vec<String>>,
    hardware: Option<HardwareInput>,
    #[serde(default)]
    refresh_hardware: bool,
}

#[derive(serde::Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
struct HardwareInput {
    cpu_model: Option<String>,
    cpu_cores: Option<u32>,
    ram_bytes: Option<u64>,
    gpus: Vec<HardwareGpuInput>,
}

#[derive(serde::Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
struct HardwareGpuInput {
    name: String,
    vram_bytes: Option<u64>,
}

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
pub(crate) struct NodePresence {
    protocol_version: u32,
    agent_version: String,
    agent_instance_id: String,
    started_id: String,
    configuration_revision: i64,
    comfyui: ComfyPresence,
    hardware: Option<Value>,
    metrics: Option<Value>,
}

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct ComfyPresence {
    ready: bool,
    #[serde(default)]
    error: String,
    object_info: Option<Value>,
    #[serde(default)]
    version: String,
}

fn validate_address(address: &str) -> Result<String> {
    let address =
        url::Url::parse(address.trim()).map_err(|_| Error::Invalid("ComfyUI 地址无效"))?;
    if !["http", "https"].contains(&address.scheme())
        || address.host_str().is_none()
        || !address.username().is_empty()
        || address.password().is_some()
        || address.query().is_some()
        || address.fragment().is_some()
    {
        return Err(Error::Invalid("ComfyUI 地址无效"));
    }
    Ok(address.as_str().trim_end_matches('/').to_owned())
}

fn credential() -> String {
    format!(
        "pxnode_{}{}",
        Uuid::new_v4().simple(),
        Uuid::new_v4().simple()
    )
}

fn recent(last_seen: Option<&str>) -> Result<bool> {
    match last_seen {
        Some(value) => {
            let seen = OffsetDateTime::parse(value, &Rfc3339)
                .map_err(|_| Error::State("计算节点心跳时间无效"))?;
            let now = OffsetDateTime::now_utc();
            Ok(seen <= now && now - seen < time::Duration::seconds(35))
        }
        None => Ok(false),
    }
}

fn node_row(row: SqliteRow) -> Result<Value> {
    let last_seen: Option<String> = row.get("last_seen_at");
    let reported: Option<i64> = row.get("reported_revision");
    let connected = reported.is_some() && recent(last_seen.as_deref())?;
    let enabled: bool = row.get("enabled");
    let ready: bool = row.get("comfyui_ready");
    let revision: i64 = row.get("configuration_revision");
    let mut issues = Vec::new();
    if !enabled {
        issues.push("节点已停用");
    }
    if !connected {
        issues.push("Agent 未连接");
    }
    let agent_error: String = row.get("local_agent_error");
    if !agent_error.is_empty() { issues.push(&agent_error); }
    if reported != Some(revision) {
        issues.push("Agent 配置尚未更新");
    }
    if !ready {
        issues.push("ComfyUI 未就绪");
    }
    let hardware: Option<String> = row.get("effective_hardware");
    let metrics: Option<String> = row.get("metrics");
    Ok(json!({
        "id": row.get::<String, _>("id"), "name": row.get::<String, _>("name"),
        "description": row.get::<String, _>("description"), "location": row.get::<String, _>("location"),
        "enabled": enabled, "comfyui_base_url": row.get::<String, _>("comfyui_base_url"),
        "configuration_revision": revision, "agent_version": row.get::<Option<String>, _>("agent_version"),
        "last_seen_at": last_seen, "connected": connected, "comfyui_ready": ready,
        "healthy": issues.is_empty(), "issues": issues, "comfyui_error": row.get::<String, _>("comfyui_error"),
        "hardware": hardware.map(|value| serde_json::from_str::<Value>(&value)).transpose()?,
        "metrics": metrics.map(|value| serde_json::from_str::<Value>(&value)).transpose()?,
        "created_at": row.get::<String, _>("created_at"), "updated_at": row.get::<String, _>("updated_at"),
        "capabilities": serde_json::from_str::<Value>(&row.get::<String, _>("capabilities"))?,
        "started_at": row.get::<Option<String>, _>("started_at"), "comfy_version": row.get::<String, _>("comfy_version"),
        "local_agent_enabled": row.get::<bool, _>("local_agent_enabled"), "local_agent_error": agent_error,
    }))
}

impl Store {
    pub(crate) fn compute_service_error(&self) -> Result<Option<String>> {
        Ok(self.compute_error.read().map_err(|_| Error::State("节点服务状态无法读取"))?.clone())
    }

    pub(crate) fn set_compute_service_error(&self, error: String) -> Result<()> {
        *self.compute_error.write().map_err(|_| Error::State("节点服务状态无法更新"))? = Some(error);
        Ok(())
    }

    fn node_health(&self, mut node: Value) -> Result<Value> {
        if let Some(error) = self.compute_service_error()? {
            node["healthy"] = json!(false);
            node["issues"].as_array_mut().ok_or(Error::State("节点健康信息无效"))?.push(json!(error));
        }
        Ok(node)
    }
    pub async fn compute_connection_settings(&self) -> Result<Value> {
        let row = sqlx::query("SELECT listen_address,public_url FROM compute_connection WHERE id=1").fetch_one(&self.pool).await?;
        let configured: String = row.get("listen_address");
        Ok(json!({ "listen_address": self.compute_address.to_string(), "configured_listen_address": configured,
            "local_url": local_url(self.compute_address), "public_url": row.get::<String, _>("public_url"),
            "remote_access": !self.compute_address.ip().is_loopback(), "environment_override": std::env::var_os("PIXOMA_STUDIO_AGENT_LISTEN").is_some(),
            "restart_required": listen_address(&configured)? != self.compute_address, "listen_error": self.compute_service_error()? }))
    }

    pub async fn save_compute_connection(&self, input: Value) -> Result<Value> {
        let address = crate::local_api::text(&input, "listen_address")?.parse::<std::net::SocketAddr>().map_err(|_| Error::Invalid("计算节点监听地址无效"))?;
        if address.port() == 0 { return Err(Error::Invalid("计算节点监听端口不能为零")); }
        let public_url = crate::local_api::text(&input, "public_url")?.trim();
        let public_url = if public_url.is_empty() { String::new() } else { validate_address(public_url)? };
        sqlx::query("UPDATE compute_connection SET listen_address=?,public_url=? WHERE id=1").bind(address.to_string()).bind(public_url).execute(&self.pool).await?;
        self.compute_connection_settings().await
    }

    pub async fn compute_node_credential(&self, id: &str) -> Result<Value> {
        let node = self.get_compute_node(id).await?;
        let token: String = sqlx::query_scalar("SELECT agent_token FROM compute_nodes WHERE id=?").bind(id).fetch_one(&self.pool).await?;
        Ok(json!({ "node": node, "agent_token": token }))
    }

    pub async fn control_local_agent(&self, id: &str, enabled: bool) -> Result<Value> {
        if enabled && let Some(error) = self.compute_service_error()? { return Err(Error::Conflict(error)); }
        let mut transaction = self.pool.begin_with("BEGIN IMMEDIATE").await?;
        let location: String = sqlx::query_scalar("SELECT location FROM compute_nodes WHERE id=?").bind(id).fetch_optional(&mut *transaction).await?.ok_or(Error::NotFound)?;
        if location != "local" { return Err(Error::Invalid("远程 Agent 需要在对应设备操作")); }
        let running: bool = sqlx::query_scalar("SELECT EXISTS(SELECT 1 FROM local_executions WHERE node_id=? AND attempt_id IS NOT NULL AND status NOT IN ('succeeded','failed','cancelled','result_unknown'))").bind(id).fetch_one(&mut *transaction).await?;
        if running { return Err(Error::Conflict("节点存在执行中的任务，等待执行结束后操作 Agent".to_owned())); }
        sqlx::query("UPDATE compute_nodes SET local_agent_enabled=?,local_agent_generation=local_agent_generation+1,local_agent_error='',last_seen_at=NULL,reported_revision=NULL,comfyui_ready=0 WHERE id=?").bind(enabled).bind(id).execute(&mut *transaction).await?;
        transaction.commit().await?;
        self.get_compute_node(id).await
    }
    pub async fn create_compute_node(&self, input: Value) -> Result<Value> {
        let input: CreateNode = serde_json::from_value(input)
            .map_err(|error| Error::Validation(format!("node：{error}")))?;
        required(&input.name, "计算节点名称不能为空")?;
        if !["local", "remote"].contains(&input.location.as_str()) {
            return Err(Error::Invalid("计算节点位置无效"));
        }
        let address = validate_address(&input.comfyui_base_url)?;
        let id = Uuid::new_v4().to_string();
        let token = credential();
        let now = timestamp()?;
        sqlx::query("INSERT INTO compute_nodes(id,name,description,location,enabled,comfyui_base_url,credential_hash,configuration_revision,created_at,updated_at,capabilities,agent_token) VALUES(?,?,?,?,?,?,?,1,?,?,?,?)")
            .bind(&id).bind(input.name.trim()).bind(input.description).bind(input.location).bind(input.enabled)
            .bind(address).bind(digest(token.as_bytes())).bind(&now).bind(&now).bind(serde_json::to_string(&input.capabilities)?).bind(&token).execute(&self.pool).await?;
        Ok(json!({ "node": self.get_compute_node(&id).await?, "agent_token": token }))
    }

    pub async fn get_compute_node(&self, id: &str) -> Result<Value> {
        let row = sqlx::query("SELECT *,CASE WHEN hardware IS NULL AND hardware_override IS NULL THEN NULL ELSE json_patch(coalesce(hardware,'{}'),coalesce(hardware_override,'{}')) END AS effective_hardware FROM compute_nodes WHERE id=?")
            .bind(id)
            .fetch_optional(&self.pool)
            .await?
            .ok_or(Error::NotFound)?;
        self.node_health(node_row(row)?)
    }

    pub async fn list_compute_nodes(&self) -> Result<Value> {
        let rows = sqlx::query("SELECT *,CASE WHEN hardware IS NULL AND hardware_override IS NULL THEN NULL ELSE json_patch(coalesce(hardware,'{}'),coalesce(hardware_override,'{}')) END AS effective_hardware FROM compute_nodes ORDER BY created_at,id")
            .fetch_all(&self.pool)
            .await?;
        Ok(json!(
            rows.into_iter().map(|row| self.node_health(node_row(row)?)).collect::<Result<Vec<_>>>()?
        ))
    }

    pub async fn authenticate_compute_node(&self, token: &str) -> Result<String> {
        if token.is_empty() {
            return Err(Error::NodeUnauthorized);
        }
        sqlx::query_scalar("SELECT id FROM compute_nodes WHERE credential_hash=?")
            .bind(digest(token.as_bytes()))
            .fetch_optional(&self.pool)
            .await?
            .ok_or(Error::NodeUnauthorized)
    }

    pub async fn update_compute_node(&self, id: &str, patch: Value) -> Result<Value> {
        let patch: UpdateNode = serde_json::from_value(patch)
            .map_err(|error| Error::Validation(format!("node：{error}")))?;
        if let Some(name) = &patch.name {
            required(name, "计算节点名称不能为空")?;
        }
        let address = patch
            .comfyui_base_url
            .as_deref()
            .map(validate_address)
            .transpose()?;
        if patch.refresh_hardware && patch.hardware.is_some() { return Err(Error::Invalid("刷新硬件与编辑硬件不能同时提交")); }
        let changed = sqlx::query("UPDATE compute_nodes SET name=coalesce(?,name),description=coalesce(?,description),enabled=coalesce(?,enabled),capabilities=coalesce(?,capabilities),hardware_override=CASE WHEN ? THEN NULL ELSE coalesce(?,hardware_override) END,hardware=CASE WHEN ? THEN NULL ELSE hardware END,comfyui_ready=CASE WHEN ? IS NULL OR ?=comfyui_base_url THEN comfyui_ready ELSE 0 END,object_info=CASE WHEN ? IS NULL OR ?=comfyui_base_url THEN object_info ELSE NULL END,comfyui_base_url=coalesce(?,comfyui_base_url),configuration_revision=configuration_revision+1,updated_at=? WHERE id=? AND configuration_revision=?")
            .bind(patch.name.as_deref().map(str::trim)).bind(patch.description).bind(patch.enabled).bind(patch.capabilities.map(|value| serde_json::to_string(&value)).transpose()?)
            .bind(patch.refresh_hardware).bind(patch.hardware.map(|value| serde_json::to_string(&value)).transpose()?)
            .bind(patch.refresh_hardware)
            .bind(&address).bind(&address).bind(&address).bind(&address).bind(&address).bind(timestamp()?).bind(id).bind(patch.expected_revision).execute(&self.pool).await?;
        if changed.rows_affected() != 1 {
            self.get_compute_node(id).await?;
            return Err(Error::Conflict("计算节点已更新，请重新读取".to_owned()));
        }
        self.get_compute_node(id).await
    }

    pub async fn rotate_compute_node_credential(&self, id: &str) -> Result<Value> {
        let token = credential();
        let changed = sqlx::query("UPDATE compute_nodes SET credential_hash=?,agent_token=?,local_agent_generation=local_agent_generation+1,configuration_revision=configuration_revision+1,agent_instance_id=NULL,started_id=NULL,last_seen_at=NULL,reported_revision=NULL,comfyui_ready=0,updated_at=? WHERE id=?")
            .bind(digest(token.as_bytes())).bind(&token).bind(timestamp()?).bind(id).execute(&self.pool).await?;
        if changed.rows_affected() != 1 {
            return Err(Error::NotFound);
        }
        Ok(json!({ "node": self.get_compute_node(id).await?, "agent_token": token }))
    }

    pub(crate) async fn record_compute_presence(
        &self,
        token: &str,
        presence: NodePresence,
    ) -> Result<Value> {
        let mut transaction = self.pool.begin_with("BEGIN IMMEDIATE").await?;
        let row = sqlx::query("SELECT * FROM compute_nodes WHERE credential_hash=?")
            .bind(digest(token.as_bytes()))
            .fetch_optional(&mut *transaction)
            .await?
            .ok_or(Error::NodeUnauthorized)?;
        if presence.protocol_version != 1 {
            return Err(Error::Invalid("Agent 协议版本不受支持"));
        }
        required(&presence.agent_version, "Agent 版本不能为空")?;
        if presence.agent_version.len() > 128
            || Uuid::parse_str(&presence.agent_instance_id).is_err()
            || Uuid::parse_str(&presence.started_id).is_err()
        {
            return Err(Error::Invalid("Agent 版本或实例标识无效"));
        }
        let revision: i64 = row.get("configuration_revision");
        if presence.configuration_revision < 0 || presence.configuration_revision > revision {
            return Err(Error::Invalid("Agent 配置修订无效"));
        }
        let previous_instance: Option<String> = row.get("agent_instance_id");
        let last_seen: Option<String> = row.get("last_seen_at");
        if previous_instance
            .as_deref()
            .is_some_and(|id| id != presence.agent_instance_id)
            && recent(last_seen.as_deref())?
        {
            return Err(Error::Conflict("计算节点已有连接中的 Agent".to_owned()));
        }
        for value in [
            &presence.hardware,
            &presence.metrics,
            &presence.comfyui.object_info,
        ]
        .into_iter()
        .flatten()
        {
            if !value.is_object() {
                return Err(Error::Invalid("节点采集信息需要对象"));
            }
        }
        if presence.comfyui.ready && !presence.comfyui.error.is_empty() {
            return Err(Error::Invalid("ComfyUI 就绪状态不能包含错误"));
        }
        let current_revision = presence.configuration_revision == revision;
        let object_info = if current_revision {
            presence.comfyui.object_info.map(|value| value.to_string())
        } else {
            None
        };
        let previous_info: Option<String> = row.get("object_info");
        if current_revision
            && presence.comfyui.ready
            && object_info.is_none()
            && previous_info.is_none()
        {
            return Err(Error::Invalid("ComfyUI 就绪状态需要节点能力信息"));
        }
        let id: String = row.get("id");
        let address: String = row.get("comfyui_base_url");
        let enabled: bool = row.get("enabled");
        let now = timestamp()?;
        if let Some(metrics) = &presence.metrics {
            let collected = metrics["collected_at"].as_str().ok_or(Error::Invalid("监控采集时间缺失"))?;
            OffsetDateTime::parse(collected, &Rfc3339).map_err(|_| Error::Invalid("监控采集时间无效"))?;
            sqlx::query("INSERT INTO compute_node_metrics(node_id,received_at,document) VALUES(?,?,?)")
                .bind(&id).bind(&now).bind(metrics.to_string()).execute(&mut *transaction).await?;
            sqlx::query("DELETE FROM compute_node_metrics WHERE node_id=? AND julianday(received_at)<julianday('now','-30 days')").bind(&id).execute(&mut *transaction).await?;
        }
        sqlx::query("UPDATE compute_nodes SET local_agent_error='',started_at=CASE WHEN started_id IS NULL OR started_id!=? THEN ? ELSE started_at END,comfy_version=?,agent_version=?,agent_instance_id=?,started_id=?,last_seen_at=?,reported_revision=?,comfyui_ready=?,comfyui_error=?,hardware=coalesce(?,hardware),metrics=coalesce(?,metrics),object_info=coalesce(?,object_info) WHERE id=?")
            .bind(&presence.started_id).bind(&now).bind(presence.comfyui.version)
            .bind(presence.agent_version).bind(presence.agent_instance_id).bind(&presence.started_id).bind(&now)
            .bind(presence.configuration_revision).bind(current_revision && presence.comfyui.ready).bind(presence.comfyui.error)
            .bind(presence.hardware.map(|value| value.to_string())).bind(presence.metrics.map(|value| value.to_string())).bind(object_info).bind(&id)
            .execute(&mut *transaction).await?;
        transaction.commit().await?;
        Ok(
            json!({ "protocol_version": 1, "node_id": id, "enabled": enabled, "configuration_revision": revision, "comfyui_base_url": address }),
        )
    }

    pub async fn delete_compute_node(&self, id: &str) -> Result<()> {
        let mut transaction = self.pool.begin_with("BEGIN IMMEDIATE").await?;
        let used: bool = sqlx::query_scalar("SELECT EXISTS(SELECT 1 FROM workflows WHERE json_extract(draft,'$.execution.node_id')=? UNION ALL SELECT 1 FROM workflow_versions WHERE json_extract(definition,'$.execution.node_id')=?)")
            .bind(id).bind(id).fetch_one(&mut *transaction).await?;
        if used {
            return Err(Error::Conflict(
                "计算节点已被工作流引用，可以停用".to_owned(),
            ));
        }
        let changed = sqlx::query("DELETE FROM compute_nodes WHERE id=?")
            .bind(id)
            .execute(&mut *transaction)
            .await?;
        if changed.rows_affected() != 1 {
            return Err(Error::NotFound);
        }
        transaction.commit().await?;
        Ok(())
    }
}
