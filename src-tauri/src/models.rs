use crate::{Error, Result, Store, settings::ModelInput};
use rig::{
    DynModel,
    completion::{CompletionRequest, Effort, Reasoning},
    operation::Completion,
    providers::{anthropic::AnthropicConfig, openai::OpenAIConfig},
};
use serde_json::{Value, json};
use sqlx::Row;
use std::time::{Duration, Instant};

pub(crate) fn build_model(input: &ModelInput) -> Result<DynModel<Completion>> {
    let http = reqwest::Client::builder()
        .redirect(reqwest::redirect::Policy::none())
        .connect_timeout(Duration::from_secs(15))
        .timeout(Duration::from_secs(300))
        .build()?;
    let http = rig::http_client::ReqwestClient::from(http);
    let base_url = provider_base_url(&input.protocol, &input.base_url)?;
    Ok(match input.protocol.as_str() {
        "openai_responses" => OpenAIConfig::new(input.api_key.clone())
            .with_base_url(base_url)
            .connect(http)
            .responses(&input.model)
            .erase(),
        "openai_chat_compatible" => chat_configuration(input)?
            .connect(http)
            .chat(&input.model)
            .erase(),
        "anthropic_messages_compatible" => AnthropicConfig::new(input.api_key.clone())
            .with_base_url(base_url)
            .connect(http)
            .completion(&input.model)
            .erase(),
        _ => return Err(Error::Invalid("模型协议无效")),
    })
}

fn chat_configuration(input: &ModelInput) -> Result<OpenAIConfig> {
    let configuration = OpenAIConfig::new(input.api_key.clone());
    let configuration = if input.model.starts_with("deepseek-") {
        let mut dialect = rig::providers::openai::wire::DEEPSEEK;
        if input.capabilities.vision {
            dialect.name = "pixoma-deepseek-vision";
            dialect.quirks.rewrite = rig::providers::openai::wire::BodyRewrite::None;
        }
        configuration.with_dialect(&dialect)
    } else {
        configuration
    };
    Ok(configuration.with_base_url(provider_base_url(&input.protocol, &input.base_url)?))
}

pub(crate) fn provider_base_url(protocol: &str, address: &str) -> Result<String> {
    let mut address = url::Url::parse(address).map_err(|_| Error::Invalid("模型地址无效"))?;
    if protocol == "anthropic_messages_compatible" {
        return Ok(rig::providers::anthropic::wire::normalize_base_url(
            address.as_str(),
        ));
    }
    let suffix = match protocol {
        "openai_chat_compatible" => "/chat/completions",
        "openai_responses" => "/responses",
        _ => return Err(Error::Invalid("模型协议无效")),
    };
    let path = address.path().trim_end_matches('/');
    let path = path.strip_suffix(suffix).unwrap_or(path).to_owned();
    address.set_path(&path);
    Ok(address.as_str().trim_end_matches('/').to_owned())
}

pub(crate) fn completion_request(input: &ModelInput, prompt: &str) -> Result<CompletionRequest> {
    let mut request = CompletionRequest::new(prompt).max_tokens(input.limits.max_output_tokens);
    if input.thinking.enabled {
        request = request.reasoning(if input.protocol == "anthropic_messages_compatible" {
            Reasoning::Budget {
                tokens: u32::try_from(
                    input
                        .thinking
                        .budget_tokens
                        .ok_or(Error::Invalid("缺少模型思考预算"))?,
                )
                .map_err(|_| Error::Invalid("模型思考预算无效"))?,
            }
        } else {
            let effort = input.thinking.effort.as_deref().unwrap_or("medium");
            let effort = if input.protocol == "openai_chat_compatible"
                && input.model.starts_with("deepseek-")
            {
                match effort {
                    "minimal" => "low",
                    "medium" | "xhigh" => "high",
                    effort => effort,
                }
            } else {
                effort
            };
            match effort {
                "none" => Reasoning::Off,
                "minimal" => Reasoning::Effort(Effort::Minimal),
                "low" => Reasoning::Effort(Effort::Low),
                "medium" => Reasoning::Effort(Effort::Medium),
                "high" => Reasoning::Effort(Effort::High),
                "xhigh" => Reasoning::Effort(Effort::XHigh),
                "max" => Reasoning::Effort(Effort::Max),
                _ => return Err(Error::Invalid("模型思考强度无效")),
            }
        });
    } else {
        request = request.reasoning(Reasoning::Off);
    }
    let reasoning = request
        .options
        .reasoning
        .take()
        .ok_or(Error::State("模型思考设置缺失"))?;
    with_reasoning(input, request, reasoning)
}

