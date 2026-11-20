use crate::{Error, Result, Store, error::timestamp, local_api::text};
use rig::{
    completion::ToolDefinition,
    message::{ToolCall, ToolName, ToolResultContent, UserContent},
};
use serde::{Deserialize, Serialize};
use serde_json::{Value, json};
use sqlx::Row;

#[derive(Clone, Deserialize, Serialize)]
pub(crate) struct RunTool {
    pub definition: ToolDefinition,
    pub target: ToolTarget,
    pub requires_approval: bool,
    #[serde(default)]
    pub read_only: bool,
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::AgentRuntime;
    use std::path::PathBuf;

    #[tokio::test]
    async fn video_batch_tools_preserve_scope_dependencies_and_wait_cancellation() -> std::result::Result<(), Box<dyn std::error::Error>> {
        let tools = builtin_tools()?;
        let generate = tools.iter().find(|tool|tool.definition.name=="studio_generate_video_batch").ok_or("批量生成工具缺失")?;
        assert!(generate.requires_approval && !generate.read_only);
        let root = PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../.local-cache");
        std::fs::create_dir_all(&root)?;
        let directory = tempfile::Builder::new().prefix("video-batch-tools-").tempdir_in(root)?;
        let store = Store::open(&directory.path().join("studio.sqlite")).await?;
        let definition: Value = serde_json::from_str(include_str!("../../docs/video-generation/h3/tea-h3-workflow.json"))?;
        let root_workflow = store.create_workflow(json!({"name":"上游视频","category":"video","enabled":false,"draft":definition})).await?;
        let root_version = store.create_workflow_version(text(&root_workflow,"id")?,json!({"expected_revision":1,"version":"1.0.0"})).await?;
        let templates = crate::LocalApi::new(store.clone()).request(crate::LocalRequest {method:"GET".into(),path:"/api/v1/studio/generation-templates".into(),body:None}).await?;
        let child = templates.as_array().ok_or("工作流模板缺失")?.iter().find(|template|template["definition"]["execution"]["capability"]=="first_frame").ok_or("首帧模板缺失")?;
        let child_workflow = store.create_workflow(json!({"name":"下游视频","category":"video","enabled":false,"draft":child["definition"]})).await?;
        let child_version = store.create_workflow_version(text(&child_workflow,"id")?,json!({"expected_revision":1,"version":"1.0.0"})).await?;
        let session = store.create_session("batch-tool-owner").await?;
        let shots: Vec<Value> = [("source",&root_workflow,&root_version),("target",&child_workflow,&child_version)].into_iter().map(|(id,workflow,version)|json!({"id":id,"name":id,"script":"","storyboard":"茶园晨光","subtitle":"","references":[],"duration":0,"sourceIn":0,"inputs":{"prompt":"茶园晨光"},"workflowId":workflow["id"],"versionId":version["id"]})).collect();
        let work = store.write_video_project(&session.id,"batch-project",json!({"expected_version":0,"project":{"version":2,"name":"茶园短片","width":1280,"height":720,"fps":30,"shots":shots,"audio":[]}})).await?;
        let input = json!({"video_id":work["asset_id"],"expected_version":1,"shot_ids":["source","target"],"dependencies":[{"shot_id":"target","source_shot_id":"source","input_key":"first_frame","position":"last","fps":30}]});
        jsonschema::validator_for(&generate.definition.parameters)?.validate(&input).map_err(|error|error.to_string())?;
        let unrelated = store.create_session("unrelated-batch-tool").await?;
        assert!(store.call_builtin(&unrelated.id,"batch-tool-request",&generate.definition.name,&input).await.is_err());
        let batch = store.call_builtin(&session.id,"batch-tool-request",&generate.definition.name,&input).await?;
        assert_eq!(batch["generations"][1]["status"],"waiting_dependencies");
        assert_eq!(store.call_builtin(&session.id,"batch-tool-request",&generate.definition.name,&input).await?["id"],batch["id"]);
        let list = ToolName::new("studio_list_video_generation_batches")?;
        assert_eq!(store.call_builtin(&session.id,"list-batches",&list,&json!({"video_id":work["asset_id"]})).await?,json!([batch]));
        let target = json!({"video_id":work["asset_id"],"generation_id":batch["generations"][1]["id"]});
        let cancelled = store.call_builtin(&session.id,"cancel-wait",&ToolName::new("studio_cancel_video_dependency_wait")?,&target).await?;
        assert_eq!(cancelled["status"],"cancelled");
        store.resume_video_generations().await?;
        let records = store.list_video_generations(text(&work,"asset_id")?).await?;
        let child = records.as_array().ok_or("生成记录缺失")?.iter().find(|record|record["shot_id"]=="target").ok_or("下游记录缺失")?;
        assert_eq!(child["status"],"cancelled");
        assert!(child["execution_id"].is_null());
        store.close().await;
        Ok(())
    }

