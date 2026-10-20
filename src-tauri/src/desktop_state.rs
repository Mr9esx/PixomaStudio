use crate::{AgentRuntime, Error, LocalApi, PlatformClient, Result, Store};
use serde_json::{Value, json};
use std::{
    collections::HashMap,
    future::Future,
    sync::{Arc, RwLock},
};
use tauri::Emitter;
use tokio::sync::{Mutex, RwLock as AsyncRwLock, RwLockReadGuard};
use tokio_util::sync::CancellationToken;

pub(crate) struct AppState {
    pub api: LocalApi,
    pub store: Store,
    pub platform: Option<Arc<PlatformClient>>,
    pub runtime: AgentRuntime,
    pub subscriptions: Mutex<HashMap<String, (String, CancellationToken)>>,
    access_gate: AsyncRwLock<()>,
    auth_gate: Mutex<()>,
    requests: RwLock<CancellationToken>,
}

pub(crate) struct AccountAccess<'a> {
    pub account: String,
    pub cancellation: CancellationToken,
    _guard: RwLockReadGuard<'a, ()>,
}

impl AccountAccess<'_> {
    pub async fn execute<T>(&self, operation: impl Future<Output = Result<T>>) -> Result<T> {
        tokio::select! {
            biased;
            _ = self.cancellation.cancelled() => Err(Error::Cancelled),
            result = operation => result,
        }
    }
}

impl AppState {
    pub fn new(store: Store, platform: Option<Arc<PlatformClient>>) -> Self {
        Self {
            api: LocalApi::new(store.clone()),
            runtime: AgentRuntime::new(store.clone()),
            store,
            platform,
            subscriptions: Mutex::new(HashMap::new()),
            access_gate: AsyncRwLock::new(()),
            auth_gate: Mutex::new(()),
            requests: RwLock::new(CancellationToken::new()),
        }
    }

    pub fn platform(&self) -> Result<&PlatformClient> {
        self.platform.as_deref().ok_or(Error::State(
            "未配置 Pixoma 平台地址，请在 desktop.json 中设置 platform_url",
        ))
    }

    fn request_token(&self) -> Result<CancellationToken> {
        Ok(self
            .requests
            .read()
            .map_err(|_| Error::State("账号请求状态无法读取"))?
            .clone())
    }

    fn rotate_requests(&self) -> Result<()> {
        let mut requests = self
            .requests
            .write()
            .map_err(|_| Error::State("账号请求状态无法更新"))?;
        requests.cancel();
        *requests = CancellationToken::new();
        Ok(())
    }

    pub async fn access(&self) -> Result<AccountAccess<'_>> {
        let guard = self.access_gate.read().await;
        let cancellation = self.request_token()?;
        let account = tokio::select! {
            biased;
            _ = cancellation.cancelled() => return Err(Error::Cancelled),
            account = self.platform()?.account_scope() => account?,
        };
        Ok(AccountAccess {
            account,
            cancellation,
            _guard: guard,
        })
    }

    pub fn start_session(&self, account: String, session: String) {
        let runtime = self.runtime.clone();
        tauri::async_runtime::spawn(async move {
            runtime
                .drain_session(&account, &session)
                .await
                .expect("执行会话运行队列失败");
        });
    }

    fn resume_account(&self, account: String) {
        let runtime = self.runtime.clone();
        tauri::async_runtime::spawn(async move {
            runtime
                .resume_account(&account)
                .await
                .expect("恢复账号排队运行失败");
        });
    }

    async fn stop_account(&self, account: &str) -> Result<()> {
        self.subscriptions
            .lock()
            .await
            .retain(|_, (owner, cancellation)| {
                if owner == account {
                    cancellation.cancel();
                    false
                } else {
                    true
                }
            });
        self.runtime.cancel_account(account).await
    }

    pub async fn clear_sessions(&self, confirmation: Option<&Value>) -> Result<()> {
        if confirmation.and_then(Value::as_str) != Some("确认清空") {
            return Err(Error::Invalid("请输入确认清空"));
        }
        let _auth = self.auth_gate.lock().await;
        self.request_token()?.cancel();
        let _access = self.access_gate.write().await;
        self.rotate_requests()?;
        let account = self.platform()?.account_scope().await?;
        self.stop_account(&account).await?;
        self.store.clear_sessions(&account).await
    }

    pub async fn check_identity(
        &self,
        app: &tauri::AppHandle,
        previous: Option<Value>,
        exclusive: bool,
    ) -> Result<()> {
        let current = self.platform()?.cached_identity()?;
        let previous_id = previous
            .as_ref()
            .and_then(|identity| identity["id"].as_str());
        let current_id = current
            .as_ref()
            .and_then(|identity| identity["id"].as_str());
        let current_requires_password = current
            .as_ref()
            .is_some_and(|identity| identity["must_change_password"] == true);
        let mut rotation = Ok(());
        let mut stopped = Ok(());
        if previous_id.is_some() && (previous_id != current_id || current_requires_password) {
            if !exclusive {
                rotation = self.rotate_requests();
            }
            if let Some(id) = previous_id {
                stopped = self
                    .stop_account(&self.platform()?.scope_for_account(id))
                    .await;
            }
        }
        let previous_requires_password = previous
            .as_ref()
            .is_some_and(|identity| identity["must_change_password"] == true);
        if previous_id != current_id || previous_requires_password != current_requires_password {
            app.emit("studio:identity-changed", json!({ "identity": current }))
                .map_err(|_| Error::State("账号状态通知失败"))?;
        }
        rotation?;
        stopped
    }

    pub async fn auth_request(
        &self,
        app: &tauri::AppHandle,
        method: &str,
        action: &str,
        body: Option<Value>,
    ) -> Result<Value> {
        let exclusive = method == "POST" && matches!(action, "login" | "register" | "logout");
        if exclusive {
            self.request_token()?.cancel();
            if let Some(account) = self.platform()?.cached_scope()? {
                self.stop_account(&account).await?;
            }
        }
        let _auth = self.auth_gate.lock().await;
        let write = if exclusive {
            Some(self.access_gate.write().await)
        } else {
            None
        };
        let read = if exclusive {
            None
        } else {
            Some(self.access_gate.read().await)
        };
        if exclusive {
            self.rotate_requests()?;
        }
        let previous = self.platform()?.cached_identity()?;
        let operation = self.platform()?.request_auth(method, action, body);
        let result = if exclusive {
            operation.await
        } else {
            let cancellation = self.request_token()?;
            tokio::select! {
                biased;
                _ = cancellation.cancelled() => Err(Error::Cancelled),
                result = operation => result,
            }
        };
        self.check_identity(app, previous, exclusive).await?;
        if result.is_ok()
            && matches!(action, "me" | "login" | "register" | "password")
            && self
                .platform()?
                .cached_identity()?
                .is_some_and(|identity| identity["must_change_password"] != true)
            && let Some(account) = self.platform()?.cached_scope()?
        {
            self.resume_account(account);
        }
        drop(read);
        drop(write);
        result
    }
}
