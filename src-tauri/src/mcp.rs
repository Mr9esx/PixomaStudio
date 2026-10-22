use crate::{
    Error, Result, Store,
    assets::digest,
    credentials,
    error::{required, timestamp},
};
use rmcp::{
    ServiceExt,
    model::Tool,
    service::{ClientLifecycleMode, ClientServiceExt},
    transport::{
        StreamableHttpClientTransport, streamable_http_client::StreamableHttpClientTransportConfig,
    },
};
use serde::{Deserialize, Serialize};
use serde_json::{Value, json};
use sqlx::Row;
use std::time::Duration;
use url::Url;
use uuid::Uuid;

pub(crate) type McpService = rmcp::service::RunningService<rmcp::RoleClient, McpClient>;

#[derive(Clone)]
pub(crate) struct McpCallContext {
    store: Store,
    account: String,
    run: String,
    id: String,
    tool: crate::tools::RunTool,
    call: rig::message::ToolCall,
    elicitation_gate: std::sync::Arc<tokio::sync::Mutex<()>>,
}

pub(crate) struct McpClient {
    info: rmcp::model::ClientConfig,
    context: Option<McpCallContext>,
}

impl rmcp::ClientHandler for McpClient {
    fn get_info(&self) -> rmcp::model::ClientConfig {
        self.info.clone()
    }

    async fn create_elicitation(
        &self,
        request: rmcp::model::ElicitRequestParams,
        context: rmcp::service::RequestContext<rmcp::RoleClient>,
    ) -> std::result::Result<rmcp::model::ElicitResult, rmcp::ErrorData> {
        let call = self
            .context
            .as_ref()
            .ok_or_else(|| rmcp::ErrorData::invalid_request("当前请求没有补充输入会话", None))?;
        call.answer_elicitation(request, context.ct)
            .await
            .map_err(|error| rmcp::ErrorData::internal_error(error.to_string(), None))
    }
}

impl McpCallContext {
    pub(crate) fn new(
        store: Store,
        account: &str,
        run: &str,
        id: &str,
        tool: &crate::tools::RunTool,
        call: &rig::message::ToolCall,
    ) -> Self {
        Self {
            store,
            account: account.to_owned(),
            run: run.to_owned(),
            id: id.to_owned(),
            tool: tool.clone(),
            call: call.clone(),
            elicitation_gate: Default::default(),
        }
    }

    async fn answer_elicitation(
        &self,
        request: rmcp::model::ElicitRequestParams,
        cancellation: tokio_util::sync::CancellationToken,
    ) -> Result<rmcp::model::ElicitResult> {
        let _input = tokio::select! {
            biased;
            _ = cancellation.cancelled() => return Err(Error::Cancelled),
            input = self.elicitation_gate.lock() => input,
        };
        let mut changes = self.store.run_changes.subscribe();
        self.store
            .save_mcp_inputs(
                &self.account,
                &self.run,
                &self.id,
                &self.tool,
                &self.call,
                &crate::mcp_inputs::McpInput::Live(request),
            )
            .await?;
        loop {
            changes.borrow_and_update();
            if cancellation.is_cancelled() {
                self.store
                    .close_live_mcp_input(
                        &self.account,
                        &self.run,
                        &self.id,
                        &self.tool,
                        &self.call,
                    )
                    .await?;
                return Err(Error::Cancelled);
            }
            let mut saved = self
                .store
                .mcp_execution(&self.account, &self.run, &self.id, &self.tool, &self.call)
                .await?
                .ok_or(Error::State("MCP 继续记录缺失"))?;
            let run = self.store.get_run(&self.account, &self.run).await?;
            if run["status"] == "running" {
                if saved["status"] == "input_required" {
                    if !self
                        .store
                        .pause_for_mcp_input(
                            &self.account,
                            &self.run,
                            &self.id,
                            &self.tool,
                            &self.call,
                        )
                        .await?
                    {
                        continue;
                    }
                } else if saved["status"] == "ready" {
                    let result =
                        serde_json::from_value(saved["input"]["responses"]["elicitation"].clone())?;
                    saved["status"] = json!("continuing");
                    self.store
                        .update_mcp_execution(&self.account, &self.run, &self.id, &saved)
                        .await?;
                    return Ok(result);
                } else {
                    return Err(Error::State("MCP 补充输入状态无效"));
                }
            } else if run["status"] != "waiting_clarification" {
                return Err(Error::Cancelled);
            }
            tokio::select! {
                _ = cancellation.cancelled() => {}
                changed = changes.changed() => changed.map_err(|_| Error::State("MCP 输入通知已关闭"))?,
            }
        }
    }