    #[tokio::test]
    async fn chat_exports_keep_work_history_subtitles_audio_and_original_retry() -> std::result::Result<(), Box<dyn std::error::Error>> {
        let root = PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../.local-cache");
        std::fs::create_dir_all(&root)?;
        let directory = tempfile::Builder::new().prefix("chat-video-export-").tempdir_in(root)?;
        let store = Store::open(&directory.path().join("studio.sqlite")).await?;
        let session = store.create_session("chat-export").await?;
        let video = store.create_asset_bytes(Some(&session.id),"背景.mp4",include_bytes!("../tests/fixtures/red.mp4"),"user","chat-export-video").await?;
        let audio_path = directory.path().join("配乐.wav");
        let mut audio = hound::WavWriter::create(&audio_path,hound::WavSpec {channels:1,sample_rate:48000,bits_per_sample:16,sample_format:hound::SampleFormat::Int})?;
        for sample in 0..48000 {audio.write_sample((f64::sin(sample as f64*440.0*std::f64::consts::TAU/48000.0)*8000.0) as i16)?;}
        audio.finalize()?;
        let audio = store.create_asset_bytes(Some(&session.id),"配乐.wav",&std::fs::read(audio_path)?,"user","chat-export-audio").await?;
        let project = json!({"version":2,"name":"清晨.mp4","width":640,"height":360,"fps":30,"shots":[{"id":"opening","name":"开场","script":"","storyboard":"清晨","references":[],"subtitle":"清晨","duration":1,"sourceIn":0,"inputs":{},"volume":0,"media":{"assetId":video.id,"versionId":video.versions[0].id,"url":video.versions[0].content_url,"duration":1}}],"audio":[{"id":"music","assetId":audio.id,"versionId":audio.versions[0].id,"url":audio.versions[0].content_url,"start":0,"duration":1,"sourceIn":0,"volume":0.4,"loop":false}]});
        let work = store.write_video_project(&session.id,"save-export-project",json!({"expected_version":0,"project":project})).await?;
        let video_id = text(&work,"asset_id")?;
        let tool = ToolName::new("studio_export_video")?;
        let exported = store.call_builtin(&session.id,"chat-export-request",&tool,&json!({})).await?;
        let history = store.list_video_work_exports(video_id).await?;
        assert_eq!(history.as_array().ok_or("导出历史缺失")?.len(),1);
        assert_eq!(history[0]["project"],project);
        assert_eq!(history[0]["sourceVersion"],1);
        assert_eq!(history[0]["result"]["assetId"],exported["id"]);
        assert_eq!(exported["name"],"清晨.mp4");
        let path = store.asset_path(text(&exported,"id")?,text(&exported["versions"][0],"id")?).await?;
        let frame_path = directory.path().join("export-frame.png");
        let frame = tokio::process::Command::new(crate::asset_analysis::media_binary("ffmpeg")?).args(["-nostdin","-v","error","-i"]).arg(&path).args(["-frames:v","1"]).arg(&frame_path).output().await?;
        assert!(frame.status.success(),"{}",String::from_utf8_lossy(&frame.stderr));
        assert!(image::open(frame_path)?.to_rgb8().pixels().any(|pixel|pixel.0.iter().all(|value|*value>200)),"导出画面没有字幕像素");
        let samples_path = directory.path().join("export-music.wav");
        let samples = tokio::process::Command::new(crate::asset_analysis::media_binary("ffmpeg")?).args(["-nostdin","-v","error","-i"]).arg(&path).args(["-map","0:a:0","-c:a","pcm_s16le"]).arg(&samples_path).output().await?;
        assert!(samples.status.success(),"{}",String::from_utf8_lossy(&samples.stderr));
        let mut samples = hound::WavReader::open(samples_path)?;
        assert!(samples.duration()>=48000);
        let count = samples.len();
        let square_sum = samples.samples::<i16>().try_fold(0.0,|sum,sample|sample.map(|sample|sum+(sample as f64/32768.0).powi(2)))?;
        let rms = (square_sum/count as f64).sqrt();
        assert!((0.03..0.09).contains(&rms),"导出音频音量无效：{rms}");
        let mut changed = project.clone(); changed["shots"][0]["subtitle"] = json!("新的字幕"); changed["audio"] = json!([]);
        store.write_video_project(&session.id,"edit-export-project",json!({"expected_version":1,"project":changed})).await?;
        assert_eq!(store.call_builtin(&session.id,"chat-export-request",&tool,&json!({})).await?["id"],exported["id"]);
        store.clear_sessions().await?;
        let reopened = store.open_video_work(video_id).await?;
        assert_eq!(store.call_builtin(text(&reopened,"main_session_id")?,"chat-export-request",&tool,&json!({})).await?["id"],exported["id"]);
        assert_eq!(store.read_video_work(video_id).await?["project"],changed);
        assert_eq!(store.list_video_work_exports(video_id).await?[0]["project"],project);
        store.close().await;
        Ok(())
    }

    #[tokio::test]
    async fn mcp_image_conversion_errors_preserve_the_received_response()
    -> std::result::Result<(), Box<dyn std::error::Error>> {
        use base64::{Engine, engine::general_purpose::STANDARD};
        let root = PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../.local-cache");
        std::fs::create_dir_all(&root)?;
        let directory = tempfile::Builder::new()
            .prefix("mcp-image-result-")
            .tempdir_in(root)?;
        let database = directory.path().join("studio.sqlite");
        let store = Store::open(&database).await?;
        let model = store.save_model(None, json!({
            "name": "图片结果检查", "protocol": "openai_chat_compatible", "base_url": "http://127.0.0.1:18082/v1", "model": "image-check",
            "enabled": true, "agent_enabled": true, "default": true,
            "limits": { "context_window_tokens": 32000, "max_input_tokens": 24000, "max_output_tokens": 4000 },
            "thinking": { "enabled": false }, "capabilities": { "tools": true, "vision": true, "image_output": false, "streaming": true }
        })).await?;
        let session = store.create_session("image-result").await?;
        let queued = store
            .queue_run(
                &session.id,
                "image-result",
                text(&model, "id")?,
                "读取图片",
                "auto_approve",
            )
            .await?;
        let run_id = text(&queued["run"], "id")?;
        assert!(store.claim_run(run_id).await?);
        let run = store.get_run(run_id).await?;
        let tool = RunTool {
            definition: ToolDefinition::new(
                ToolName::new("read_image")?,
                "读取图片",
                json!({ "type": "object" }),
            ),
            target: ToolTarget::Mcp {
                connector_id: "images".to_owned(),
                url: "http://127.0.0.1:18082/mcp".to_owned(),
                name: "read_image".to_owned(),
            },
            requires_approval: false,
            read_only: true,
        };
        let call = ToolCall::from_wire(
            "image-result",
            rig::message::ToolFunction::new(tool.definition.name.clone(), json!({})),
        );
        store.save_tool_part(run_id, 1, 0, &call).await?;
        let record = store.tool_call_records(run_id, 1).await?.remove(0);
        store
            .begin_tool_execution(run_id, &record, &tool, &call)
            .await?;
        let mut image = std::io::Cursor::new(Vec::new());
        image::RgbImage::new(32, 32).write_to(&mut image, image::ImageFormat::Png)?;
        let response = serde_json::to_value(rmcp::model::CallToolResult::success(vec![
            rmcp::model::ContentBlock::image(STANDARD.encode(image.get_ref()), "image/x-png"),
        ]))?;
        let error = store
            .finish_tool_execution(&run, &record, &tool, &call, Ok(response.clone()))
            .await
            .expect_err("不支持的图片 MIME 需要明确失败");
        assert!(matches!(error, Error::Invalid("MCP 图片格式不受支持")));
        store
            .finish_run(run_id, "failed", Some(&error))
            .await?;
        store.close().await;
        let store = Store::open(&database).await?;
        let row = sqlx::query(
            "SELECT status,result,raw_result FROM tool_calls WHERE run_id=?",
        )
        .bind(run_id)
        .fetch_one(&store.pool)
        .await?;
        assert_eq!(row.get::<String, _>("status"), "failed");
        assert_eq!(
            serde_json::from_str::<Value>(row.get("raw_result"))?,
            response
        );
        let UserContent::ToolResult(result) = serde_json::from_str(row.get("result"))? else {
            return Err("工具结果格式无效".into());
        };
        assert!(result.is_error);
        assert!(
            store
                .run_events_after(run_id, 0)
                .await?
                .iter()
                .any(|event| event["type"] == "TOOL_CALL_RESULT"
                    && event["payload"]["isError"] == true)
        );
        store.close().await;
        Ok(())
    }

