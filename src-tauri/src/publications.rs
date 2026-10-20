use crate::{Error, Result, Store, local_api::text};
use serde_json::{Value, json};

impl Store {
    pub(crate) async fn publication_asset(
        &self,
        account: &str,
        kind: &str,
        id: &str,
    ) -> Result<Value> {
        let asset = match kind {
            "skill" => self.get_skill(account, id).await?,
            "expert" => self.get_expert(account, id).await?,
            _ => return Err(Error::Invalid("发布对象类型无效")),
        };
        if asset["source"] != "user" {
            return Err(Error::Invalid("只能发布个人技能或专家"));
        }
        Ok(asset)
    }

    pub(crate) async fn asset_sync(
        &self,
        account: &str,
        kind: &str,
        id: &str,
    ) -> Result<Option<Value>> {
        Self::check_account(account)?;
        let document: Option<String> = sqlx::query_scalar(
            "SELECT document FROM asset_syncs WHERE account_id=? AND kind=? AND asset_id=?",
        )
        .bind(account)
        .bind(kind)
        .bind(id)
        .fetch_optional(&self.pool)
        .await?;
        document
            .map(|document| serde_json::from_str(&document).map_err(Error::from))
            .transpose()
    }

    pub(crate) async fn save_asset_sync(
        &self,
        account: &str,
        kind: &str,
        id: &str,
        sync: &Value,
    ) -> Result<()> {
        sqlx::query("INSERT INTO asset_syncs(account_id,kind,asset_id,document) VALUES(?,?,?,?) ON CONFLICT(account_id,kind,asset_id) DO UPDATE SET document=excluded.document")
            .bind(account).bind(kind).bind(id).bind(sync.to_string()).execute(&self.pool).await?;
        Ok(())
    }

    pub(crate) async fn apply_publication_state(
        &self,
        account: &str,
        kind: &str,
        asset: &mut Value,
    ) -> Result<()> {
        if asset["source"] != "user" {
            return Ok(());
        }
        if let Some(sync) = self.asset_sync(account, kind, text(asset, "id")?).await?
            && sync["remote"].is_object()
        {
            for field in [
                "published",
                "published_version",
                "published_at",
                "market_locked",
                "market_lock_reason",
            ] {
                if let Some(value) = sync["remote"].get(field) {
                    asset[field] = value.clone();
                }
            }
        }
        Ok(())
    }

    pub(crate) async fn publication_history(
        &self,
        account: &str,
        kind: &str,
        id: &str,
    ) -> Result<Value> {
        self.publication_asset(account, kind, id).await?;
        Ok(self
            .asset_sync(account, kind, id)
            .await?
            .map(|sync| sync["publications"].clone())
            .unwrap_or_else(|| json!([])))
    }
}
