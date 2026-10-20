use crate::{
    Error, Result, Store, assets::digest, error::timestamp, local_api::text, settings::ModelInput,
};
use base64::{Engine, engine::general_purpose::STANDARD};
use rig::{
    completion::{CompletionRequest, CompletionResponse},
    message::{
        AssistantContent, DocumentSourceKind, Image, ImageDetail, Message, ToolResultContent,
        UserContent,
    },
};
use serde_json::{Value, json};
use sqlx::Row;
use uuid::Uuid;

pub(crate) fn estimated_tokens(content: &str) -> usize {
    tiktoken_rs::cl100k_base_singleton()
        .encode_with_special_tokens(content)
        .len()
}

fn context_part(category: &str, source: &str, label: &str, content: String, index: usize) -> Value {
    json!({ "id": format!("{category}:{index}:{}", &digest(content.as_bytes())[..16]),
        "category": category, "source": source, "label": label,
        "estimated_tokens": estimated_tokens(&content), "content": content })
}

fn image_tokens(width: u32, height: u32, image: &Image, model: &str) -> (usize, &'static str) {
    let model = model.to_ascii_lowercase();
    let patches = u64::from(width).div_ceil(32) * u64::from(height).div_ceil(32);
    if model.contains("deepseek") && (model.contains("v4") || model.contains("flash")) {
        return (1024, "deepseek_image_upper_bound");
    }
    if model.contains("claude") {
        let patches = u64::from(width).div_ceil(28) * u64::from(height).div_ceil(28);
        return (patches.min(4784) as usize, "claude_image_upper_bound");
    }
    let low = matches!(image.detail, Some(ImageDetail::Low));
    let tile_cost = if model.starts_with("gpt-4o-mini") {
        Some((2833, 5667))
    } else if model.starts_with("gpt-4o") || model == "gpt-4.1" || model.starts_with("gpt-4.1-20") {
        Some((85, 170))
    } else if model.starts_with("gpt-5.1") || model == "gpt-5" || model.starts_with("gpt-5-20") {
        Some((70, 140))
    } else if model.starts_with("o1") || model == "o3" || model.starts_with("o3-20") {
        Some((75, 150))
    } else {
        None
    };
    if let Some((base, tile)) = tile_cost {
        if low {
            return (base, "openai_image_tiles");
        }
        let width = f64::from(width);
        let height = f64::from(height);
        let scale = (2048.0 / width.max(height)).min(1.0);
        let width = (width * scale).floor().max(1.0);
        let height = (height * scale).floor().max(1.0);
        let scale = (768.0 / width.min(height)).min(1.0);
        let tiles =
            ((width * scale).floor() / 512.0).ceil() * ((height * scale).floor() / 512.0).ceil();
        return (base + tile * tiles as usize, "openai_image_tiles");
    }
    let (budget, multiplier) = if model.starts_with("gpt-6") || model.starts_with("gpt-5.6") {
        (
            if low {
                256
            } else if matches!(image.detail, Some(ImageDetail::High)) {
                2500
            } else {
                u64::MAX
            },
            1.2,
        )
    } else if model.starts_with("gpt-5.5") {
        (
            if low {
                256
            } else if matches!(image.detail, Some(ImageDetail::High)) {
                2500
            } else {
                10000
            },
            1.2,
        )
    } else if model.starts_with("gpt-5.4") {
        (if low { 6144 } else { 2500 }, 1.2)
    } else if model.starts_with("gpt-5.2") {
        (6144, 1.2)
    } else if model.starts_with("gpt-4.1-mini") {
        (6144, 1.62)
    } else if model.starts_with("gpt-4.1-nano") {
        (1536, 2.46)
    } else if model.starts_with("gpt-5-mini") {
        (1536, 1.2)
    } else if model.starts_with("gpt-5-nano") {
        (1536, 1.5)
    } else if model.starts_with("o4-mini") {
        (1536, 1.72)
    } else {
        // 未识别模型按原始尺寸估算，不把此估算作为服务计费数据。
        return (
            (patches as f64 * 2.46).ceil() as usize,
            "image_32px_patch_estimate",
        );
    };
    (
        (patches.min(budget) as f64 * multiplier).ceil() as usize,
        "openai_image_patch_upper_bound",
    )
}