    pub(crate) async fn call_tool(
        &self,
        request: rmcp::model::CallToolRequestParams,
    ) -> Result<rmcp::model::CallToolResponse> {
        let crate::tools::ToolTarget::Mcp { name, .. } = &self.tool.target else {
            return Err(Error::State("MCP 工具配置无效"));
        };
        if request.name.as_ref() != name {
            return Err(Error::Invalid("MCP 请求与工具名称不一致"));
        }
        let service = self
            .store
            .connect_tool_service_with_context(&self.account, &self.tool, Some(self.clone()))
            .await?;
        let response = {
            let mut changes = self.store.run_changes.subscribe();
            let operation = service.call_tool_once(request);
            tokio::pin!(operation);
            let mut remaining = Duration::from_secs(300);
            let mut previous = tokio::time::Instant::now();
            let mut waiting = false;
            let mut keep_alive = tokio::time::interval(Duration::from_secs(30));
            keep_alive.set_missed_tick_behavior(tokio::time::MissedTickBehavior::Skip);
            loop {
                changes.borrow_and_update();
                if !waiting {
                    remaining = remaining.saturating_sub(previous.elapsed());
                }
                previous = tokio::time::Instant::now();
                waiting = self.store.get_run(&self.account, &self.run).await?["status"]
                    == "waiting_clarification";
                tokio::select! {
                    response = &mut operation => break response.map_err(|error| Error::ToolResultUnknown(error.to_string())),
                    _ = tokio::time::sleep(remaining), if !waiting => break Err(Error::ToolResultUnknown("MCP 工具请求超时".to_owned())),
                    _ = keep_alive.tick(), if waiting => {
                        let request = rmcp::model::ClientRequest::PingRequest(rmcp::model::PingRequest::default());
                        match tokio::time::timeout(Duration::from_secs(15), service.send_request(request)).await {
                            Ok(Ok(rmcp::model::ServerResult::EmptyResult(_))) => {}
                            Ok(Ok(_)) => break Err(Error::ToolResultUnknown("MCP 连接检查返回了无法识别的结果".to_owned())),
                            Ok(Err(error)) => break Err(Error::ToolResultUnknown(error.to_string())),
                            Err(_) => break Err(Error::ToolResultUnknown("MCP 连接检查超时".to_owned())),
                        }
                    }
                    changed = changes.changed() => changed.map_err(|_| Error::State("MCP 输入通知已关闭"))?,
                }
            }
        };
        let cancelled = service.cancel().await;
        let _input = self.elicitation_gate.lock().await;
        self.store
            .close_live_mcp_input(&self.account, &self.run, &self.id, &self.tool, &self.call)
            .await?;
        cancelled.map_err(|error| Error::Mcp(error.to_string()))?;
        response
    }

    pub(crate) async fn get_task(&self, id: &str) -> Result<rmcp::model::GetTaskResult> {
        let service = self
            .store
            .connect_tool_service_with_context(&self.account, &self.tool, Some(self.clone()))
            .await?;
        let response = tokio::time::timeout(
            Duration::from_secs(30),
            service.get_task(rmcp::model::GetTaskParams::new(id)),
        )
        .await;
        service
            .cancel()
            .await
            .map_err(|error| Error::Mcp(error.to_string()))?;
        response
            .map_err(|_| Error::Mcp(format!("MCP 任务 {id} 查询超时")))?
            .map_err(|error| Error::Mcp(format!("MCP 任务 {id} 查询失败：{error}")))
    }

    pub(crate) async fn update_task(
        &self,
        id: &str,
        responses: rmcp::model::InputResponses,
    ) -> Result<()> {
        let service = self
            .store
            .connect_tool_service_with_context(&self.account, &self.tool, Some(self.clone()))
            .await?;
        let response = tokio::time::timeout(
            Duration::from_secs(30),
            service.update_task(rmcp::model::UpdateTaskParams::new(id, responses)),
        )
        .await;
        service
            .cancel()
            .await
            .map_err(|error| Error::Mcp(error.to_string()))?;
        response
            .map_err(|_| Error::Mcp(format!("MCP 任务 {id} 输入提交超时")))?
            .map_err(|error| Error::Mcp(format!("MCP 任务 {id} 输入提交失败：{error}")))
    }
}

#[derive(Deserialize, Serialize)]
#[serde(deny_unknown_fields)]
struct ConnectorInput {
    name: String,
    url: String,
    #[serde(default, skip_serializing)]
    credential: String,
    enabled: bool,
    policy: String,
}