    #[tokio::test]
    #[ignore = "需要 PIXOMA_TEST_MODEL_CONFIG 并连接真实模型服务"]
    async fn real_tool_result_remains_unknown_after_an_interrupted_result_save()
    -> std::result::Result<(), Box<dyn std::error::Error>> {
        let configuration: Value =
            serde_json::from_str(&std::env::var("PIXOMA_TEST_MODEL_CONFIG")?)?;
        let root = PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../.local-cache");
        std::fs::create_dir_all(&root)?;
        let directory = tempfile::Builder::new()
            .prefix("tool-recovery-")
            .tempdir_in(root)?;
        let database = directory.path().join("studio.sqlite");
        let mut store = Store::open(&database).await?;
        let model = store.save_model(None, configuration).await?;
        let model_id = text(&model, "id")?;
        let session = store.create_session("recovery").await?;
        let marker = uuid::Uuid::new_v4().to_string();
        let prompt = format!(
            "请调用 create_text_asset 创建名为执行恢复检查的 Markdown 文档，正文严格为 {marker}，只调用一次该工具。"
        );
        let queued = store
            .queue_run(
                &session.id,
                "recovery",
                model_id,
                &prompt,
                "request_approval",
            )
            .await?;
        let run_id = text(&queued["run"], "id")?;
        let outcome = AgentRuntime::new(store.clone())
            .execute(run_id)
            .await?;
        assert_eq!(
            outcome["status"], "waiting_approval",
            "{}",
            outcome["error_message"]
        );
        let turn = store.run_checkpoint(run_id).await?.turn();
        let records = store.tool_call_records(run_id, turn).await?;
        assert_eq!(records.len(), 1);
        let call: ToolCall = serde_json::from_value(records[0]["call"].clone())?;
        assert_eq!(call.function.name.as_str(), "create_text_asset");
        let tool = store
            .run_tools(run_id)
            .await?
            .into_iter()
            .find(|tool| tool.definition.name == call.function.name)
            .ok_or("工具定义缺失")?;
        let detail = store.session_detail(&session.id).await?;
        let approval = text(&detail["pending_approvals"][0], "id")?;
        store
            .resolve_tool_approval(approval, true, None)
            .await?;
        assert!(store.claim_run(run_id).await?);
        let record = &store.tool_call_records(run_id, turn).await?[0];
        let (run, executing_call) = store
            .begin_tool_execution(run_id, record, &tool, &call)
            .await?;
        let arguments = Value::Object(executing_call.function.arguments.clone());
        let content = text(&arguments, "content")?.as_bytes().to_vec();
        assert!(text(&arguments, "content")?.contains(&marker));
        store
            .call_builtin(
                text(&run, "session_id")?,
                text(record, "id")?,
                &tool.definition.name,
                &arguments,
            )
            .await?;
        assert!(
            store
                .execute_local_tool(run_id, record, &tool, &call)
                .await
                .is_err()
        );
        assert!(
            store
                .tool_display_name(run_id, &ToolName::new("unknown_tool")?)
                .await
                .is_err()
        );
        store.close().await;
        store = Store::open(&database).await?;
        assert_eq!(store.recover_interrupted_runs().await?, 1);
        let interrupted = store.get_run(run_id).await?;
        assert_eq!(interrupted["status"], "failed");
        assert_eq!(interrupted["error_code"], "tool_result_unknown");
        let recovered = store.tool_call_records(run_id, turn).await?;
        assert_eq!(recovered[0]["status"], "result_unknown");
        assert!(recovered[0]["result"].is_null());
        let outcome = AgentRuntime::new(store.clone())
            .execute(run_id)
            .await?;
        assert_eq!(outcome["status"], "failed");
        assert!(
            store
                .execute_local_tool(run_id, &recovered[0], &tool, &call)
                .await
                .is_err()
        );
        let assets = store.list_session_assets(&session.id).await?;
        assert_eq!(assets.len(), 1);
        assert_eq!(assets[0].versions.len(), 1);
        assert_eq!(
            store
                .read_asset(&assets[0].id, &assets[0].versions[0].id)
                .await?,
            content
        );
        assert_eq!(store.recover_interrupted_runs().await?, 0);
        let continued = store
            .queue_run(
                &session.id,
                "read-recovery-state",
                model_id,
                "只说明上一项工具调用的结果是否已经确认，保持原有文档。",
                "auto_approve",
            )
            .await?;
        let history = store
            .previous_run_history(text(&continued["run"], "id")?)
            .await?;
        rig::transcript::validate_canonical(&history)?;
        assert!(history.iter().any(|message| matches!(message, rig::message::Message::User { content } if content.iter().any(|content| matches!(content, UserContent::ToolResult(result) if result.is_error && result.content.iter().any(|content| matches!(content, ToolResultContent::Text(text) if text.text.contains("工具结果未知"))))))));
        assert_eq!(store.list_session_assets(&session.id).await?.len(), 1);
        store.close().await;
        Ok(())
    }
}

#[derive(Clone, Deserialize, Serialize)]
#[serde(tag = "kind", rename_all = "snake_case")]
pub(crate) enum ToolTarget {
    Builtin,
    Expert,
    Mcp {
        connector_id: String,
        url: String,
        name: String,
    },
}

impl RunTool {
    pub fn display_name(&self) -> &str {
        match &self.target {
            ToolTarget::Builtin | ToolTarget::Expert => self.definition.name.as_str(),
            ToolTarget::Mcp { name, .. } => name,
        }
    }
}

