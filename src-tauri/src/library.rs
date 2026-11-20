use crate::{
    Error, Result, Store,
    error::{required, timestamp},
    local_api::text,
};
use serde::Deserialize;
use serde_json::{Value, json};
use sqlx::{QueryBuilder, Row, Sqlite};
use uuid::Uuid;

#[derive(Debug, Default, Deserialize)]
#[serde(default)]
pub struct LibraryQuery {
    pub project_id: String,
    pub category_id: Option<String>,
    pub session_id: Option<String>,
    pub kind: Option<String>,
    pub format: Option<String>,
    pub rating: Option<i64>,
    pub tag_ids: Option<String>,
    pub sort: Option<String>,
    pub archived: Option<bool>,
    pub q: Option<String>,
    pub width_min: Option<i64>,
    pub width_max: Option<i64>,
    pub height_min: Option<i64>,
    pub height_max: Option<i64>,
    pub size_min: Option<i64>,
    pub size_max: Option<i64>,
    pub added_from: Option<String>,
    pub added_to: Option<String>,
    pub duplicates: Option<bool>,
    pub cursor: Option<String>,
    pub limit: Option<i64>,
}

impl LibraryQuery {
    pub(crate) fn sql<'a>(&'a self, projection: &str) -> Result<QueryBuilder<'a, Sqlite>> {
        let mut sql = QueryBuilder::new(format!(
            "SELECT {projection} FROM project_assets p JOIN assets a ON a.id=p.asset_id JOIN asset_versions v ON v.id=p.asset_version_id AND v.asset_id=p.asset_id WHERE coalesce(p.project_id,'')="
        ));
        sql.push_bind(&self.project_id);
        sql.push(if self.archived == Some(true) {
            " AND p.archived_at IS NOT NULL"
        } else {
            " AND p.archived_at IS NULL"
        });
        if let Some(category) = &self.category_id {
            if ["none", "uncategorized"].contains(&category.as_str()) {
                sql.push(" AND p.category_id IS NULL");
            } else if let Some(category) = category.strip_prefix("direct:") {
                sql.push(" AND p.category_id=")
                    .push_bind(category.to_owned());
            } else {
                sql.push(" AND p.category_id IN (WITH RECURSIVE children(id) AS (SELECT id FROM categories WHERE id=").push_bind(category)
                    .push(" UNION SELECT c.id FROM categories c JOIN children ON c.parent_id=children.id) SELECT id FROM children)");
            }
        }
        if let Some(kind) = &self.kind {
            sql.push(" AND a.kind=").push_bind(kind);
        }
        if let Some(format) = &self.format {
            sql.push(" AND v.format=")
                .push_bind(if format == "none" { "" } else { format });
        }
        if let Some(rating) = self.rating {
            if !(0..=5).contains(&rating) {
                return Err(Error::Invalid("评分范围为 0 至 5"));
            }
            sql.push(" AND p.rating=").push_bind(rating);
        }
        if let Some(session) = &self.session_id {
            if session == "none" {
                sql.push(
                    " AND NOT EXISTS(SELECT 1 FROM asset_usages u WHERE u.asset_id=p.asset_id)",
                );
            } else {
                sql.push(" AND EXISTS(SELECT 1 FROM asset_usages u WHERE u.asset_id=p.asset_id AND u.session_id=").push_bind(session).push(")");
            }
        }
        if let Some(tags) = &self.tag_ids {
            if ["none", "untagged"].contains(&tags.as_str()) {
                sql.push(" AND NOT EXISTS(SELECT 1 FROM project_asset_tags t WHERE t.project_asset_id=p.id)");
            } else {
                for tag in tags.split(',') {
                    required(tag, "标签标识不能为空")?;
                    sql.push(" AND EXISTS(SELECT 1 FROM project_asset_tags t WHERE t.project_asset_id=p.id AND t.tag_id=").push_bind(tag.to_owned()).push(")");
                }
            }
        }
        if let Some(search) = &self.q {
            sql.push(" AND (instr(lower(p.display_name),lower(").push_bind(search).push("))>0 OR instr(lower(a.name),lower(").push_bind(search)
                .push("))>0 OR EXISTS(SELECT 1 FROM project_asset_tags j JOIN tags t ON t.id=j.tag_id WHERE j.project_asset_id=p.id AND instr(lower(t.name),lower(")
                .push_bind(search).push("))>0))");
        }
        for (column, lower, upper) in [
            ("v.width_px", self.width_min, self.width_max),
            ("v.height_px", self.height_min, self.height_max),
            ("v.size_bytes", self.size_min, self.size_max),
        ] {
            if lower.is_some_and(|value| value < 0)
                || upper.is_some_and(|value| value < 0)
                || matches!((lower, upper), (Some(a), Some(b)) if a > b)
            {
                return Err(Error::Invalid("资产范围筛选参数无效"));
            }
            if let Some(value) = lower {
                sql.push(" AND ").push(column).push(">=").push_bind(value);
            }
            if let Some(value) = upper {
                sql.push(" AND ").push(column).push("<=").push_bind(value);
            }
        }
        if let (Some(from), Some(to)) = (&self.added_from, &self.added_to) {
            let parse = |date: &str| {
                time::OffsetDateTime::parse(date, &time::format_description::well_known::Rfc3339)
                    .map_err(|_| Error::Invalid("日期筛选参数无效"))
            };
            if parse(from)? >= parse(to)? {
                return Err(Error::Invalid("开始日期必须早于结束日期"));
            }
        }
        for (date, operator) in [(&self.added_from, ">="), (&self.added_to, "<")] {
            if let Some(date) = date {
                time::OffsetDateTime::parse(date, &time::format_description::well_known::Rfc3339)
                    .map_err(|_| Error::Invalid("日期筛选参数无效"))?;
                sql.push(" AND julianday(p.added_at)")
                    .push(operator)
                    .push("julianday(")
                    .push_bind(date)
                    .push(")");
            }
        }
        if self.duplicates == Some(true) {
            sql.push(" AND EXISTS(SELECT 1 FROM project_assets p2 JOIN asset_versions v2 ON v2.id=p2.asset_version_id WHERE coalesce(p2.project_id,'')=coalesce(p.project_id,'') AND p2.asset_id<>p.asset_id AND p2.archived_at IS NULL AND v2.digest=v.digest)");
        }
        Ok(sql)
    }
}