fn validate_url(address: &str) -> Result<()> {
    let address = Url::parse(address).map_err(|_| Error::Invalid("MCP 地址无效"))?;
    if !["http", "https"].contains(&address.scheme())
        || address.host_str().is_none()
        || !address.username().is_empty()
        || address.password().is_some()
        || address.fragment().is_some()
        || address.query().is_some()
    {
        return Err(Error::Invalid("MCP 地址无效"));
    }
    Ok(())
}

async fn connect_service(
    address: &str,
    credential: &str,
    context: Option<McpCallContext>,
) -> Result<McpService> {
    validate_url(address)?;
    let http = reqwest::Client::builder()
        .redirect(reqwest::redirect::Policy::none())
        .connect_timeout(Duration::from_secs(15))
        .build()?;
    let mut config = StreamableHttpClientTransportConfig::with_uri(address.to_owned())
        .reinit_on_expired_session(false);
    if !credential.is_empty() {
        config = config.auth_header(credential);
    }
    let transport = || StreamableHttpClientTransport::with_client(http.clone(), config.clone());
    let mut client = rmcp::model::ClientConfig::default();
    client
        .capabilities
        .extensions
        .get_or_insert_with(Default::default)
        .insert(
            rmcp::model::TASKS_EXTENSION_ID.to_owned(),
            Default::default(),
        );
    if context.is_some() {
        client.capabilities.elicitation = Some(
            rmcp::model::ElicitationCapability::new()
                .with_form(
                    rmcp::model::FormElicitationCapability::new().with_schema_validation(true),
                )
                .with_url(rmcp::model::UrlElicitationCapability::new()),
        );
    }
    tokio::time::timeout(Duration::from_secs(30), async {
        let discovered = McpClient {
            info: client.clone(),
            context: context.clone(),
        }
        .serve_with_lifecycle(
            transport(),
            ClientLifecycleMode::Discover {
                preferred_versions: rmcp::model::ProtocolVersion::KNOWN_VERSIONS
                    .iter()
                    .rev()
                    .cloned()
                    .collect(),
            },
        )
        .await;
        client.protocol_version = match discovered {
            Ok(service) => {
                let info = service
                    .peer_info()
                    .ok_or(Error::State("MCP 服务信息缺失"))?;
                if !info.protocol_version.has_initialize() {
                    return Ok(service);
                }
                let version = info.protocol_version.clone();
                service
                    .cancel()
                    .await
                    .map_err(|error| Error::Mcp(error.to_string()))?;
                version
            }
            Err(rmcp::service::ClientInitializeError::JsonRpcError(error))
                if matches!(
                    error.code,
                    rmcp::model::ErrorCode::METHOD_NOT_FOUND
                        | rmcp::model::ErrorCode::INVALID_REQUEST
                ) =>
            {
                rmcp::model::ProtocolVersion::LATEST_WITH_INITIALIZE
            }
            Err(error) => return Err(Error::Mcp(error.to_string())),
        };
        McpClient {
            info: client,
            context,
        }
        .serve(transport())
        .await
        .map_err(|error| Error::Mcp(error.to_string()))
    })
    .await
    .map_err(|_| Error::Mcp("服务初始化超时".to_owned()))?
}

async fn discover_tools(address: &str, credential: &str) -> Result<Vec<Tool>> {
    let service = connect_service(address, credential, None).await?;
    let tools = tokio::time::timeout(Duration::from_secs(30), service.list_all_tools())
        .await
        .map_err(|_| Error::Mcp("读取工具列表超时".to_owned()))?
        .map_err(|error| Error::Mcp(error.to_string()))?;
    service
        .cancel()
        .await
        .map_err(|error| Error::Mcp(error.to_string()))?;
    Ok(tools)
}

fn tool_summaries(tools: &[Tool]) -> Value {
    json!(tools.iter().map(|tool| json!({ "name": tool.name, "description": tool.description.as_deref().unwrap_or("") })).collect::<Vec<_>>())
}

pub(crate) fn model_result_content(output: &Value) -> Result<Vec<rig::message::ToolResultContent>> {
    use rig::message::{ImageMediaType, MimeType, ToolResultContent};
    let mut result: rmcp::model::CallToolResult = serde_json::from_value(output.clone())?;
    let mut content = Vec::new();
    for part in &result.content {
        if let rmcp::model::ContentBlock::Image(image) = part {
            let media_type = ImageMediaType::from_mime_type(&image.mime_type)
                .ok_or(Error::Invalid("MCP 图片格式不受支持"))?;
            content.push(ToolResultContent::image_base64(
                image.data.clone(),
                Some(media_type),
                None,
            ));
        }
    }
    result
        .content
        .retain(|part| !matches!(part, rmcp::model::ContentBlock::Image(_)));
    content.insert(0, ToolResultContent::json(serde_json::to_value(result)?));
    Ok(content)
}