fn builtin_tools() -> Result<Vec<RunTool>> {
    let string = json!({ "type": "string", "minLength": 1 });
    [
        ("studio_read_video_project", "读取当前会话视频项目与asset_version，包含分镜、工作流版本、生成参数和采用视频。修改前调用此工具。", json!({"type":"object","properties":{},"additionalProperties":false})),
        ("studio_propose_video_script", "保存完整脚本提案，供用户在编辑器中查看和采用。blocks为完整有序文本块，已有文本块沿用id，新文本块提供新id；每项包含id和text。分开或合并原文时，新文本块通过sourceBlockIds指定本次请求的来源文本块，原来源块从blocks移除；服务保存完整来源关系。expected_version使用最新asset_version。不会修改当前脚本、分镜或视频。", json!({"type":"object","properties":{"expected_version":{"type":"integer","minimum":0},"blocks":{"type":"array","items":{"type":"object","properties":{"id":string,"text":{"type":"string"},"sourceBlockIds":{"type":"array","minItems":1,"items":string,"uniqueItems":true}},"required":["id","text"],"additionalProperties":false}}},"required":["expected_version","blocks"],"additionalProperties":false})),
        ("studio_propose_video_storyboard", "根据完整脚本保存分镜提案，供用户逐项采用。已有分镜使用action=update，新增使用action=add与新shotId；删除建议使用action=remove，服务保存原分镜供采用时检查，尚不删除分镜。name为名称，storyboard为画面描述，sourceBlockIds引用本次请求的脚本文本块；删除建议填写原名称、空描述和空来源列表。可选order包含建议调整后的全部分镜标识，排除建议删除的分镜，包含建议新增的分镜；仅调整顺序时changes为空。提案保留当前视频、工作流、素材和剪辑。", json!({"type":"object","properties":{"expected_version":{"type":"integer","minimum":0},"order":{"type":"array","items":string,"uniqueItems":true},"changes":{"type":"array","maxItems":100,"items":{"type":"object","properties":{"shotId":string,"action":{"type":"string","enum":["add","update","remove"]},"name":string,"storyboard":{"type":"string"},"sourceBlockIds":{"type":"array","items":string,"uniqueItems":true}},"required":["shotId","action","name","storyboard","sourceBlockIds"],"additionalProperties":false}}},"required":["expected_version","changes"],"additionalProperties":false})),
        ("studio_write_video_project", "创建或修改当前会话完整视频项目。expected_version使用最新asset_version。新项目含version=2、name、width、height、fps、完整shots列表；省略project.audio时保留现有音轨。相同id分镜省略media、versions、executionId、executionInputs、workflowDrafts、volume、storyboard、references、plannedDuration时保留现有值；显式media:null清除采用视频，audio:[]清除音轨。分镜含id、name、script、storyboard（画面描述）、references（参考列表，可为空）、subtitle、duration、sourceIn、inputs，可引用workflowId/versionId。duration为采用视频的使用秒数，以1/fps秒为间隔，新分镜尚未采用视频时为0；可选plannedDuration保存用户计划，inputs中的生成时长独立保存。单个分镜修改使用studio_update_video_shot。", json!({"type":"object","properties":{"expected_version":{"type":"integer","minimum":0},"project":{"type":"object"}},"required":["expected_version","project"],"additionalProperties":false})),
        ("studio_update_video_shot", "修改当前会话指定分镜，expected_version使用最新asset_version。只提交需要修改的changes字段，保留其他分镜、采用视频、生成记录和项目音轨。script为剧情，storyboard为画面描述，subtitle为画面字幕。references为参考列表，每项包含id、assetId、versionId、url、kind和role；kind使用真实资产类型，role使用storyboard、character、scene、style、first_frame、last_frame、voice、rhythm或reference。plannedDuration为独立计划时长，null清除计划；duration为采用视频的使用时长，尚未采用视频时为0。时长以项目1/fps秒为间隔。inputs中的生成时长独立保存。inputs替换完整生成参数对象，需保留其中未修改参数；workflowId/versionId需要同时选择或同时设为null。", json!({"type":"object","properties":{"expected_version":{"type":"integer","minimum":0},"shot_id":string,"changes":{"type":"object","properties":{"name":{"type":"string","minLength":1},"script":{"type":"string"},"storyboard":{"type":"string"},"references":{"type":"array","items":{"type":"object"}},"subtitle":{"type":"string"},"plannedDuration":{"type":["number","null"],"exclusiveMinimum":0,"maximum":3600},"duration":{"type":"number","minimum":0,"maximum":3600},"sourceIn":{"type":"number","minimum":0},"inputs":{"type":"object"},"workflowId":{"type":["string","null"]},"versionId":{"type":["string","null"]},"volume":{"type":"number","minimum":0,"maximum":2}},"minProperties":1,"additionalProperties":false}},"required":["expected_version","shot_id","changes"],"additionalProperties":false})),
        ("studio_apply_video_result", "将明确选定的生成视频及使用范围采用到指定分镜。采用前读取项目，说明分镜及成片时长变化。asset_version_id必须属于本次执行的视频输出，source_in和duration使用秒并符合项目帧间隔。保留原视频与剪辑历史。", json!({"type":"object","properties":{"expected_version":{"type":"integer","minimum":0},"shot_id":string,"execution_id":string,"asset_version_id":string,"source_in":{"type":"number","minimum":0},"duration":{"type":"number","exclusiveMinimum":0}},"required":["expected_version","shot_id","execution_id","asset_version_id","source_in","duration"],"additionalProperties":false})),
        ("studio_generate_video_shot", "使用视频作品中已保存的分镜工作流版本和输入提交生成。video_id使用读取作品返回的asset_id，expected_version使用asset_version。任务独立保存原始输入，所有结果等待采用。调用后读取生成记录确认真实状态。", json!({"type":"object","properties":{"video_id":string,"shot_id":string,"expected_version":{"type":"integer","minimum":1}},"required":["video_id","shot_id","expected_version"],"additionalProperties":false})),
        ("studio_list_video_generations", "读取指定作品的全部生成记录，包括已删除分镜的任务、原始输入、真实执行状态、错误和输出。", json!({"type":"object","properties":{"video_id":string},"required":["video_id"],"additionalProperties":false})),
        ("studio_generate_video_batch", "按明确选择的分镜范围提交批量生成，固定作品修订及各分镜的工作流版本和输入。dependencies指定本次其他分镜视频的首帧、尾帧或指定帧；position=fixed时必须提供frame，其他位置省略frame。fps为提取帧率。来源和目标同时包含在shot_ids中，循环引用会拒绝提交。多视频输出可通过output固定field与index，未指定时等待明确选择。", json!({"type":"object","properties":{"video_id":string,"expected_version":{"type":"integer","minimum":1},"shot_ids":{"type":"array","items":string,"minItems":1,"maxItems":100,"uniqueItems":true},"dependencies":{"type":"array","items":{"type":"object","properties":{"shot_id":string,"source_shot_id":string,"input_key":string,"position":{"type":"string","enum":["first","last","fixed"]},"fps":{"type":"integer","minimum":1,"maximum":60},"frame":{"type":"integer","minimum":0},"output":{"type":"object","properties":{"field":string,"index":{"type":"integer","minimum":0}},"required":["field","index"],"additionalProperties":false}},"required":["shot_id","source_shot_id","input_key","position","fps"],"additionalProperties":false}}},"required":["video_id","expected_version","shot_ids"],"additionalProperties":false})),
        ("studio_list_video_generation_batches", "读取作品全部生成批次及各分镜的真实状态、固定输入和依赖等待信息。", json!({"type":"object","properties":{"video_id":string},"required":["video_id"],"additionalProperties":false})),
        ("studio_choose_video_dependency_output", "用户明确选择上游输出后，为等待中的下游参数固定来源视频。generation_id为下游生成记录，source_generation_id为同一作品的上游生成记录；output使用真实视频输出的field_key与index。选择后原批次继续执行。", json!({"type":"object","properties":{"video_id":string,"generation_id":string,"source_generation_id":string,"input_key":string,"output":{"type":"object","properties":{"field":string,"index":{"type":"integer","minimum":0}},"required":["field","index"],"additionalProperties":false}},"required":["video_id","generation_id","source_generation_id","input_key","output"],"additionalProperties":false})),
        ("studio_cancel_video_dependency_wait", "用户明确取消等待上游结果时，取消指定下游生成记录的依赖等待，保留当前采用视频、原输入和历史记录。", json!({"type":"object","properties":{"video_id":string,"generation_id":string},"required":["video_id","generation_id"],"additionalProperties":false})),
        ("studio_read_video_material", "读取素材对象的名称、类型、创作要求、全部固定资产版本和创作结果来源。material_id使用当前素材对话上下文中的标识。", json!({"type":"object","properties":{"material_id":string},"required":["material_id"],"additionalProperties":false})),
        ("studio_adopt_video_material_result", "用户明确采用素材结果时，将当前对话中的资产版本登记为素材对象的新版本，保留已有版本与视频引用。expected_version使用素材资产的current_version，尚无结果时为0。", json!({"type":"object","properties":{"material_id":string,"asset_id":string,"asset_version_id":string,"expected_version":{"type":"integer","minimum":0}},"required":["material_id","asset_id","asset_version_id","expected_version"],"additionalProperties":false})),
        ("studio_retry_video_generation", "明确要求沿用原任务输入再次生成时调用。使用原工作流版本和输入快照创建新任务，保留原任务记录。generation_id使用生成记录的id。", json!({"type":"object","properties":{"video_id":string,"generation_id":string},"required":["video_id","generation_id"],"additionalProperties":false})),
        ("studio_export_video", "按当前视频项目的分镜顺序、剪辑范围、字幕、音频、尺寸和帧率导出真实MP4成片。保存作品导出历史及固定来源快照，返回结果资产和确切版本。", json!({"type":"object","properties":{},"additionalProperties":false})),
        ("studio_list_video_exports", "读取作品的全部导出记录，包括固定作品快照、来源修订、状态、错误和结果资产版本。", json!({"type":"object","properties":{"video_id":string},"required":["video_id"],"additionalProperties":false})),
        ("studio_list_workflows", "列出本地已启用工作流、执行类型和默认版本。", json!({ "type": "object", "properties": {}, "additionalProperties": false })),
        ("studio_get_workflow", "读取本地工作流的指定版本及输入要求，省略 version_id 时读取默认版本。", json!({
            "type": "object", "properties": { "workflow_id": string, "version_id": string }, "required": ["workflow_id"], "additionalProperties": false
        })),
        ("studio_run_workflow", "执行本地工作流的指定版本。inputs 使用版本声明的字段，媒体使用 asset_id 和 asset_version_id。缺少必填输入时等待用户填写。", json!({
            "type": "object", "properties": { "workflow_id": string, "version_id": string, "inputs": { "type": "object" } },
            "required": ["workflow_id", "inputs"], "additionalProperties": false
        })),
        ("studio_get_workflow_execution", "读取本地工作流执行状态、节点和输出资产版本。", json!({
            "type": "object", "properties": { "execution_id": string }, "required": ["execution_id"], "additionalProperties": false
        })),
        ("load_skill", "读取当前运行绑定的技能版本。省略 path 时读取 SKILL.md，path 为 . 时分页列出可读文件；offset 使用 Unicode 字符位置或目录序号，truncated 为 true 时继续读取 next。", json!({
            "type": "object", "properties": { "skill_id": string, "path": { "type": "string" }, "offset": { "type": "integer", "minimum": 0 } },
            "required": ["skill_id"], "additionalProperties": false
        })),
        ("list_assets", "列出当前会话的资产、版本和引用。", json!({ "type": "object", "properties": {}, "additionalProperties": false })),
        ("read_asset", "读取指定资产版本的文本内容。", json!({
            "type": "object", "properties": { "asset_id": string, "asset_version_id": string },
            "required": ["asset_id", "asset_version_id"], "additionalProperties": false
        })),
        ("create_text_asset", "在当前会话中创建 Markdown 文档并加入资产库。", json!({
            "type": "object", "properties": { "name": string, "content": { "type": "string", "minLength": 1, "maxLength": 1048576 } },
            "required": ["name", "content"], "additionalProperties": false
        })),
        ("update_text_asset", "更新手动文档并保留原有版本。", json!({
            "type": "object", "properties": { "asset_id": string, "content": { "type": "string", "minLength": 1, "maxLength": 1048576 } },
            "required": ["asset_id", "content"], "additionalProperties": false
        })),
    ].into_iter().map(|(name, description, schema)| Ok(RunTool {
        definition: ToolDefinition::new(ToolName::new(name).map_err(|_| Error::State("工具名称无效"))?, description, schema),
        target: ToolTarget::Builtin, requires_approval: matches!(name,"studio_run_workflow" | "studio_generate_video_shot" | "studio_generate_video_batch" | "studio_retry_video_generation"), read_only: matches!(name, "studio_read_video_project" | "studio_list_video_generations" | "studio_list_video_generation_batches" | "studio_list_video_exports" | "studio_read_video_material" | "list_assets" | "read_asset" | "load_skill" | "studio_list_workflows" | "studio_get_workflow" | "studio_get_workflow_execution"),
    })).collect()
}

