use crate::{Error, Result};
use reqwest::{Client, RequestBuilder};
use serde::{Deserialize, Serialize};
use serde_json::{Value, json};
use std::collections::BTreeMap;
use url::Url;

#[derive(Debug, Clone, Deserialize, Serialize)]
#[serde(deny_unknown_fields)]
pub(crate) struct HttpOperation {
    pub method: String,
    pub path: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub body: Option<Value>,
}

fn operation_body(value: &Value, task: Option<&str>) -> Value {
    match value {
        Value::String(text) => Value::String(text.replace("{task_id}",task.unwrap_or(""))),
        Value::Array(values) => Value::Array(values.iter().map(|value|operation_body(value,task)).collect()),
        Value::Object(values) => Value::Object(values.iter().map(|(key,value)|(key.clone(),operation_body(value,task))).collect()),
        _=>value.clone(),
    }
}

#[derive(Debug, Clone, Deserialize, Serialize)]
#[serde(deny_unknown_fields)]
pub(crate) struct HttpJsonTemplate {
    pub submit: HttpOperation,
    pub query: HttpOperation,
    pub cancel: Option<HttpOperation>,
    #[serde(default)]
    pub cancel_confirms_termination: bool,
    pub body: Value,
    pub task_id_pointer: String,
    pub status_pointer: String,
    pub error_pointer: String,
    pub status_mapping: BTreeMap<String, String>,
    pub authentication: String,
    pub authentication_header: Option<String>,
    #[serde(default)]
    pub fixed_mapping: BTreeMap<String,String>,
    #[serde(default)]
    pub request_schema: Option<Value>,
}

pub(crate) fn validate_base_url(value: &str) -> Result<Url> {
    let url = Url::parse(value).map_err(|_|Error::Invalid("生成服务地址无效"))?;
    if url.scheme() != "https" || url.host_str().is_none() || !url.username().is_empty() || url.password().is_some() || url.query().is_some() || url.fragment().is_some() {
        return Err(Error::Invalid("生成服务地址需要不含账号、查询参数的 HTTPS 地址"));
    }
    Ok(url)
}

impl HttpJsonTemplate {
    pub(crate) fn validate(&self) -> Result<()> {
        if self.submit.method != "POST" || !["GET","POST"].contains(&self.query.method.as_str()) || self.cancel.as_ref().is_some_and(|op| !["POST","DELETE"].contains(&op.method.as_str())) {return Err(Error::Invalid("生成服务请求方法无效"));}
        for operation in [&self.submit,&self.query].into_iter().chain(self.cancel.as_ref()) {
            if !operation.path.starts_with('/') || operation.path.starts_with("//") || operation.path.contains("://") || operation.path.contains('#') {return Err(Error::Invalid("生成服务请求需要同源相对路径"));}
            if operation.method == "GET" && operation.body.is_some() {return Err(Error::Invalid("GET 请求不能包含 JSON 内容"));}
        }
        for pointer in [&self.task_id_pointer,&self.status_pointer,&self.error_pointer] {
            if !pointer.starts_with('/') {return Err(Error::Invalid("生成结果字段需要 JSON pointer"));}
        }
        if !self.body.is_object() || self.status_mapping.is_empty() || self.status_mapping.values().any(|status| !["pending","running","collecting","failed","cancelled"].contains(&status.as_str())) {return Err(Error::Invalid("生成服务状态映射或请求内容无效"));}
        if !["bearer","header","none"].contains(&self.authentication.as_str()) {return Err(Error::Invalid("生成服务认证方式无效"));}
        if self.authentication == "header" {
            let name = self.authentication_header.as_deref().ok_or(Error::Invalid("填写认证 Header"))?;
            reqwest::header::HeaderName::from_bytes(name.as_bytes()).map_err(|_|Error::Invalid("认证 Header 无效"))?;
            if ["host","content-length","content-type"].contains(&name.to_ascii_lowercase().as_str()) {return Err(Error::Invalid("认证 Header 不能覆盖请求地址或内容类型"));}
        }
        if let Some(schema) = &self.request_schema {jsonschema::validator_for(schema).map_err(|_|Error::Invalid("HTTP 请求 Schema 无效"))?;}
        for pointer in self.fixed_mapping.values() {if !pointer.starts_with('/') || self.body.pointer(pointer).is_none() {return Err(Error::Invalid("固定参数需要映射到请求内容中的 JSON pointer"));}}
        Ok(())
    }

