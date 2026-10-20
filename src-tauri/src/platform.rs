use crate::{Error, Result, assets::digest, credentials};
use reqwest::Method;
use reqwest_cookie_store::{CookieStore, CookieStoreMutex};
use serde::Deserialize;
use serde_json::Value;
use std::sync::{
    Arc, Mutex, RwLock,
    atomic::{AtomicBool, Ordering},
};
use std::time::Duration;
use url::{Host, Url};

mod publications;

#[cfg(test)]
mod publications_tests;

#[derive(Deserialize)]
struct Envelope {
    code: i64,
    message: Option<String>,
    data: Option<Value>,
}

pub struct PlatformClient {
    base: Url,
    http: reqwest::Client,
    cookies: Arc<CookieStoreMutex>,
    restored: Mutex<bool>,
    remember: AtomicBool,
    identity: RwLock<Option<Value>>,
    credential_name: String,
}

impl PlatformClient {
    pub(crate) fn scope_for_account(&self, id: &str) -> String {
        format!("{}:{id}", digest(self.base.as_str().as_bytes()))
    }

    pub(crate) fn cached_identity(&self) -> Result<Option<Value>> {
        Ok(self
            .identity
            .read()
            .map_err(|_| Error::State("账号状态无法读取"))?
            .clone())
    }

    pub(crate) fn cached_scope(&self) -> Result<Option<String>> {
        self.cached_identity()?
            .map(|identity| {
                let id = identity["id"]
                    .as_str()
                    .ok_or(Error::State("账号标识不存在"))?;
                Ok(self.scope_for_account(id))
            })
            .transpose()
    }

    fn clear_auth(&self) -> Result<()> {
        *self
            .identity
            .write()
            .map_err(|_| Error::State("账号状态无法更新"))? = None;
        self.cookies
            .lock()
            .map_err(|_| Error::State("Cookie 无法清除"))?
            .clear();
        self.remember.store(false, Ordering::Relaxed);
        credentials::remove(&self.credential_name)
    }

    pub fn new(address: &str) -> Result<Self> {
        let base = Url::parse(address).map_err(|_| Error::Invalid("平台地址无效"))?;
        let local = match base.host() {
            Some(Host::Domain(host)) => host == "localhost",
            Some(Host::Ipv4(ip)) => ip.is_loopback(),
            Some(Host::Ipv6(ip)) => ip.is_loopback(),
            None => false,
        };
        if base.host().is_none()
            || (base.scheme() != "https" && !(base.scheme() == "http" && local))
            || !base.username().is_empty()
            || base.password().is_some()
            || base.query().is_some()
            || base.fragment().is_some()
            || (base.path() != "/" && !base.path().is_empty())
        {
            return Err(Error::Invalid(
                "平台地址需要 HTTPS，本机地址允许 HTTP，地址不能包含凭据、查询参数或子路径",
            ));
        }
        let cookies = Arc::new(CookieStoreMutex::new(CookieStore::new()));
        let http = reqwest::Client::builder()
            .cookie_provider(cookies.clone())
            .redirect(reqwest::redirect::Policy::none())
            .connect_timeout(Duration::from_secs(10))
            .timeout(Duration::from_secs(60))
            .user_agent("PixomaStudio/0.1.0")
            .build()?;
        let credential_name = format!("platform-{}", digest(base.as_str().as_bytes()));
        Ok(Self {
            base,
            http,
            cookies,
            restored: Mutex::new(false),
            remember: AtomicBool::new(false),
            identity: RwLock::new(None),
            credential_name,
        })
    }

    fn restore_credentials(&self) -> Result<()> {
        let mut restored = self
            .restored
            .lock()
            .map_err(|_| Error::State("登录状态无法读取"))?;
        if *restored {
            return Ok(());
        }
        if let Some(saved) = credentials::load(&self.credential_name)? {
            let store = cookie_store::serde::json::load(saved.as_bytes())
                .map_err(|_| Error::State("保存的登录凭据格式无效"))?;
            *self
                .cookies
                .lock()
                .map_err(|_| Error::State("Cookie 无法读取"))? = store;
            self.remember.store(true, Ordering::Relaxed);
        }
        *restored = true;
        Ok(())
    }