impl Store {
    pub async fn library_assets(&self, query: &LibraryQuery) -> Result<Value> {
        let limit = query.limit.unwrap_or(50);
        let offset: i64 = match &query.cursor {
            Some(cursor) => cursor.parse().map_err(|_| Error::Invalid("分页标识无效"))?,
            None => 0,
        };
        if !(1..=100).contains(&limit) || offset < 0 {
            return Err(Error::Invalid("分页范围无效"));
        }
        let total: i64 = query
            .sql("count(*)")?
            .build_query_scalar()
            .fetch_one(&self.pool)
            .await?;
        let mut statement = query.sql("p.id")?;
        statement.push(match query.sort.as_deref().unwrap_or("recent") {
            "recent" => " ORDER BY p.added_at DESC,p.id",
            "name" => " ORDER BY p.display_name,p.id",
            "rating" => " ORDER BY p.rating DESC,p.added_at DESC,p.id",
            _ => return Err(Error::Invalid("排序方式无效")),
        });
        statement
            .push(" LIMIT ")
            .push_bind(limit)
            .push(" OFFSET ")
            .push_bind(offset);
        let ids: Vec<String> = statement.build_query_scalar().fetch_all(&self.pool).await?;
        let mut items = Vec::with_capacity(ids.len());
        for id in ids {
            items.push(self.project_asset(&id).await?);
        }
        Ok(
            json!({ "items": items, "total": total, "next_cursor": if offset + limit < total { Some((offset + limit).to_string()) } else { None } }),
        )
    }