impl Store {
    pub(crate) async fn tool_display_name(
        &self,
        id: &str,
        name: &ToolName,
    ) -> Result<String> {
        let tools = self.run_tools(id).await?;
        let tool = tools
            .iter()
            .find(|tool| tool.definition.name == *name)
            .ok_or(Error::Invalid("调用的工具未注册"))?;
        Ok(tool.display_name().to_owned())
    }

    pub(crate) async fn run_tools(&self, id: &str) -> Result<Vec<RunTool>> {
        let run = self.get_run(id).await?;
        let saved: Option<String> =
            sqlx::query_scalar("SELECT tool_snapshot FROM runs WHERE id=?")
                .bind(id)
                .fetch_one(&self.pool)
                .await?;
        if let Some(saved) = saved {
            return Ok(serde_json::from_str(&saved)?);
        }
        let mut tools = builtin_tools()?;
        if run["expert_id"].as_str().is_some_and(|id| !id.is_empty()) {
            tools.extend(crate::expert_tasks::expert_tools()?);
        }
        tools.extend(self.mcp_run_tools().await?);
        for tool in &tools {
            jsonschema::validator_for(&tool.definition.parameters)
                .map_err(|_| Error::Invalid("工具参数 Schema 无效"))?;
        }
        sqlx::query(
            "UPDATE runs SET tool_snapshot=? WHERE id=? AND tool_snapshot IS NULL",
        )
        .bind(serde_json::to_string(&tools)?)
        .bind(id)
        .execute(&self.pool)
        .await?;
        Ok(tools)
    }