impl Store {
    pub(crate) async fn active_connector_policy(&self, account: &str, id: &str) -> Result<String> {
        let document: String =
            sqlx::query_scalar("SELECT document FROM connectors WHERE account_id=? AND id=?")
                .bind(account)
                .bind(id)
                .fetch_optional(&self.pool)
                .await?
                .ok_or(Error::NotFound)?;
        let input: ConnectorInput = serde_json::from_str(&document)?;
        if !input.enabled || input.policy == "forbidden" {
            return Err(Error::Invalid("MCP 连接已停用或禁止调用"));
        }
        Ok(input.policy)
    }

    pub(crate) async fn mcp_run_tools(&self, account: &str) -> Result<Vec<crate::tools::RunTool>> {
        let connectors = self.list_connectors(account, true).await?;
        let mut tools = Vec::new();
        for connector in connectors
            .as_array()
            .ok_or(Error::State("MCP 列表格式无效"))?
        {
            let id = crate::local_api::text(connector, "id")?;
            if connector["tools"].is_null() {
                self.probe_connector(account, id).await?;
            }
            let row = sqlx::query(
                "SELECT document,credential_ref,tools FROM connectors WHERE account_id=? AND id=?",
            )
            .bind(account)
            .bind(id)
            .fetch_one(&self.pool)
            .await?;
            let input: ConnectorInput = serde_json::from_str(row.get("document"))?;
            let available: Vec<Tool> = serde_json::from_str(
                &row.get::<Option<String>, _>("tools")
                    .ok_or(Error::State("MCP 工具列表尚未就绪"))?,
            )?;
            for tool in available {
                let alias = format!(
                    "mcp_{}",
                    &digest(format!("{id}:{}", tool.name).as_bytes())[..32]
                );
                let read_only = tool
                    .annotations
                    .as_ref()
                    .is_some_and(|annotations| annotations.read_only_hint == Some(true));
                tools.push(crate::tools::RunTool {
                    definition: rig::completion::ToolDefinition::new(
                        rig::message::ToolName::new(alias)
                            .map_err(|_| Error::State("MCP 工具名称无效"))?,
                        format!(
                            "{} / {}：{}",
                            input.name,
                            tool.name,
                            tool.description.as_deref().unwrap_or("")
                        ),
                        Value::Object((*tool.input_schema).clone()),
                    ),
                    target: crate::tools::ToolTarget::Mcp {
                        connector_id: id.to_owned(),
                        url: input.url.clone(),
                        name: tool.name.into_owned(),
                        credential_ref: row.get("credential_ref"),
                    },
                    requires_approval: input.policy == "approval" || !read_only,
                    read_only,
                });
            }
        }
        Ok(tools)
    }

    pub(crate) async fn connect_tool_service(
        &self,
        account: &str,
        tool: &crate::tools::RunTool,
    ) -> Result<McpService> {
        self.connect_tool_service_with_context(account, tool, None)
            .await
    }

    async fn connect_tool_service_with_context(
        &self,
        account: &str,
        tool: &crate::tools::RunTool,
        context: Option<McpCallContext>,
    ) -> Result<McpService> {
        let crate::tools::ToolTarget::Mcp {
            connector_id,
            url,
            credential_ref,
            ..
        } = &tool.target
        else {
            return Err(Error::State("MCP 工具配置无效"));
        };
        let row = sqlx::query(
            "SELECT document,credential_ref FROM connectors WHERE account_id=? AND id=?",
        )
        .bind(account)
        .bind(connector_id)
        .fetch_optional(&self.pool)
        .await?
        .ok_or(Error::NotFound)?;
        let current: ConnectorInput = serde_json::from_str(row.get("document"))?;
        if !current.enabled || current.policy == "forbidden" {
            return Err(Error::Invalid("MCP 连接已停用或禁止调用"));
        }
        if current.url != *url || row.get::<Option<String>, _>("credential_ref") != *credential_ref
        {
            return Err(Error::Invalid("MCP 配置已更改，请重新运行"));
        }
        let credential = match credential_ref {
            Some(reference) => {
                credentials::load(reference)?.ok_or(Error::State("MCP 凭据不存在，请重新配置"))?
            }
            None => String::new(),
        };
        connect_service(url, &credential, context).await
    }