    pub(crate) fn request(&self, client: &Client, base: &str, operation: &HttpOperation, task: Option<&str>, key: &str) -> Result<RequestBuilder> {
        self.validate()?;
        if self.authentication != "none" && key.trim().is_empty() {return Err(Error::Invalid("生成服务连接缺少 API Key"));}
        let base = validate_base_url(base)?;
        let encoded = task.map(|id| url::form_urlencoded::byte_serialize(id.as_bytes()).collect::<String>()).unwrap_or_default();
        let path = operation.path.replace("{task_id}",&encoded);
        let url = base.join(&path).map_err(|_|Error::Invalid("生成服务路径无效"))?;
        if url.origin() != base.origin() {return Err(Error::Invalid("生成服务请求地址超出连接范围"));}
        let request = client.request(operation.method.parse().map_err(|_|Error::Invalid("请求方法无效"))?,url);
        let request = match &operation.body {Some(body)=>request.json(&operation_body(body,task)),None=>request};
        Ok(match self.authentication.as_str() {
            "bearer" => request.bearer_auth(key),
            "header" => request.header(self.authentication_header.as_deref().ok_or(Error::Invalid("认证 Header 缺失"))?,key),
            "none" => request,
            _ => return Err(Error::Invalid("认证方式无效")),
        })
    }
}

pub(crate) fn seedance_http() -> HttpJsonTemplate {
    serde_json::from_value(json!({"submit":{"method":"POST","path":"/api/v3/contents/generations/tasks"},"query":{"method":"GET","path":"/api/v3/contents/generations/tasks/{task_id}"},"cancel":{"method":"DELETE","path":"/api/v3/contents/generations/tasks/{task_id}"},"cancel_confirms_termination":true,"body":{"model":"doubao-seedance-2-0-260128","content":[{"type":"text","text":""}],"resolution":"720p","ratio":"16:9","duration":4,"generate_audio":true,"return_last_frame":true},"task_id_pointer":"/id","status_pointer":"/status","error_pointer":"/error/message","status_mapping":{"queued":"pending","running":"running","succeeded":"collecting","failed":"failed","expired":"failed","cancelled":"cancelled"},"authentication":"bearer","authentication_header":null})).expect("内置 Seedance HTTP 定义无效")
}

pub(crate) fn minimax_http() -> HttpJsonTemplate {
    serde_json::from_value(json!({"submit":{"method":"POST","path":"/v2/video_generation"},"query":{"method":"GET","path":"/v2/query/video_generation/{task_id}"},"cancel":null,"body":{"model":"MiniMax-H3","content":[],"resolution":"768P","ratio":"16:9","duration":5,"extra":{"prompt_expansion_mode":"balanced"}},"task_id_pointer":"/task_id","status_pointer":"/task/status","error_pointer":"/task/error/message","status_mapping":{"queued":"pending","running":"running","succeeded":"collecting","failed":"failed","cancelled":"cancelled"},"authentication":"bearer","authentication_header":null,"fixed_mapping":{"prompt_expansion_mode":"/extra/prompt_expansion_mode"}})).expect("内置 MiniMax HTTP 定义无效")
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn requests_preserve_connection_origin_and_encode_task_identifier() -> Result<()> {
        let template = seedance_http();
        let request = template.request(&Client::new(),"https://ark.cn-beijing.volces.com",&template.query,Some("task/a?b"),"credential")?.build()?;
        assert_eq!(request.url().host_str(),Some("ark.cn-beijing.volces.com"));
        assert!(request.url().path().ends_with("task%2Fa%3Fb"));
        assert!(request.url().query().is_none());
        assert!(validate_base_url("http://example.com").is_err());
        assert!(validate_base_url("https://user:password@example.com").is_err());
        Ok(())
    }

    #[test]
    fn template_rejects_foreign_paths_and_missing_authentication() {
        let mut template = seedance_http();
        template.query.path = "//example.com/tasks/{task_id}".into();
        assert!(template.validate().is_err());
        template.query.path = "/tasks/{task_id}".into();
        assert!(template.request(&Client::new(),"https://example.com",&template.query,Some("task"),"").is_err());
        template.authentication = "none".into();
        assert!(template.request(&Client::new(),"https://example.com",&template.query,Some("task"),"").is_ok());
        template.status_mapping.insert("complete".into(),"unknown".into());
        assert!(template.validate().is_err());
    }

    #[test]
    fn post_query_maps_task_identifier_into_json_body() -> Result<()> {
        let mut template = seedance_http();
        template.query = HttpOperation {method:"POST".into(),path:"/tasks/query".into(),body:Some(json!({"task":{"id":"{task_id}"}}))};
        let request = template.request(&Client::new(),"https://example.com",&template.query,Some("task/a"),"credential")?.build()?;
        let bytes = request.body().and_then(reqwest::Body::as_bytes).ok_or(Error::State("查询请求内容缺失"))?;
        assert_eq!(serde_json::from_slice::<Value>(bytes)?,json!({"task":{"id":"task/a"}}));
        template.fixed_mapping.insert("duration".into(),"/missing".into());
        assert!(template.validate().is_err());
        Ok(())
    }
}
