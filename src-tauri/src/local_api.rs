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

    pub async fn request(&self, account: &str, request: LocalRequest) -> Result<Value> {
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
            ("GET", ["api", "v1", "studio", "skills", id, "publications"]) => {
                self.store.publication_history(account, "skill", id).await
            }
            ("GET", ["api", "v1", "studio", "experts", id, "publications"]) => {
                self.store.publication_history(account, "expert", id).await
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
            ) => self.store.reconcile_workflow_submission(account, id).await,
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
                self.store
                    .retry_asset_palette(account, asset, version)
                    .await?;
                Ok(Value::Null)
            }
            ("POST", ["api", "v1", "studio", "expert-cards", id, "response"]) => {
                self.store
                    .resolve_expert_card_response(account, id, body)
                    .await
            }
            ("GET", ["api", "v1", "studio", "asset-settings"]) => {
                Store::check_account(account)?;
                Ok(json!({ "allow_user_create_experts": true }))
            }
            ("GET", ["api", "v1", "studio", "experts"]) => self.store.list_experts(account).await,
            ("POST", ["api", "v1", "studio", "experts"]) => {
                self.store.save_expert(account, None, body).await
            }
            ("GET", ["api", "v1", "studio", "experts", id]) => {
                self.store.get_expert(account, id).await
            }
            ("PATCH", ["api", "v1", "studio", "experts", id]) => {
                self.store.save_expert(account, Some(id), body).await
            }
            ("PATCH", ["api", "v1", "studio", "experts", id, "enabled"]) => {
                self.store
                    .set_expert_enabled(
                        account,
                        id,
                        body["enabled"]
                            .as_bool()
                            .ok_or(Error::Invalid("专家启停状态无效"))?,
                    )
                    .await
            }
            ("GET", ["api", "v1", "studio", "experts", id, "versions"]) => {
                self.store.expert_versions(account, id).await
            }
            ("GET", ["api", "v1", "studio", "experts", id, "versions", version]) => {
                self.store.expert_version(account, id, version).await
            }
            (
                "GET",
                [
                    "api",
                    "v1",
                    "studio" | "agent-assets" | "studio-operations",
                    "skills",
                ],
            ) => self.store.list_skills(account).await,
            ("POST", ["api", "v1", "studio" | "agent-assets", "skills"]) => {
                self.store.save_skill(account, None, body).await
            }
            ("GET", ["api", "v1", "studio" | "agent-assets", "skills", id]) => {
                self.store.get_skill(account, id).await
            }
            ("PATCH", ["api", "v1", "studio" | "agent-assets", "skills", id]) => {
                self.store.save_skill(account, Some(id), body).await
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
                        account,
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
            ) => self.store.skill_versions(account, id).await,
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
            ) => self.store.skill_version(account, id, version).await,
            ("GET", ["api", "v1", "studio", "sessions", session, "trajectory"]) => {
                self.store
                    .trajectory(account, session, query.get("before").map(String::as_str))
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
                    .trajectory_detail(account, session, run, id)
                    .await
            }
            ("GET", ["api", "v1", "studio", "sessions", session, "context"]) => {
                self.store.context_overview(account, session).await
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
            ) => self.store.context_request(account, session, None).await,
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
                    .context_requests(account, session, integer(&query, "offset", 0)?)
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
            ) => self.store.context_request(account, session, Some(id)).await,
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
                        account,
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
                    .resolve_tool_approval(account, id, approved, body.get("edited_args").cloned())
                    .await
            }
            ("DELETE", ["api", "v1", "studio", "sessions"]) => {
                if body["confirmation"] != "确认清空" {
                    return Err(Error::Invalid("请输入确认清空"));
                }
                self.store.clear_sessions(account).await?;
                Ok(Value::Null)
            }
            ("POST", ["api", "v1", "studio", "messages"]) => {
                self.store.submit_message(account, body).await
            }
            ("GET", ["api", "v1", "studio", "runs", id]) => self.store.get_run(account, id).await,
            ("GET", ["api", "v1", "studio", "runs", id, "events"]) => {
                Ok(json!(self.store.run_events_after(account, id, 0).await?))
            }
            ("GET", ["api", "v1", "studio", "sessions", id, "runs"]) => {
                self.store.get_session(account, id).await?;
                Ok(json!(
                    self.store.session_documents("runs", account, id).await?
                ))
            }
            ("GET", ["api", "v1", "studio", "connectors"]) => {
                self.store.list_connectors(account, true).await
            }
            ("GET", ["api", "v1", "studio-operations", "connectors"]) => {
                self.store.list_connectors(account, false).await
            }
            ("POST", ["api", "v1", "studio-operations", "connectors"]) => {
                self.store.save_connector(account, None, body).await
            }
            ("PATCH", ["api", "v1", "studio-operations", "connectors", id]) => {
                self.store.save_connector(account, Some(id), body).await
            }
            ("POST", ["api", "v1", "studio-operations", "connectors", "discover"]) => {
                self.store.discover_connector(account, body).await
            }
            ("POST", ["api", "v1", "studio-operations", "connectors", id, "probe"]) => {
                self.store.probe_connector(account, id).await
            }
            ("POST", ["api", "v1", "studio-operations", "models", "test"]) => {
                self.store.test_model_input(account, body).await
            }
            ("POST", ["api", "v1", "studio-operations", "models", id, "test"]) => {
                self.store.test_model(account, id).await
            }
            ("GET", ["api", "v1", "studio", "models"]) => {
                self.store.list_models(account, true).await
            }
            ("GET", ["api", "v1", "studio-operations", "models"]) => {
                self.store.list_models(account, false).await
            }
            ("POST", ["api", "v1", "studio-operations", "models"]) => {
                self.store.save_model(account, None, body).await
            }
            ("PATCH", ["api", "v1", "studio-operations", "models", id]) => {
                self.store.save_model(account, Some(id), body).await
            }
            ("POST", ["api", "v1", "studio", "sessions", session, "flow", "nodes"]) => {
                self.store
                    .create_flow_node(account, session, serde_json::from_value(body)?)
                    .await
            }
            (
                "DELETE",
                [
                    "api",
                    "v1",
                    "studio",
                    "sessions",
                    session,
                    "flow",
                    "nodes",
                    id,
                ],
            ) => {
                self.store.delete_flow_node(account, session, id).await?;
                Ok(Value::Null)
            }
            ("POST", ["api", "v1", "studio", "sessions", session, "flow", "edges"]) => {
                self.store
                    .create_flow_edge(account, session, serde_json::from_value(body)?)
                    .await
            }
            (
                "DELETE",
                [
                    "api",
                    "v1",
                    "studio",
                    "sessions",
                    session,
                    "flow",
                    "edges",
                    id,
                ],
            ) => {
                self.store.delete_flow_edge(account, session, id).await?;
                Ok(Value::Null)
            }
            ("PATCH", ["api", "v1", "studio", "sessions", session, "flow"]) => {
                self.store
                    .update_flow_positions(
                        account,
                        session,
                        serde_json::from_value(
                            body.get("nodes")
                                .cloned()
                                .ok_or(Error::Invalid("缺少节点列表"))?,
                        )?,
                    )
                    .await?;
                Ok(Value::Null)
            }
            ("GET", ["api", "v1", "studio", "library", "tree"]) => {
                self.store
                    .library_tree(
                        account,
                        serde_urlencoded::from_str(url.query().unwrap_or(""))
                            .map_err(|_| Error::Invalid("资产分组参数无效"))?,
                    )
                    .await
            }
            ("GET", ["api", "v1", "studio", "library", "projects"]) => {
                self.store.library_projects(account).await
            }
            ("GET", ["api", "v1", "studio", "library", "formats"]) => {
                self.store
                    .library_formats(
                        account,
                        query.get("project_id").map(String::as_str).unwrap_or(""),
                    )
                    .await
            }
            ("POST", ["api", "v1", "studio", "library", "assets", "batch"]) => {
                self.store
                    .library_batch(account, serde_json::from_value(body)?)
                    .await
            }
            ("PUT", ["api", "v1", "studio", "library", "assets", id, "tags"]) => {
                let tags: Vec<String> = serde_json::from_value(
                    body.get("tag_ids")
                        .cloned()
                        .ok_or(Error::Invalid("缺少标签列表"))?,
                )?;
                self.store.set_project_asset_tags(account, id, &tags).await
            }
            ("PATCH", ["api", "v1", "studio", "library", "assets", id, "version"]) => {
                self.store
                    .set_project_asset_version(account, id, text(&body, "asset_version_id")?)
                    .await
            }
            ("POST", ["api", "v1", "studio", "library", "assets", id, "projects"]) => {
                self.store
                    .copy_project_asset(
                        account,
                        id,
                        text(&body, "project_id")?,
                        Some(text(&body, "asset_version_id")?),
                    )
                    .await
            }
            ("GET", ["api", "v1", "studio", "library", "assets", id, "duplicates"]) => {
                self.store.project_asset_duplicates(account, id).await
            }
            ("GET", ["api", "v1", "studio", "library", "assets"]) => {
                let filters = serde_urlencoded::from_str(url.query().unwrap_or(""))
                    .map_err(|_| Error::Invalid("资产筛选参数无效"))?;
                self.store.library_assets(account, &filters).await
            }
            ("GET", ["api", "v1", "studio", "library", "assets", id]) => {
                self.store.project_asset(account, id).await
            }
            ("PATCH", ["api", "v1", "studio", "library", "assets", id]) => {
                self.store.patch_project_asset(account, id, &body).await
            }
            ("GET", ["api", "v1", "studio", "library", "categories"]) => Ok(serde_json::to_value(
                self.store
                    .categories(
                        account,
                        query.get("project_id").map(String::as_str).unwrap_or(""),
                    )
                    .await?,
            )?),
            ("POST", ["api", "v1", "studio", "library", "categories"]) => {
                self.store
                    .create_category(
                        account,
                        text(&body, "project_id")?,
                        text(&body, "name")?,
                        text(&body, "parent_id")?,
                    )
                    .await
            }
            ("PATCH", ["api", "v1", "studio", "library", "categories", id]) => {
                self.store.update_category(account, id, &body).await
            }
            ("DELETE", ["api", "v1", "studio", "library", "categories", id]) => {
                self.store.delete_category(account, id).await?;
                Ok(Value::Null)
            }
            ("GET", ["api", "v1", "studio", "library", "tags"]) => self.store.tags(account).await,
            ("POST", ["api", "v1", "studio", "library", "tags"]) => {
                self.store.create_tag(account, text(&body, "name")?).await
            }
            ("GET", ["api", "v1", "studio", "library", "preferences"]) => {
                self.store.library_preferences(account).await
            }
            ("PUT", ["api", "v1", "studio", "library", "preferences"]) => {
                self.store.save_library_preferences(account, body).await
            }
            ("GET", ["api", "v1", "studio", "projects"]) => Ok(serde_json::to_value(
                self.store.list_projects(account).await?,
            )?),
            ("POST", ["api", "v1", "studio", "projects"]) => Ok(serde_json::to_value(
                self.store
                    .create_project(account, text(&body, "name")?)
                    .await?,
            )?),
            ("PATCH", ["api", "v1", "studio", "projects", id]) => Ok(serde_json::to_value(
                self.store
                    .rename_project(account, id, text(&body, "name")?)
                    .await?,
            )?),
            ("DELETE", ["api", "v1", "studio", "projects", id]) => {
                self.store.delete_project(account, id).await?;
                Ok(Value::Null)
            }
            ("POST", ["api", "v1", "studio", "sessions"]) => Ok(serde_json::to_value(
                self.store
                    .create_session(account, text(&body, "request_id")?)
                    .await?,
            )?),
            ("GET", ["api", "v1", "studio", "sessions"]) => {
                let project = query.get("project_id").map(String::as_str);
                let unassigned = project == Some("");
                Ok(serde_json::to_value(
                    self.store
                        .list_sessions(
                            account,
                            project.filter(|id| !id.is_empty()),
                            unassigned,
                            integer(&query, "limit", 30)?,
                            integer(&query, "offset", 0)?,
                        )
                        .await?,
                )?)
            }
            ("GET", ["api", "v1", "studio", "sessions", id]) => {
                self.store.session_detail(account, id).await
            }
            ("PATCH", ["api", "v1", "studio", "sessions", id, "project"]) => {
                let project = text(&body, "project_id")?;
                Ok(serde_json::to_value(
                    self.store
                        .move_session(
                            account,
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
                        account,
                        text(&body, "session_id")?,
                        text(&body, "name")?,
                        text(&body, "content")?,
                        text(&body, "request_id")?,
                    )
                    .await?,
            )?),
            ("PATCH", ["api", "v1", "studio", "assets", id, "text"]) => Ok(serde_json::to_value(
                self.store
                    .update_text_asset(
                        account,
                        id,
                        text(&body, "content")?,
                        text(&body, "request_id")?,
                    )
                    .await?,
            )?),
            ("GET", ["api", "v1", "studio", "assets", id]) => Ok(serde_json::to_value(
                self.store.get_asset(account, id).await?,
            )?),
            ("GET", ["api", "v1", "studio", "sessions", id, "assets"]) => Ok(serde_json::to_value(
                self.store.list_session_assets(account, id).await?,
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
                self.store.reference_asset_input(account, id, body).await?,
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
    pub async fn session_detail(&self, account: &str, id: &str) -> Result<Value> {
        let session = self.get_session(account, id).await?;
        for execution in self
            .session_documents("workflow_executions", account, id)
            .await?
        {
            if execution["status"] == "submitted"
                && execution["task_id"]
                    .as_str()
                    .is_some_and(|id| !id.is_empty())
            {
                self.refresh_workflow_execution(account, text(&execution, "id")?)
                    .await?;
            } else if execution["submission_state"] == "result_unknown"
                && execution["reconciliation"].is_null()
            {
                self.reconcile_workflow_submission(account, text(&execution, "id")?)
                    .await?;
            }
        }
        let mut messages = self.session_documents("messages", account, id).await?;
        let events = self.session_documents("run_events", account, id).await?;
        let runs = self.session_documents("runs", account, id).await?;
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
            "assets": self.list_session_assets(account, id).await?,
            "flow": { "nodes": self.session_documents("flow_nodes", account, id).await?, "edges": self.session_documents("flow_edges", account, id).await? },
            "pending_approvals": self.session_documents("approvals", account, id).await?,
            "pending_clarifications": self.session_documents("clarifications", account, id).await?,
            "workflow_executions": self.session_documents("workflow_executions", account, id).await?,
            "expert_tasks": self.session_documents("expert_tasks", account, id).await?,
            "expert_cards": self.session_documents("expert_cards", account, id).await?,
            "expert_capabilities": self.session_expert_capabilities(account, id).await?,
        }))
    }

    pub(crate) async fn session_documents(
        &self,
        table: &str,
        account: &str,
        session_id: &str,
    ) -> Result<Vec<Value>> {
        if ![
            "messages",
            "run_events",
            "runs",
            "flow_nodes",
            "flow_edges",
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
        let order = if table == "flow_nodes" {
            "CAST(json_extract(document,'$.sort_order') AS INTEGER),created_at,id"
        } else {
            "created_at,id"
        };
        let pending = if matches!(table, "approvals" | "clarifications") {
            " AND json_extract(document,'$.status')='pending'"
        } else {
            ""
        };
        let rows: Vec<String> = sqlx::query_scalar(&format!("SELECT document FROM {table} WHERE account_id=? AND session_id=?{pending} ORDER BY {order}"))
            .bind(account).bind(session_id).fetch_all(&self.pool).await?;
        rows.iter()
            .map(|value| serde_json::from_str(value).map_err(Error::from))
            .collect()
    }
}
