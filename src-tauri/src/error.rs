#[derive(Debug, thiserror::Error)]
pub enum Error {
    #[error("{0}")]
    Invalid(&'static str),
    #[error("记录不存在或无权访问")]
    NotFound,
    #[error("数据库操作失败：{0}")]
    Database(#[from] sqlx::Error),
    #[error("数据库迁移失败：{0}")]
    Migration(#[from] sqlx::migrate::MigrateError),
    #[error("文件操作失败：{0}")]
    Io(#[from] std::io::Error),
    #[error("序列化失败：{0}")]
    Json(#[from] serde_json::Error),
    #[error("压缩文件操作失败：{0}")]
    Zip(#[from] zip::result::ZipError),
    #[error("时间格式错误：{0}")]
    Time(#[from] time::error::Format),
    #[error("网络请求失败：{0}")]
    Http(#[from] reqwest::Error),
    #[error("模型请求失败：{0}")]
    Model(#[source] Box<rig::error::ProviderError>),
    #[error("MCP 连接失败：{0}")]
    Mcp(String),
    #[error("工具执行结果未知，禁止自动重复执行：{0}")]
    ToolResultUnknown(String),
    #[error("Agent 执行失败：{0}")]
    Agent(#[source] Box<rig::completion::PromptError>),
    #[error("运行已取消")]
    Cancelled,
    #[error("凭据操作失败：{0}")]
    Credentials(#[from] keyring::Error),
    #[error("{0}")]
    State(&'static str),
    #[error("{message}")]
    Remote {
        status: u16,
        code: i64,
        message: String,
    },
}

pub type Result<T> = std::result::Result<T, Error>;

impl From<rig::error::ProviderError> for Error {
    fn from(error: rig::error::ProviderError) -> Self {
        Self::Model(Box::new(error))
    }
}

impl From<rig::completion::PromptError> for Error {
    fn from(error: rig::completion::PromptError) -> Self {
        Self::Agent(Box::new(error))
    }
}

pub(crate) fn required(value: &str, message: &'static str) -> Result<()> {
    if value.trim().is_empty() {
        return Err(Error::Invalid(message));
    }
    Ok(())
}

pub(crate) fn timestamp() -> Result<String> {
    Ok(time::OffsetDateTime::now_utc().format(&time::format_description::well_known::Rfc3339)?)
}
