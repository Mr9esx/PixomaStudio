use crate::{Error, Result, Store};
use serde::{Deserialize, Serialize};
use serde_json::{Value, json};
use std::collections::HashMap;
use url::Url;

#[derive(Debug, Deserialize, Serialize)]
pub struct LocalRequest {
    pub method: String,
    pub path: String,
    pub body: Option<Value>,
}

pub struct LocalApi {
    pub(crate) store: Store,
}

impl LocalApi {
    pub fn new(store: Store) -> Self {
        Self { store }
    }

    pub async fn request(&self, request: LocalRequest) -> Result<Value> {
        if !request.path.starts_with("/api/v1/") {
            return Err(Error::Invalid("本地请求地址无效"));
        }
        let url = Url::parse(&format!("http://studio.localhost{}", request.path))
            .map_err(|_| Error::Invalid("本地请求地址无效"))?;
        let segments = url
            .path_segments()
            .ok_or(Error::Invalid("请求路径无效"))?
            .map(|part| {
                percent_encoding::percent_decode_str(part)
                    .decode_utf8()
                    .map(|value| value.into_owned())
                    .map_err(|_| Error::Invalid("请求路径编码错误"))
            })
            .collect::<Result<Vec<_>>>()?;
        let parts: Vec<&str> = segments.iter().map(String::as_str).collect();
        let query: HashMap<String, String> = url.query_pairs().into_owned().collect();
        let body = request.body.unwrap_or(Value::Null);
        match (request.method.as_str(), parts.as_slice()) {
            ("GET", ["api", "v1", "studio", "assets", asset, "video-usages"]) => self.store.asset_video_usages(asset).await,
            ("POST", ["api", "v1", "studio", "assets", asset, "versions", version, "frame"]) => Ok(serde_json::to_value(self.store.create_video_frame(asset,version,body).await?)?),
            ("GET", ["api", "v1", "studio", "sessions", session, "video-project"]) => self.store.read_video_project(session).await,
            ("PUT", ["api", "v1", "studio", "sessions", session, "video-project"]) => self.store.write_video_project(session,text(&body,"request_id")?,body.clone()).await,
            ("GET", ["api", "v1", "studio", "video-projects"]) => self.store.list_video_works(query.get("project_id").map(String::as_str).unwrap_or("")).await,
            ("GET", ["api", "v1", "studio", "video-projects", id]) => self.store.read_video_work(id).await,
            ("GET", ["api", "v1", "studio", "video-projects", id, "editor-state"]) => self.store.video_editor_state(id).await,
            ("PUT", ["api", "v1", "studio", "video-projects", id, "editor-state"]) => self.store.save_video_editor_state(id,body).await,
            ("POST", ["api", "v1", "studio", "video-projects", id, "open"]) => self.store.open_video_work(id).await,
            ("GET", ["api", "v1", "studio", "video-projects", id, "generations"]) => self.store.list_video_generations(id).await,
            ("GET", ["api", "v1", "studio", "video-projects", id, "generation-batches"]) => self.store.list_video_generation_batches(id).await,
            ("POST", ["api", "v1", "studio", "video-projects", id, "generation-batches"]) => self.store.prepare_video_generation_batch(id,body).await,
            ("GET", ["api", "v1", "studio", "video-projects", id, "generation-batches", batch]) => self.store.get_video_generation_batch(id,batch).await,
            ("POST", ["api", "v1", "studio", "video-projects", id, "generations"]) => self.store.create_video_generation(id,body).await,
            ("GET", ["api", "v1", "studio", "video-projects", id, "generations", generation]) => self.store.get_video_generation(id,generation).await,
            ("POST", ["api", "v1", "studio", "video-projects", id, "generations", generation, "dependency-output"]) => self.store.choose_video_dependency_output(id,generation,body).await,
            ("POST", ["api", "v1", "studio", "video-projects", id, "generations", generation, "cancel-waiting"]) => self.store.cancel_video_dependency_wait(id,generation,body).await,
            ("POST", ["api", "v1", "studio", "video-projects", id, "generations", generation, "retry"]) => self.store.retry_video_generation(id,generation,body).await,
            ("PUT", ["api", "v1", "studio", "video-projects", id]) => self.store.write_video_work(id,text(&body,"session_id")?,text(&body,"request_id")?,body.clone()).await,
            ("POST", ["api", "v1", "studio", "video-projects", id, "chats"]) => self.store.bind_video_chat(id,text(&body,"session_id")?,body["asset_id"].as_str()).await,
            ("POST", ["api", "v1", "studio", "video-projects", id, "material-chats"]) => self.store.create_video_material_chat(id,body).await,
            ("GET", ["api", "v1", "studio", "video-projects", id, "chats", session, "material-context"]) => self.store.video_material_chat_context(id,session,query.get("asset_version_id").map(String::as_str)).await,
            ("GET", ["api", "v1", "studio", "video-materials", id]) => self.store.read_video_material(id).await,
            ("GET", ["api", "v1", "studio", "video-materials"]) => self.store.video_material_for_asset(query.get("asset_id").ok_or(Error::Invalid("素材资产标识缺失"))?).await,
            ("POST", ["api", "v1", "studio", "video-materials", id, "results"]) => self.store.adopt_video_material_result(id,body).await,
            ("POST", ["api", "v1", "studio", "video-exports"]) => self.store.export_video_project(body).await,
            ("POST", ["api", "v1", "studio", "video-projects", id, "exports"]) => self.store.export_video_work(id,body).await,
            ("GET", ["api", "v1", "studio", "video-projects", id, "exports"]) => self.store.list_video_work_exports(id).await,
            ("GET", ["api", "v1", "studio", "video-projects", id, "exports", request]) => self.store.get_video_work_export(id,request).await,
            ("GET", ["api", "v1", "studio", "sessions", session, "video-exports", request]) => self.store.video_export_status(session, request).await,
            ("GET", ["api", "v1", "studio", "generation-templates"]) => Ok(crate::online_generation::templates()),
            ("GET", ["api", "v1", "studio", "generation-connections"]) => self.store.list_generation_connections().await,
            ("POST", ["api", "v1", "studio", "generation-connections"]) => self.store.save_generation_connection(body).await,
            ("POST", ["api", "v1", "studio", "workflow-executions"]) => {
                self.store.create_local_execution(body).await
            }
            ("GET", ["api", "v1", "studio", "workflow-executions"]) => {
                self.store
                    .list_local_executions(
                        query.get("workflow_id").map(String::as_str),
                        query.get("session_id").map(String::as_str),
                    )
                    .await
            }
            ("GET", ["api", "v1", "studio", "workflow-executions", id]) => {
                self.store.get_local_execution(id).await
            }
            ("POST", ["api", "v1", "studio", "workflow-executions", id, "cancel"]) => {
                self.store.cancel_local_execution(id, false).await
            }
            (
                "POST",
                [
                    "api",
                    "v1",
                    "studio",
                    "workflow-executions",
                    id,
                    "stop-waiting",
                ],
            ) => self.store.cancel_local_execution(id, true).await,
            ("GET", ["api", "v1", "studio", "compute-connection"]) => {
                self.store.compute_connection_settings().await
            }
            ("PUT", ["api", "v1", "studio", "compute-connection"]) => self.store.save_compute_connection(body).await,
            ("GET", ["api", "v1", "studio", "compute-nodes"]) => {
                self.store.list_compute_nodes().await
            }
            ("POST", ["api", "v1", "studio", "compute-nodes"]) => {
                self.store.create_compute_node(body).await
            }
            ("GET", ["api", "v1", "studio", "compute-nodes", id]) => {
                self.store.get_compute_node(id).await
            }
            ("GET", ["api", "v1", "studio", "compute-nodes", id, "metrics"]) => {
                self.store.compute_node_metrics(id, query.get("window").map(String::as_str).unwrap_or("1h"), query.get("from").map(String::as_str), query.get("to").map(String::as_str)).await
            }
            ("GET", ["api", "v1", "studio", "compute-nodes", id, "observation"]) => {
                self.store.compute_node_observation(id, integer(&query, "offset", 0)?).await
            }
            ("PATCH", ["api", "v1", "studio", "compute-nodes", id]) => {
                self.store.update_compute_node(id, body).await
            }
            ("DELETE", ["api", "v1", "studio", "compute-nodes", id]) => {
                self.store.delete_compute_node(id).await?;
                Ok(Value::Null)
            }
            ("POST", ["api", "v1", "studio", "compute-nodes", id, "credential"]) => {
                self.store.rotate_compute_node_credential(id).await
            }
            ("GET", ["api", "v1", "studio", "compute-nodes", id, "credential"]) => self.store.compute_node_credential(id).await,
            ("POST", ["api", "v1", "studio", "compute-nodes", id, "agent"]) => self.store.control_local_agent(id, body["enabled"].as_bool().ok_or(Error::Invalid("Agent 操作无效"))?).await,
            ("GET", ["api", "v1", "studio", "workflows"]) => {
                let enabled = query
                    .get("enabled")
                    .map(|value| value.parse::<bool>())
                    .transpose()
                    .map_err(|_| Error::Invalid("工作流启停筛选无效"))?;
                self.store
                    .list_workflows(
                        query.get("type").map(String::as_str),
                        query.get("category").map(String::as_str),
                        enabled,
                        query.get("q").map(String::as_str),
                    )
                    .await
            }
            ("POST", ["api", "v1", "studio", "workflows"]) => {
                self.store.create_workflow(body).await
            }
            ("GET", ["api", "v1", "studio", "workflows", id]) => self.store.get_workflow(id).await,
            ("PATCH", ["api", "v1", "studio", "workflows", id]) => {
                self.store.update_workflow(id, body).await
            }
            ("DELETE", ["api", "v1", "studio", "workflows", id]) => {
                self.store.delete_workflow(id).await?;
                Ok(Value::Null)
            }
            ("GET", ["api", "v1", "studio", "workflows", id, "versions"]) => {
                self.store.list_workflow_versions(id).await
            }
            ("POST", ["api", "v1", "studio", "workflows", id, "versions"]) => {
                self.store.create_workflow_version(id, body).await
            }
            ("GET", ["api", "v1", "studio", "workflows", id, "versions", version]) => {
                self.store.get_workflow_version(id, version).await
            }
            ("POST", ["api", "v1", "studio", "workflows", id, "default-version"]) => {
                self.store
                    .set_default_workflow_version(id, text(&body, "version_id")?)
                    .await
            }
            (
                "POST",
                [
                    "api",
                    "v1",
                    "studio",
                    "workflow-executions",
                    id,
                    "reconcile",
                ],
            ) => self.store.reconcile_workflow_submission(id).await,
            (
                "POST",
                [
                    "api",
                    "v1",
                    "studio",
                    "assets",
                    asset,
                    "versions",
                    version,
                    "palette",
                    "retry",
                ],
            ) => {
                self.store.retry_asset_palette(asset, version).await?;
                Ok(Value::Null)
            }
            ("POST", ["api", "v1", "studio", "expert-cards", id, "response"]) => {
                self.store
                    .resolve_expert_card_response(id, body)
                    .await
            }
            ("GET", ["api", "v1", "studio", "asset-settings"]) => {
                Ok(json!({ "allow_user_create_experts": true }))
            }
            ("GET", ["api", "v1", "studio", "experts"]) => self.store.list_experts().await,
            ("POST", ["api", "v1", "studio", "experts"]) => {
                self.store.save_expert(None, body).await
            }
            ("GET", ["api", "v1", "studio", "experts", id]) => {
                self.store.get_expert(id).await
            }
            ("PATCH", ["api", "v1", "studio", "experts", id]) => {
                self.store.save_expert(Some(id), body).await
            }
            ("PATCH", ["api", "v1", "studio", "experts", id, "enabled"]) => {
                self.store
                    .set_expert_enabled(
                        id,
                        body["enabled"]
                            .as_bool()
                            .ok_or(Error::Invalid("专家启停状态无效"))?,
                    )
                    .await
            }
            ("GET", ["api", "v1", "studio", "experts", id, "versions"]) => {
                self.store.expert_versions(id).await
            }
            ("GET", ["api", "v1", "studio", "experts", id, "versions", version]) => {
                self.store.expert_version(id, version).await
            }
            (
                "GET",
                [
                    "api",
                    "v1",
                    "studio" | "agent-assets" | "studio-operations",
                    "skills",
                ],
            ) => self.store.list_skills().await,
            ("POST", ["api", "v1", "studio" | "agent-assets", "skills"]) => {
                self.store.save_skill(None, body).await
            }
            ("GET", ["api", "v1", "studio" | "agent-assets", "skills", id]) => {
                self.store.get_skill(id).await
            }
            ("PATCH", ["api", "v1", "studio" | "agent-assets", "skills", id]) => {
                self.store.save_skill(Some(id), body).await
            }
            (
                "PATCH",
                [
                    "api",
                    "v1",
                    "studio" | "agent-assets",
                    "skills",
                    id,
                    "enabled",
                ],
            ) => {
                self.store
                    .set_skill_enabled(
                        id,
                        body["enabled"]
                            .as_bool()
                            .ok_or(Error::Invalid("技能启停状态无效"))?,
                    )
                    .await
            }
            (
                "GET",
                [
                    "api",
                    "v1",
                    "studio" | "agent-assets",
                    "skills",
                    id,
                    "versions",
                ],
            ) => self.store.skill_versions(id).await,
            (
                "GET",
                [
                    "api",
                    "v1",
                    "studio" | "agent-assets",
                    "skills",
                    id,
                    "versions",
                    version,
                ],
            ) => self.store.skill_version(id, version).await,
            ("GET", ["api", "v1", "studio", "sessions", session, "trajectory"]) => {
                self.store
                    .trajectory(session, query.get("before").map(String::as_str))
                    .await
            }
            (
                "GET",
                [
                    "api",
                    "v1",
                    "studio",
                    "sessions",
                    session,
                    "trajectory",
                    "runs",
                    run,
                    "records",
                    id,
                ],
            ) => {
                self.store
                    .trajectory_detail(session, run, id)
                    .await
            }
            ("GET", ["api", "v1", "studio", "sessions", session, "context"]) => {
                self.store.context_overview(session).await
            }
            (
                "GET",
                [
                    "api",
                    "v1",
                    "studio",
                    "sessions",
                    session,
                    "context",
                    "current",
                ],
            ) => self.store.context_request(session, None).await,
            (
                "GET",
                [
                    "api",
                    "v1",
                    "studio",
                    "sessions",
                    session,
                    "context",
                    "requests",
                ],
            ) => {
                self.store
                    .context_requests(session, integer(&query, "offset", 0)?)
                    .await
            }
            (
                "GET",
                [
                    "api",
                    "v1",
                    "studio",
                    "sessions",
                    session,
                    "context",
                    "requests",
                    id,
                ],
            ) => self.store.context_request(session, Some(id)).await,
            (
                "GET",
                [
                    "api",
                    "v1",
                    "studio",
                    "sessions",
                    session,
                    "context",
                    "events",
                ],
            ) => {
                self.store
                    .context_events(
                        session,
                        integer(&query, "offset", 0)?,
                        query.get("kind").map(String::as_str).unwrap_or(""),
                    )
                    .await
            }
            ("POST", ["api", "v1", "studio", "approvals", id]) => {
                let approved = body["approved"]
                    .as_bool()
                    .ok_or(Error::Invalid("缺少审批决定"))?;
                self.store
                    .resolve_tool_approval(id, approved, body.get("edited_args").cloned())
                    .await
            }
            ("DELETE", ["api", "v1", "studio", "sessions"]) => {
                if body["confirmation"] != "确认清空" {
                    return Err(Error::Invalid("请输入确认清空"));
                }
                self.store.clear_sessions().await?;
                Ok(Value::Null)
            }
            ("POST", ["api", "v1", "studio", "messages"]) => {
                self.store.submit_message(body).await
            }
            ("GET", ["api", "v1", "studio", "runs", id]) => self.store.get_run(id).await,
            ("GET", ["api", "v1", "studio", "runs", id, "events"]) => {
                Ok(json!(self.store.run_events_after(id, 0).await?))
            }
            ("GET", ["api", "v1", "studio", "sessions", id, "runs"]) => {
                self.store.get_session(id).await?;
                Ok(json!(
                    self.store.session_documents("runs", id).await?
                ))
            }
            ("GET", ["api", "v1", "studio", "connectors"]) => {
                self.store.list_connectors(true).await
            }
            ("GET", ["api", "v1", "studio-operations", "connectors"]) => {
                self.store.list_connectors(false).await
            }
            ("POST", ["api", "v1", "studio-operations", "connectors"]) => {
                self.store.save_connector(None, body).await
            }
            ("PATCH", ["api", "v1", "studio-operations", "connectors", id]) => {
                self.store.save_connector(Some(id), body).await
            }
            ("POST", ["api", "v1", "studio-operations", "connectors", "discover"]) => {
                self.store.discover_connector(body).await
            }
            ("POST", ["api", "v1", "studio-operations", "connectors", id, "probe"]) => {
                self.store.probe_connector(id).await
            }
            ("POST", ["api", "v1", "studio-operations", "models", "test"]) => {
                self.store.test_model_input(body).await
            }
            ("POST", ["api", "v1", "studio-operations", "models", id, "test"]) => {
                self.store.test_model(id).await
            }
            ("GET", ["api", "v1", "studio", "models"]) => self.store.list_models(true).await,
            ("GET", ["api", "v1", "studio-operations", "models"]) => {
                self.store.list_models(false).await
            }
            ("POST", ["api", "v1", "studio-operations", "models"]) => {
                self.store.save_model(None, body).await
            }
            ("PATCH", ["api", "v1", "studio-operations", "models", id]) => {
                self.store.save_model(Some(id), body).await
            }
            ("GET", ["api", "v1", "studio", "library", "tree"]) => {
                self.store
                    .library_tree(
                        serde_urlencoded::from_str(url.query().unwrap_or(""))
                            .map_err(|_| Error::Invalid("资产分组参数无效"))?,
                    )
                    .await
            }
            ("GET", ["api", "v1", "studio", "library", "projects"]) => {
                self.store.library_projects().await
            }
            ("GET", ["api", "v1", "studio", "library", "formats"]) => {
                self.store
                    .library_formats(query.get("project_id").map(String::as_str).unwrap_or(""))
                    .await
            }
            ("POST", ["api", "v1", "studio", "library", "assets", "batch"]) => {
                self.store
                    .library_batch(serde_json::from_value(body)?)
                    .await
            }
            ("PUT", ["api", "v1", "studio", "library", "assets", id, "tags"]) => {
                let tags: Vec<String> = serde_json::from_value(
                    body.get("tag_ids")
                        .cloned()
                        .ok_or(Error::Invalid("缺少标签列表"))?,
                )?;
                self.store.set_project_asset_tags(id, &tags).await
            }
            ("PATCH", ["api", "v1", "studio", "library", "assets", id, "version"]) => {
                self.store
                    .set_project_asset_version(id, text(&body, "asset_version_id")?)
                    .await
            }
            ("POST", ["api", "v1", "studio", "library", "assets", id, "projects"]) => {
                self.store
                    .copy_project_asset(
                        id,
                        text(&body, "project_id")?,
                        Some(text(&body, "asset_version_id")?),
                    )
                    .await
            }
            ("GET", ["api", "v1", "studio", "library", "assets", id, "duplicates"]) => {
                self.store.project_asset_duplicates(id).await
            }
            ("GET", ["api", "v1", "studio", "library", "assets"]) => {
                let filters = serde_urlencoded::from_str(url.query().unwrap_or(""))
                    .map_err(|_| Error::Invalid("资产筛选参数无效"))?;
                self.store.library_assets(&filters).await
            }
            ("GET", ["api", "v1", "studio", "library", "assets", id]) => {
                self.store.project_asset(id).await
            }
            ("PATCH", ["api", "v1", "studio", "library", "assets", id]) => {
                self.store.patch_project_asset(id, &body).await
            }
            ("GET", ["api", "v1", "studio", "library", "categories"]) => Ok(serde_json::to_value(
                self.store
                    .categories(query.get("project_id").map(String::as_str).unwrap_or(""))
                    .await?,
            )?),
            ("POST", ["api", "v1", "studio", "library", "categories"]) => {
                self.store
                    .create_category(
                        text(&body, "project_id")?,
                        text(&body, "name")?,
                        text(&body, "parent_id")?,
                    )
                    .await
            }
            ("PATCH", ["api", "v1", "studio", "library", "categories", id]) => {
                self.store.update_category(id, &body).await
            }
            ("DELETE", ["api", "v1", "studio", "library", "categories", id]) => {
                self.store.delete_category(id).await?;
                Ok(Value::Null)
            }
            ("GET", ["api", "v1", "studio", "library", "tags"]) => self.store.tags().await,
            ("POST", ["api", "v1", "studio", "library", "tags"]) => {
                self.store.create_tag(text(&body, "name")?).await
            }
            ("GET", ["api", "v1", "studio", "library", "preferences"]) => {
                self.store.library_preferences().await
            }
            ("PUT", ["api", "v1", "studio", "library", "preferences"]) => {
                self.store.save_library_preferences(body).await
            }
            ("GET", ["api", "v1", "studio", "projects"]) => {
                Ok(serde_json::to_value(self.store.list_projects().await?)?)
            }
            ("POST", ["api", "v1", "studio", "projects"]) => {
                let project_type = body.get("project_type").filter(|value| !value.is_null())
                    .map(|value| value.as_str().ok_or(Error::Invalid("项目类型无效"))).transpose()?.unwrap_or("general");
                Ok(serde_json::to_value(self.store.create_project_with_type(text(&body, "name")?, project_type).await?)?)
            }
            ("PATCH", ["api", "v1", "studio", "projects", id]) => {
                let project_type = match body.get("project_type") {
                    None => None,
                    Some(Value::Null) => Some("general"),
                    Some(value) => Some(value.as_str().ok_or(Error::Invalid("项目类型无效"))?),
                };
                Ok(serde_json::to_value(self.store.update_project(id, text(&body, "name")?, project_type).await?)?)
            }
            ("DELETE", ["api", "v1", "studio", "projects", id]) => {
                self.store.delete_project(id).await?;
                Ok(Value::Null)
            }
            ("POST", ["api", "v1", "studio", "sessions"]) => Ok(serde_json::to_value(
                self.store
                    .create_session(text(&body, "request_id")?)
                    .await?,
            )?),
            ("GET", ["api", "v1", "studio", "sessions"]) => {
                let project = query.get("project_id").map(String::as_str);
                let unassigned = project == Some("");
                Ok(serde_json::to_value(
                    self.store
                        .list_sessions(
                            project.filter(|id| !id.is_empty()),
                            unassigned,
                            integer(&query, "limit", 30)?,
                            integer(&query, "offset", 0)?,
                        )
                        .await?,
                )?)
            }
            ("GET", ["api", "v1", "studio", "sessions", id]) => {
                self.store.session_detail(id).await
            }
            ("PATCH", ["api", "v1", "studio", "sessions", id, "project"]) => {
                let project = text(&body, "project_id")?;
                Ok(serde_json::to_value(
                    self.store
                        .move_session(
                            id,
                            if project.is_empty() {
                                None
                            } else {
                                Some(project)
                            },
                        )
                        .await?,
                )?)
            }
            ("POST", ["api", "v1", "studio", "assets", "text"]) => Ok(serde_json::to_value(
                self.store
                    .create_text_asset(
                        text(&body, "session_id")?,
                        text(&body, "name")?,
                        text(&body, "content")?,
                        text(&body, "request_id")?,
                    )
                    .await?,
            )?),
            ("PATCH", ["api", "v1", "studio", "assets", id, "text"]) => Ok(serde_json::to_value(
                self.store
                    .update_text_asset(id, text(&body, "content")?, text(&body, "request_id")?)
                    .await?,
            )?),
            ("GET", ["api", "v1", "studio", "assets", id]) => {
                Ok(serde_json::to_value(self.store.get_asset(id).await?)?)
            }
            ("GET", ["api", "v1", "studio", "sessions", id, "assets"]) => Ok(serde_json::to_value(
                self.store.list_session_assets(id).await?,
            )?),
            (
                "POST",
                [
                    "api",
                    "v1",
                    "studio",
                    "sessions",
                    id,
                    "assets",
                    "references",
                ],
            ) => Ok(serde_json::to_value(
                self.store.reference_asset_input(id, body).await?,
            )?),
            _ => Err(Error::Invalid("本地操作尚未注册")),
        }
    }
}