    fn persist_credentials(&self) -> Result<()> {
        if !self.remember.load(Ordering::Relaxed) {
            return credentials::remove(&self.credential_name);
        }
        let mut output = Vec::new();
        {
            let store = self
                .cookies
                .lock()
                .map_err(|_| Error::State("Cookie 无法读取"))?;
            cookie_store::serde::json::save(&store, &mut output)
                .map_err(|_| Error::State("登录凭据无法保存"))?;
        }
        let output = String::from_utf8(output).map_err(|_| Error::State("登录凭据编码错误"))?;
        credentials::save(&self.credential_name, &output)
    }

    async fn request(&self, method: Method, path: &str, body: Option<Value>) -> Result<Value> {
        self.restore_credentials()?;
        let url = self
            .base
            .join(path)
            .map_err(|_| Error::Invalid("平台请求地址无效"))?;
        if url.origin() != self.base.origin() {
            return Err(Error::Invalid("平台请求地址超出允许范围"));
        }
        let mut request = self
            .http
            .request(method, url)
            .header(reqwest::header::ACCEPT, "application/json");
        if let Some(body) = body {
            request = request.json(&body);
        }
        let response = request.send().await?;
        self.response_data(response).await
    }

    async fn response_data(&self, response: reqwest::Response) -> Result<Value> {
        let status = response.status();
        let content_type = response
            .headers()
            .get(reqwest::header::CONTENT_TYPE)
            .and_then(|value| value.to_str().ok())
            .unwrap_or("");
        if !content_type.starts_with("application/json") {
            return Err(Error::State("平台返回的内容格式无效，检查平台地址"));
        }
        let envelope: Envelope = response.json().await?;
        if !status.is_success() || envelope.code != 2000000 {
            if status.as_u16() == 401 {
                self.clear_auth()?;
            }
            return Err(Error::Remote {
                status: if status.is_success() {
                    400
                } else {
                    status.as_u16()
                },
                code: envelope.code,
                message: envelope
                    .message
                    .ok_or(Error::State("平台错误响应缺少说明"))?,
            });
        }
        envelope.data.ok_or(Error::State("平台响应缺少数据"))
    }

    pub async fn request_auth(
        &self,
        method: &str,
        action: &str,
        body: Option<Value>,
    ) -> Result<Value> {
        if !matches!(
            (method, action),
            ("GET", "registration" | "me")
                | ("POST", "login" | "register" | "logout" | "password")
                | ("PATCH", "profile")
        ) {
            return Err(Error::Invalid("账号操作无效"));
        }
        let remember = body
            .as_ref()
            .and_then(|value| value["remember_me"].as_bool())
            .unwrap_or(false);
        let result = self
            .request(
                Method::from_bytes(method.as_bytes())
                    .map_err(|_| Error::Invalid("请求方法无效"))?,
                &format!("/api/v1/studio/auth/{action}"),
                body,
            )
            .await;
        let value = match result {
            Ok(value) => value,
            Err(error) => {
                if matches!(action, "login" | "register" | "logout") {
                    self.clear_auth()?;
                }
                return Err(error);
            }
        };
        if action == "logout" {
            self.clear_auth()?;
        } else if matches!(action, "me" | "login" | "register" | "profile" | "password") {
            if !value["id"].as_str().is_some_and(|id| !id.trim().is_empty())
                || value["enabled"].as_bool() != Some(true)
                || value["must_change_password"].as_bool().is_none()
            {
                self.clear_auth()?;
                return Err(Error::State("平台账号响应字段无效"));
            }
            *self
                .identity
                .write()
                .map_err(|_| Error::State("账号状态无法更新"))? = Some(value.clone());
            if matches!(action, "login" | "register") {
                self.remember.store(remember, Ordering::Relaxed);
            }
            self.persist_credentials()?;
        }
        Ok(value)
    }

    pub async fn account_scope(&self) -> Result<String> {
        let cached = {
            self.identity
                .read()
                .map_err(|_| Error::State("账号状态无法读取"))?
                .clone()
        };
        let identity = match cached {
            Some(identity) => identity,
            None => self.request_auth("GET", "me", None).await?,
        };
        if identity["must_change_password"].as_bool() == Some(true) {
            return Err(Error::Remote {
                status: 403,
                code: 0,
                message: "需要修改密码".to_owned(),
            });
        }
        let id = identity["id"]
            .as_str()
            .ok_or(Error::State("账号标识不存在"))?;
        Ok(self.scope_for_account(id))
    }

