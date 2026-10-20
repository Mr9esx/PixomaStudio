use crate::{
    Error, Result, Store,
    assets::digest,
    credentials,
    error::{required, timestamp},
};
use serde::{Deserialize, Serialize};
use serde_json::{Value, json};
use sqlx::Row;
use url::Url;
use uuid::Uuid;

#[derive(Deserialize, Serialize)]
#[serde(deny_unknown_fields)]
pub(crate) struct ModelLimits {
    pub context_window_tokens: u64,
    pub max_input_tokens: u64,
    pub max_output_tokens: u64,
}

#[derive(Deserialize, Serialize)]
#[serde(deny_unknown_fields)]
pub(crate) struct ModelThinking {
    pub enabled: bool,
    pub effort: Option<String>,
    pub budget_tokens: Option<u64>,
}

#[derive(Deserialize, Serialize)]
#[serde(deny_unknown_fields)]
pub(crate) struct ModelCapabilities {
    pub tools: bool,
    pub vision: bool,
    pub image_output: bool,
    pub streaming: bool,
}

#[derive(Deserialize, Serialize)]
#[serde(deny_unknown_fields)]
pub(crate) struct ModelInput {
    pub name: String,
    pub protocol: String,
    pub base_url: String,
    pub model: String,
    #[serde(default, skip_serializing)]
    pub api_key: String,
    pub enabled: bool,
    pub agent_enabled: bool,
    pub default: bool,
    pub limits: ModelLimits,
    pub thinking: ModelThinking,
    pub capabilities: ModelCapabilities,
}

impl ModelInput {
    pub(crate) fn validate(&mut self) -> Result<()> {
        self.name = self.name.trim().to_owned();
        self.model = self.model.trim().to_owned();
        self.base_url = self.base_url.trim().trim_end_matches('/').to_owned();
        required(&self.name, "模型名称不能为空")?;
        required(&self.model, "模型标识不能为空")?;
        if ![
            "openai_responses",
            "openai_chat_compatible",
            "anthropic_messages_compatible",
        ]
        .contains(&self.protocol.as_str())
        {
            return Err(Error::Invalid("模型协议无效"));
        }
        let address = Url::parse(&self.base_url).map_err(|_| Error::Invalid("模型地址无效"))?;
        if !["http", "https"].contains(&address.scheme())
            || address.host_str().is_none()
            || !address.username().is_empty()
            || address.password().is_some()
            || address.query().is_some()
            || address.fragment().is_some()
        {
            return Err(Error::Invalid("模型地址无效"));
        }
        let limits = &self.limits;
        if limits.context_window_tokens == 0
            || limits.max_input_tokens == 0
            || limits.max_output_tokens == 0
            || limits.max_input_tokens > limits.context_window_tokens
            || limits.max_output_tokens > limits.context_window_tokens
            || limits.max_output_tokens > u32::MAX as u64
        {
            return Err(Error::Invalid("模型上下文与输入输出限制无效"));
        }
        if self.default && (!self.enabled || !self.agent_enabled) {
            return Err(Error::Invalid("默认模型需要启用并允许 Agent 使用"));
        }
        if let Some(effort) = &self.thinking.effort
            && !["none", "minimal", "low", "medium", "high", "xhigh", "max"]
                .contains(&effort.as_str())
        {
            return Err(Error::Invalid("模型思考强度无效"));
        }
        if self.thinking.enabled
            && self.protocol == "anthropic_messages_compatible"
            && !self
                .thinking
                .budget_tokens
                .is_some_and(|budget| budget >= 1024 && budget < limits.max_output_tokens)
        {
            return Err(Error::Invalid(
                "思考预算需要至少 1024 tokens，并小于输出限制",
            ));
        }
        Ok(())
    }
}

impl Store {
    pub(crate) async fn list_models(&self, account: &str, agent_only: bool) -> Result<Value> {
        Self::check_account(account)?;
        let rows = sqlx::query("SELECT * FROM model_configs WHERE account_id=? AND (?=0 OR (json_extract(document,'$.enabled')=1 AND json_extract(document,'$.agent_enabled')=1)) ORDER BY created_at,id")
            .bind(account).bind(agent_only).fetch_all(&self.pool).await?;
        let mut models = Vec::with_capacity(rows.len());
        for row in rows {
            let mut document: Value = serde_json::from_str(row.get("document"))?;
            let has_key = row.get::<Option<String>, _>("credential_ref").is_some();
            document["id"] = json!(row.get::<String, _>("id"));
            document["has_api_key"] = json!(has_key);
            document["api_key_masked"] = json!(if has_key {
                "••••••••"
            } else {
                ""
            });
            models.push(document);
        }
        Ok(json!(models))
    }