    pub(crate) async fn save_tool_part(
        &self,
        id: &str,
        turn: usize,
        part: usize,
        call: &ToolCall,
    ) -> Result<()> {
        self.get_run(id).await?;
        let now = timestamp()?;
        sqlx::query("INSERT INTO tool_calls(id,run_id,turn,part,call,created_at,updated_at) VALUES(?,?,?,?,?,?,?)")
            .bind(format!("{id}-{turn}-{part}")).bind(id).bind(turn as i64).bind(i64::try_from(part).map_err(|_| Error::State("模型输出内容块超过范围"))?)
            .bind(serde_json::to_string(call)?).bind(&now).bind(&now).execute(&self.pool).await?;
        Ok(())
    }

    pub(crate) async fn tool_call_records(
        &self,
        id: &str,
        turn: usize,
    ) -> Result<Vec<Value>> {
        self.get_run(id).await?;
        let rows = sqlx::query("SELECT id,call,status,result,definition,approved_arguments FROM tool_calls WHERE run_id=? AND turn=? ORDER BY part")
            .bind(id).bind(turn as i64).fetch_all(&self.pool).await?;
        rows.into_iter().map(|row| Ok(json!({
            "id": row.get::<String, _>("id"), "call": serde_json::from_str::<Value>(row.get("call"))?,
            "status": row.get::<String, _>("status"),
            "result": row.get::<Option<String>, _>("result").map(|result| serde_json::from_str::<Value>(&result)).transpose()?,
            "definition": row.get::<Option<String>, _>("definition").map(|definition| serde_json::from_str::<Value>(&definition)).transpose()?,
            "approved_arguments": row.get::<Option<String>, _>("approved_arguments").map(|arguments| serde_json::from_str::<Value>(&arguments)).transpose()?,
        }))).collect()
    }

    async fn begin_tool_execution(
        &self,
        run_id: &str,
        record: &Value,
        tool: &RunTool,
        original_call: &ToolCall,
    ) -> Result<(Value, ToolCall)> {
        let mut call = original_call.clone();
        if let Some(arguments) = record["approved_arguments"].as_object() {
            call.function.arguments = arguments.clone();
        }
        let run = self.get_run(run_id).await?;
        self.guard_expert_tool_write(&run, tool).await?;
        let arguments = Value::Object(call.function.arguments.clone());
        if call.function.invalid_arguments.is_some() {
            return Err(Error::Invalid("工具参数需要有效的 JSON 对象"));
        }
        let validator = jsonschema::validator_for(&tool.definition.parameters)
            .map_err(|_| Error::Invalid("工具参数 Schema 无效"))?;
        if !validator.is_valid(&arguments) {
            return Err(Error::Invalid("工具参数不符合 Schema"));
        }
        let id = text(record, "id")?;
        let started = timestamp()?;
        let changed = sqlx::query("UPDATE tool_calls SET status='executing',definition=?,updated_at=?,started_at=? WHERE run_id=? AND id=? AND status IN ('prepared','approved')")
            .bind(serde_json::to_string(tool)?).bind(&started).bind(&started).bind(run_id).bind(id).execute(&self.pool).await?;
        if changed.rows_affected() != 1 {
            return Err(Error::State("工具调用已经执行或结果未知"));
        }
        Ok((run, call))
    }