    pub async fn request_catalog(&self, path: &str) -> Result<Value> {
        if ![
            "/api/v1/studio/skill-market",
            "/api/v1/studio/expert-market",
            "/api/v1/studio/workflow-market",
        ]
        .iter()
        .any(|prefix| {
            path == *prefix
                || path.starts_with(&format!("{prefix}/"))
                || path.starts_with(&format!("{prefix}?"))
        }) {
            return Err(Error::Invalid("官方内容请求地址无效"));
        }
        self.account_scope().await?;
        self.request(Method::GET, path, None).await
    }

    pub async fn expert_avatar_response(
        &self,
        id: &str,
        version: &str,
        method: &str,
    ) -> Result<crate::media::MediaResponse> {
        if !["GET", "HEAD"].contains(&method) {
            return Err(Error::Invalid("头像请求方法无效"));
        }
        uuid::Uuid::parse_str(id).map_err(|_| Error::Invalid("专家标识无效"))?;
        crate::skills::stable_version(version)?;
        self.account_scope().await?;
        let mut address = self
            .base
            .join("/api/v1/studio/expert-market/")
            .map_err(|_| Error::Invalid("官方头像地址无效"))?;
        address
            .path_segments_mut()
            .map_err(|_| Error::Invalid("官方头像地址无效"))?
            .pop_if_empty()
            .push(id)
            .push("avatar");
        address.query_pairs_mut().append_pair("version", version);
        let mut response = self.http.get(address).send().await?;
        if !response.status().is_success() {
            let status = response.status().as_u16();
            if status == 401 {
                self.clear_auth()?;
            }
            return Err(Error::Remote {
                status,
                code: 0,
                message: format!("读取官方专家头像失败：HTTP {status}"),
            });
        }
        if response
            .content_length()
            .is_some_and(|length| length > 5 << 20)
        {
            return Err(Error::Invalid("官方专家头像超过 5 MiB"));
        }
        let mut bytes = Vec::new();
        while let Some(chunk) = response.chunk().await? {
            if bytes.len() + chunk.len() > 5 << 20 {
                return Err(Error::Invalid("官方专家头像超过 5 MiB"));
            }
            bytes.extend_from_slice(&chunk);
        }
        let mime_type = infer::get(&bytes)
            .filter(|kind| kind.mime_type().starts_with("image/"))
            .ok_or(Error::Invalid("官方专家头像格式无效"))?
            .mime_type()
            .to_owned();
        image::ImageReader::new(std::io::Cursor::new(&bytes))
            .with_guessed_format()?
            .decode()
            .map_err(|_| Error::Invalid("官方专家头像图片无效"))?;
        Ok(crate::media::MediaResponse {
            status: 200,
            content_length: bytes.len() as u64,
            body: if method == "HEAD" { Vec::new() } else { bytes },
            mime_type,
            content_range: None,
        })
    }