    pub(crate) async fn save_model(
        &self,
        account: &str,
        id: Option<&str>,
        input: Value,
    ) -> Result<Value> {
        Self::check_account(account)?;
        let mut input: ModelInput =
            serde_json::from_value(input).map_err(|_| Error::Invalid("模型配置字段无效"))?;
        input.validate()?;
        let _model_update = self.model_updates.lock().await;
        let existing = match id {
            Some(id) => Some(
                sqlx::query("SELECT credential_ref FROM model_configs WHERE account_id=? AND id=?")
                    .bind(account)
                    .bind(id)
                    .fetch_optional(&self.pool)
                    .await?
                    .ok_or(Error::NotFound)?,
            ),
            None => None,
        };
        let id = id
            .map(str::to_owned)
            .unwrap_or_else(|| Uuid::new_v4().to_string());
        let previous: Option<String> = existing.as_ref().and_then(|row| row.get("credential_ref"));
        let credential = if input.api_key.is_empty() {
            previous.clone()
        } else {
            let reference = format!("model:{}:{}", digest(account.as_bytes()), Uuid::new_v4());
            credentials::save(&reference, &input.api_key)?;
            Some(reference)
        };
        let now = timestamp()?;
        let document = serde_json::to_string(&input)?;
        let mut transaction = self.pool.begin().await?;
        if input.default {
            sqlx::query("UPDATE model_configs SET document=json_set(document,'$.default',json('false')),updated_at=? WHERE account_id=? AND json_extract(document,'$.default')=1")
                .bind(&now).bind(account).execute(&mut *transaction).await?;
        }
        sqlx::query("INSERT INTO model_configs(id,account_id,document,credential_ref,created_at,updated_at) VALUES(?,?,?,?,?,?) ON CONFLICT(id,account_id) DO UPDATE SET document=excluded.document,credential_ref=excluded.credential_ref,updated_at=excluded.updated_at")
            .bind(&id).bind(account).bind(document).bind(&credential).bind(&now).bind(&now).execute(&mut *transaction).await?;
        transaction.commit().await?;
        if previous != credential
            && let Some(previous) = previous
        {
            let retained: bool = sqlx::query_scalar("SELECT EXISTS(SELECT 1 FROM runs WHERE account_id=? AND status NOT IN ('succeeded','failed','cancelled') AND json_extract(model_snapshot,'$.credential_ref')=?) OR EXISTS(SELECT 1 FROM sessions WHERE account_id=? AND json_extract(expert_snapshot,'$.runtime_model_snapshot.credential_ref')=?)")
                .bind(account).bind(&previous).bind(account).bind(&previous).fetch_one(&self.pool).await?;
            if !retained {
                credentials::remove(&previous)?;
            }
        }
        self.list_models(account, false)
            .await?
            .as_array()
            .ok_or(Error::State("模型列表格式无效"))?
            .iter()
            .find(|model| model["id"] == id)
            .cloned()
            .ok_or(Error::NotFound)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    struct TestCredentials(Vec<String>);

    impl Drop for TestCredentials {
        fn drop(&mut self) {
            for reference in &self.0 {
                credentials::remove(reference).expect("删除检查凭据失败");
            }
        }
    }

    fn credential_configuration() -> Value {
        json!({
            "name": "凭据生命周期检查", "protocol": "openai_chat_compatible", "base_url": "http://127.0.0.1:1/v1", "model": "local-model",
            "api_key": Uuid::new_v4().to_string(), "enabled": true, "agent_enabled": true, "default": false,
            "limits": { "context_window_tokens": 32000, "max_input_tokens": 24000, "max_output_tokens": 4000 },
            "thinking": { "enabled": false }, "capabilities": { "tools": true, "vision": false, "image_output": false, "streaming": true }
        })
    }

    #[tokio::test]
    async fn clearing_sessions_removes_rotated_credentials_retained_by_ordinary_runs()
    -> std::result::Result<(), Box<dyn std::error::Error>> {
        let root = std::path::PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../.local-cache");
        std::fs::create_dir_all(&root)?;
        let directory = tempfile::Builder::new()
            .prefix("retained-credentials-")
            .tempdir_in(root)?;
        let store = Store::open(&directory.path().join("studio.sqlite")).await?;
        let account = "credential-lifecycle";
        let configuration = credential_configuration();
        let model = store
            .save_model(account, None, configuration.clone())
            .await?;
        let model_id = model["id"].as_str().ok_or("模型标识缺失")?;
        let original: String = sqlx::query_scalar(
            "SELECT credential_ref FROM model_configs WHERE account_id=? AND id=?",
        )
        .bind(account)
        .bind(model_id)
        .fetch_one(&store.pool)
        .await?;
        let mut references = TestCredentials(vec![original.clone()]);
        let session = store.create_session(account, "ordinary").await?;
        let turn = store
            .queue_run(
                account,
                &session.id,
                "ordinary",
                model_id,
                "保存消息",
                "auto_approve",
            )
            .await?;
        store
            .save_model(account, Some(model_id), configuration)
            .await?;
        let current: String = sqlx::query_scalar(
            "SELECT credential_ref FROM model_configs WHERE account_id=? AND id=?",
        )
        .bind(account)
        .bind(model_id)
        .fetch_one(&store.pool)
        .await?;
        references.0.push(current.clone());
        assert!(credentials::load(&original)?.is_some());
        store
            .finish_run(
                account,
                turn["run"]["id"].as_str().ok_or("运行标识缺失")?,
                "cancelled",
                None,
            )
            .await?;
        store.clear_sessions(account).await?;
        assert!(
            credentials::load(&original)?.is_none(),
            "已删除会话仍然保留旧凭据"
        );
        assert!(
            credentials::load(&current)?.is_some(),
            "当前模型凭据需要保留"
        );
        store.close().await;
        Ok(())
    }

    #[tokio::test]
    async fn concurrent_model_updates_and_queued_snapshots_keep_valid_credentials()
    -> std::result::Result<(), Box<dyn std::error::Error>> {
        let root = std::path::PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../.local-cache");
        std::fs::create_dir_all(&root)?;
        let directory = tempfile::Builder::new()
            .prefix("concurrent-credentials-")
            .tempdir_in(root)?;
        let store = Store::open(&directory.path().join("studio.sqlite")).await?;
        let account = "credential-concurrency";
        let model = store
            .save_model(account, None, credential_configuration())
            .await?;
        let model_id = model["id"].as_str().ok_or("模型标识缺失")?;
        let original: String = sqlx::query_scalar(
            "SELECT credential_ref FROM model_configs WHERE account_id=? AND id=?",
        )
        .bind(account)
        .bind(model_id)
        .fetch_one(&store.pool)
        .await?;
        let mut references = TestCredentials(vec![original]);
        for index in 0..4 {
            let with_key = credential_configuration();
            let mut without_key = with_key.clone();
            without_key["api_key"] = json!("");
            let session = store
                .create_session(account, &format!("session-{index}"))
                .await?;
            let request = format!("request-{index}");
            let (changed, unchanged, queued) = tokio::join!(
                store.save_model(account, Some(model_id), with_key),
                store.save_model(account, Some(model_id), without_key),
                store.queue_run(
                    account,
                    &session.id,
                    &request,
                    model_id,
                    "保存凭据配置",
                    "auto_approve"
                ),
            );
            let current: String = sqlx::query_scalar(
                "SELECT credential_ref FROM model_configs WHERE account_id=? AND id=?",
            )
            .bind(account)
            .bind(model_id)
            .fetch_one(&store.pool)
            .await?;
            references.0.push(current.clone());
            changed?;
            unchanged?;
            let queued = queued?;
            assert!(
                credentials::load(&current)?.is_some(),
                "当前配置引用的凭据已被删除"
            );
            let configuration = store
                .run_model(account, queued["run"]["id"].as_str().ok_or("运行标识缺失")?)
                .await?;
            assert!(
                !configuration.api_key.is_empty(),
                "排队运行引用的凭据已被删除"
            );
        }
        let runs = store.unfinished_runs(account).await?;
        for run in runs {
            store.run_model(account, &run).await?;
            store.finish_run(account, &run, "cancelled", None).await?;
        }
        store.clear_sessions(account).await?;
        store.close().await;
        Ok(())
    }

    #[tokio::test]
    async fn model_keys_use_the_actual_system_credential_store()
    -> std::result::Result<(), Box<dyn std::error::Error>> {
        let root = std::path::PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../.local-cache");
        std::fs::create_dir_all(&root)?;
        let directory = tempfile::Builder::new()
            .prefix("credentials-")
            .tempdir_in(root)?;
        let store = Store::open(&directory.path().join("studio.sqlite")).await?;
        let secret = Uuid::new_v4().to_string();
        let input = json!({ "name": "凭据检查", "protocol": "openai_chat_compatible", "base_url": "http://127.0.0.1:11434/v1", "model": "local-model", "api_key": secret, "enabled": true, "agent_enabled": true, "default": false, "limits": { "context_window_tokens": 32000, "max_input_tokens": 24000, "max_output_tokens": 4000 }, "thinking": { "enabled": false }, "capabilities": { "tools": true, "vision": false, "image_output": false, "streaming": true } });
        let model = store.save_model("credential-test", None, input).await?;
        assert_eq!(model["has_api_key"], true);
        assert!(!model.to_string().contains(&secret));
        let row = sqlx::query("SELECT document,credential_ref FROM model_configs WHERE id=?")
            .bind(model["id"].as_str().ok_or("模型标识缺失")?)
            .fetch_one(&store.pool)
            .await?;
        assert!(!row.get::<String, _>("document").contains(&secret));
        let reference: String = row.get("credential_ref");
        assert_eq!(credentials::load(&reference)?, Some(secret));
        credentials::remove(&reference)?;
        assert_eq!(credentials::load(&reference)?, None);
        store.close().await;
        Ok(())
    }
}