fn push_image_part(parts: &mut Vec<Value>, image: &Image, model: &str, source: &str) -> Result<()> {
    let bytes = match &image.data {
        DocumentSourceKind::Base64(encoded) => STANDARD
            .decode(encoded)
            .map_err(|_| Error::Invalid("图片编码无效"))?,
        DocumentSourceKind::Raw(bytes) => bytes.clone(),
        _ => return Err(Error::Invalid("图片用量估算需要本地图片内容")),
    };
    let (width, height) = image::ImageReader::new(std::io::Cursor::new(&bytes))
        .with_guessed_format()?
        .into_dimensions()
        .map_err(|_| Error::Invalid("图片尺寸无法读取"))?;
    if width == 0 || height == 0 {
        return Err(Error::Invalid("图片尺寸无效"));
    }
    let (tokens, estimator) = image_tokens(width, height, image, model);
    let metadata = json!({ "width": width, "height": height, "bytes": bytes.len(),
        "digest": digest(&bytes), "media_type": image.media_type, "detail": image.detail });
    let mut part = context_part(
        "injection",
        source,
        "图片资料",
        metadata.to_string(),
        parts.len(),
    );
    part["estimated_tokens"] = json!(tokens);
    part["token_estimator"] = json!(estimator);
    part["image"] = metadata;
    parts.push(part);
    Ok(())
}

fn request_parts(request: &CompletionRequest, model: &str) -> Result<Vec<Value>> {
    let mut parts = Vec::new();
    for (index, message) in request.chat_history.iter().enumerate() {
        let mut message = message.clone();
        match &mut message {
            Message::Assistant(assistant) => {
                for (block, content) in assistant.content.iter().enumerate() {
                    if let AssistantContent::Image(image) = content {
                        push_image_part(
                            &mut parts,
                            image,
                            model,
                            &format!("message:{index}:{block}"),
                        )?;
                    }
                }
                assistant
                    .content
                    .retain(|content| !matches!(content, AssistantContent::Image(_)));
            }
            Message::User { content } => {
                for (block, content) in content.iter_mut().enumerate() {
                    match content {
                        UserContent::Image(image) => push_image_part(
                            &mut parts,
                            image,
                            model,
                            &format!("message:{index}:{block}"),
                        )?,
                        UserContent::ToolResult(result) => {
                            for (item, content) in result.content.iter().enumerate() {
                                if let ToolResultContent::Image(image) = content {
                                    push_image_part(
                                        &mut parts,
                                        image,
                                        model,
                                        &format!("message:{index}:{block}:{item}"),
                                    )?;
                                }
                            }
                            result
                                .content
                                .retain(|content| !matches!(content, ToolResultContent::Image(_)));
                        }
                        _ => {}
                    }
                }
                content.retain(|content| !matches!(content, UserContent::Image(_)));
            }
            Message::System { .. } => {}
        }
        match &message {
            Message::System { content } => parts.push(context_part(
                "system_prompt",
                "system",
                "系统提示词",
                content.clone(),
                index,
            )),
            Message::Assistant(assistant) => parts.push(context_part(
                "assistant_message",
                "history",
                "助手消息",
                serde_json::to_string(assistant)?,
                index,
            )),
            Message::User { content } => {
                for (block, content) in content.iter().enumerate() {
                    let (category, label, value) = match content {
                        UserContent::Text(text) if block == 0 => {
                            ("user_message", "用户消息", text.text.clone())
                        }
                        UserContent::Text(text) => ("injection", "参考资料", text.text.clone()),
                        UserContent::ToolResult(_) => {
                            ("tool_result", "工具结果", serde_json::to_string(content)?)
                        }
                        _ => ("injection", "媒体资料", serde_json::to_string(content)?),
                    };
                    parts.push(context_part(
                        category,
                        &format!("message:{index}:{block}"),
                        label,
                        value,
                        parts.len(),
                    ));
                }
            }
        }
    }
    for tool in &request.tools {
        parts.push(context_part(
            "tool_definition",
            tool.name.as_str(),
            &format!("工具 {}", tool.name),
            serde_json::to_string(tool)?,
            parts.len(),
        ));
    }
    for document in &request.documents {
        parts.push(context_part(
            "injection",
            &document.id,
            "参考资料",
            document.to_string(),
            parts.len(),
        ));
    }
    Ok(parts)
}