    pub(crate) async fn execute_local_tool(
        &self,
        run_id: &str,
        record: &Value,
        tool: &RunTool,
        original_call: &ToolCall,
    ) -> Result<Option<UserContent>> {
        let mut call = original_call.clone();
        if let Some(arguments) = record["approved_arguments"].as_object() {
            call.function.arguments = arguments.clone();
        }
        if record["status"] != "waiting_workflow"
            && record["status"] != "waiting_mcp"
            && self
                .pause_for_workflow_input(run_id, record, tool, &call)
                .await?
        {
            return Ok(None);
        }
        let (run, call) =
            if record["status"] == "waiting_workflow" || record["status"] == "waiting_mcp" {
                (self.get_run(run_id).await?, call)
            } else {
                self.begin_tool_execution(run_id, record, tool, original_call)
                    .await?
            };
        let arguments = Value::Object(call.function.arguments.clone());
        let id = text(record, "id")?;
        let video_context = &run["input"]["video_edit_context"];
        let script_tool = match video_context["scope"].as_str() {
            Some("project") => "studio_write_video_project",
            Some("script") => "studio_propose_video_script",
            Some("storyboard") => "studio_propose_video_storyboard",
            _ => "studio_update_video_shot",
        };
        let output = if video_context.is_object() && !tool.read_only && tool.definition.name.as_str() != script_tool {
            Err(Error::Invalid("本次剧本请求只能使用对应范围的视频项目编辑工具"))
        } else if run["input"]["material_context"].is_object() && tool.definition.name == "studio_adopt_video_material_result" && arguments["material_id"] != run["input"]["material_context"]["material_id"] {
            Err(Error::Invalid("素材结果需要登记到本次消息指定的素材对象"))
        } else { match &tool.target {
            ToolTarget::Builtin if video_context["video_id"].is_string() && tool.definition.name == "studio_read_video_project" => {
                self.read_video_work(text(video_context,"video_id")?).await
            }
            ToolTarget::Builtin if video_context.is_object() && ["studio_propose_video_script","studio_propose_video_storyboard"].contains(&tool.definition.name.as_str()) => {
                self.propose_video_creation(text(&run,"session_id")?,id,&arguments,video_context).await
            }
            ToolTarget::Builtin if tool.definition.name.as_str() == "studio_run_workflow" => {
                let workflow = self.workflow_for_call(run_id, tool, &call).await?.ok_or(Error::State("工作流版本缺失"))?;
                self.execute_workflow(&run, id, &call, &workflow).await
            }
            ToolTarget::Builtin if run["input"]["video_edit_context"].is_object() && tool.definition.name.as_str() == "studio_update_video_shot" => {
                self.update_video_script_from_chat(text(&run,"session_id")?,id,&arguments,&run["input"]["video_edit_context"]).await
            }
            ToolTarget::Builtin if video_context["scope"] == "project" && tool.definition.name.as_str() == "studio_write_video_project" => {
                self.write_video_script_project_from_chat(text(&run,"session_id")?,id,&arguments,video_context).await
            }
            ToolTarget::Builtin => {
                self.call_builtin(
                    text(&run, "session_id")?,
                    id,
                    &tool.definition.name,
                    &arguments,
                )
                .await
            }
            ToolTarget::Expert => {
                self.call_expert_control(run_id, id, &tool.definition.name, &arguments)
                    .await
            }
            ToolTarget::Mcp { .. } => match self
                .execute_mcp_tool(run_id, id, tool, &call)
                .await
            {
                Ok(Some(output)) => Ok(output),
                Ok(None) => return Ok(None),
                Err(error) => Err(error),
            },
        }};
        if let Ok(output) = &output
            && matches!(tool.target, ToolTarget::Builtin)
            && matches!(
                tool.definition.name.as_str(),
                "create_text_asset" | "update_text_asset"
            )
        {
            self.record_expert_asset(&run, output).await?;
        }
        self.finish_tool_execution(&run, record, tool, &call, output)
            .await
            .map(Some)
    }

    async fn finish_tool_execution(
        &self,
        run: &Value,
        record: &Value,
        tool: &RunTool,
        call: &ToolCall,
        output: Result<Value>,
    ) -> Result<UserContent> {
        let run_id = text(run, "id")?;
        let id = text(record, "id")?;
        let result = match &output {
            Ok(output) if matches!(&tool.target, ToolTarget::Mcp { .. }) => {
                crate::mcp::model_result_content(output).map(|content| call.result(content))
            }
            Ok(output) => Ok(call.result(vec![ToolResultContent::json(output.clone())])),
            Err(error) => Ok(call.error_result(vec![ToolResultContent::text(error.to_string())])),
        };
        let (result, conversion_error) = match result {
            Ok(result) => (result, None),
            Err(error) => (
                call.error_result(vec![ToolResultContent::text(error.to_string())]),
                Some(error),
            ),
        };
        let content = UserContent::ToolResult(result);
        let now = timestamp()?;
        let mut transaction = self.pool.begin().await?;
        sqlx::query("UPDATE runs SET document=json_set(document,'$.status','running') WHERE id=? AND status='waiting_workflow'")
            .bind(run_id).execute(&mut *transaction).await?;
        sqlx::query("UPDATE tool_calls SET status=?,result=?,raw_result=?,updated_at=?,ended_at=? WHERE run_id=? AND id=? AND status IN ('executing','waiting_workflow','waiting_mcp')")
            .bind(if conversion_error.is_some() { "failed" } else { match &output { Ok(_) => "succeeded", Err(Error::ToolResultUnknown(_)) => "result_unknown", Err(_) => "failed" } }).bind(serde_json::to_string(&content)?)
            .bind(output.as_ref().ok().map(serde_json::to_string).transpose()?).bind(&now)
            .bind(&now)
            .bind(run_id).bind(id).execute(&mut *transaction).await?;
        let result_text = if let Some(error) = &conversion_error {
            json!({ "response": output.as_ref().map_err(|_| Error::State("工具响应缺失"))?, "model_content_error": error.to_string() }).to_string()
        } else {
            match &output {
                Ok(output) => serde_json::to_string(output)?,
                Err(error) => error.to_string(),
            }
        };
        Self::append_event_in(&mut transaction, text(run, "session_id")?, run_id, json!({
            "type": "TOOL_CALL_RESULT", "toolCallId": id, "messageId": format!("{id}-result"),
            "content": result_text, "role": "tool", "isError": output.is_err() || conversion_error.is_some(),
        })).await?;
        let message = json!({ "id": format!("{id}-result"), "session_id": run["session_id"], "run_id": run_id, "role": "tool", "tool_call_id": id, "content": [{ "type": "text", "text": result_text }], "created_at": now });
        sqlx::query(
            "INSERT INTO messages(id,session_id,created_at,document) VALUES(?,?,?,?)",
        )
        .bind(format!("{id}-result"))
        .bind(text(run, "session_id")?)
        .bind(now)
        .bind(message.to_string())
        .execute(&mut *transaction)
        .await?;
        transaction.commit().await?;
        output?;
        if let Some(error) = conversion_error {
            return Err(error);
        }
        Ok(content)
    }

