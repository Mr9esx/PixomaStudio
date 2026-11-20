use crate::Result;
use sqlx::{
    SqlitePool,
    sqlite::{SqliteConnectOptions, SqliteJournalMode, SqlitePoolOptions},
};
use std::{
    collections::HashMap,
    path::{Path, PathBuf},
    sync::{Arc, Weak},
    time::Duration,
};
use tokio::sync::Mutex;

#[derive(Clone)]
pub struct Store {
    pub(crate) pool: SqlitePool,
    pub(crate) root: PathBuf,
    pub(crate) compute_address: std::net::SocketAddr,
    pub(crate) compute_error: Arc<std::sync::RwLock<Option<String>>>,
    pub(crate) model_updates: Arc<Mutex<()>>,
    pub(crate) asset_uploads: Arc<Mutex<()>>,
    pub(crate) video_exports: Arc<Mutex<HashMap<String, (String, Weak<Mutex<()>>)>>>,
    pub(crate) run_changes: tokio::sync::watch::Sender<u64>,
}

impl Store {
    pub(crate) fn try_lock_application_directory(root: &Path) -> Result<Option<std::fs::File>> {
        std::fs::create_dir_all(root)?;
        let path = root.join(".application.lock");
        if path.is_symlink() {
            return Err(crate::Error::Invalid("应用目录锁不能使用符号链接"));
        }
        let mut options = std::fs::OpenOptions::new();
        options.read(true).write(true).create(true).truncate(false);
        #[cfg(unix)]
        {
            use std::os::unix::fs::OpenOptionsExt;
            options.mode(0o600);
        }
        let file = options.open(path)?;
        match file.try_lock() {
            Ok(()) => Ok(Some(file)),
            Err(std::fs::TryLockError::WouldBlock) => Ok(None),
            Err(std::fs::TryLockError::Error(error)) => Err(error.into()),
        }
    }

    pub async fn open(path: &Path) -> Result<Self> {
        let root = path
            .parent()
            .ok_or(crate::Error::Invalid("数据库目录不存在"))?;
        std::fs::create_dir_all(root)?;
        let options = SqliteConnectOptions::new()
            .filename(path)
            .create_if_missing(true)
            .foreign_keys(true)
            .journal_mode(SqliteJournalMode::Wal)
            .busy_timeout(Duration::from_secs(5));
        let pool = SqlitePoolOptions::new()
            .max_connections(4)
            .connect_with(options)
            .await?;
        sqlx::migrate!("./migrations").run(&pool).await?;
        let configured: String = sqlx::query_scalar("SELECT listen_address FROM compute_connection WHERE id=1").fetch_one(&pool).await?;
        let compute_address = crate::compute_nodes::listen_address(&configured)?;
        sqlx::query("UPDATE compute_nodes SET reported_revision=NULL,comfyui_ready=0")
            .execute(&pool)
            .await?;
        let store = Self {
            pool,
            root: root.to_path_buf(),
            compute_address,
            compute_error: Arc::new(std::sync::RwLock::new(None)),
            model_updates: Arc::new(Mutex::new(())),
            asset_uploads: Arc::new(Mutex::new(())),
            video_exports: Arc::new(Mutex::new(HashMap::new())),
            run_changes: tokio::sync::watch::channel(0).0,
        };
        store.collect_unreferenced_asset_files().await?;
        Ok(store)
    }

    pub async fn close(&self) {
        self.pool.close().await;
    }

    pub(crate) fn notify_run_events(&self) {
        self.run_changes.send_modify(|revision| {
            *revision = revision.checked_add(1).expect("运行事件通知序号超过范围");
        });
    }

    pub fn data_root(&self) -> &Path {
        &self.root
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn application_directory_lock_excludes_another_process_and_releases_on_close() -> Result<()> {
        if let Some(root) = std::env::var_os("PIXOMA_TEST_APPLICATION_LOCK_ROOT") {
            assert!(Store::try_lock_application_directory(Path::new(&root))?.is_none());
            return Ok(());
        }
        let root = PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../.local-cache");
        std::fs::create_dir_all(&root)?;
        let directory = tempfile::Builder::new()
            .prefix("application-lock-")
            .tempdir_in(root)?;
        let lock = Store::try_lock_application_directory(directory.path())?
            .ok_or(crate::Error::State("应用目录锁缺失"))?;
        let child = std::process::Command::new(std::env::current_exe()?)
            .args([
                "--exact",
                "store::tests::application_directory_lock_excludes_another_process_and_releases_on_close",
                "--nocapture",
            ])
            .env("PIXOMA_TEST_APPLICATION_LOCK_ROOT", directory.path())
            .output()?;
        assert!(
            child.status.success(),
            "{}{}",
            String::from_utf8_lossy(&child.stdout),
            String::from_utf8_lossy(&child.stderr)
        );
        drop(lock);
        assert!(Store::try_lock_application_directory(directory.path())?.is_some());
        Ok(())
    }
}
