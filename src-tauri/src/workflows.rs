use crate::{Error, Result, Store, local_api::text};
use serde_json::{Value, json};
use std::collections::BTreeMap;

pub(crate) fn requested_workflows(input: &Value) -> Result<Vec<(String, Option<String>)>> {
    let mut selected: BTreeMap<String, Option<String>> = BTreeMap::new();
    if let Some(parts) = input.get("parts").filter(|parts| !parts.is_null()) {
        for part in parts.as_array().ok_or(Error::Invalid("消息内容块需要数组"))? {
            if part["type"] != "workflow_ref" { continue; }
            let id = text(part, "workflow_id")?;
            if id.is_empty() { return Err(Error::Invalid("工作流标识不能为空")); }
            let version = part.get("version_id").filter(|value| !value.is_null()).map(|value| {
                value.as_str().filter(|value| !value.is_empty()).map(str::to_owned)
                    .ok_or(Error::Invalid("工作流版本标识无效"))
            }).transpose()?;
            if selected.get(id).and_then(Option::as_ref).zip(version.as_ref())
                .is_some_and(|(previous, version)| previous != version) {
                return Err(Error::Invalid("同一工作流需要引用相同版本"));
            }
            if version.is_some() || !selected.contains_key(id) { selected.insert(id.to_owned(), version); }
        }
    }
    Ok(selected.into_iter().collect())
}

impl Store {
    pub(crate) async fn workflow_version(&self, id: &str, version: Option<&str>) -> Result<Value> {
        let workflow = self.get_workflow(id).await?;
        if workflow["enabled"] != true { return Err(Error::Invalid("工作流已停用")); }
        let version = version.or_else(|| workflow["default_version_id"].as_str())
            .ok_or(Error::Invalid("工作流尚未设置默认版本"))?;
        let version = self.get_workflow_version(id, version).await?;
        let definition = &version["definition"];
        Ok(json!({
            "id": id, "version_id": version["id"], "version": version["version"],
            "name": workflow["name"], "category": workflow["category"], "categories": [workflow["category"]], "tags": workflow["tags"],
            "type": definition["type"], "description": definition["description"], "usage": definition["usage"],
            "preview": definition["preview"], "examples": definition["examples"],
            "input_fields": definition["inputs"], "output_fields": definition["outputs"],
            "inputs": definition["inputs"].as_array().map(Vec::len), "outputs": definition["outputs"].as_array().map(Vec::len),
            "input_schema": definition["input_schema"], "execution": definition["execution"],
            "content_digest": version["content_digest"],
        }))
    }
}