pub(crate) fn projected_tokens(request: &CompletionRequest, model: &str) -> Result<usize> {
    let content = request_parts(request, model)?
        .iter()
        .map(|part| part["estimated_tokens"].as_u64().expect("上下文估算缺失") as usize)
        .sum::<usize>();
    Ok(content + 128 + request.chat_history.len() * 12 + request.tools.len() * 24)
}

pub(crate) fn elapsed_ms(start: &str, end: &str) -> Result<u64> {
    let start = time::OffsetDateTime::parse(start, &time::format_description::well_known::Rfc3339)
        .map_err(|_| Error::State("运行开始时间无效"))?;
    let end = time::OffsetDateTime::parse(end, &time::format_description::well_known::Rfc3339)
        .map_err(|_| Error::State("运行结束时间无效"))?;
    u64::try_from((end - start).whole_milliseconds().max(0))
        .map_err(|_| Error::State("运行耗时超过范围"))
}

impl Store {
    pub(crate) async fn begin_model_request(
        &self,
        account: &str,
        run_id: &str,
        turn: usize,
        model: &ModelInput,
        request: &CompletionRequest,
        purpose: &str,
    ) -> Result<String> {
        self.check_run_model_enabled(account, run_id).await?;
        let run = self.get_run(account, run_id).await?;
        let id = Uuid::new_v4().to_string();
        let parts = request_parts(request, &model.model)?;
        let projected = projected_tokens(request, &model.model)?;
        let turn_number: i64 = sqlx::query_scalar("SELECT count(*) FROM runs WHERE account_id=? AND session_id=? AND (created_at<? OR (created_at=? AND id<=?))")
            .bind(account).bind(text(&run, "session_id")?).bind(text(&run, "created_at")?).bind(text(&run, "created_at")?).bind(run_id).fetch_one(&self.pool).await?;
        let preview = request
            .chat_history
            .last()
            .map(serde_json::to_string)
            .transpose()?
            .unwrap_or_default();
        let now = timestamp()?;
        let document = json!({
            "attempt_id": id, "session_id": run["session_id"], "run_id": run_id, "turn_id": run_id,
            "turn_number": turn_number, "step_number": turn, "purpose": purpose, "protocol": model.protocol, "model": model.model,
            "preview": preview.chars().take(200).collect::<String>(), "status": "running", "started_at": now,
            "context_window_tokens": model.limits.context_window_tokens, "max_input_tokens": model.limits.max_input_tokens,
            "max_output_tokens": request.max_tokens.unwrap_or(model.limits.max_output_tokens), "input_tokens": null, "output_tokens": null,
            "cache_read_tokens": null, "cache_write_tokens": null, "reasoning_tokens": null,
            "projected_tokens": projected, "token_estimator": "cl100k_base+image_budget_v1", "parts": parts, "content_available": true,
        });
        sqlx::query("INSERT INTO model_requests(id,account_id,session_id,run_id,created_at,document,request) VALUES(?,?,?,?,?,?,?)")
            .bind(&id).bind(account).bind(text(&run, "session_id")?).bind(run_id).bind(&now).bind(document.to_string())
            .bind(serde_json::to_string(request)?).execute(&self.pool).await?;
        Ok(id)
    }

    pub(crate) async fn mark_model_first_token(&self, account: &str, id: &str) -> Result<()> {
        sqlx::query("UPDATE model_requests SET document=json_set(document,'$.first_token_at',?) WHERE account_id=? AND id=? AND json_extract(document,'$.status')='running' AND json_extract(document,'$.first_token_at') IS NULL")
            .bind(timestamp()?).bind(account).bind(id).execute(&self.pool).await?;
        Ok(())
    }

    pub(crate) async fn finish_model_request(
        &self,
        account: &str,
        id: &str,
        response: &CompletionResponse,
    ) -> Result<()> {
        let now = timestamp()?;
        let usage = &response.usage;
        let status = if matches!(
            response.stop(),
            rig::message::StopReason::Length
                | rig::message::StopReason::Error(_)
                | rig::message::StopReason::Aborted(_)
        ) {
            "failed"
        } else {
            "succeeded"
        };
        let output = json!({ "choice": response.choice, "usage": usage, "finish_reason": response.finish_reason() });
        sqlx::query("UPDATE model_requests SET document=json_set(document,'$.status',?,'$.ended_at',?,'$.input_tokens',?,'$.output_tokens',?,'$.cache_read_tokens',?,'$.cache_write_tokens',?,'$.reasoning_tokens',?),response=? WHERE account_id=? AND id=? AND json_extract(document,'$.status')='running'")
            .bind(status).bind(now).bind(usage.input_tokens.map(|tokens| tokens as i64)).bind(usage.output_tokens.map(|tokens| tokens as i64))
            .bind(usage.cached_input_tokens.map(|tokens| tokens as i64)).bind(usage.cache_creation_input_tokens.map(|tokens| tokens as i64))
            .bind(usage.reasoning_tokens.map(|tokens| tokens as i64)).bind(output.to_string()).bind(account).bind(id).execute(&self.pool).await?;
        Ok(())
    }

