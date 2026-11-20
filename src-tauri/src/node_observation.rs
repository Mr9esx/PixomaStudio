use crate::{Error, Result, Store};
use serde_json::{Value, json};
use sqlx::Row;
use time::{OffsetDateTime, format_description::well_known::Rfc3339};

impl Store {
    pub async fn compute_node_metrics(&self, id: &str, window: &str, from: Option<&str>, to: Option<&str>) -> Result<Value> {
        self.get_compute_node(id).await?;
        let now = OffsetDateTime::now_utc();
        let (from, to) = match window {
            "1h" => (now - time::Duration::hours(1), now),
            "6h" => (now - time::Duration::hours(6), now),
            "24h" => (now - time::Duration::hours(24), now),
            "custom" => {
                let from = OffsetDateTime::parse(from.ok_or(Error::Invalid("缺少监控开始时间"))?, &Rfc3339).map_err(|_| Error::Invalid("监控开始时间无效"))?;
                let to = OffsetDateTime::parse(to.ok_or(Error::Invalid("缺少监控结束时间"))?, &Rfc3339).map_err(|_| Error::Invalid("监控结束时间无效"))?;
                if from >= to || to > now || to - from > time::Duration::days(30) { return Err(Error::Invalid("监控时间范围无效或超过 30 天")); }
                (from, to)
            }
            _ => return Err(Error::Invalid("监控时间范围无效")),
        };
        let bucket = ((to - from).whole_seconds() / 900).max(1);
        let documents: Vec<String> = sqlx::query_scalar("SELECT document FROM (SELECT document,received_at,row_number() OVER (PARTITION BY CAST(unixepoch(received_at) / ? AS INTEGER) ORDER BY received_at DESC) AS position FROM compute_node_metrics WHERE node_id=? AND julianday(received_at)>=julianday(?) AND julianday(received_at)<=julianday(?)) WHERE position=1 ORDER BY received_at")
            .bind(bucket).bind(id).bind(from.format(&Rfc3339)?).bind(to.format(&Rfc3339)?).fetch_all(&self.pool).await?;
        let series = documents.iter().map(|document| serde_json::from_str::<Value>(document)).collect::<std::result::Result<Vec<_>, _>>()?;
        Ok(json!({ "latest": series.last(), "series": series }))
    }

    pub async fn compute_node_observation(&self, id: &str, offset: i64) -> Result<Value> {
        self.get_compute_node(id).await?;
        if offset < 0 { return Err(Error::Invalid("分页参数无效")); }
        let row = sqlx::query("SELECT count(*) AS task_count,sum(CASE WHEN status='succeeded' THEN 1 ELSE 0 END) AS succeeded,sum(CASE WHEN status IN ('succeeded','failed','cancelled','result_unknown') THEN 1 ELSE 0 END) AS finished FROM local_executions WHERE node_id=?")
            .bind(id).fetch_one(&self.pool).await?;
        let runtime: f64 = sqlx::query_scalar("SELECT coalesce(sum(max(0.0,(julianday(CASE WHEN e.status IN ('succeeded','failed','cancelled','result_unknown') THEN e.updated_at ELSE strftime('%Y-%m-%dT%H:%M:%fZ','now') END)-julianday((SELECT min(created_at) FROM local_execution_events WHERE execution_id=e.id AND json_extract(document,'$.kind')='submitted')))*86400000)),0.0) FROM local_executions e WHERE node_id=?")
            .bind(id).fetch_one(&self.pool).await?;
        let count: i64 = row.get("task_count");
        let finished: Option<i64> = row.get("finished");
        let succeeded: Option<i64> = row.get("succeeded");
        let success_rate = finished.filter(|value| *value > 0).map(|finished| succeeded.unwrap_or(0) as f64 / finished as f64);
        let workflows = sqlx::query("SELECT id,name,enabled FROM workflows WHERE json_extract(draft,'$.execution.node_id')=? OR id IN (SELECT workflow_id FROM workflow_versions WHERE json_extract(definition,'$.execution.node_id')=?) ORDER BY name,id")
            .bind(id).bind(id).fetch_all(&self.pool).await?.iter().map(|row| json!({ "id": row.get::<String, _>("id"), "name": row.get::<String, _>("name"), "enabled": row.get::<bool, _>("enabled") })).collect::<Vec<_>>();
        let ids: Vec<String> = sqlx::query_scalar("SELECT id FROM local_executions WHERE node_id=? ORDER BY created_at DESC,id LIMIT 10 OFFSET ?").bind(id).bind(offset).fetch_all(&self.pool).await?;
        let mut executions = Vec::new();
        for id in ids { executions.push(self.get_local_execution(&id).await?); }
        Ok(json!({ "stats": { "task_count": count, "runtime_ms": runtime.round() as i64, "success_rate": success_rate }, "workflows": workflows, "executions": executions, "offset": offset }))
    }
}