    pub(crate) async fn refuse_tool_while_waiting_for_expert_card(
        &self,
        run_id: &str,
        record: &Value,
        call: &ToolCall,
    ) -> Result<UserContent> {
        let run = self.get_run(run_id).await?;
        let id = text(record, "id")?;
        let reason = "等待专家卡片答复，当前调用未执行";
        let content =
            UserContent::ToolResult(call.error_result(vec![ToolResultContent::text(reason)]));
        let now = timestamp()?;
        let mut transaction = self.pool.begin().await?;
        let changed = sqlx::query("UPDATE tool_calls SET status='not_executed',result=?,updated_at=?,ended_at=? WHERE run_id=? AND id=? AND status IN ('prepared','approved')")
            .bind(serde_json::to_string(&content)?).bind(&now).bind(&now).bind(run_id).bind(id).execute(&mut *transaction).await?;
        if changed.rows_affected() != 1 {
            return Err(Error::State("工具调用状态已更新"));
        }
        Self::append_event_in(&mut transaction, text(&run, "session_id")?, run_id, json!({
            "type": "TOOL_CALL_RESULT", "toolCallId": id, "messageId": format!("{id}-result"), "content": reason, "role": "tool", "isError": true,
        })).await?;
        let message = json!({ "id": format!("{id}-result"), "session_id": run["session_id"], "run_id": run_id, "role": "tool", "tool_call_id": id, "content": [{ "type": "text", "text": reason }], "created_at": now });
        sqlx::query(
            "INSERT INTO messages(id,session_id,created_at,document) VALUES(?,?,?,?)",
        )
        .bind(format!("{id}-result"))
        .bind(text(&run, "session_id")?)
        .bind(now)
        .bind(message.to_string())
        .execute(&mut *transaction)
        .await?;
        transaction.commit().await?;
        Ok(content)
    }

    async fn call_builtin(
        &self,
        session: &str,
        request: &str,
        name: &ToolName,
        input: &Value,
    ) -> Result<Value> {
        match name.as_str() {
            "studio_read_video_project" => self.read_video_project(session).await,
            "studio_write_video_project" => self.write_video_project_from_chat(session,request,input.clone()).await,
            "studio_update_video_shot" => self.update_video_shot(session,request,input).await,
            "studio_propose_video_script" | "studio_propose_video_storyboard" => {
                let record = self.read_video_project(session).await?;
                let scope = if name == "studio_propose_video_script" {"script"} else {"storyboard"};
                self.propose_video_creation(session,request,input,&json!({"scope":scope,"project":record["project"]})).await
            }
            "studio_apply_video_result" => self.apply_video_result(session,request,input).await,
            "studio_generate_video_shot" => self.create_video_generation(text(input,"video_id")?,json!({"request_id":request,"session_id":session,"shot_id":input["shot_id"],"expected_version":input["expected_version"]})).await,
            "studio_list_video_generations" => self.list_video_generations(text(input,"video_id")?).await,
            "studio_generate_video_batch" => {
                let mut batch = input.as_object().ok_or(Error::Invalid("批量生成参数需要对象"))?.clone();
                batch.remove("video_id");
                batch.insert("request_id".into(),json!(request));
                batch.insert("session_id".into(),json!(session));
                self.prepare_video_generation_batch(text(input,"video_id")?,Value::Object(batch)).await
            },
            "studio_list_video_generation_batches" => self.list_video_generation_batches(text(input,"video_id")?).await,
            "studio_choose_video_dependency_output" => self.choose_video_dependency_output(text(input,"video_id")?,text(input,"generation_id")?,json!({"session_id":session,"source_generation_id":input["source_generation_id"],"input_key":input["input_key"],"output":input["output"]})).await,
            "studio_cancel_video_dependency_wait" => self.cancel_video_dependency_wait(text(input,"video_id")?,text(input,"generation_id")?,json!({"session_id":session})).await,
            "studio_read_video_material" => self.read_video_material(text(input,"material_id")?).await,
            "studio_adopt_video_material_result" => self.adopt_video_material_result(text(input,"material_id")?,json!({"request_id":request,"session_id":session,"asset_id":input["asset_id"],"asset_version_id":input["asset_version_id"],"expected_version":input["expected_version"]})).await,
            "studio_retry_video_generation" => self.retry_video_generation(text(input,"video_id")?,text(input,"generation_id")?,json!({"request_id":request,"session_id":session})).await,
            "studio_export_video" => self.export_current_video(session,request).await,
            "studio_list_video_exports" => self.list_video_work_exports(text(input,"video_id")?).await,
            "studio_list_workflows" => {
                let workflows = self.list_workflows(None, None, Some(true), None).await?;
                Ok(json!(workflows.as_array().ok_or(Error::State("工作流列表缺失"))?.iter().map(|workflow| json!({
                    "id": workflow["id"], "name": workflow["name"], "type": workflow["type"], "category": workflow["category"],
                    "description": workflow["draft"]["description"], "default_version_id": workflow["default_version_id"],
                })).collect::<Vec<_>>()))
            }
            "studio_get_workflow" => self.workflow_version(text(input, "workflow_id")?, input.get("version_id").map(|value| value.as_str().ok_or(Error::Invalid("工作流版本标识无效"))).transpose()?).await,
            "studio_get_workflow_execution" => self.get_local_execution(text(input, "execution_id")?).await,
            "load_skill" => self.load_run_skill(request, input).await,
            "list_assets" => Ok(serde_json::to_value(
                self.list_session_assets(session).await?,
            )?),
            "create_text_asset" => Ok(serde_json::to_value(
                self.create_text_asset_with_origin(
                    session,
                    text(input, "name")?,
                    text(input, "content")?,
                    request,
                    "agent",
                )
                .await?,
            )?),
            "update_text_asset" => Ok(serde_json::to_value(
                self.update_text_asset_in_session(
                    session,
                    text(input, "asset_id")?,
                    text(input, "content")?,
                    request,
                )
                .await?,
            )?),
            "read_asset" => {
                let asset = self.get_asset(text(input, "asset_id")?).await?;
                let version_id = text(input, "asset_version_id")?;
                let version = asset
                    .versions
                    .iter()
                    .find(|version| version.id == version_id)
                    .ok_or(Error::NotFound)?;
                if version.size_bytes > 1024 * 1024 {
                    return Err(Error::Invalid("文本资产超过 1 MiB"));
                }
                if !version.mime_type.starts_with("text/")
                    && version.mime_type != "application/json"
                {
                    return Err(Error::Invalid("当前工具仅读取文本资产"));
                }
                let content = String::from_utf8(self.read_asset(&asset.id, &version.id).await?)
                    .map_err(|_| Error::Invalid("资产需要 UTF-8 文本编码"))?;
                Ok(
                    json!({ "asset_id": asset.id, "asset_version_id": version.id, "name": asset.name, "content": content }),
                )
            }
            _ => Err(Error::Invalid("调用的工具未注册")),
        }
    }
}