    pub(crate) async fn context_request(
        &self,
        account: &str,
        session: &str,
        id: Option<&str>,
    ) -> Result<Value> {
        self.get_session(account, session).await?;
        let document: Option<String> = sqlx::query_scalar("SELECT document FROM model_requests WHERE account_id=? AND session_id=? AND (? IS NULL OR id=?) ORDER BY created_at DESC,id DESC LIMIT 1")
            .bind(account).bind(session).bind(id).bind(id).fetch_optional(&self.pool).await?;
        Ok(serde_json::from_str(&document.ok_or(Error::NotFound)?)?)
    }

    pub(crate) async fn context_requests(
        &self,
        account: &str,
        session: &str,
        offset: i64,
    ) -> Result<Value> {
        self.get_session(account, session).await?;
        if offset < 0 {
            return Err(Error::Invalid("分页参数无效"));
        }
        let rows: Vec<String> = sqlx::query_scalar("SELECT document FROM model_requests WHERE account_id=? AND session_id=? ORDER BY created_at DESC,id DESC LIMIT 51 OFFSET ?")
            .bind(account).bind(session).bind(offset).fetch_all(&self.pool).await?;
        let requests: Vec<Value> = rows
            .iter()
            .take(50)
            .map(|document| serde_json::from_str(document).map_err(Error::from))
            .collect::<Result<_>>()?;
        Ok(
            json!({ "has_more": rows.len() > 50, "next_offset": offset + requests.len() as i64, "requests": requests }),
        )
    }

    pub(crate) async fn context_events(
        &self,
        account: &str,
        session: &str,
        offset: i64,
        kind: &str,
    ) -> Result<Value> {
        self.get_session(account, session).await?;
        if offset < 0 {
            return Err(Error::Invalid("分页参数无效"));
        }
        let rows: Vec<String> = sqlx::query_scalar("SELECT document FROM context_events WHERE account_id=? AND session_id=? AND (?='' OR json_extract(document,'$.kind')=?) ORDER BY created_at DESC,id DESC LIMIT 51 OFFSET ?")
            .bind(account).bind(session).bind(kind).bind(kind).bind(offset).fetch_all(&self.pool).await?;
        let events: Vec<Value> = rows
            .iter()
            .take(50)
            .map(|document| serde_json::from_str(document).map_err(Error::from))
            .collect::<Result<_>>()?;
        Ok(
            json!({ "has_more": rows.len() > 50, "next_offset": offset + events.len() as i64, "events": events }),
        )
    }