    pub async fn project_asset(&self, id: &str) -> Result<Value> {
        let row = sqlx::query("SELECT * FROM project_assets WHERE id=?")
            .bind(id)
            .fetch_optional(&self.pool)
            .await?
            .ok_or(Error::NotFound)?;
        let asset = self.get_asset(&row.get::<String, _>("asset_id")).await?;
        let version_id: String = row.get("asset_version_id");
        let version = asset
            .versions
            .iter()
            .find(|version| version.id == version_id)
            .ok_or(Error::State("项目资产版本不存在"))?;
        let tags = sqlx::query("SELECT t.id,t.name FROM tags t JOIN project_asset_tags j ON j.tag_id=t.id WHERE j.project_asset_id=? ORDER BY t.name")
            .bind(id).fetch_all(&self.pool).await?.iter().map(|row| json!({ "id": row.get::<String, _>("id"), "name": row.get::<String, _>("name") })).collect::<Vec<_>>();
        Ok(json!({
            "id": id, "project_id": row.get::<Option<String>, _>("project_id").unwrap_or_default(),
            "asset_id": asset.id, "asset_version_id": version_id, "display_name": row.get::<String, _>("display_name"),
            "category_id": row.get::<Option<String>, _>("category_id").unwrap_or_default(), "rating": row.get::<i64, _>("rating"),
            "tags": tags, "added_at": row.get::<String, _>("added_at"), "updated_at": row.get::<String, _>("updated_at"),
            "archived_at": row.get::<Option<String>, _>("archived_at"), "copy_source": row.get::<Option<String>, _>("copy_source").map(|value| serde_json::from_str::<Value>(&value)).transpose()?,
            "version": version, "usages": asset.usages, "versions": asset.versions, "asset": asset,
        }))
    }

    pub async fn place_asset(
        &self,
        project_id: &str,
        asset_id: &str,
        version_id: &str,
    ) -> Result<Value> {
        if !project_id.is_empty() {
            self.get_project(project_id).await?;
        }
        let asset = self.get_asset(asset_id).await?;
        if !asset
            .versions
            .iter()
            .any(|version| version.id == version_id)
        {
            return Err(Error::NotFound);
        }
        let project = if project_id.is_empty() {
            None
        } else {
            Some(project_id)
        };
        let now = timestamp()?;
        sqlx::query("INSERT INTO project_assets(id,project_id,asset_id,asset_version_id,display_name,added_at,updated_at) VALUES(?,?,?,?,?,?,?) ON CONFLICT DO NOTHING")
            .bind(Uuid::new_v4().to_string()).bind(project).bind(asset_id).bind(version_id).bind(asset.name).bind(&now).bind(&now).execute(&self.pool).await?;
        let id: String = sqlx::query_scalar(
            "SELECT id FROM project_assets WHERE coalesce(project_id,'')=? AND asset_id=?",
        )
        .bind(project_id)
        .bind(asset_id)
        .fetch_one(&self.pool)
        .await?;
        self.project_asset(&id).await
    }

    pub async fn create_tag(&self, name: &str) -> Result<Value> {
        required(name, "标签名称不能为空")?;
        sqlx::query("INSERT INTO tags(id,name) VALUES(?,?) ON CONFLICT(name) DO NOTHING")
            .bind(Uuid::new_v4().to_string())
            .bind(name.trim())
            .execute(&self.pool)
            .await?;
        let id: String = sqlx::query_scalar("SELECT id FROM tags WHERE name=?")
            .bind(name.trim())
            .fetch_one(&self.pool)
            .await?;
        Ok(json!({ "id": id, "name": name.trim() }))
    }

    pub async fn create_category(
        &self,
        project_id: &str,
        name: &str,
        parent_id: &str,
    ) -> Result<Value> {
        required(name, "分类名称不能为空")?;
        if name.trim().chars().count() > 128 {
            return Err(Error::Invalid("分类名称不能超过 128 个字符"));
        }
        if !project_id.is_empty() {
            self.get_project(project_id).await?;
        }
        if !parent_id.is_empty() {
            self.check_category(project_id, parent_id).await?;
        }
        let id = Uuid::new_v4().to_string();
        let now = timestamp()?;
        sqlx::query("INSERT INTO categories(id,project_id,parent_id,name,created_at,updated_at) VALUES(?,?,?,?,?,?)")
            .bind(&id).bind(if project_id.is_empty() { None } else { Some(project_id) })
            .bind(if parent_id.is_empty() { None } else { Some(parent_id) }).bind(name.trim()).bind(&now).bind(&now).execute(&self.pool).await?;
        Ok(
            json!({ "id": id, "project_id": project_id, "parent_id": parent_id, "name": name.trim(), "created_at": now, "updated_at": now }),
        )
    }