    pub async fn prepare_capabilities(&self, store: &crate::Store, input: &Value) -> Result<()> {
        let account = self.account_scope().await?;
        if let Some(request) = input["request_id"].as_str() {
            let exists: bool = sqlx::query_scalar(
                "SELECT EXISTS(SELECT 1 FROM runs WHERE account_id=? AND request_id=?)",
            )
            .bind(&account)
            .bind(request)
            .fetch_one(&store.pool)
            .await?;
            if exists {
                return Ok(());
            }
        }
        let session_id = input["session_id"].as_str().unwrap_or("");
        let mut selected_id = input["expert_id"]
            .as_str()
            .filter(|id| !id.is_empty())
            .map(str::to_owned);
        let mut selected_version = input["expert_version"]
            .as_str()
            .filter(|version| !version.is_empty())
            .map(str::to_owned);
        if !session_id.is_empty() {
            let session = store.get_session(&account, session_id).await?;
            if selected_id
                .as_ref()
                .is_some_and(|id| session.expert_id.as_ref() != Some(id))
                || selected_version
                    .as_ref()
                    .is_some_and(|version| session.expert_version.as_ref() != Some(version))
            {
                return Err(Error::Invalid("当前会话不能更换专家或版本"));
            }
            selected_id = session.expert_id;
            selected_version = session.expert_version;
        }
        if let Some(id) = selected_id {
            let official = match store.get_expert(&account, &id).await {
                Ok(expert) => expert["source"] == "official",
                Err(Error::NotFound) => true,
                Err(error) => return Err(error),
            };
            if official {
                let mut address = self
                    .base
                    .join("/api/v1/studio/expert-market/")
                    .map_err(|_| Error::Invalid("官方专家地址无效"))?;
                address
                    .path_segments_mut()
                    .map_err(|_| Error::Invalid("官方专家地址无效"))?
                    .pop_if_empty()
                    .push(&id);
                let current = self.request_catalog(address.path()).await?;
                if current["id"] != id
                    || current["enabled"] != true
                    || current["available"] == false
                {
                    return Err(Error::Invalid("官方专家已停用或不可用"));
                }
                let latest = crate::local_api::text(&current, "version")?;
                let mut latest_url = address.clone();
                latest_url
                    .path_segments_mut()
                    .map_err(|_| Error::Invalid("官方专家地址无效"))?
                    .push("versions")
                    .push(latest);
                store
                    .cache_official_expert(
                        &account,
                        current.clone(),
                        current.clone(),
                        latest_url.as_str(),
                    )
                    .await?;
                if let Some(version) = selected_version.filter(|version| version != latest) {
                    match store.expert_version(&account, &id, &version).await {
                        Ok(_) => {}
                        Err(Error::NotFound) => {
                            address
                                .path_segments_mut()
                                .map_err(|_| Error::Invalid("官方专家地址无效"))?
                                .push("versions")
                                .push(&version);
                            let snapshot = self.request_catalog(address.path()).await?;
                            if snapshot["version"] != version {
                                return Err(Error::State("官方专家返回的版本号与请求不一致"));
                            }
                            store
                                .cache_official_expert(
                                    &account,
                                    current,
                                    snapshot,
                                    address.as_str(),
                                )
                                .await?;
                        }
                        Err(error) => return Err(error),
                    }
                }
            }
        }
        let expert = store.resolve_expert(&account, session_id, input).await?;
        let resolved = crate::Store::expert_context_input(input, expert.as_ref())?;
        self.prepare_skills(store, &resolved).await?;
        self.prepare_workflows(store, &resolved).await
    }

    pub async fn prepare_expert_save(&self, store: &crate::Store, input: &Value) -> Result<()> {
        let mut content: crate::experts::ExpertContent = serde_json::from_value(input.clone())
            .map_err(|_| Error::Invalid("专家内容格式无效"))?;
        content.validate()?;
        let input = crate::Store::expert_context_input(
            &Value::Null,
            Some(&serde_json::to_value(content)?),
        )?;
        self.prepare_skills(store, &input).await?;
        self.prepare_workflows(store, &input).await
    }

    pub async fn prepare_skills(&self, store: &crate::Store, input: &Value) -> Result<()> {
        let selected = crate::skills::requested_skills(input)?;
        if selected.is_empty() {
            return Ok(());
        }
        let account = self.account_scope().await?;
        if let Some(request) = input["request_id"].as_str() {
            let exists: bool = sqlx::query_scalar(
                "SELECT EXISTS(SELECT 1 FROM runs WHERE account_id=? AND request_id=?)",
            )
            .bind(&account)
            .bind(request)
            .fetch_one(&store.pool)
            .await?;
            if exists {
                return Ok(());
            }
        }
        for (id, selected_version) in selected {
            match store.get_skill(&account, &id).await {
                Ok(skill) if skill["source"] == "user" => continue,
                Ok(_) | Err(Error::NotFound) => {}
                Err(error) => return Err(error),
            }
            let mut address = self
                .base
                .join("/api/v1/studio/skill-market/")
                .map_err(|_| Error::Invalid("官方技能地址无效"))?;
            address
                .path_segments_mut()
                .map_err(|_| Error::Invalid("官方技能地址无效"))?
                .pop_if_empty()
                .push(&id);
            let path = address.path().to_owned();
            let current = self.request_catalog(&path).await?;
            if current["id"] != id || current["enabled"] != true || current["available"] == false {
                return Err(Error::Invalid("官方技能已停用或不可用"));
            }
            let latest_version = crate::local_api::text(&current, "version")?;
            let version_number = selected_version.as_deref().unwrap_or(latest_version);
            let mut source_url = address;
            source_url
                .path_segments_mut()
                .map_err(|_| Error::Invalid("官方技能地址无效"))?
                .push("versions")
                .push(version_number);
            let version_path = source_url.path().to_owned();
            let version = if version_number == latest_version {
                current.clone()
            } else {
                self.request_catalog(&version_path).await?
            };
            if version["version"] != version_number {
                return Err(Error::State("官方技能返回的版本号与请求不一致"));
            }
            store
                .cache_official_skill(&account, current, version, source_url.as_str())
                .await?;
        }
        Ok(())
    }

