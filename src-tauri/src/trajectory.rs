use crate::{Error, Result, Store, context_records::elapsed_ms, local_api::text, tools::RunTool};
use rig::message::ToolCall;
use serde_json::{Value, json};
use sqlx::Row;

fn model_record(document: &Value) -> Value {
    json!({ "id": format!("model:{}", document["attempt_id"].as_str().expect("模型请求标识缺失")),
        "run_id": document["run_id"], "kind": if document["purpose"] == "compaction" { "compaction" } else { "model" },
        "step": document["step_number"], "attempt": 1, "title": document["model"], "preview": document["preview"],
        "status": document["status"], "started_at": document["started_at"], "ended_at": document["ended_at"] })
}

fn message_record(message: &Value, kind: &str, title: &str) -> Value {
    let preview = message["content"]
        .as_array()
        .into_iter()
        .flatten()
        .filter(|part| {
            kind == "user"
                || part["type"]
                    == if kind == "reasoning" {
                        "reasoning"
                    } else {
                        "text"
                    }
        })
        .filter_map(|part| part["text"].as_str())
        .collect::<Vec<_>>()
        .join("\n");
    json!({ "id": format!("{kind}:{}", message["id"].as_str().expect("消息标识缺失")), "run_id": message["run_id"],
        "kind": kind, "title": title, "preview": preview.chars().take(200).collect::<String>(), "status": "succeeded",
        "started_at": message["created_at"], "ended_at": message["created_at"] })
}

fn tool_record(row: &sqlx::sqlite::SqliteRow, tool: Option<&RunTool>, call: &ToolCall) -> Value {
    let status = row.get::<String, _>("status");
    json!({ "id": format!("tool:{}", row.get::<String, _>("id")), "run_id": row.get::<String, _>("run_id"),
        "kind": "tool", "step": row.get::<i64, _>("turn"), "title": tool.map(RunTool::display_name).unwrap_or(call.function.name.as_str()),
        "status": match status.as_str() { "prepared" | "approved" => "pending", "executing" | "waiting_mcp" => "running", status => status },
        "started_at": row.get::<Option<String>, _>("started_at").unwrap_or_else(|| row.get("created_at")),
        "ended_at": row.get::<Option<String>, _>("ended_at") })
}

impl Store {
    pub(crate) async fn trajectory_records(
        &self,
        account: &str,
        run_id: &str,
    ) -> Result<Vec<Value>> {
        let run = self.get_run(account, run_id).await?;
        let mut records = Vec::new();
        let requests: Vec<String> = sqlx::query_scalar("SELECT json_remove(document,'$.parts') FROM model_requests WHERE account_id=? AND run_id=? ORDER BY created_at,id")
            .bind(account).bind(run_id).fetch_all(&self.pool).await?;
        for request in requests {
            records.push(model_record(&serde_json::from_str::<Value>(&request)?));
        }
        let tools = self.run_tools_for_history(account, run_id).await?;
        let calls = sqlx::query(
            "SELECT * FROM tool_calls WHERE account_id=? AND run_id=? ORDER BY created_at,id",
        )
        .bind(account)
        .bind(run_id)
        .fetch_all(&self.pool)
        .await?;
        for row in calls {
            let call: ToolCall = serde_json::from_str(row.get("call"))?;
            records.push(tool_record(
                &row,
                tools
                    .iter()
                    .find(|tool| tool.definition.name == call.function.name),
                &call,
            ));
        }
        let messages: Vec<String> = sqlx::query_scalar("SELECT document FROM messages WHERE account_id=? AND session_id=? AND json_extract(document,'$.run_id')=? ORDER BY created_at,id")
            .bind(account).bind(text(&run, "session_id")?).bind(run_id).fetch_all(&self.pool).await?;
        for message in messages {
            let message: Value = serde_json::from_str(&message)?;
            if message["role"] == "user" {
                records.push(message_record(&message, "user", "用户消息"));
            }
            if message["role"] == "assistant" {
                if message["content"]
                    .as_array()
                    .is_some_and(|parts| parts.iter().any(|part| part["type"] == "text"))
                {
                    records.push(message_record(&message, "assistant", "助手回复"));
                }
                if message["content"]
                    .as_array()
                    .is_some_and(|parts| parts.iter().any(|part| part["type"] == "reasoning"))
                {
                    records.push(message_record(&message, "reasoning", "模型思考"));
                }
            }
        }
        records.sort_by(|left, right| {
            left["started_at"]
                .as_str()
                .cmp(&right["started_at"].as_str())
                .then_with(|| left["id"].as_str().cmp(&right["id"].as_str()))
        });
        Ok(records)
    }