    pub(crate) async fn read_mcp_resource(
        &self,
        account: &str,
        tool: &crate::tools::RunTool,
        uri: &str,
    ) -> Result<Vec<rmcp::model::ResourceContents>> {
        let service = self.connect_tool_service(account, tool).await?;
        let response = tokio::time::timeout(
            Duration::from_secs(300),
            service.read_resource(rmcp::model::ReadResourceRequestParams::new(uri)),
        )
        .await;
        service
            .cancel()
            .await
            .map_err(|error| Error::Mcp(error.to_string()))?;
        Ok(response
            .map_err(|_| Error::Mcp("读取工作流结果超时".to_owned()))?
            .map_err(|error| Error::Mcp(error.to_string()))?
            .contents)
    }

    pub(crate) async fn call_mcp_tool(
        &self,
        account: &str,
        tool: &crate::tools::RunTool,
        arguments: &serde_json::Map<String, Value>,
    ) -> Result<Value> {
        let crate::tools::ToolTarget::Mcp { name, .. } = &tool.target else {
            return Err(Error::State("MCP 工具配置无效"));
        };
        let response = self
            .call_mcp_tool_once(
                account,
                tool,
                rmcp::model::CallToolRequestParams::new(name.clone())
                    .with_arguments(arguments.clone()),
            )
            .await?;
        match response {
            rmcp::model::CallToolResponse::Complete(result) => {
                if result.is_error == Some(true) {
                    return Err(Error::Mcp(serde_json::to_string(&result)?));
                }
                Ok(serde_json::to_value(result)?)
            }
            rmcp::model::CallToolResponse::InputRequired(_) => {
                Err(Error::Invalid("MCP 工具需要补充输入"))
            }
            rmcp::model::CallToolResponse::Task(task) => {
                Err(Error::ToolResultUnknown(serde_json::to_string(&task)?))
            }
            _ => Err(Error::ToolResultUnknown(
                "MCP 服务返回了无法识别的结果".to_owned(),
            )),
        }
    }

    pub(crate) async fn call_mcp_tool_once(
        &self,
        account: &str,
        tool: &crate::tools::RunTool,
        request: rmcp::model::CallToolRequestParams,
    ) -> Result<rmcp::model::CallToolResponse> {
        let crate::tools::ToolTarget::Mcp { name, .. } = &tool.target else {
            return Err(Error::State("MCP 工具配置无效"));
        };
        if request.name.as_ref() != name {
            return Err(Error::Invalid("MCP 请求与工具名称不一致"));
        }
        let service = self.connect_tool_service(account, tool).await?;
        let response =
            tokio::time::timeout(Duration::from_secs(300), service.call_tool_once(request)).await;
        service
            .cancel()
            .await
            .map_err(|error| Error::Mcp(error.to_string()))?;
        response
            .map_err(|_| Error::ToolResultUnknown("MCP 工具请求超时".to_owned()))?
            .map_err(|error| Error::ToolResultUnknown(error.to_string()))
    }

    pub(crate) async fn list_connectors(
        &self,
        account: &str,
        available_only: bool,
    ) -> Result<Value> {
        Self::check_account(account)?;
        let rows = sqlx::query("SELECT * FROM connectors WHERE account_id=? AND (?=0 OR (json_extract(document,'$.enabled')=1 AND json_extract(document,'$.policy')<>'forbidden')) ORDER BY created_at,id")
            .bind(account).bind(available_only).fetch_all(&self.pool).await?;
        let mut items = Vec::with_capacity(rows.len());
        for row in rows {
            let mut document: Value = serde_json::from_str(row.get("document"))?;
            document["id"] = json!(row.get::<String, _>("id"));
            document["credential_masked"] = json!(if row
                .get::<Option<String>, _>("credential_ref")
                .is_some()
            {
                "••••••••"
            } else {
                ""
            });
            document["tools"] = match row.get::<Option<String>, _>("tools") {
                Some(tools) => tool_summaries(&serde_json::from_str::<Vec<Tool>>(&tools)?),
                None => Value::Null,
            };
            document["created_at"] = json!(row.get::<String, _>("created_at"));
            document["updated_at"] = json!(row.get::<String, _>("updated_at"));
            items.push(document);
        }
        Ok(json!(items))
    }

