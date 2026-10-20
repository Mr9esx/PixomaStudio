use crate::{Error, Result, Store, assets::digest, error::timestamp, local_api::text};
use serde_json::{Value, json};
use std::collections::{BTreeMap, HashSet};

pub(crate) fn requested_workflows(input: &Value) -> Result<Vec<(String, Option<u64>)>> {
    let mut selected = BTreeMap::new();
    if let Some(parts) = input.get("parts").filter(|parts| !parts.is_null()) {
        for part in parts
            .as_array()
            .ok_or(Error::Invalid("消息内容块需要数组"))?
        {
            if part["type"] != "workflow_ref" {
                continue;
            }
            let id = text(part, "workflow_id")?;
            workflow_id(id)?;
            let version = part
                .get("version_id")
                .filter(|version| !version.is_null())
                .map(|version| {
                    version
                        .as_u64()
                        .filter(|version| *version > 0 && *version <= i64::MAX as u64)
                        .ok_or(Error::Invalid("工作流版本标识无效"))
                })
                .transpose()?;
            if selected
                .get(id)
                .and_then(|version: &Option<u64>| *version)
                .zip(version)
                .is_some_and(|(previous, version)| previous != version)
            {
                return Err(Error::Invalid("同一工作流需要引用相同版本"));
            }
            if version.is_some() || !selected.contains_key(id) {
                selected.insert(id.to_owned(), version);
            }
        }
    }
    Ok(selected.into_iter().collect())
}

fn workflow_id(id: &str) -> Result<u64> {
    id.parse::<u64>()
        .ok()
        .filter(|value| *value > 0 && value.to_string() == id)
        .ok_or(Error::Invalid("工作流标识无效"))
}

impl Store {
    pub(crate) async fn cache_official_workflow(
        &self,
        account: &str,
        mut workflow: Value,
        source_url: &str,
    ) -> Result<()> {
        Self::check_account(account)?;
        if serde_json::to_vec(&workflow)?.len() > 1 << 20 {
            return Err(Error::Invalid("官方工作流目录内容超过 1 MiB"));
        }
        let id = text(&workflow, "id")?.to_owned();
        workflow_id(&id)?;
        let version = workflow["version_id"]
            .as_u64()
            .filter(|version| *version > 0 && *version <= i64::MAX as u64)
            .ok_or(Error::Invalid("工作流版本标识无效"))?;
        if text(&workflow, "name")?.trim().is_empty() {
            return Err(Error::Invalid("工作流名称不能为空"));
        }
        text(&workflow, "description")?;
        if !workflow["input_schema"].is_object() {
            return Err(Error::Invalid("工作流输入 Schema 需要对象"));
        }
        jsonschema::validator_for(&workflow["input_schema"])
            .map_err(|_| Error::Invalid("工作流输入 Schema 无效"))?;
        for property in ["input_fields", "output_fields"] {
            let mut keys = HashSet::new();
            for field in workflow[property]
                .as_array()
                .ok_or(Error::Invalid("工作流输入输出字段需要数组"))?
            {
                if text(field, "key")?.is_empty()
                    || text(field, "type")?.is_empty()
                    || !keys.insert(text(field, "key")?)
                {
                    return Err(Error::Invalid("工作流字段缺少名称、类型或名称重复"));
                }
            }
        }
        let source =
            url::Url::parse(source_url).map_err(|_| Error::Invalid("工作流来源地址无效"))?;
        if !source.username().is_empty() || source.password().is_some() {
            return Err(Error::Invalid("工作流来源地址不能包含凭据"));
        }
        let mut canonical = workflow.clone();
        canonical.sort_all_objects();
        let content_digest = digest(&serde_json::to_vec(&canonical)?);
        workflow["source_url"] = json!(source_url);
        workflow["content_digest"] = json!(content_digest);
        workflow["received_at"] = json!(timestamp()?);
        let mut transaction = self.pool.begin().await?;
        let existing: Option<String> = sqlx::query_scalar("SELECT content_digest FROM workflow_versions WHERE account_id=? AND workflow_id=? AND version_id=?")
            .bind(account).bind(&id).bind(version as i64).fetch_optional(&mut *transaction).await?;
        if existing.is_some_and(|existing| existing != content_digest) {
            return Err(Error::State("官方工作流同一版本的目录内容发生变化"));
        }
        sqlx::query("INSERT INTO workflow_catalog(account_id,id,version_id) VALUES(?,?,?) ON CONFLICT(account_id,id) DO UPDATE SET version_id=excluded.version_id")
            .bind(account).bind(&id).bind(version as i64).execute(&mut *transaction).await?;
        sqlx::query("INSERT INTO workflow_versions(account_id,workflow_id,version_id,content_digest,document) VALUES(?,?,?,?,?) ON CONFLICT DO NOTHING")
            .bind(account).bind(&id).bind(version as i64).bind(content_digest).bind(workflow.to_string()).execute(&mut *transaction).await?;
        transaction.commit().await?;
        Ok(())
    }

    pub(crate) async fn workflow_version(
        &self,
        account: &str,
        id: &str,
        version: Option<u64>,
    ) -> Result<Value> {
        Self::check_account(account)?;
        workflow_id(id)?;
        if version.is_some_and(|version| version == 0 || version > i64::MAX as u64) {
            return Err(Error::Invalid("工作流版本标识无效"));
        }
        let document: String = sqlx::query_scalar("SELECT v.document FROM workflow_versions v JOIN workflow_catalog c ON c.account_id=v.account_id AND c.id=v.workflow_id WHERE v.account_id=? AND v.workflow_id=? AND v.version_id=coalesce(?,c.version_id)")
            .bind(account).bind(id).bind(version.map(|version| version as i64)).fetch_optional(&self.pool).await?.ok_or(Error::NotFound)?;
        Ok(serde_json::from_str(&document)?)
    }
}