    async fn check_category(&self, project_id: &str, id: &str) -> Result<()> {
        let count: i64 = sqlx::query_scalar(
            "SELECT count(*) FROM categories WHERE coalesce(project_id,'')=? AND id=?",
        )
        .bind(project_id)
        .bind(id)
        .fetch_one(&self.pool)
        .await?;
        if count != 1 {
            return Err(Error::NotFound);
        }
        Ok(())
    }

    pub async fn patch_project_asset(&self, id: &str, patch: &Value) -> Result<Value> {
        let fields = patch.as_object().ok_or(Error::Invalid("资产参数无效"))?;
        if fields.keys().any(|key| {
            !["display_name", "category_id", "rating", "archived"].contains(&key.as_str())
        }) {
            return Err(Error::Invalid("资产字段无效"));
        }
        let existing = self.project_asset(id).await?;
        if let Some(rating) = patch.get("rating") {
            let rating = rating.as_i64().ok_or(Error::Invalid("评分类型无效"))?;
            if !(0..=5).contains(&rating) {
                return Err(Error::Invalid("评分范围为 0 至 5"));
            }
        }
        if let Some(name) = patch.get("display_name") {
            required(
                name.as_str().ok_or(Error::Invalid("资产名称类型无效"))?,
                "资产名称不能为空",
            )?;
        }
        if let Some(category) = patch.get("category_id") {
            let category = category
                .as_str()
                .ok_or(Error::Invalid("分类标识类型无效"))?;
            if !category.is_empty() {
                self.check_category(text(&existing, "project_id")?, category)
                    .await?;
            }
        }
        let mut sql = QueryBuilder::<Sqlite>::new("UPDATE project_assets SET updated_at=");
        sql.push_bind(timestamp()?);
        if let Some(name) = patch.get("display_name") {
            sql.push(",display_name=").push_bind(
                name.as_str()
                    .ok_or(Error::Invalid("资产名称类型无效"))?
                    .trim()
                    .to_owned(),
            );
        }
        if let Some(rating) = patch.get("rating") {
            sql.push(",rating=")
                .push_bind(rating.as_i64().ok_or(Error::Invalid("评分类型无效"))?);
        }
        if let Some(category) = patch.get("category_id") {
            sql.push(",category_id=").push_bind(
                category
                    .as_str()
                    .filter(|id| !id.is_empty())
                    .map(str::to_owned),
            );
        }
        if let Some(archived) = patch.get("archived") {
            let archived = archived
                .as_bool()
                .ok_or(Error::Invalid("归档状态类型无效"))?;
            sql.push(",archived_at=")
                .push_bind(if archived { Some(timestamp()?) } else { None });
        }
        sql.push(" WHERE id=").push_bind(id);
        sql.build().execute(&self.pool).await?;
        self.project_asset(id).await
    }

    pub async fn set_project_asset_tags(&self, id: &str, tags: &[String]) -> Result<Value> {
        self.project_asset(id).await?;
        for tag in tags {
            let exists: i64 = sqlx::query_scalar("SELECT count(*) FROM tags WHERE id=?")
                .bind(tag)
                .fetch_one(&self.pool)
                .await?;
            if exists != 1 {
                return Err(Error::NotFound);
            }
        }
        let mut transaction = self.pool.begin().await?;
        sqlx::query("DELETE FROM project_asset_tags WHERE project_asset_id=?")
            .bind(id)
            .execute(&mut *transaction)
            .await?;
        for tag in tags {
            sqlx::query("INSERT INTO project_asset_tags(project_asset_id,tag_id) VALUES(?,?) ON CONFLICT DO NOTHING")
                .bind(id).bind(tag).execute(&mut *transaction).await?;
        }
        transaction.commit().await?;
        self.project_asset(id).await
    }
}
