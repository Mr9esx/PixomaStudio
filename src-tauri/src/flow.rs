use crate::{
    Error, Result, Store,
    error::{required, timestamp},
    local_api::text,
};
use serde::{Deserialize, Serialize};
use serde_json::{Value, json};
use uuid::Uuid;

#[derive(Deserialize, Serialize)]
#[serde(deny_unknown_fields)]
pub(crate) struct Position {
    x: f64,
    y: f64,
}

impl Position {
    fn validate(&self) -> Result<()> {
        if !self.x.is_finite() || !self.y.is_finite() {
            return Err(Error::Invalid("节点位置必须为有限数值"));
        }
        Ok(())
    }
}

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
pub(crate) struct CreateNode {
    r#type: String,
    title: String,
    #[serde(default)]
    body: String,
    position: Position,
}

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
pub(crate) struct NodePosition {
    id: String,
    position: Position,
    sort_order: i64,
}

#[derive(Deserialize, Serialize)]
#[serde(deny_unknown_fields)]
pub(crate) struct CreateEdge {
    source: String,
    target: String,
    #[serde(default)]
    label: String,
    #[serde(default)]
    source_output_key: String,
    #[serde(default)]
    target_input_key: String,
}

impl Store {
    pub(crate) async fn flow_node(&self, account: &str, session: &str, id: &str) -> Result<Value> {
        let document: String = sqlx::query_scalar(
            "SELECT document FROM flow_nodes WHERE account_id=? AND session_id=? AND id=?",
        )
        .bind(account)
        .bind(session)
        .bind(id)
        .fetch_optional(&self.pool)
        .await?
        .ok_or(Error::NotFound)?;
        Ok(serde_json::from_str(&document)?)
    }

