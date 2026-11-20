use crate::{Error, LibraryQuery, Result, Store, local_api::text, taxonomy::check_tree_mode};
use serde::Deserialize;
use serde_json::{Value, json};

#[derive(Default, Deserialize)]
#[serde(default, deny_unknown_fields)]
pub(crate) struct TreeQuery {
    project_id: String,
    mode: String,
    parent_id: String,
    cursor: Option<String>,
    limit: Option<i64>,
}

fn group(mode: &str, value: &str, label: &str, count: i64) -> Value {
    json!({ "id": format!("{mode}:{value}"), "label": label, "count": count, "kind": "group", "group_value": if mode == "category-direct" { format!("direct:{value}") } else { value.to_owned() } })
}

impl Store {
    pub(crate) async fn library_tree(&self, tree: TreeQuery) -> Result<Value> {
        let mode = if tree.mode.is_empty() {
            "asset"
        } else {
            &tree.mode
        };
        check_tree_mode(mode)?;
        if !tree.project_id.is_empty() {
            self.get_project(&tree.project_id).await?;
        }
        let mut query = LibraryQuery {
            project_id: tree.project_id.clone(),
            cursor: tree.cursor.clone(),
            limit: tree.limit,
            ..Default::default()
        };
        let parent = if tree.parent_id.is_empty() {
            None
        } else {
            let (kind, value) = tree
                .parent_id
                .split_once(':')
                .ok_or(Error::Invalid("资产分组标识无效"))?;
            if value.is_empty()
                || (kind != mode && !(mode == "category" && kind == "category-direct"))
                || mode == "asset"
            {
                return Err(Error::Invalid("资产分组标识无效"));
            }
            Some((kind, value))
        };
        if let Some((kind, value)) = parent {
            match kind {
                "format" => query.format = Some(value.to_owned()),
                "rating" => {
                    query.rating = Some(value.parse().map_err(|_| Error::Invalid("评分分组无效"))?)
                }
                "tag" => query.tag_ids = Some(value.to_owned()),
                "session" => query.session_id = Some(value.to_owned()),
                "category-direct" => query.category_id = Some(format!("direct:{value}")),
                "category" if value == "uncategorized" => {
                    query.category_id = Some("none".to_owned())
                }
                "category" => {
                    let category = self.category(value).await?;
                    if category.project_id != tree.project_id {
                        return Err(Error::NotFound);
                    }
                    let groups = self.category_groups(&tree.project_id, value).await?;
                    if !groups.is_empty() {
                        return paginate(groups, &tree);
                    }
                    query.category_id = Some(format!("direct:{value}"));
                }
                _ => return Err(Error::Invalid("资产分组标识无效")),
            }
        }
        if mode == "asset" || parent.is_some() {
            let page = self.library_assets(&query).await?;
            let nodes = page["items"].as_array().ok_or(Error::State("资产列表无效"))?.iter().map(|item| Ok(json!({
                "id": format!("asset:{}", text(item, "id")?), "label": item["display_name"], "kind": "asset", "count": 0,
                "asset_id": item["id"], "asset_kind": item["asset"]["kind"],
            }))).collect::<Result<Vec<Value>>>()?;
            return Ok(json!({ "nodes": nodes, "next_cursor": page["next_cursor"] }));
        }
        if mode == "category" {
            return paginate(self.category_groups(&tree.project_id, "").await?, &tree);
        }
        let rows: Vec<(String, String, i64)> = match mode {
            "format" => query.sql("v.format,CASE WHEN v.format='' THEN '未知格式' ELSE v.format END,count(*)")?
                .push(" GROUP BY v.format ORDER BY v.format").build_query_as().fetch_all(&self.pool).await?,
            "rating" => query.sql("CAST(p.rating AS TEXT),CASE WHEN p.rating=0 THEN '未评分' ELSE CAST(p.rating AS TEXT) END,count(*)")?
                .push(" GROUP BY p.rating ORDER BY p.rating DESC").build_query_as().fetch_all(&self.pool).await?,
            "tag" => sqlx::query_as("SELECT t.id,t.name,count(*) FROM tags t JOIN project_asset_tags j ON j.tag_id=t.id JOIN project_assets p ON p.id=j.project_asset_id WHERE coalesce(p.project_id,'')=? AND p.archived_at IS NULL GROUP BY t.id,t.name ORDER BY t.name,t.id")
                .bind(&tree.project_id).fetch_all(&self.pool).await?,
            "session" => sqlx::query_as("SELECT u.session_id,max(coalesce(s.title,u.session_title_snapshot)),count(DISTINCT p.id) FROM project_assets p JOIN asset_usages u ON u.asset_id=p.asset_id LEFT JOIN sessions s ON s.id=u.session_id WHERE coalesce(p.project_id,'')=? AND p.archived_at IS NULL GROUP BY u.session_id ORDER BY max(coalesce(s.title,u.session_title_snapshot)),u.session_id")
                .bind(&tree.project_id).fetch_all(&self.pool).await?,
            _ => return Err(Error::Invalid("资产分组方式无效")),
        };
        let mut groups: Vec<Value> = rows
            .iter()
            .map(|(value, label, count)| group(mode, value, label, *count))
            .collect();
        if mode == "tag" || mode == "session" {
            let (value, label) = if mode == "tag" {
                query.tag_ids = Some("untagged".to_owned());
                ("untagged", "无标签")
            } else {
                query.session_id = Some("none".to_owned());
                ("none", "无来源对话")
            };
            let count = self.library_count(&query).await?;
            if count > 0 {
                groups.push(group(mode, value, label, count));
            }
        }
        paginate(groups, &tree)
    }

    async fn library_count(&self, query: &LibraryQuery) -> Result<i64> {
        Ok(query
            .sql("count(*)")?
            .build_query_scalar()
            .fetch_one(&self.pool)
            .await?)
    }

    async fn category_groups(&self, project: &str, parent: &str) -> Result<Vec<Value>> {
        let categories = self.categories(project).await?;
        let mut groups = Vec::new();
        for category in categories
            .iter()
            .filter(|category| category.parent_id == parent)
        {
            let query = LibraryQuery {
                project_id: project.to_owned(),
                category_id: Some(category.id.clone()),
                ..Default::default()
            };
            groups.push(group(
                "category",
                &category.id,
                &category.name,
                self.library_count(&query).await?,
            ));
        }
        let query = LibraryQuery {
            project_id: project.to_owned(),
            category_id: Some(if parent.is_empty() {
                "none".to_owned()
            } else {
                format!("direct:{parent}")
            }),
            ..Default::default()
        };
        let count = self.library_count(&query).await?;
        if count > 0 {
            groups.push(if parent.is_empty() {
                group("category", "uncategorized", "未分类", count)
            } else {
                group("category-direct", parent, "本级资产", count)
            });
        }
        Ok(groups)
    }
}

fn paginate(nodes: Vec<Value>, tree: &TreeQuery) -> Result<Value> {
    let limit = tree.limit.unwrap_or(50);
    if !(1..=100).contains(&limit) {
        return Err(Error::Invalid("分页范围无效"));
    }
    let start: usize = match &tree.cursor {
        Some(cursor) => cursor.parse().map_err(|_| Error::Invalid("分页标识无效"))?,
        None => 0,
    };
    if start > nodes.len() {
        return Err(Error::Invalid("分页标识无效"));
    }
    let end = start.saturating_add(limit as usize).min(nodes.len());
    Ok(
        json!({ "nodes": nodes[start..end], "next_cursor": if end < nodes.len() { Some(end.to_string()) } else { None } }),
    )
}