    pub(crate) async fn context_overview(&self, account: &str, session: &str) -> Result<Value> {
        self.get_session(account, session).await?;
        let rows = sqlx::query("SELECT run_id,document FROM model_requests WHERE account_id=? AND session_id=? ORDER BY created_at,id")
            .bind(account).bind(session).fetch_all(&self.pool).await?;
        let mut turns = std::collections::HashSet::new();
        let mut tokens = json!({ "input": 0, "output": 0, "cache_read": 0, "cache_write": 0, "uncached": 0, "unclassified_input": 0, "reasoning": 0, "missing_requests": 0, "cache_known_requests": 0, "cache_known_input": 0 });
        let mut model_wait = 0;
        let mut generation = 0;
        let mut current = Value::Null;
        let now = timestamp()?;
        for row in &rows {
            turns.insert(row.get::<String, _>("run_id"));
            let document: Value = serde_json::from_str(row.get("document"))?;
            for (target, source) in [
                ("input", "input_tokens"),
                ("output", "output_tokens"),
                ("cache_read", "cache_read_tokens"),
                ("cache_write", "cache_write_tokens"),
                ("reasoning", "reasoning_tokens"),
            ] {
                if let Some(amount) = document[source].as_u64() {
                    tokens[target] = json!(
                        tokens[target]
                            .as_u64()
                            .ok_or(Error::State("上下文用量无效"))?
                            + amount
                    );
                }
            }
            if document["input_tokens"].is_null() || document["output_tokens"].is_null() {
                tokens["missing_requests"] = json!(
                    tokens["missing_requests"]
                        .as_u64()
                        .ok_or(Error::State("上下文用量无效"))?
                        + 1
                );
            }
            if let Some(input) = document["input_tokens"].as_u64() {
                if document["cache_read_tokens"].is_number()
                    || document["cache_write_tokens"].is_number()
                {
                    tokens["cache_known_requests"] = json!(
                        tokens["cache_known_requests"]
                            .as_u64()
                            .ok_or(Error::State("上下文用量无效"))?
                            + 1
                    );
                    tokens["cache_known_input"] = json!(
                        tokens["cache_known_input"]
                            .as_u64()
                            .ok_or(Error::State("上下文用量无效"))?
                            + input
                    );
                    tokens["uncached"] = json!(
                        tokens["uncached"]
                            .as_u64()
                            .ok_or(Error::State("上下文用量无效"))?
                            + input.saturating_sub(
                                document["cache_read_tokens"].as_u64().unwrap_or(0)
                                    + document["cache_write_tokens"].as_u64().unwrap_or(0)
                            )
                    );
                } else {
                    tokens["unclassified_input"] = json!(
                        tokens["unclassified_input"]
                            .as_u64()
                            .ok_or(Error::State("上下文用量无效"))?
                            + input
                    );
                }
            }
            let start = text(&document, "started_at")?;
            let end = document["ended_at"].as_str().unwrap_or(&now);
            if let Some(first) = document["first_token_at"].as_str() {
                model_wait += elapsed_ms(start, first)?;
                generation += elapsed_ms(first, end)?;
            } else {
                model_wait += elapsed_ms(start, end)?;
            }
            current = document;
        }
        let tools = sqlx::query("SELECT t.started_at,t.ended_at FROM tool_calls t JOIN runs r ON r.id=t.run_id WHERE t.account_id=? AND r.session_id=?")
            .bind(account).bind(session).fetch_all(&self.pool).await?;
        let mut tools_ms = 0;
        for tool in &tools {
            if let Some(start) = tool.get::<Option<String>, _>("started_at") {
                tools_ms += elapsed_ms(
                    &start,
                    tool.get::<Option<String>, _>("ended_at")
                        .as_deref()
                        .unwrap_or(&now),
                )?;
            }
        }
        let event_counts = sqlx::query("SELECT json_extract(document,'$.kind') AS kind,count(*) AS amount FROM context_events WHERE account_id=? AND session_id=? GROUP BY kind")
            .bind(account).bind(session).fetch_all(&self.pool).await?;
        let counts: std::collections::HashMap<String, i64> = event_counts
            .into_iter()
            .map(|row| (row.get("kind"), row.get("amount")))
            .collect();
        Ok(
            json!({ "turns": turns.len(), "steps": rows.len(), "tool_calls": tools.len(),
            "injections": counts.get("CONTEXT_INJECTED").copied().unwrap_or(0), "compactions": counts.get("CONTEXT_COMPACTED").copied().unwrap_or(0), "prunes": counts.get("CONTEXT_PRUNED").copied().unwrap_or(0),
            "tokens": tokens, "timing": { "active_ms": model_wait + generation + tools_ms, "model_wait_ms": model_wait, "generation_ms": generation, "model_other_ms": 0, "tools_ms": tools_ms, "overlap_ms": 0, "other_ms": 0 }, "current": current }),
        )
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn image_budget_counts_pixels_instead_of_encoded_bytes() -> Result<()> {
        use base64::{Engine, engine::general_purpose::STANDARD};
        use rig::message::ImageMediaType;
        let mut bytes = std::io::Cursor::new(Vec::new());
        image::RgbImage::new(1024, 1024)
            .write_to(&mut bytes, image::ImageFormat::Png)
            .map_err(|_| Error::State("图片写入失败"))?;
        let mut request = CompletionRequest::new("读取图片");
        request.chat_history.push(Message::User {
            content: vec![UserContent::image_base64(
                STANDARD.encode(bytes.get_ref()),
                Some(ImageMediaType::PNG),
                None,
            )],
        });
        let Message::User { content } = request.chat_history.last().unwrap() else {
            return Err(Error::State("图片消息缺失"));
        };
        let UserContent::Image(image) = &content[0] else {
            return Err(Error::State("图片内容缺失"));
        };
        let image = image.clone();
        let call = rig::message::ToolCall::from_wire(
            "image-result",
            rig::message::ToolFunction::new(
                rig::message::ToolName::new("read_image")
                    .map_err(|_| Error::State("工具名称无效"))?,
                json!({}),
            ),
        );
        request
            .chat_history
            .push(Message::Assistant(rig::message::AssistantMessage::new(
                vec![AssistantContent::Image(image.clone())],
            )));
        request.chat_history.push(Message::tool_results(vec![
            call.result(vec![ToolResultContent::Image(image)]),
        ]));
        let original = serde_json::to_value(&request)?;
        let parts = request_parts(&request, "gpt-6-astra")?;
        let part = parts
            .iter()
            .find(|part| part["label"] == "图片资料")
            .ok_or(Error::State("图片资料缺失"))?;
        assert_eq!(part["label"], "图片资料");
        assert_eq!(part["image"]["width"], 1024);
        assert_eq!(part["image"]["height"], 1024);
        assert!(
            !part["content"]
                .as_str()
                .unwrap()
                .contains(&STANDARD.encode(bytes.get_ref()))
        );
        assert!(part["estimated_tokens"].as_u64().unwrap() < 5000);
        assert_eq!(
            parts
                .iter()
                .filter(|part| part["label"] == "图片资料")
                .count(),
            3
        );
        assert_eq!(serde_json::to_value(&request)?, original);
        assert!(projected_tokens(&request, "gpt-6-astra")? < 5000);
        let encoded = serde_json::to_string(&parts)?;
        assert!(!encoded.contains(&STANDARD.encode(bytes.get_ref())));
        Ok(())
    }

    #[tokio::test]
    async fn disabled_models_cannot_start_another_agent_or_compaction_request()
    -> std::result::Result<(), Box<dyn std::error::Error>> {
        let root = std::path::PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../.local-cache");
        std::fs::create_dir_all(&root)?;
        let directory = tempfile::Builder::new()
            .prefix("request-model-state-")
            .tempdir_in(root)?;
        let store = Store::open(&directory.path().join("studio.sqlite")).await?;
        let account = "request-model-state";
        let mut configuration = json!({
            "name": "请求启停检查", "protocol": "openai_chat_compatible", "base_url": "http://127.0.0.1:1/v1", "model": "local-model",
            "enabled": true, "agent_enabled": true, "default": false,
            "limits": { "context_window_tokens": 32000, "max_input_tokens": 24000, "max_output_tokens": 4000 },
            "thinking": { "enabled": false }, "capabilities": { "tools": true, "vision": false, "image_output": false, "streaming": true }
        });
        let model = store
            .save_model(account, None, configuration.clone())
            .await?;
        let model_id = text(&model, "id")?;
        let session = store.create_session(account, "request").await?;
        let queued = store
            .queue_run(
                account,
                &session.id,
                "request",
                model_id,
                "检查启停",
                "auto_approve",
            )
            .await?;
        let run_id = text(&queued["run"], "id")?;
        assert!(store.claim_run(account, run_id).await?);
        let saved = store.run_model(account, run_id).await?;
        let request = CompletionRequest::new("检查启停");
        for field in ["enabled", "agent_enabled"] {
            configuration[field] = json!(false);
            store
                .save_model(account, Some(model_id), configuration.clone())
                .await?;
            for purpose in ["agent", "compaction"] {
                let error = store
                    .begin_model_request(account, run_id, 0, &saved, &request, purpose)
                    .await
                    .expect_err("停用模型不能建立新的请求");
                assert!(matches!(error, Error::Invalid("模型尚未启用 Agent 调用")));
            }
            configuration[field] = json!(true);
            store
                .save_model(account, Some(model_id), configuration.clone())
                .await?;
        }
        let count: i64 =
            sqlx::query_scalar("SELECT count(*) FROM model_requests WHERE account_id=?")
                .bind(account)
                .fetch_one(&store.pool)
                .await?;
        assert_eq!(count, 0);
        store.close().await;
        Ok(())
    }
}