pub(crate) fn text<'a>(body: &'a Value, key: &str) -> Result<&'a str> {
    body.get(key)
        .and_then(Value::as_str)
        .ok_or(Error::Invalid("请求字段缺失或类型错误"))
}

pub(crate) fn optional_text<'a>(body: &'a Value, key: &str, default: &'a str) -> Result<&'a str> {
    match body.get(key) {
        Some(value) => value.as_str().ok_or(Error::Invalid("请求字段类型错误")),
        None => Ok(default),
    }
}

pub(crate) fn integer(query: &HashMap<String, String>, key: &str, default: i64) -> Result<i64> {
    match query.get(key) {
        Some(value) => value.parse().map_err(|_| Error::Invalid("分页参数无效")),
        None => Ok(default),
    }
}

impl Store {
    pub async fn session_detail(&self, id: &str) -> Result<Value> {
        let session = self.get_session(id).await?;
        for execution in self
            .session_documents("workflow_executions", id)
            .await?
        {
            if execution["status"] == "submitted"
                && execution["execution_id"]
                    .as_str()
                    .is_some_and(|id| !id.is_empty())
            {
                self.refresh_workflow_execution(text(&execution, "id")?)
                    .await?;
            }
        }
        let mut messages = self.session_documents("messages", id).await?;
        let events = self.session_documents("run_events", id).await?;
        let runs = self.session_documents("runs", id).await?;
        let interrupted: HashMap<&str, &Value> = runs
            .iter()
            .filter(|run| matches!(run["status"].as_str(), Some("failed" | "cancelled")))
            .map(|run| Ok((text(run, "id")?, run)))
            .collect::<Result<_>>()?;
        let mut ordered_events: Vec<_> = events.iter().collect();
        ordered_events.sort_by_key(|event| (event["run_id"].as_str(), event["sequence"].as_i64()));
        for event in ordered_events {
            if event["type"] != "TEXT_MESSAGE_CONTENT" {
                continue;
            }
            let run = text(event, "run_id")?;
            if !interrupted.contains_key(run) {
                continue;
            }
            let message_id = text(&event["payload"], "messageId")?;
            let delta = text(&event["payload"], "delta")?;
            let complete = messages.iter().any(|message| {
                message["run_id"] == run
                    && message["role"] == "assistant"
                    && message["id"]
                        .as_str()
                        .is_some_and(|id| message_id.starts_with(&format!("{id}-")))
            });
            if complete || delta.is_empty() {
                continue;
            }
            if let Some(message) = messages
                .iter_mut()
                .find(|message| message["id"] == message_id)
            {
                let mut content = text(&message["content"][0], "text")?.to_owned();
                content.push_str(delta);
                message["content"][0]["text"] = json!(content);
            } else {
                messages.push(json!({
                    "id": message_id, "run_id": run, "session_id": id,
                    "role": "assistant", "content": [{ "type": "text", "text": delta }],
                    "created_at": text(event, "created_at")?,
                }));
            }
        }
        messages.sort_by(|left, right| {
            left["created_at"]
                .as_str()
                .cmp(&right["created_at"].as_str())
                .then_with(|| left["id"].as_str().cmp(&right["id"].as_str()))
        });
        let transcript_messages: Vec<Value> = messages.iter().map(|message| json!({
            "id": message["id"], "runId": message["run_id"], "role": message["role"],
            "content": message["content"].as_array().map(|parts| parts.iter().filter_map(|part| part["text"].as_str()).collect::<Vec<_>>().join("\n")).unwrap_or_default(),
            "toolCallId": message["tool_call_id"],
            "parts": message.get("parts").cloned().unwrap_or_else(|| json!([])),
            "toolCalls": message.get("tool_calls").cloned().unwrap_or_else(|| json!([])),
            "clarification": message["clarification"],
        })).collect();
        let transcript_events: Vec<Value> = events.iter().map(|event| json!({
            "id": event["id"], "runId": event["run_id"], "sequence": event["sequence"],
            "type": event["type"], "payload": event["payload"], "createdAt": event["created_at"],
        })).collect();
        let mut session = serde_json::to_value(session)?;
        session["latest_run"] = runs.last().cloned().unwrap_or(Value::Null);
        Ok(json!({
            "session": session, "messages": messages, "transcript": { "messages": transcript_messages, "events": transcript_events },
            "assets": self.list_session_assets(id).await?,
            "pending_approvals": self.session_documents("approvals", id).await?,
            "pending_clarifications": self.session_documents("clarifications", id).await?,
            "workflow_executions": self.session_documents("workflow_executions", id).await?,
            "expert_tasks": self.session_documents("expert_tasks", id).await?,
            "expert_cards": self.session_documents("expert_cards", id).await?,
            "expert_capabilities": self.session_expert_capabilities(id).await?,
        }))
    }

    pub(crate) async fn session_documents(
        &self,
        table: &str,
        session_id: &str,
    ) -> Result<Vec<Value>> {
        if ![
            "messages",
            "run_events",
            "runs",
            "approvals",
            "clarifications",
            "workflow_executions",
            "expert_tasks",
            "expert_cards",
        ]
        .contains(&table)
        {
            return Err(Error::Invalid("数据类型无效"));
        }
        let pending = if matches!(table, "approvals" | "clarifications") {
            " AND json_extract(document,'$.status')='pending'"
        } else {
            ""
        };
        let statement = format!(
            "SELECT document FROM {table} WHERE session_id=?{pending} ORDER BY created_at,id"
        );
        let query = sqlx::query_scalar(&statement);
        let rows: Vec<String> = query.bind(session_id).fetch_all(&self.pool).await?;
        rows.iter()
            .map(|value| serde_json::from_str(value).map_err(Error::from))
            .collect()
    }
}