    pub(crate) async fn save_connector(
        &self,
        account: &str,
        id: Option<&str>,
        document: Value,
    ) -> Result<Value> {
        Self::check_account(account)?;
        let mut input: ConnectorInput =
            serde_json::from_value(document).map_err(|_| Error::Invalid("MCP 配置字段无效"))?;
        input.name = input.name.trim().to_owned();
        input.url = input.url.trim().to_owned();
        required(&input.name, "MCP 名称不能为空")?;
        validate_url(&input.url)?;
        if !["auto", "approval", "forbidden"].contains(&input.policy.as_str()) {
            return Err(Error::Invalid("MCP 调用策略无效"));
        }
        if input.credential.contains(['\r', '\n']) {
            return Err(Error::Invalid("MCP 凭据格式无效"));
        }
        let existing = match id {
            Some(id) => Some(sqlx::query("SELECT document,credential_ref,tools FROM connectors WHERE account_id=? AND id=?").bind(account).bind(id).fetch_optional(&self.pool).await?.ok_or(Error::NotFound)?),
            None => None,
        };
        let previous: Option<String> = existing.as_ref().and_then(|row| row.get("credential_ref"));
        let credential = if input.credential.is_empty() {
            previous.clone()
        } else {
            let reference = format!("mcp:{}:{}", digest(account.as_bytes()), Uuid::new_v4());
            credentials::save(&reference, &input.credential)?;
            Some(reference)
        };
        let tools: Option<String> = match existing {
            Some(row)
                if serde_json::from_str::<Value>(row.get("document"))?["url"] == input.url
                    && credential == previous =>
            {
                row.get("tools")
            }
            _ => None,
        };
        let id = id
            .map(str::to_owned)
            .unwrap_or_else(|| Uuid::new_v4().to_string());
        let now = timestamp()?;
        sqlx::query("INSERT INTO connectors(id,account_id,document,credential_ref,tools,created_at,updated_at) VALUES(?,?,?,?,?,?,?) ON CONFLICT(id,account_id) DO UPDATE SET document=excluded.document,credential_ref=excluded.credential_ref,tools=excluded.tools,updated_at=excluded.updated_at")
            .bind(&id).bind(account).bind(serde_json::to_string(&input)?).bind(&credential).bind(tools).bind(&now).bind(&now).execute(&self.pool).await?;
        if credential != previous
            && let Some(previous) = previous
        {
            credentials::remove(&previous)?;
        }
        self.list_connectors(account, false)
            .await?
            .as_array()
            .ok_or(Error::State("MCP 列表格式无效"))?
            .iter()
            .find(|item| item["id"] == id)
            .cloned()
            .ok_or(Error::NotFound)
    }

    async fn connector_credential(&self, account: &str, id: &str) -> Result<String> {
        let row = sqlx::query("SELECT credential_ref FROM connectors WHERE account_id=? AND id=?")
            .bind(account)
            .bind(id)
            .fetch_optional(&self.pool)
            .await?
            .ok_or(Error::NotFound)?;
        match row.get::<Option<String>, _>("credential_ref") {
            Some(reference) => {
                credentials::load(&reference)?.ok_or(Error::State("MCP 凭据不存在，请重新配置"))
            }
            None => Ok(String::new()),
        }
    }

    pub(crate) async fn discover_connector(&self, account: &str, input: Value) -> Result<Value> {
        Self::check_account(account)?;
        #[derive(Deserialize)]
        #[serde(deny_unknown_fields)]
        struct Discovery {
            url: String,
            credential: String,
            connector_id: Option<String>,
        }
        let input: Discovery =
            serde_json::from_value(input).map_err(|_| Error::Invalid("MCP 发现参数无效"))?;
        let credential = match input.connector_id {
            Some(id) => {
                let saved = self.connector_credential(account, &id).await?;
                if input.credential.is_empty() {
                    saved
                } else {
                    input.credential
                }
            }
            None => input.credential,
        };
        Ok(tool_summaries(
            &discover_tools(&input.url, &credential).await?,
        ))
    }