    pub(crate) async fn create_flow_node(
        &self,
        account: &str,
        session: &str,
        input: CreateNode,
    ) -> Result<Value> {
        self.get_session(account, session).await?;
        required(&input.title, "节点标题不能为空")?;
        if !["stage", "plan", "operation"].contains(&input.r#type.as_str()) {
            return Err(Error::Invalid("节点类型无效"));
        }
        input.position.validate()?;
        let id = Uuid::new_v4().to_string();
        let now = timestamp()?;
        let mut transaction = self.pool.begin().await?;
        let order: i64 = sqlx::query_scalar("SELECT coalesce(max(CAST(json_extract(document,'$.sort_order') AS INTEGER))+10,0) FROM flow_nodes WHERE account_id=? AND session_id=?")
            .bind(account).bind(session).fetch_one(&mut *transaction).await?;
        let document = json!({ "id": id, "type": input.r#type, "title": input.title.trim(), "body": input.body.trim(), "position": input.position, "sort_order": order, "updated_at": now });
        sqlx::query("INSERT INTO flow_nodes(id,account_id,session_id,created_at,document) VALUES(?,?,?,?,?)")
            .bind(&id).bind(account).bind(session).bind(&now).bind(document.to_string()).execute(&mut *transaction).await?;
        transaction.commit().await?;
        Ok(document)
    }

    pub(crate) async fn update_flow_positions(
        &self,
        account: &str,
        session: &str,
        nodes: Vec<NodePosition>,
    ) -> Result<()> {
        self.get_session(account, session).await?;
        if nodes.len() > 10000 {
            return Err(Error::Invalid("一次最多更新 10000 个节点"));
        }
        let now = timestamp()?;
        let mut transaction = self.pool.begin().await?;
        for node in nodes {
            node.position.validate()?;
            if node.sort_order < 0 {
                return Err(Error::Invalid("节点顺序不能小于零"));
            }
            let changed = sqlx::query("UPDATE flow_nodes SET document=json_set(document,'$.position',json(?),'$.sort_order',?,'$.updated_at',?) WHERE account_id=? AND session_id=? AND id=?")
                .bind(serde_json::to_string(&node.position)?).bind(node.sort_order).bind(&now).bind(account).bind(session).bind(node.id).execute(&mut *transaction).await?;
            if changed.rows_affected() != 1 {
                return Err(Error::NotFound);
            }
        }
        transaction.commit().await?;
        Ok(())
    }

    pub(crate) async fn delete_flow_node(
        &self,
        account: &str,
        session: &str,
        id: &str,
    ) -> Result<()> {
        self.get_session(account, session).await?;
        let mut transaction = self.pool.begin().await?;
        let protected: i64 = sqlx::query_scalar("SELECT count(*) FROM workflow_executions WHERE account_id=? AND session_id=? AND json_extract(document,'$.operation_node_id')=?")
            .bind(account).bind(session).bind(id).fetch_one(&mut *transaction).await?;
        if protected != 0 {
            return Err(Error::Invalid("工作流执行节点不能删除"));
        }
        let changed =
            sqlx::query("DELETE FROM flow_nodes WHERE account_id=? AND session_id=? AND id=?")
                .bind(account)
                .bind(session)
                .bind(id)
                .execute(&mut *transaction)
                .await?;
        if changed.rows_affected() != 1 {
            return Err(Error::NotFound);
        }
        transaction.commit().await?;
        Ok(())
    }

    pub(crate) async fn create_flow_edge(
        &self,
        account: &str,
        session: &str,
        input: CreateEdge,
    ) -> Result<Value> {
        self.get_session(account, session).await?;
        if input.source == input.target {
            return Err(Error::Invalid("节点不能连接自身"));
        }
        let source = self.flow_node(account, session, &input.source).await?;
        self.flow_node(account, session, &input.target).await?;
        self.validate_flow_ports(account, session, &source, &input)
            .await?;
        let id = Uuid::new_v4().to_string();
        let mut document = serde_json::to_value(&input)?;
        document["id"] = json!(id);
        document["label"] = json!(input.label.trim());
        sqlx::query("INSERT INTO flow_edges(id,account_id,session_id,source,target,created_at,document) VALUES(?,?,?,?,?,?,?)")
            .bind(&id).bind(account).bind(session).bind(input.source).bind(input.target).bind(timestamp()?).bind(document.to_string()).execute(&self.pool).await?;
        Ok(document)
    }

    async fn validate_flow_ports(
        &self,
        account: &str,
        session: &str,
        source: &Value,
        input: &CreateEdge,
    ) -> Result<()> {
        let outputs = source.get("outputs").and_then(Value::as_array);
        let output = outputs.and_then(|outputs| {
            outputs
                .iter()
                .find(|output| output["key"] == input.source_output_key)
        });
        if (outputs.is_some_and(|outputs| !outputs.is_empty()) && output.is_none())
            || (output.is_none() && !input.source_output_key.is_empty())
        {
            return Err(Error::Invalid("工作流输出字段无效"));
        }
        let executions = self
            .session_documents("workflow_executions", account, session)
            .await?;
        let execution = executions
            .iter()
            .find(|execution| execution["operation_node_id"] == input.target);
        let fields = execution.and_then(|execution| execution["input_fields"].as_array());
        let field = fields.and_then(|fields| {
            fields
                .iter()
                .find(|field| field["key"] == input.target_input_key)
        });
        if (fields.is_some_and(|fields| !fields.is_empty()) && field.is_none())
            || (field.is_none() && !input.target_input_key.is_empty())
        {
            return Err(Error::Invalid("工作流输入字段无效"));
        }
        if output.is_some() && field.is_none() {
            return Err(Error::Invalid("请选择匹配的工作流输入字段"));
        }
        if let Some(field) = field {
            let (asset_id, version_id) = match output {
                Some(output) => {
                    if output["type"] != field["type"] {
                        return Err(Error::Invalid("工作流输入与输出类型不同"));
                    }
                    (text(output, "asset_id")?, text(output, "asset_version_id")?)
                }
                None => {
                    if source["type"] != "asset" {
                        return Err(Error::Invalid("工作流输入需要资产来源"));
                    }
                    let asset = self.get_asset(account, text(source, "asset_id")?).await?;
                    if json!(asset.kind) != field["type"] {
                        return Err(Error::Invalid("工作流输入与资产类型不同"));
                    }
                    (text(source, "asset_id")?, text(source, "asset_version_id")?)
                }
            };
            let matched = execution
                .and_then(|execution| execution["inputs"].as_array())
                .is_some_and(|inputs| {
                    inputs.iter().any(|item| {
                        item["key"] == field["key"]
                            && item["asset_id"] == asset_id
                            && item["asset_version_id"] == version_id
                    })
                });
            if !matched {
                return Err(Error::Invalid("工作流输入未使用所选资产版本"));
            }
        }
        Ok(())
    }

    pub(crate) async fn delete_flow_edge(
        &self,
        account: &str,
        session: &str,
        id: &str,
    ) -> Result<()> {
        self.get_session(account, session).await?;
        let mut transaction = self.pool.begin().await?;
        let document: String = sqlx::query_scalar(
            "SELECT document FROM flow_edges WHERE account_id=? AND session_id=? AND id=?",
        )
        .bind(account)
        .bind(session)
        .bind(id)
        .fetch_optional(&mut *transaction)
        .await?
        .ok_or(Error::NotFound)?;
        let document: Value = serde_json::from_str(&document)?;
        if document["target_input_key"]
            .as_str()
            .is_some_and(|key| !key.is_empty())
        {
            return Err(Error::Invalid("工作流输入连线不能删除"));
        }
        sqlx::query("DELETE FROM flow_edges WHERE account_id=? AND session_id=? AND id=?")
            .bind(account)
            .bind(session)
            .bind(id)
            .execute(&mut *transaction)
            .await?;
        transaction.commit().await?;
        Ok(())
    }
}