pub(crate) fn with_reasoning(
    input: &ModelInput,
    mut request: CompletionRequest,
    reasoning: Reasoning,
) -> Result<CompletionRequest> {
    if input.protocol == "openai_chat_compatible"
        && input.model.starts_with("deepseek-")
        && input.capabilities.vision
    {
        let parameters = match reasoning {
            Reasoning::Off => json!({ "thinking": { "type": "disabled" } }),
            Reasoning::Effort(effort @ (Effort::Low | Effort::High | Effort::Max)) => json!({
                "thinking": { "type": "enabled" }, "reasoning_effort": effort.as_str(),
            }),
            _ => return Err(Error::Invalid("模型思考强度无效")),
        };
        request.options.reasoning = None;
        return Ok(request.additional_params(parameters));
    }
    Ok(request.reasoning(reasoning))
}

impl Store {
    pub(crate) async fn configured_model(&self, id: &str) -> Result<ModelInput> {
        let row = sqlx::query("SELECT document,api_key FROM model_configs WHERE id=?")
            .bind(id)
            .fetch_optional(&self.pool)
            .await?
            .ok_or(Error::NotFound)?;
        let mut input: ModelInput = serde_json::from_str(row.get("document"))?;
        input.api_key = row.get("api_key");
        input.validate()?;
        Ok(input)
    }

    pub(crate) async fn test_model(&self, id: &str) -> Result<Value> {
        self.probe_model(self.configured_model(id).await?).await
    }

    pub(crate) async fn test_model_input(&self, mut document: Value) -> Result<Value> {
        let fields = document
            .as_object_mut()
            .ok_or(Error::Invalid("模型配置无效"))?;
        let existing = fields.remove("existing_model_id");
        let mut input: ModelInput =
            serde_json::from_value(document).map_err(|_| Error::Invalid("模型配置字段无效"))?;
        input.validate()?;
        if let Some(existing) = existing {
            let saved = self
                .configured_model(existing.as_str().ok_or(Error::Invalid("模型标识无效"))?)
                .await?;
            if input.api_key.is_empty() {
                input.api_key = saved.api_key;
            }
        }
        self.probe_model(input).await
    }