    async fn run_tools_for_history(&self, account: &str, id: &str) -> Result<Vec<RunTool>> {
        let document: Option<String> =
            sqlx::query_scalar("SELECT tool_snapshot FROM runs WHERE account_id=? AND id=?")
                .bind(account)
                .bind(id)
                .fetch_one(&self.pool)
                .await?;
        document
            .map(|document| serde_json::from_str(&document).map_err(Error::from))
            .transpose()
            .map(Option::unwrap_or_default)
    }

    pub(crate) async fn trajectory(
        &self,
        account: &str,
        session: &str,
        before: Option<&str>,
    ) -> Result<Value> {
        self.get_session(account, session).await?;
        let cursor = if let Some(before) = before.filter(|before| !before.is_empty()) {
            let run = self.get_run(account, before).await?;
            if run["session_id"] != session {
                return Err(Error::NotFound);
            }
            Some((text(&run, "created_at")?.to_owned(), before.to_owned()))
        } else {
            None
        };
        let rows: Vec<String> = sqlx::query_scalar("SELECT document FROM runs WHERE account_id=? AND session_id=? AND (? IS NULL OR created_at<? OR (created_at=? AND id<?)) ORDER BY created_at DESC,id DESC LIMIT 21")
            .bind(account).bind(session).bind(cursor.as_ref().map(|cursor| &cursor.0)).bind(cursor.as_ref().map(|cursor| &cursor.0))
            .bind(cursor.as_ref().map(|cursor| &cursor.0)).bind(cursor.as_ref().map(|cursor| &cursor.1)).fetch_all(&self.pool).await?;
        let mut runs = Vec::new();
        for row in rows.iter().take(20) {
            let run: Value = serde_json::from_str(row)?;
            let records = self.trajectory_records(account, text(&run, "id")?).await?;
            runs.push(json!({ "run": run, "records": records }));
        }
        let count: i64 =
            sqlx::query_scalar("SELECT count(*) FROM runs WHERE account_id=? AND session_id=?")
                .bind(account)
                .bind(session)
                .fetch_one(&self.pool)
                .await?;
        Ok(
            json!({ "has_more": rows.len() > 20, "next_cursor": runs.last().map(|run| run["run"]["id"].clone()).unwrap_or_else(|| json!("")), "total_runs": count, "runs": runs }),
        )
    }