    pub async fn prepare_workflows(&self, store: &crate::Store, input: &Value) -> Result<()> {
        let selected = crate::workflows::requested_workflows(input)?;
        if selected.is_empty() {
            return Ok(());
        }
        let account = self.account_scope().await?;
        for (id, version) in selected {
            let mut address = self
                .base
                .join("/api/v1/studio/workflow-market/")
                .map_err(|_| Error::Invalid("官方工作流地址无效"))?;
            address
                .path_segments_mut()
                .map_err(|_| Error::Invalid("官方工作流地址无效"))?
                .pop_if_empty()
                .push(&id);
            let current = self.request_catalog(address.path()).await?;
            if current["id"] != id {
                return Err(Error::State("官方工作流标识与请求不一致"));
            }
            store
                .cache_official_workflow(&account, current, address.as_str())
                .await?;
            if let Some(version) = version {
                store
                    .workflow_version(&account, &id, Some(version))
                    .await
                    .map_err(|error| match error {
                        Error::NotFound => Error::Invalid("平台没有提供所选工作流版本的目录内容"),
                        error => error,
                    })?;
            }
        }
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    struct TestConnectorCredential(keyring::Entry);

    impl Drop for TestConnectorCredential {
        fn drop(&mut self) {
            match self.0.delete_credential() {
                Ok(()) | Err(keyring::Error::NoEntry) => {}
                Err(error) => panic!("删除测试 MCP 凭据失败：{error}"),
            }
        }
    }

    #[tokio::test]
    #[ignore = "需要真实平台地址、MCP 凭据和专用账号的环境变量"]
    async fn real_workflow_catalog_keeps_the_received_version_after_reopening()
    -> std::result::Result<(), Box<dyn std::error::Error>> {
        let address = std::env::var("PIXOMA_TEST_PLATFORM_URL")?;
        let client = PlatformClient::new(&address)?;
        let password = std::env::var("PIXOMA_TEST_STUDIO_PASSWORD")?;
        client.request_auth("POST", "login", Some(json!({ "username": std::env::var("PIXOMA_TEST_STUDIO_USERNAME")?, "password": password, "remember_me": false }))).await?;
        client.request_auth("POST", "password", Some(json!({ "current_password": password, "new_password": uuid::Uuid::new_v4().to_string() }))).await?;
        let account = client.account_scope().await?;
        let catalog = client
            .request_catalog("/api/v1/studio/workflow-market")
            .await?;
        let workflow = catalog
            .as_array()
            .and_then(|items| items.first())
            .ok_or("真实平台缺少已发布的工作流")?;
        let id = crate::local_api::text(workflow, "id")?;
        let version = workflow["version_id"]
            .as_u64()
            .ok_or("工作流版本标识缺失")?;
        let detail = client
            .request_catalog(&format!("/api/v1/studio/workflow-market/{id}"))
            .await?;
        let root = std::path::PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../.local-cache");
        std::fs::create_dir_all(&root)?;
        let directory = tempfile::Builder::new()
            .prefix("workflow-catalog-")
            .tempdir_in(root)?;
        let database = directory.path().join("studio.sqlite");
        let store = crate::Store::open(&database).await?;
        let selected = json!({ "parts": [{ "type": "workflow_ref", "workflow_id": id, "version_id": version }] });
        client.prepare_workflows(&store, &selected).await?;
        client.prepare_workflows(&store, &selected).await?;
        store.close().await;
        let store = crate::Store::open(&database).await?;
        let saved = store.workflow_version(&account, id, Some(version)).await?;
        assert_eq!(saved["id"], id);
        assert_eq!(saved["version_id"], version);
        assert_eq!(saved["name"], detail["name"]);
        assert_eq!(saved["input_schema"], detail["input_schema"]);
        assert_eq!(saved["input_fields"], detail["input_fields"]);
        assert!(
            saved["source_url"]
                .as_str()
                .ok_or("工作流来源地址缺失")?
                .starts_with(&address)
        );
        assert!(
            store
                .workflow_version("another-account", id, Some(version))
                .await
                .is_err()
        );
        let mut connector_input = json!({ "name": "官方工作流 MCP", "url": std::env::var("PIXOMA_TEST_MCP_URL")?, "credential": std::env::var("PIXOMA_TEST_MCP_TOKEN")?, "enabled": true, "policy": "auto" });
        let connector = store
            .save_connector(&account, None, connector_input.clone())
            .await?;
        let connector_id = crate::local_api::text(&connector, "id")?;
        let credential: String =
            sqlx::query_scalar("SELECT credential_ref FROM connectors WHERE account_id=? AND id=?")
                .bind(&account)
                .bind(connector_id)
                .fetch_one(&store.pool)
                .await?;
        let _credential =
            TestConnectorCredential(keyring::Entry::new("com.pixoma.studio", &credential)?);
        let model = store.save_model(&account, None, json!({
            "name": "本地会话配置", "protocol": "openai_chat_compatible", "base_url": "http://127.0.0.1:11434/v1", "model": "local-model", "api_key": "",
            "enabled": true, "agent_enabled": true, "default": true,
            "limits": { "context_window_tokens": 32000, "max_input_tokens": 24000, "max_output_tokens": 4000 },
            "thinking": { "enabled": false }, "capabilities": { "tools": true, "vision": false, "image_output": false, "streaming": true },
        })).await?;
        let skills = client
            .request_catalog("/api/v1/studio/skill-market")
            .await?;
        let skill = skills
            .as_array()
            .and_then(|items| items.first())
            .ok_or("真实平台没有已发布的技能")?;
        let expert_input = json!({
            "schema_version": 2, "name": "工作流资料检查", "description": "保存工作流输入说明", "model_id": model["id"],
            "prompt_parts": [{ "type": "workflow", "workflow_id": id, "version_id": version }, { "type": "skill", "skill_id": skill["id"], "version": skill["version"] }],
            "avatar_asset_id": "", "avatar_version_id": "", "examples": [], "skill_refs": [], "workflow_refs": [], "sop": [], "version": "1.0.0", "enabled": true,
        });
        assert!(
            store
                .save_expert(&account, None, expert_input.clone())
                .await
                .is_err(),
            "保存专家前需要取得引用技能的指定版本"
        );
        client.prepare_expert_save(&store, &expert_input).await?;
        let expert = store.save_expert(&account, None, expert_input).await?;
        assert_eq!(
            expert["available"], false,
            "MCP 工具尚未获取时专家不能显示可用"
        );
        store.probe_connector(&account, connector_id).await?;
        let input = json!({ "request_id": "workflow-capabilities", "text": "查看工作流资料", "expert_id": expert["id"], "permission_mode": "auto_approve" });
        client.prepare_capabilities(&store, &input).await?;
        let queued = store.submit_message(&account, input).await?;
        let session = crate::local_api::text(&queued["session"], "id")?;
        let session_detail = store.session_detail(&account, session).await?;
        assert_eq!(
            session_detail["expert_capabilities"]["workflows"]
                .as_array()
                .ok_or("专家工作流列表缺失")?
                .len(),
            1
        );
        assert_eq!(
            session_detail["expert_capabilities"]["workflows"][0]["id"],
            id
        );
        assert_eq!(
            session_detail["expert_capabilities"]["workflows"][0]["version_id"],
            version
        );
        assert_eq!(
            session_detail["expert_capabilities"]["workflows"][0]["input_schema"],
            detail["input_schema"]
        );
        let events: i64 = sqlx::query_scalar("SELECT count(*) FROM context_events WHERE account_id=? AND session_id=? AND json_extract(document,'$.kind')='CONTEXT_INJECTED'")
            .bind(&account).bind(session).fetch_one(&store.pool).await?;
        assert_eq!(
            events, 3,
            "专家、技能和工作流资料需要分别保存上下文注入记录"
        );
        assert_eq!(
            session_detail["expert_capabilities"]["skills"][0]["id"],
            skill["id"]
        );
        assert_eq!(
            session_detail["expert_capabilities"]["skills"][0]["version"],
            skill["version"]
        );
        let snapshot: String =
            sqlx::query_scalar("SELECT context_snapshot FROM runs WHERE account_id=? AND id=?")
                .bind(&account)
                .bind(crate::local_api::text(&queued["run"], "id")?)
                .fetch_one(&store.pool)
                .await?;
        let context: crate::context::RunContext = serde_json::from_str(&snapshot)?;
        assert_eq!(context.workflows[0]["input_fields"], detail["input_fields"]);
        assert!(
            queued["message"]["parts"]
                .as_array()
                .ok_or("消息引用缺失")?
                .iter()
                .any(|part| part["type"] == "workflow_ref"
                    && part["workflow_id"] == id
                    && part["version_id"] == version)
        );
        let expert_id = crate::local_api::text(&expert, "id")?;
        assert_eq!(
            store.get_expert(&account, expert_id).await?["available"],
            true
        );
        connector_input["credential"] = json!("");
        connector_input["enabled"] = json!(false);
        store
            .save_connector(&account, Some(connector_id), connector_input.clone())
            .await?;
        let unavailable = store.get_expert(&account, expert_id).await?;
        assert_eq!(
            unavailable["available"], false,
            "工作流 MCP 停用后专家应显示不可用"
        );
        assert!(!crate::local_api::text(&unavailable, "unavailable_reason")?.is_empty());
        let mut changed = detail.clone();
        changed["name"] = json!("修改同一版本的内容");
        assert!(
            store
                .cache_official_workflow(
                    &account,
                    changed,
                    &format!("{address}/api/v1/studio/workflow-market/{id}")
                )
                .await
                .is_err()
        );
        assert_eq!(
            store.workflow_version(&account, id, Some(version)).await?["name"],
            detail["name"]
        );
        store.close().await;
        client.request_auth("POST", "logout", None).await?;
        Ok(())
    }

    #[tokio::test]
    #[ignore = "需要 PIXOMA_TEST_PLATFORM_URL 并连接真实平台"]
    async fn real_published_expert_avatar_uses_authenticated_platform_bytes()
    -> std::result::Result<(), Box<dyn std::error::Error>> {
        let address = std::env::var("PIXOMA_TEST_PLATFORM_URL")?;
        let client = PlatformClient::new(&address)?;
        client.request_auth("POST", "register", Some(json!({
            "username": format!("desktop-avatar-{}", uuid::Uuid::new_v4()), "password": uuid::Uuid::new_v4().to_string(), "nickname": "头像验证", "email": "",
        }))).await?;
        let catalog = client
            .request_catalog("/api/v1/studio/expert-market")
            .await?;
        let expert = catalog
            .as_array()
            .and_then(|items| items.first())
            .ok_or("真实平台缺少已发布的专家")?;
        let content: crate::experts::ExpertContent = serde_json::from_value(expert.clone())?;
        let image = include_bytes!("../../public/images/pixoma-ghost.png");
        let form = reqwest::multipart::Form::new()
            .text("request_id", uuid::Uuid::new_v4().to_string())
            .part(
                "file",
                reqwest::multipart::Part::bytes(image.to_vec())
                    .file_name("头像.png")
                    .mime_str("image/png")?,
            );
        let response = client
            .http
            .post(client.base.join("/api/v1/studio/expert-avatars/upload")?)
            .multipart(form)
            .send()
            .await?;
        let status = response.status();
        let envelope: Envelope = response.json().await?;
        assert!(
            status.is_success(),
            "{}",
            envelope.message.unwrap_or_default()
        );
        let avatar = envelope.data.ok_or("上传头像没有返回资产")?;
        let mut input = serde_json::to_value(content)?;
        input["name"] = json!("头像分发验证");
        input["avatar_asset_id"] = avatar["id"].clone();
        input["avatar_version_id"] = avatar["versions"][0]["id"].clone();
        input["version"] = json!("1.0.0");
        input["enabled"] = json!(true);
        let saved = client
            .request(Method::POST, "/api/v1/studio/experts", Some(input))
            .await?;
        let id = crate::local_api::text(&saved, "id")?;
        client
            .request(
                Method::POST,
                &format!("/api/v1/studio/experts/{id}/publish"),
                Some(json!({ "request_id": uuid::Uuid::new_v4().to_string(), "version": "1.0.0" })),
            )
            .await?;
        let media = client.expert_avatar_response(id, "1.0.0", "GET").await;
        client.request(Method::POST, &format!("/api/v1/studio/experts/{id}/unpublish"), Some(json!({ "request_id": uuid::Uuid::new_v4().to_string(), "reason": "完成头像验证" }))).await?;
        let media = media?;
        assert_eq!(media.mime_type, "image/png");
        assert_eq!(media.body, image);
        assert!(
            PlatformClient::new(&address)?
                .expert_avatar_response(id, "1.0.0", "GET")
                .await
                .is_err()
        );
        client.request_auth("POST", "logout", None).await?;
        Ok(())
    }
}
