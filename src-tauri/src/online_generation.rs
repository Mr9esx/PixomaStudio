use crate::{Error, Result, Store, assets::digest, error::{required, timestamp}, workflow_definitions::{ExecutionDefinition, GenerationProvider, WorkflowDefinition}};
use base64::{Engine, engine::general_purpose::STANDARD};
use serde::Deserialize;
use serde_json::{Value, json};
use sqlx::Row;
use std::time::Duration;
use uuid::Uuid;

use crate::http_generation::{minimax_http, seedance_http, validate_base_url};

fn online_cancellation_issue(provider: &str, status: &str, has_cancel: bool) -> Option<&'static str> {
    if !matches!(status, "pending" | "running") { return Some("此任务当前不支持取消"); }
    match provider {
        "seedance" if status == "pending" => None,
        "seedance" => Some("任务已开始生成，仅支持停止等待"),
        "http_json" if has_cancel => None,
        "http_json" => Some("此服务未配置取消操作"),
        _ => Some("此服务仅支持停止等待"),
    }
}

fn validate_seedance_body(body: &Value, capability: &str) -> Result<()> {
    if !["480p","720p","1080p","4k"].contains(&body["resolution"].as_str().unwrap_or("")) {return Err(Error::Invalid("分辨率无效"));}
    if !["21:9","16:9","4:3","1:1","3:4","9:16","adaptive"].contains(&body["ratio"].as_str().unwrap_or("")) {return Err(Error::Invalid("画幅无效"));}
    if !body["duration"].as_i64().is_some_and(|value| (4..=15).contains(&value)) {return Err(Error::Invalid("生成时长需要 4 至 15 秒整数"));}
    if !body["generate_audio"].is_boolean() || !body["return_last_frame"].is_boolean() {return Err(Error::Invalid("声音和尾帧参数需要布尔值"));}
    let content = body["content"].as_array().ok_or(Error::Invalid("生成内容需要数组"))?;
    if !content.iter().any(|item| item["type"] == "text" && item["text"].as_str().is_some_and(|value| !value.trim().is_empty())) {return Err(Error::Invalid("填写生成要求"));}
    let frames = content.iter().filter(|item| item["type"] == "image_url").count();
    let expected = match capability {"text_to_video"=>0,"first_frame"=>1,"first_last_frame"=>2,_=>return Err(Error::Invalid("生成能力无效"))};
    if frames != expected {return Err(Error::Invalid("首尾帧数量与工作流能力不一致"));}
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn cancellation_follows_provider_and_execution_state() {
        assert_eq!(online_cancellation_issue("seedance", "pending", true), None);
        assert_eq!(online_cancellation_issue("seedance", "running", true), Some("任务已开始生成，仅支持停止等待"));
        assert_eq!(online_cancellation_issue("minimax", "running", false), Some("此服务仅支持停止等待"));
        assert_eq!(online_cancellation_issue("seedream", "pending", false), Some("此服务仅支持停止等待"));
        assert_eq!(online_cancellation_issue("http_json", "running", true), None);
        assert_eq!(online_cancellation_issue("http_json", "running", false), Some("此服务未配置取消操作"));
        for state in ["dispatching", "submission_unknown", "collecting", "succeeded", "failed", "cancelled", "result_unknown"] {
            assert!(online_cancellation_issue("seedance", state, true).is_some());
            assert!(online_cancellation_issue("http_json", state, true).is_some());
        }
    }

    #[test]
    fn templates_validate_and_bind_every_exposed_input() -> Result<()> {
        for template in templates().as_array().ok_or(Error::State("模板列表无效"))? {
            let definition = WorkflowDefinition::parse(template["definition"].clone())?;
            assert_eq!(definition.kind(), "online");
            assert!(["video", "image"].contains(&definition.outputs[0].r#type.as_str()));
        }
        Ok(())
    }

    #[test]
    fn built_in_templates_cover_image_and_video_providers() {
        let templates = templates();
        let providers: Vec<_> = templates.as_array().expect("模板列表无效").iter().map(|template| template["definition"]["execution"]["provider"].as_str().expect("模板供应商缺失")).collect();
        assert!(providers.contains(&"seedream"));
        assert!(providers.contains(&"minimax"));
        assert!(providers.contains(&"seedance"));
    }

    #[test]
    fn video_parameters_validate_after_defaults_and_open_inputs_are_combined() {
        let mut body = json!({"content":[{"type":"text","text":"清晨茶叶上的露珠"}],"resolution":"720p","ratio":"16:9","duration":4,"generate_audio":true,"return_last_frame":true});
        assert!(validate_seedance_body(&body,"text_to_video").is_ok());
        body["duration"] = json!(3);
        assert!(validate_seedance_body(&body,"text_to_video").is_err());
        body["duration"] = json!(4);
        body["resolution"] = json!("768P");
        assert!(validate_seedance_body(&body,"text_to_video").is_err());
        body["resolution"] = json!("720p");
        assert!(validate_seedance_body(&body,"first_frame").is_err());
    }
}

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct ConnectionInput { name: String, provider: String, api_key: String, #[serde(default)] base_url: Option<String> }

pub(crate) fn templates() -> Value {
    let mut result = Vec::new();
    for (capability, name, frames) in [
        ("text_to_video", "Seedance 2 · 文生视频", vec![]),
        ("first_frame", "Seedance 2 · 首帧生视频", vec!["first_frame"]),
        ("first_last_frame", "Seedance 2 · 首尾帧生视频", vec!["first_frame", "last_frame"]),
    ] {
        let mut inputs = vec![json!({"key":"prompt","type":"string","required":true,"description":"生成要求"}), json!({"key":"resolution","type":"enum","required":true,"description":"分辨率"}), json!({"key":"ratio","type":"enum","required":true,"description":"画幅"}), json!({"key":"duration","type":"number","required":true,"description":"生成时长（秒）"}), json!({"key":"generate_audio","type":"boolean","required":true,"description":"生成声音"})];
        let mut properties = json!({"prompt":{"type":"string","minLength":1,"maxLength":10000},"resolution":{"type":"string","enum":["480p","720p","1080p","4k"],"default":"720p"},"ratio":{"type":"string","enum":["21:9","16:9","4:3","1:1","3:4","9:16","adaptive"],"default":"16:9"},"duration":{"type":"integer","minimum":4,"maximum":15,"default":4},"generate_audio":{"type":"boolean","default":true}});
        let mut mapping = json!({"prompt":"prompt","resolution":"resolution","ratio":"ratio","duration":"duration","generate_audio":"generate_audio"});
        let mut required_fields = vec!["prompt", "resolution", "ratio", "duration", "generate_audio"];
        for frame in frames {
            inputs.push(json!({"key":frame,"type":"image","required":true,"description":if frame == "first_frame" {"首帧"} else {"尾帧"}}));
            properties[frame] = json!({"type":"object","properties":{"asset_id":{"type":"string","minLength":1},"asset_version_id":{"type":"string","minLength":1}},"required":["asset_id","asset_version_id"],"additionalProperties":false});
            mapping[frame] = json!(format!("content.{frame}"));
            required_fields.push(frame);
        }
        let mut template = json!({"id":format!("seedance2-{capability}-v1"),"name":name,"description":"Seedance 2.0 · 北京地域","definition":{"type":"online","inputs":inputs,"input_schema":{"type":"object","properties":properties,"required":required_fields,"additionalProperties":false},"outputs":[{"key":"video","type":"video","description":"分镜视频","media_type":"video/mp4"}],"execution":{"provider":"seedance","operation":"video","connection_id":"","model":"doubao-seedance-2-0-260128","capability":capability,"defaults":{"resolution":"720p","ratio":"16:9","duration":4,"generate_audio":true,"return_last_frame":true},"input_mapping":mapping,"output_mapping":{"video":"/content/video_url"},"http":seedance_http()}}});
        // 首尾帧输入仍由资产适配器转换为服务支持的媒体形式。
        template["definition"]["execution"]["http"]["body"]["content"] = json!([]);
        result.push(template);
    }
    result.extend(crate::generation_providers::templates());
    json!(result)
}

impl Store {
    pub async fn list_generation_connections(&self) -> Result<Value> {
        let rows = sqlx::query("SELECT id,name,provider,base_url,length(api_key)>0 AS has_api_key,created_at FROM generation_services ORDER BY created_at,id").fetch_all(&self.pool).await?;
        Ok(json!(rows.iter().map(|row| json!({"id":row.get::<String,_>("id"),"name":row.get::<String,_>("name"),"provider":row.get::<String,_>("provider"),"base_url":row.get::<String,_>("base_url"),"has_api_key":row.get::<bool,_>("has_api_key"),"created_at":row.get::<String,_>("created_at")})).collect::<Vec<_>>()))
    }

    pub async fn save_generation_connection(&self, document: Value) -> Result<Value> {
        let input: ConnectionInput = serde_json::from_value(document)?;
        required(&input.name, "填写连接名称")?;
        if input.provider != "http_json" {required(&input.api_key, "填写 API Key")?;}
        if !["seedance","seedream","minimax","http_json"].contains(&input.provider.as_str()) { return Err(Error::Invalid("生成服务类型无效")); }
        let base_url = match input.base_url.as_deref() {Some(value)=>value,None if ["seedance","seedream"].contains(&input.provider.as_str())=>"https://ark.cn-beijing.volces.com",None if input.provider == "minimax"=>"https://api.minimax.cn",None=>return Err(Error::Invalid("填写生成服务地址"))};
        validate_base_url(base_url)?;
        let id = Uuid::new_v4().to_string();
        let now = timestamp()?;
        sqlx::query("INSERT INTO generation_services(id,name,provider,base_url,api_key,created_at) VALUES(?,?,?,?,?,?)").bind(&id).bind(input.name.trim()).bind(&input.provider).bind(base_url).bind(input.api_key.trim()).bind(&now).execute(&self.pool).await?;
        Ok(json!({"id":id,"name":input.name.trim(),"provider":input.provider,"base_url":base_url,"has_api_key":!input.api_key.trim().is_empty(),"created_at":now}))
    }

    pub(crate) async fn is_online_execution(&self, id: &str) -> Result<bool> {
        Ok(sqlx::query_scalar("SELECT EXISTS(SELECT 1 FROM online_executions WHERE id=?)").bind(id).fetch_one(&self.pool).await?)
    }

    pub(crate) async fn create_online_execution(&self, mut request: Value) -> Result<Value> {
        request.sort_all_objects();
        let fingerprint = digest(&serde_json::to_vec(&request)?);
        let request_id = crate::local_api::text(&request,"request_id")?;
        required(request_id,"请求标识不能为空")?;
        let _guard = self.model_updates.lock().await;
        if let Some(row) = sqlx::query("SELECT id,request_digest FROM online_executions WHERE request_id=?").bind(request_id).fetch_optional(&self.pool).await? {
            if row.get::<String,_>("request_digest") != fingerprint { return Err(Error::Conflict("请求标识已用于其他执行内容".into())); }
            return self.online_execution_document(&row.get::<String,_>("id")).await;
        }
        let workflow_id = crate::local_api::text(&request,"workflow_id")?;
        let workflow = self.get_workflow(workflow_id).await?;
        if workflow["enabled"] != true { return Err(Error::Invalid("工作流已停用")); }
        if request["version_id"].is_string() && request["draft_revision"].is_number() { return Err(Error::Invalid("执行版本与草稿修订不能同时指定")); }
        let version = request["version_id"].as_str().or_else(|| if request["draft_revision"].is_null() { workflow["default_version_id"].as_str() } else {None});
        let document = if let Some(version) = version { self.get_workflow_version(workflow_id,version).await?["definition"].clone() } else {
            if request["draft_revision"] != workflow["revision"] { return Err(Error::Conflict("请选择工作流版本或当前草稿修订".into())); }
            workflow["draft"].clone()
        };
        let definition = WorkflowDefinition::parse(document.clone())?;
        let ExecutionDefinition::Online { execution } = &definition.execution else { return Err(Error::Invalid("此接口需要在线工作流")); };
        let is_seedream = matches!(execution.provider,GenerationProvider::Seedream);
        let transport = if matches!(execution.provider,GenerationProvider::Minimax) {minimax_http()} else {execution.http.clone().unwrap_or_else(seedance_http)};
        let connection = sqlx::query("SELECT api_key,base_url,provider FROM generation_services WHERE id=?").bind(&execution.connection_id).fetch_optional(&self.pool).await?.ok_or(Error::Invalid("请选择生成服务连接"))?;
        let key: String = connection.get("api_key");
        let base: String = connection.get("base_url");
        let expected_provider = match execution.provider {GenerationProvider::Seedance=>"seedance",GenerationProvider::Seedream=>"seedream",GenerationProvider::Minimax=>"minimax",GenerationProvider::HttpJson=>"http_json"};
        if connection.get::<String,_>("provider") != expected_provider {return Err(Error::Invalid("服务连接与工作流类型不一致"));}
        let mut values = request["inputs"].as_object().ok_or(Error::Invalid("工作流输入需要对象"))?.clone();
        for field in &definition.inputs { if !values.contains_key(&field.key) { if let Some(value) = definition.input_schema["properties"][&field.key].get("default") { values.insert(field.key.clone(),value.clone()); } } }
        let inputs = Value::Object(values.clone());
        jsonschema::validator_for(&definition.input_schema).map_err(|_|Error::State("输入 Schema 无效"))?.validate(&inputs).map_err(|error|Error::Validation(error.to_string()))?;
        let mut body = if is_seedream {json!({"model":"","prompt":"","image":[],"size":"2K","watermark":true,"response_format":"url","stream":false,"output_format":"jpeg","sequential_image_generation":"disabled","sequential_image_generation_options":{"max_images":1}})} else {transport.body.clone()};
        for (key,value) in &execution.defaults {
            if is_seedream && key=="max_images" {body["sequential_image_generation_options"]["max_images"]=value.clone();}
            else if let Some(pointer) = if is_seedream {None} else {transport.fixed_mapping.get(key)} {*body.pointer_mut(pointer).ok_or(Error::Invalid("固定参数映射位置不存在"))?=value.clone();}
            else {body[key] = value.clone();}
        }
        if !matches!(execution.provider,GenerationProvider::HttpJson) || body.get("model").is_some() {body["model"] = json!(execution.model);}
        if matches!(execution.provider,GenerationProvider::Minimax) && execution.model!="MiniMax-H3-Max" {body.as_object_mut().ok_or(Error::State("生成参数无效"))?.remove("extra");}
        let mut content = Vec::new();
        let mut values: serde_json::Map<String,Value> = execution.defaults.iter().map(|(key,value)|(key.clone(),value.clone())).collect();
        values.extend(inputs.as_object().ok_or(Error::State("生成输入需要对象"))?.clone());
        for (field,target) in &execution.input_mapping {
            let Some(value) = values.get(field) else {continue};
            if target.starts_with('/') {
                let mapped = if value.get("asset_id").is_some() || value.as_array().is_some_and(|references|references.iter().any(|reference|reference.get("asset_id").is_some())) {
                    let mut media = Vec::new();
                    for reference in value.as_array().map(Vec::as_slice).unwrap_or_else(||std::slice::from_ref(value)) {
                        media.push(json!(self.generation_media(reference,if is_seedream {"image"} else {"file"},if is_seedream {30} else {20}).await?));
                    }
                    if value.is_array() {json!(media)} else {media.into_iter().next().ok_or(Error::Invalid("生成素材缺失"))?}
                } else {value.clone()};
                *body.pointer_mut(target).ok_or(Error::Invalid("HTTP 输入映射位置不存在"))? = mapped;
            }
            else if target == "prompt" { content.push(json!({"type":"text","text":value})); }
            else if let Some(role) = target.strip_prefix("content.") {
                let (kind,maximum)=match role {"first_frame"|"last_frame"|"reference_image"=>("image",30),"reference_video"=>("video",50),"reference_audio"=>("audio",15),_=>return Err(Error::Invalid("素材映射类型无效"))};
                if !matches!(execution.provider,GenerationProvider::Minimax) && !["first_frame","last_frame"].contains(&role) {return Err(Error::Invalid("此模型不支持参考素材"));}
                let media_type=format!("{kind}_url");
                for reference in value.as_array().map(Vec::as_slice).unwrap_or_else(||std::slice::from_ref(value)) {
                    let url=self.generation_media(reference,kind,maximum).await?;
                    content.push(json!({"type":media_type,"role":role,media_type.clone():{"url":url}}));
                }
            } else if ["resolution","ratio","duration","generate_audio","return_last_frame"].contains(&target.as_str()) { body[target] = value.clone(); }
            else { return Err(Error::Invalid("生成参数映射无效")); }
        }
        if let Some(prompt) = if matches!(execution.provider,GenerationProvider::Seedance|GenerationProvider::Minimax) {body.as_object_mut().ok_or(Error::State("生成默认参数无效"))?.remove("prompt")} else {None} {
            if !content.iter().any(|item| item["type"] == "text") { content.push(json!({"type":"text","text":prompt})); }
        }
        if matches!(execution.provider,GenerationProvider::Seedance) {body["content"] = json!(content);validate_seedance_body(&body, &execution.capability)?;}
        else if matches!(execution.provider,GenerationProvider::Minimax) {body["content"] = json!(content);crate::generation_providers::validate_minimax(&body,&execution.capability)?;}
        else if is_seedream {crate::generation_providers::validate_seedream(&mut body,&execution.capability)?;}
        if let Some(schema) = &transport.request_schema {jsonschema::validator_for(schema).map_err(|_|Error::Invalid("HTTP 请求 Schema 无效"))?.validate(&body).map_err(|error|Error::Validation(error.to_string()))?;}
        let id = Uuid::new_v4().to_string();
        let now = timestamp()?;
        sqlx::query("INSERT INTO online_executions(id,request_id,request_digest,workflow_id,workflow_version_id,session_id,definition,inputs,request_body,connection_id,status,created_at,updated_at) VALUES(?,?,?,?,?,?,?,?,?,?,'dispatching',?,?)")
            .bind(&id).bind(request_id).bind(fingerprint).bind(workflow_id).bind(version).bind(request["session_id"].as_str()).bind(document.to_string()).bind(inputs.to_string()).bind(body.to_string()).bind(&execution.connection_id).bind(&now).bind(&now).execute(&self.pool).await?;
        let client = reqwest::Client::builder().redirect(reqwest::redirect::Policy::none()).timeout(Duration::from_secs(90)).build()?;
        let response = if is_seedream {client.post(validate_base_url(&base)?.join("/api/v3/images/generations").map_err(|_|Error::Invalid("图片生成地址无效"))?).bearer_auth(&key).timeout(Duration::from_secs(600))} else {transport.request(&client,&base,&transport.submit,None,&key)?}.json(&body).send().await;
        match response {
            Ok(response) => {
                let status = response.status();
                sqlx::query("UPDATE online_executions SET status='submission_unknown',error='提交结果尚未确认',updated_at=? WHERE id=?").bind(timestamp()?).bind(&id).execute(&self.pool).await?;
                let result: Value = response.json().await?;
                sqlx::query("UPDATE online_executions SET provider_response=? WHERE id=?").bind(result.to_string()).bind(&id).execute(&self.pool).await?;
                if status.is_success() && is_seedream {
                    sqlx::query("UPDATE online_executions SET status='collecting',error='',updated_at=? WHERE id=?").bind(timestamp()?).bind(&id).execute(&self.pool).await?;
                } else if status.is_success() {
                    let task = result.pointer(&transport.task_id_pointer).and_then(Value::as_str).ok_or(Error::State("生成服务未返回任务编号"))?;
                    sqlx::query("UPDATE online_executions SET provider_task_id=?,status='pending',error='',updated_at=? WHERE id=?").bind(task).bind(timestamp()?).bind(&id).execute(&self.pool).await?;
                } else {
                    let message = result.pointer(&transport.error_pointer).and_then(Value::as_str).unwrap_or("生成服务拒绝请求");
                    sqlx::query("UPDATE online_executions SET status=?,error=?,updated_at=? WHERE id=?").bind(if status.is_server_error() {"submission_unknown"} else {"failed"}).bind(message).bind(timestamp()?).bind(&id).execute(&self.pool).await?;
                }
            }
            Err(_) => {sqlx::query("UPDATE online_executions SET status='submission_unknown',error='提交结果未知，禁止重复提交',updated_at=? WHERE id=?").bind(timestamp()?).bind(&id).execute(&self.pool).await?;}
        }
        self.online_execution_document(&id).await
    }

    async fn generation_media(&self, reference: &Value, kind: &str, maximum_mib: usize) -> Result<String> {
        let asset=self.get_asset(crate::local_api::text(reference,"asset_id")?).await?;
        let version=asset.versions.iter().find(|version|Some(version.id.as_str())==reference["asset_version_id"].as_str()).ok_or(Error::NotFound)?;
        if kind!="file" && !version.mime_type.starts_with(&format!("{kind}/")) {return Err(Error::Invalid("生成素材类型不一致"));}
        if version.size_bytes> (maximum_mib*1024*1024) as i64 {return Err(Error::Validation(format!("生成素材超过 {maximum_mib} MiB")));}
        let bytes=self.read_asset(&asset.id,&version.id).await?;
        Ok(format!("data:{};base64,{}",version.mime_type,STANDARD.encode(bytes)))
    }

    async fn online_execution_document(&self, id: &str) -> Result<Value> {
        let row = sqlx::query("SELECT * FROM online_executions WHERE id=?").bind(id).fetch_optional(&self.pool).await?.ok_or(Error::NotFound)?;
        let status: String = row.get("status");
        let waiting = row.get::<bool,_>("waiting") && !crate::executions::terminal(&status);
        let definition: Value=serde_json::from_str(&row.get::<String,_>("definition"))?;
        let provider=definition["execution"]["provider"].as_str().ok_or(Error::State("生成供应商缺失"))?;
        let can_cancel=online_cancellation_issue(provider,&status,definition["execution"]["http"]["cancel"].is_object()).is_none();
        let response: Option<Value>=row.get::<Option<String>,_>("provider_response").map(|value|serde_json::from_str(&value)).transpose()?;
        Ok(json!({"id":id,"workflow_id":row.get::<String,_>("workflow_id"),"workflow_version_id":row.get::<Option<String>,_>("workflow_version_id"),"node_id":"","session_id":row.get::<Option<String>,_>("session_id"),"status":status,"attempt_id":null,"prompt_id":row.get::<Option<String>,_>("provider_task_id"),"sequence":0,"cancel_requested":row.get::<bool,_>("cancel_requested"),"can_cancel":can_cancel,"waiting":waiting,"error":row.get::<String,_>("error"),"inputs":serde_json::from_str::<Value>(&row.get::<String,_>("inputs"))?,"usage":response.as_ref().map(|response|if provider=="minimax" {response["task"]["usage"].clone()} else {response["usage"].clone()}),"outputs":serde_json::from_str::<Value>(&row.get::<String,_>("outputs"))?,"created_at":row.get::<String,_>("created_at"),"updated_at":row.get::<String,_>("updated_at")}))
    }

    async fn collect_seedream_execution(&self, id: &str) -> Result<Value> {
        let row=sqlx::query("SELECT provider_response,outputs,session_id,definition FROM online_executions WHERE id=?").bind(id).fetch_one(&self.pool).await?;
        let response: Value=serde_json::from_str(&row.get::<Option<String>,_>("provider_response").ok_or(Error::State("图片生成响应尚未保存"))?)?;
        let mut outputs: Vec<Value>=serde_json::from_str(&row.get::<String,_>("outputs"))?;
        let session: Option<String>=row.get("session_id");
        let definition: Value=serde_json::from_str(&row.get::<String,_>("definition"))?;
        let field=definition["outputs"][0]["key"].as_str().ok_or(Error::State("图片输出字段缺失"))?;
        let mut errors=Vec::new();
        if let Some(message)=response["error"]["message"].as_str() {errors.push(message.to_owned());}
        let empty=Vec::new();
        let images=response["data"].as_array().unwrap_or(&empty);
        if images.is_empty() && errors.is_empty() {return Err(Error::State("图片生成响应缺少结果"));}
        let client=reqwest::Client::builder().redirect(reqwest::redirect::Policy::none()).timeout(Duration::from_secs(120)).build()?;
        for (index,image) in images.iter().enumerate() {
            if let Some(message)=image["error"]["message"].as_str() {errors.push(format!("图片 {}：{message}",index+1));continue;}
            if outputs.iter().any(|output|output["index"]==index) {continue;}
            let bytes=if let Some(encoded)=image["b64_json"].as_str() {STANDARD.decode(encoded).map_err(|_|Error::Invalid("生成图片 Base64 内容无效"))?} else {
                let address=url::Url::parse(image["url"].as_str().ok_or(Error::State("生成图片地址缺失"))?).map_err(|_|Error::Invalid("生成图片地址无效"))?;
                if address.scheme()!="https" || !address.username().is_empty() || address.password().is_some() || address.fragment().is_some() {return Err(Error::Invalid("生成图片需要不含账号的 HTTPS 地址"));}
                let response=client.get(address).send().await?.error_for_status()?;
                if response.content_length().is_some_and(|size|size>256*1024*1024) {return Err(Error::Invalid("生成图片超过 256 MiB"));}
                response.bytes().await?.to_vec()
            };
            if bytes.len()>256*1024*1024 {return Err(Error::Invalid("生成图片超过 256 MiB"));}
            let format=image::guess_format(&bytes).map_err(|_|Error::Invalid("生成结果不是有效图片"))?;
            let extension=format.extensions_str().first().ok_or(Error::Invalid("生成图片格式无法保存"))?;
            let asset=self.create_asset_bytes(session.as_deref(),&format!("{field}-{}.{extension}",index+1),&bytes,"workflow",&format!("online:{id}:{field}:{index}")).await?;
            let version=asset.versions.last().ok_or(Error::State("生成图片缺少资产版本"))?;
            self.place_video_generation_output(id,&asset.id,&version.id).await?;
            outputs.push(json!({"output_id":format!("{id}:{field}:{index}"),"field_key":field,"index":index,"sha256":digest(&bytes),"size_bytes":bytes.len(),"media_type":version.mime_type,"asset_id":asset.id,"asset_version_id":version.id}));
            sqlx::query("UPDATE online_executions SET outputs=?,updated_at=? WHERE id=?").bind(json!(outputs).to_string()).bind(timestamp()?).bind(id).execute(&self.pool).await?;
        }
        let status=if errors.is_empty() {"succeeded"} else {"failed"};
        sqlx::query("UPDATE online_executions SET status=?,error=?,updated_at=? WHERE id=?").bind(status).bind(errors.join("；")).bind(timestamp()?).bind(id).execute(&self.pool).await?;
        self.online_execution_document(id).await
    }

    pub(crate) async fn refresh_online_execution(&self, id: &str) -> Result<Value> {
        let _guard = self.model_updates.lock().await;
        let row = sqlx::query("SELECT * FROM online_executions WHERE id=?").bind(id).fetch_optional(&self.pool).await?.ok_or(Error::NotFound)?;
        let current: String = row.get("status");
        if current == "dispatching" {
            sqlx::query("UPDATE online_executions SET status='submission_unknown',error='提交结果未知，需要核实服务任务记录',updated_at=? WHERE id=?").bind(timestamp()?).bind(id).execute(&self.pool).await?;
            return self.online_execution_document(id).await;
        }
        if current == "succeeded" {
            let mut outputs: Vec<Value> = serde_json::from_str(&row.get::<String,_>("outputs"))?;
            if !outputs.iter().any(|output|output["field_key"]=="first_frame") {
                let videos: Vec<Value> = outputs.iter().filter(|output|output["media_type"].as_str().is_some_and(|kind|kind.starts_with("video/"))).cloned().collect();
                if !videos.is_empty() {
                    outputs.retain(|output|output["field_key"]!="first_frame" && output["field_key"]!="last_frame");
                    let session: Option<String> = row.get("session_id");
                    for video in videos {outputs.extend(self.ensure_video_boundary_outputs(session.as_deref(),id,crate::local_api::text(&video,"asset_id")?,crate::local_api::text(&video,"asset_version_id")?,video["index"].as_i64().unwrap_or(0)).await?);}
                    sqlx::query("UPDATE online_executions SET outputs=?,updated_at=? WHERE id=?").bind(json!(outputs).to_string()).bind(timestamp()?).bind(id).execute(&self.pool).await?;
                }
            }
        }
        if !matches!(current.as_str(),"pending"|"running"|"collecting") {return self.online_execution_document(id).await;}
        let definition = WorkflowDefinition::parse(serde_json::from_str(&row.get::<String,_>("definition"))?)?;
        let ExecutionDefinition::Online {execution} = &definition.execution else {return Err(Error::State("在线执行定义无效"));};
        if matches!(execution.provider,GenerationProvider::Seedream) {return self.collect_seedream_execution(id).await;}
        let Some(task) = row.get::<Option<String>,_>("provider_task_id") else {return self.online_execution_document(id).await;};
        let transport = if matches!(execution.provider,GenerationProvider::Minimax) {minimax_http()} else {execution.http.clone().unwrap_or_else(seedance_http)};
        let connection = sqlx::query("SELECT api_key,base_url FROM generation_services WHERE id=?").bind(row.get::<String,_>("connection_id")).fetch_one(&self.pool).await?;
        let key: String = connection.get("api_key");
        let base: String = connection.get("base_url");
        let client = reqwest::Client::builder().redirect(reqwest::redirect::Policy::none()).timeout(Duration::from_secs(120)).build()?;
        let response = transport.request(&client,&base,&transport.query,Some(&task),&key)?.send().await?.error_for_status()?.json::<Value>().await?;
        let provider_status = response.pointer(&transport.status_pointer).and_then(Value::as_str).ok_or(Error::State("生成服务未返回状态"))?;
        let status = transport.status_mapping.get(provider_status).ok_or(Error::State("生成服务状态尚未映射"))?;
        sqlx::query("UPDATE online_executions SET status=?,error=?,provider_response=?,updated_at=? WHERE id=?").bind(status).bind(response.pointer(&transport.error_pointer).and_then(Value::as_str).unwrap_or("")).bind(response.to_string()).bind(timestamp()?).bind(id).execute(&self.pool).await?;
        if status == "collecting" {
            let session: Option<String> = row.get("session_id");
            let mut outputs = Vec::new();
            for (field, path) in execution.output_mapping.clone() {
                let pointer = if path.starts_with('/') {path} else {format!("/{}",path.replace('.',"/"))};
                let Some(url) = response.pointer(&pointer).and_then(Value::as_str) else { if definition.outputs.iter().any(|output|output.key==field) {return Err(Error::State("生成结果缺少声明的输出"));} continue; };
                let address = url::Url::parse(url).map_err(|_|Error::Invalid("生成产物地址无效"))?;
                if address.scheme() != "https" || !address.username().is_empty() || address.password().is_some() || address.fragment().is_some() {return Err(Error::Invalid("生成产物需要不含账号的 HTTPS 地址"));}
                let response = client.get(url).send().await?.error_for_status()?;
                if response.content_length().is_some_and(|size|size>256*1024*1024) {return Err(Error::Invalid("生成产物超过 256 MiB"));}
                let bytes = response.bytes().await?;
                if bytes.len()>256*1024*1024 {return Err(Error::Invalid("生成产物超过 256 MiB"));}
                let name = if field.contains("frame") {format!("{field}.jpg")} else {format!("{field}.mp4")};
                let asset = self.create_asset_bytes(session.as_deref(),&name,&bytes,"workflow",&format!("online:{id}:{field}")).await?;
                let version = asset.versions.last().ok_or(Error::State("生成资产缺少版本"))?;
                self.place_video_generation_output(id,&asset.id,&version.id).await?;
                outputs.push(json!({"output_id":format!("{id}:{field}"),"field_key":field,"index":0,"sha256":digest(&bytes),"size_bytes":bytes.len(),"media_type":version.mime_type,"asset_id":asset.id,"asset_version_id":version.id}));
                if version.mime_type.starts_with("video/") {
                    outputs.extend(self.ensure_video_boundary_outputs(session.as_deref(),id,&asset.id,&version.id,0).await?);
                }
            }
            sqlx::query("UPDATE online_executions SET status='succeeded',outputs=?,updated_at=? WHERE id=?").bind(json!(outputs).to_string()).bind(timestamp()?).bind(id).execute(&self.pool).await?;
        }
        self.online_execution_document(id).await
    }

    pub(crate) async fn cancel_online_execution(&self, id: &str, stop_waiting: bool) -> Result<Value> {
        if stop_waiting {sqlx::query("UPDATE online_executions SET waiting=0,updated_at=? WHERE id=?").bind(timestamp()?).bind(id).execute(&self.pool).await?;return self.online_execution_document(id).await;}
        let current = self.refresh_online_execution(id).await?;
        if crate::executions::terminal(crate::local_api::text(&current,"status")?) { return Ok(current); }
        let row = sqlx::query("SELECT * FROM online_executions WHERE id=?").bind(id).fetch_optional(&self.pool).await?.ok_or(Error::NotFound)?;
        if !crate::executions::terminal(&row.get::<String,_>("status")) {
            let task = row.get::<Option<String>,_>("provider_task_id").ok_or(Error::Invalid("提交结果未知，无法取消未确认的任务"))?;
            let document: Value = serde_json::from_str(&row.get::<String,_>("definition"))?;
            let provider = crate::local_api::text(&document["execution"],"provider")?;
            if let Some(issue) = online_cancellation_issue(provider,&row.get::<String,_>("status"),document["execution"]["http"]["cancel"].is_object()) { return Err(Error::Invalid(issue)); }
            let definition = WorkflowDefinition::parse(document)?;
            let ExecutionDefinition::Online {execution} = definition.execution else {return Err(Error::State("在线执行定义无效"));};
            let transport = execution.http.unwrap_or_else(seedance_http);
            let operation = transport.cancel.as_ref().ok_or(Error::Invalid("此服务不支持取消任务"))?;
            let connection = sqlx::query("SELECT api_key,base_url FROM generation_services WHERE id=?").bind(row.get::<String,_>("connection_id")).fetch_one(&self.pool).await?;
            let key: String = connection.get("api_key");
            let base: String = connection.get("base_url");
            let client = reqwest::Client::builder().redirect(reqwest::redirect::Policy::none()).timeout(Duration::from_secs(30)).build()?;
            let cancel = transport.request(&client,&base,operation,Some(&task),&key)?;
            sqlx::query("UPDATE online_executions SET cancel_requested=1,updated_at=? WHERE id=?").bind(timestamp()?).bind(id).execute(&self.pool).await?;
            let response = cancel.send().await?;
            if response.status().is_client_error() && response.status() != reqwest::StatusCode::REQUEST_TIMEOUT || response.status().is_redirection() {
                sqlx::query("UPDATE online_executions SET cancel_requested=0,updated_at=? WHERE id=?").bind(timestamp()?).bind(id).execute(&self.pool).await?;
            }
            let response = response.error_for_status()?;
            if !response.status().is_success() { return Err(Error::Invalid("生成服务未确认取消请求")); }
            if matches!(execution.provider,GenerationProvider::Seedance) { return self.refresh_online_execution(id).await; }
            if transport.cancel_confirms_termination {sqlx::query("UPDATE online_executions SET status='cancelled',updated_at=? WHERE id=?").bind(timestamp()?).bind(id).execute(&self.pool).await?;}
        }
        self.online_execution_document(id).await
    }

    pub(crate) async fn list_online_execution_documents(&self, workflow: Option<&str>, session: Option<&str>) -> Result<Vec<Value>> {
        let ids:Vec<String> = sqlx::query_scalar("SELECT id FROM online_executions WHERE (? IS NULL OR workflow_id=?) AND (? IS NULL OR session_id=?) ORDER BY created_at DESC,id").bind(workflow).bind(workflow).bind(session).bind(session).fetch_all(&self.pool).await?;
        let mut result = Vec::new();
        for id in ids { result.push(self.online_execution_document(&id).await?); }
        Ok(result)
    }
}
