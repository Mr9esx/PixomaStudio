use crate::{AgentRuntime, Error, LocalApi, Result, Store};
use serde_json::Value;
use std::{collections::HashMap, future::Future, sync::RwLock};
use tokio::sync::{Mutex, RwLock as AsyncRwLock, RwLockReadGuard};
use tokio_util::sync::CancellationToken;

pub(crate) struct AppState {
    pub api: LocalApi,
    pub store: Store,
    pub runtime: AgentRuntime,
    pub subscriptions: Mutex<HashMap<String, CancellationToken>>,
    access_gate: AsyncRwLock<()>,
    clear_gate: Mutex<()>,
    requests: RwLock<CancellationToken>,
}

pub(crate) struct RequestAccess<'a> {
    cancellation: CancellationToken,
    _guard: RwLockReadGuard<'a, ()>,
}

impl RequestAccess<'_> {
    pub async fn execute<T>(&self, operation: impl Future<Output = Result<T>>) -> Result<T> {
        tokio::select! {
            biased;
            _ = self.cancellation.cancelled() => Err(Error::Cancelled),
            result = operation => result,
        }
    }
}

impl AppState {
    pub fn new(store: Store) -> Self {
        Self { api: LocalApi::new(store.clone()), runtime: AgentRuntime::new(store.clone()), store,
            subscriptions: Mutex::new(HashMap::new()), access_gate: AsyncRwLock::new(()),
            clear_gate: Mutex::new(()), requests: RwLock::new(CancellationToken::new()) }
    }

    pub async fn access(&self) -> Result<RequestAccess<'_>> {
        let guard = self.access_gate.read().await;
        let cancellation = self.requests.read().map_err(|_| Error::State("请求状态无法读取"))?.clone();
        Ok(RequestAccess { cancellation, _guard: guard })
    }

    pub fn start_session(&self, session: String) {
        let runtime = self.runtime.clone();
        tauri::async_runtime::spawn(async move { runtime.drain_session(&session).await.expect("执行会话运行队列失败"); });
    }

    pub fn resume_pending(&self) {
        let runtime = self.runtime.clone();
        tauri::async_runtime::spawn(async move { runtime.resume_pending().await.expect("恢复会话运行失败"); });
        let store = self.store.clone();
        tauri::async_runtime::spawn(async move {
            let mut interval = tokio::time::interval(std::time::Duration::from_secs(2));
            loop {
                interval.tick().await;
                store.resume_video_generations().await.expect("恢复视频生成记录失败");
            }
        });
    }

    pub async fn clear_sessions(&self, confirmation: Option<&Value>) -> Result<()> {
        if confirmation.and_then(Value::as_str) != Some("确认清空") { return Err(Error::Invalid("请输入确认清空")); }
        let _clear = self.clear_gate.lock().await;
        self.requests.read().map_err(|_| Error::State("请求状态无法读取"))?.cancel();
        let _access = self.access_gate.write().await;
        for cancellation in self.subscriptions.lock().await.drain().map(|(_, cancellation)| cancellation) { cancellation.cancel(); }
        let result = async {
            self.runtime.cancel_all().await?;
            self.store.clear_sessions().await
        }.await;
        *self.requests.write().map_err(|_| Error::State("请求状态无法更新"))? = CancellationToken::new();
        result
    }
}