    pub(crate) async fn probe_connector(&self, account: &str, id: &str) -> Result<Value> {
        Self::check_account(account)?;
        let document: String =
            sqlx::query_scalar("SELECT document FROM connectors WHERE account_id=? AND id=?")
                .bind(account)
                .bind(id)
                .fetch_optional(&self.pool)
                .await?
                .ok_or(Error::NotFound)?;
        let input: ConnectorInput = serde_json::from_str(&document)?;
        let tools =
            discover_tools(&input.url, &self.connector_credential(account, id).await?).await?;
        sqlx::query(
            "UPDATE connectors SET tools=?,updated_at=? WHERE account_id=? AND id=? AND document=?",
        )
        .bind(serde_json::to_string(&tools)?)
        .bind(timestamp()?)
        .bind(account)
        .bind(id)
        .bind(document)
        .execute(&self.pool)
        .await?;
        self.list_connectors(account, false)
            .await?
            .as_array()
            .ok_or(Error::State("MCP 列表格式无效"))?
            .iter()
            .find(|item| item["id"] == id)
            .cloned()
            .ok_or(Error::NotFound)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[tokio::test]
    async fn cancelled_elicitation_releases_its_question_and_preserves_the_active_gate()
    -> std::result::Result<(), Box<dyn std::error::Error>> {
        let root = std::path::PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../.local-cache");
        std::fs::create_dir_all(&root)?;
        let directory = tempfile::Builder::new()
            .prefix("mcp-input-cancel-")
            .tempdir_in(root)?;
        let store = Store::open(&directory.path().join("studio.sqlite")).await?;
        let account = "mcp-input-cancel";
        let model = store.save_model(account, None, json!({
            "name": "输入取消检查", "protocol": "openai_chat_compatible", "base_url": "http://127.0.0.1:18082/v1", "model": "input-state",
            "enabled": true, "agent_enabled": true, "default": true,
            "limits": { "context_window_tokens": 32000, "max_input_tokens": 24000, "max_output_tokens": 4000 },
            "thinking": { "enabled": false }, "capabilities": { "tools": true, "vision": false, "image_output": false, "streaming": true }
        })).await?;
        let session = store.create_session(account, "input-cancel").await?;
        let queued = store
            .queue_run(
                account,
                &session.id,
                "input-cancel",
                crate::local_api::text(&model, "id")?,
                "确认输入",
                "full_access",
            )
            .await?;
        let run = crate::local_api::text(&queued["run"], "id")?;
        assert!(store.claim_run(account, run).await?);
        let tool = crate::tools::RunTool {
            definition: rig::completion::ToolDefinition::new(
                rig::message::ToolName::new("confirm_input")?,
                "确认输入",
                json!({ "type": "object" }),
            ),
            target: crate::tools::ToolTarget::Mcp {
                connector_id: "input-cancel".to_owned(),
                url: "http://127.0.0.1:18082/mcp".to_owned(),
                name: "confirm_input".to_owned(),
                credential_ref: None,
            },
            requires_approval: false,
            read_only: true,
        };
        let call = rig::message::ToolCall::from_wire(
            "input-cancel",
            rig::message::ToolFunction::new(tool.definition.name.clone(), json!({})),
        );
        store.save_tool_part(account, run, 1, 0, &call).await?;
        let record = store.tool_call_records(account, run, 1).await?.remove(0);
        let id = crate::local_api::text(&record, "id")?;
        sqlx::query("UPDATE tool_calls SET status='executing',definition=? WHERE account_id=? AND run_id=? AND id=?")
            .bind(serde_json::to_string(&tool)?).bind(account).bind(run).bind(id).execute(&store.pool).await?;
        let request: rmcp::model::ElicitRequestParams = serde_json::from_value(json!({
            "mode": "form", "message": "确认输入", "requestedSchema": { "type": "object", "properties": { "confirm": { "type": "boolean" } }, "required": ["confirm"] }
        }))?;
        let context = McpCallContext::new(store.clone(), account, run, id, &tool, &call);
        let cancellation = tokio_util::sync::CancellationToken::new();
        let worker_context = context.clone();
        let worker_cancellation = cancellation.clone();
        let worker_request = request.clone();
        let worker = tokio::spawn(async move {
            worker_context
                .answer_elicitation(worker_request, worker_cancellation)
                .await
        });
        tokio::time::timeout(Duration::from_secs(5), async {
            while store.get_run(account, run).await?["status"] != "waiting_clarification" {
                tokio::time::sleep(Duration::from_millis(5)).await;
            }
            Ok::<_, Error>(())
        })
        .await??;
        let before = store.run_events_after(account, run, 0).await?;
        let cancelled = tokio_util::sync::CancellationToken::new();
        cancelled.cancel();
        assert!(matches!(
            context.answer_elicitation(request, cancelled).await,
            Err(Error::Cancelled)
        ));
        assert_eq!(
            store.get_run(account, run).await?["status"],
            "waiting_clarification"
        );
        assert_eq!(store.run_events_after(account, run, 0).await?, before);
        cancellation.cancel();
        assert!(matches!(
            tokio::time::timeout(Duration::from_secs(5), worker).await??,
            Err(Error::Cancelled)
        ));
        assert_eq!(store.get_run(account, run).await?["status"], "running");
        let pending: bool = sqlx::query_scalar("SELECT EXISTS(SELECT 1 FROM clarifications WHERE account_id=? AND run_id=? AND json_extract(document,'$.status')='pending')")
            .bind(account).bind(run).fetch_one(&store.pool).await?;
        assert!(!pending);
        assert_eq!(
            store
                .run_events_after(account, run, 0)
                .await?
                .last()
                .ok_or("继续事件缺失")?["type"],
            "RUN_STARTED"
        );
        store.finish_run(account, run, "cancelled", None).await?;
        store.close().await;
        Ok(())
    }

    #[test]
    fn image_results_keep_actual_pixels_outside_json_text() -> Result<()> {
        use base64::{Engine, engine::general_purpose::STANDARD};
        use rig::message::{DocumentSourceKind, ToolResultContent};
        let mut bytes = std::io::Cursor::new(Vec::new());
        image::RgbImage::new(32, 32)
            .write_to(&mut bytes, image::ImageFormat::Png)
            .map_err(|_| Error::State("图片写入失败"))?;
        let encoded = STANDARD.encode(bytes.get_ref());
        let response = rmcp::model::CallToolResult::success(vec![
            rmcp::model::ContentBlock::text("读取图片完成"),
            rmcp::model::ContentBlock::image(encoded.clone(), "image/png"),
        ]);
        let output = serde_json::to_value(response)?;
        let content = model_result_content(&output)?;
        assert!(
            content
                .iter()
                .any(|part| matches!(part, ToolResultContent::Image(image)
            if matches!(&image.data, DocumentSourceKind::Base64(data) if data == &encoded)))
        );
        for part in content {
            if let ToolResultContent::Json { value } = part {
                assert!(!value.to_string().contains(&encoded));
                assert!(value.to_string().contains("读取图片完成"));
            }
        }
        assert!(output.to_string().contains(&encoded));
        Ok(())
    }

    struct TestCredential(String);

    impl Drop for TestCredential {
        fn drop(&mut self) {
            credentials::remove(&self.0).expect("删除检查 MCP 凭据失败");
        }
    }

    #[tokio::test]
    #[ignore = "需要 PIXOMA_TEST_MCP_URL 与 PIXOMA_TEST_MCP_TOKEN 并连接真实 MCP 服务"]
    async fn real_connector_returns_service_errors_without_losing_the_saved_configuration()
    -> std::result::Result<(), Box<dyn std::error::Error>> {
        let root = std::path::PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../.local-cache");
        std::fs::create_dir_all(&root)?;
        let directory = tempfile::Builder::new()
            .prefix("mcp-errors-")
            .tempdir_in(root)?;
        let store = Store::open(&directory.path().join("studio.sqlite")).await?;
        let account = "mcp-error-check";
        let connector = store.save_connector(account, None, json!({
            "name": "真实 MCP 错误检查", "url": std::env::var("PIXOMA_TEST_MCP_URL")?,
            "credential": std::env::var("PIXOMA_TEST_MCP_TOKEN")?, "enabled": true, "policy": "auto",
        })).await?;
        let id = crate::local_api::text(&connector, "id")?;
        let reference: String =
            sqlx::query_scalar("SELECT credential_ref FROM connectors WHERE account_id=? AND id=?")
                .bind(account)
                .bind(id)
                .fetch_one(&store.pool)
                .await?;
        let _credential = TestCredential(reference);
        let tools = store.mcp_run_tools(account).await?;
        let get_task = tools
            .iter()
            .find(|tool| tool.display_name() == "get_task")
            .ok_or("任务查询工具缺失")?;
        let arguments = json!({ "task_id": Uuid::new_v4().to_string() });
        let error = store
            .call_mcp_tool(
                account,
                get_task,
                arguments.as_object().ok_or("任务查询参数无效")?,
            )
            .await
            .expect_err("不存在的任务不能返回成功结果");
        assert!(matches!(error, Error::Mcp(_)), "{error}");
        let list = tools
            .iter()
            .find(|tool| tool.display_name() == "list_workflows")
            .ok_or("工作流列表工具缺失")?;
        let response = store
            .call_mcp_tool(account, list, &serde_json::Map::new())
            .await?;
        assert_ne!(response["isError"], true);
        assert!(
            !store
                .list_connectors(account, false)
                .await?
                .as_array()
                .ok_or("连接器列表缺失")?
                .is_empty()
        );
        store.close().await;
        Ok(())
    }
}