    pub(crate) async fn trajectory_detail(
        &self,
        account: &str,
        session: &str,
        run_id: &str,
        id: &str,
    ) -> Result<Value> {
        self.get_session(account, session).await?;
        let run = self.get_run(account, run_id).await?;
        if run["session_id"] != session {
            return Err(Error::NotFound);
        }
        if let Some(attempt) = id.strip_prefix("model:") {
            let row = sqlx::query("SELECT document,request,response FROM model_requests WHERE account_id=? AND session_id=? AND run_id=? AND id=?")
                .bind(account).bind(session).bind(run_id).bind(attempt).fetch_optional(&self.pool).await?.ok_or(Error::NotFound)?;
            let document: Value = serde_json::from_str(row.get("document"))?;
            let input: Value = serde_json::from_str(row.get("request"))?;
            let output: Value = row
                .get::<Option<String>, _>("response")
                .map(|response| serde_json::from_str(&response))
                .transpose()?
                .unwrap_or(Value::Null);
            let timing = json!({ "started_at": document["started_at"], "first_token_at": document["first_token_at"], "ended_at": document["ended_at"],
                "duration_ms": document["ended_at"].as_str().map(|end| elapsed_ms(text(&document, "started_at")?, end)).transpose()? });
            return Ok(
                json!({ "record": model_record(&document), "overview": { "model": document["model"], "protocol": document["protocol"], "status": document["status"], "projected_tokens": document["projected_tokens"] },
                "input": input, "output": output, "raw": { "request": input, "response": output }, "schema": input["output_schema"], "usage": output["usage"], "timing": timing }),
            );
        }
        if let Some(call_id) = id.strip_prefix("tool:") {
            let row =
                sqlx::query("SELECT * FROM tool_calls WHERE account_id=? AND run_id=? AND id=?")
                    .bind(account)
                    .bind(run_id)
                    .bind(call_id)
                    .fetch_optional(&self.pool)
                    .await?
                    .ok_or(Error::NotFound)?;
            let call: ToolCall = serde_json::from_str(row.get("call"))?;
            let tools = self.run_tools_for_history(account, run_id).await?;
            let tool = tools
                .iter()
                .find(|tool| tool.definition.name == call.function.name)
                .ok_or(Error::State("工具轨迹缺少定义"))?;
            let result: Value = row
                .get::<Option<String>, _>("result")
                .map(|result| serde_json::from_str(&result))
                .transpose()?
                .unwrap_or(Value::Null);
            let raw_result: Value = row
                .get::<Option<String>, _>("raw_result")
                .map(|result| serde_json::from_str(&result))
                .transpose()?
                .unwrap_or(Value::Null);
            let mcp_execution: Option<String> = sqlx::query_scalar(
                "SELECT document FROM mcp_executions WHERE account_id=? AND run_id=? AND id=?",
            )
            .bind(account)
            .bind(run_id)
            .bind(call_id)
            .fetch_optional(&self.pool)
            .await?;
            let mcp_execution = mcp_execution
                .map(|document| serde_json::from_str::<Value>(&document))
                .transpose()?;
            let edited: Option<Value> = row
                .get::<Option<String>, _>("approved_arguments")
                .map(|arguments| serde_json::from_str(&arguments))
                .transpose()?;
            let input = edited.unwrap_or_else(|| Value::Object(call.function.arguments.clone()));
            return Ok(
                json!({ "record": tool_record(&row, Some(tool), &call), "overview": { "tool_name": tool.display_name(), "status": row.get::<String, _>("status"), "provider_call_id": call.id },
                "input": input, "output": result, "raw": { "call": call, "executed_arguments": input, "result": result, "response": raw_result, "mcp_execution": mcp_execution }, "schema": tool.definition.parameters,
                "timing": { "started_at": row.get::<Option<String>, _>("started_at"), "ended_at": row.get::<Option<String>, _>("ended_at") } }),
            );
        }
        for kind in ["user", "assistant", "reasoning"] {
            if let Some(message_id) = id.strip_prefix(&format!("{kind}:")) {
                let document: String = sqlx::query_scalar("SELECT document FROM messages WHERE account_id=? AND session_id=? AND id=? AND json_extract(document,'$.run_id')=?")
                    .bind(account).bind(session).bind(message_id).bind(run_id).fetch_optional(&self.pool).await?.ok_or(Error::NotFound)?;
                let message: Value = serde_json::from_str(&document)?;
                if message["role"] != if kind == "user" { "user" } else { "assistant" } {
                    return Err(Error::NotFound);
                }
                let title = match kind {
                    "user" => "用户消息",
                    "reasoning" => "模型思考",
                    _ => "助手回复",
                };
                return Ok(
                    json!({ "record": message_record(&message, kind, title), "overview": { "role": message["role"] },
                    "input": if kind == "user" { message.clone() } else { Value::Null }, "output": if kind == "user" { Value::Null } else { message.clone() }, "raw": message }),
                );
            }
        }
        Err(Error::NotFound)
    }
}