    async fn probe_model(&self, input: ModelInput) -> Result<Value> {
        let model = build_model(&input)?;
        let start = Instant::now();
        let response = model
            .call(completion_request(&input, "请回复 OK。")?)
            .await?;
        if response.text().trim().is_empty() {
            return Err(Error::State("模型未返回正文"));
        }
        Ok(json!({ "success": true, "latency_ms": start.elapsed().as_millis() }))
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn vision_models_encode_actual_images_and_keep_thinking_settings() -> Result<()> {
        use base64::{Engine, engine::general_purpose::STANDARD};
        use rig::{
            message::{ImageMediaType, Message, UserContent},
            wire::{Mode, Wire},
        };
        let mut input: ModelInput = serde_json::from_value(json!({
            "name": "图片请求检查", "protocol": "openai_chat_compatible",
            "base_url": "https://ark.cn-beijing.volces.com/api/v3", "model": "deepseek-v4-1-flash-260910",
            "enabled": true, "agent_enabled": true, "default": true,
            "limits": { "context_window_tokens": 32000, "max_input_tokens": 24000, "max_output_tokens": 4000 },
            "thinking": { "enabled": false, "effort": "low" },
            "capabilities": { "tools": true, "vision": true, "image_output": false, "streaming": true }
        }))?;
        let mut image = std::io::Cursor::new(Vec::new());
        image::RgbImage::new(32, 32)
            .write_to(&mut image, image::ImageFormat::Png)
            .map_err(|_| Error::State("图片写入失败"))?;
        let encoded_image = STANDARD.encode(image.get_ref());
        for thinking in [false, true] {
            input.thinking.enabled = thinking;
            let mut request = completion_request(&input, "读取图片")?;
            request.chat_history.push(Message::User {
                content: vec![UserContent::image_base64(
                    encoded_image.clone(),
                    Some(ImageMediaType::PNG),
                    None,
                )],
            });
            let wire = chat_configuration(&input)?.chat(&input.model);
            request.chat_history = rig::completion::history::adapt(&request.chat_history, &wire);
            let encoded = wire
                .encode(request, Mode::Unary)
                .map_err(|_| Error::State("模型请求编码失败"))?;
            let rig::wire::Body::Bytes(body) = encoded.request.body() else {
                return Err(Error::State("模型请求正文无法读取"));
            };
            let body: Value = serde_json::from_slice(body)?;
            assert_eq!(
                body["thinking"]["type"],
                if thinking { "enabled" } else { "disabled" }
            );
            let messages = body["messages"]
                .as_array()
                .ok_or(Error::State("请求消息缺失"))?;
            assert!(
                messages.iter().any(
                    |message| message["content"].as_array().is_some_and(|parts| {
                        parts.iter().any(|part| {
                            part["image_url"]["url"]
                                == format!("data:image/png;base64,{encoded_image}")
                        })
                    })
                ),
                "请求必须包含原始图片内容"
            );
        }
        Ok(())
    }

    #[test]
    fn disabled_thinking_is_explicit_in_completion_requests() -> Result<()> {
        let input: ModelInput = serde_json::from_value(json!({
            "name": "思考配置检查", "protocol": "openai_chat_compatible",
            "base_url": "https://ark.cn-beijing.volces.com/api/v3/chat/completions", "model": "deepseek-v4-1-flash-260910",
            "enabled": true, "agent_enabled": true, "default": true,
            "limits": { "context_window_tokens": 32000, "max_input_tokens": 24000, "max_output_tokens": 12000 },
            "thinking": { "enabled": false },
            "capabilities": { "tools": true, "vision": false, "image_output": false, "streaming": false }
        }))?;
        assert_eq!(
            completion_request(&input, "检查思考设置")?
                .options
                .reasoning,
            Some(Reasoning::Off)
        );
        use rig::wire::{Mode, Wire};
        let wire = chat_configuration(&input)?.chat(&input.model);
        let encoded = wire
            .encode(completion_request(&input, "检查思考设置")?, Mode::Unary)
            .map_err(|_| Error::State("模型请求编码失败"))?;
        let rig::wire::Body::Bytes(body) = encoded.request.body() else {
            return Err(Error::State("模型请求正文无法读取"));
        };
        let body: Value = serde_json::from_slice(body)?;
        assert_eq!(body["thinking"]["type"], "disabled");
        Ok(())
    }

    #[test]
    fn endpoint_addresses_keep_provider_prefixes_without_repeating_operation_paths() -> Result<()> {
        for (protocol, address, expected) in [
            (
                "openai_chat_compatible",
                "https://example.com/api/v3/chat/completions",
                "https://example.com/api/v3",
            ),
            (
                "openai_responses",
                "https://example.com/proxy/v1/responses",
                "https://example.com/proxy/v1",
            ),
            (
                "anthropic_messages_compatible",
                "https://example.com/proxy/v1/messages",
                "https://example.com/proxy",
            ),
            (
                "openai_chat_compatible",
                "https://example.com/v1/",
                "https://example.com/v1",
            ),
            (
                "anthropic_messages_compatible",
                "https://example.com/proxy/v1",
                "https://example.com/proxy",
            ),
            (
                "openai_responses",
                "https://example.com",
                "https://example.com",
            ),
        ] {
            assert_eq!(provider_base_url(protocol, address)?, expected);
        }
        Ok(())
    }
}
