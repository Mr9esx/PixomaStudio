use super::*;

#[tokio::test]
async fn local_workflow_input_reply_keeps_the_selected_version_after_reopening()
-> std::result::Result<(), Box<dyn std::error::Error>> {
    let root = std::path::PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../.local-cache");
    std::fs::create_dir_all(&root)?;
    let directory = tempfile::Builder::new().prefix("workflow-input-local-").tempdir_in(root)?;
    let database = directory.path().join("studio.sqlite");
    let store = Store::open(&database).await?;
    let model = store.save_model(None, json!({
        "name": "输入保存检查", "protocol": "openai_chat_compatible", "base_url": "http://127.0.0.1:1/v1", "model": "local-model",
        "enabled": true, "agent_enabled": true, "default": true,
        "limits": { "context_window_tokens": 32000, "max_input_tokens": 24000, "max_output_tokens": 4000 },
        "thinking": { "enabled": false }, "capabilities": { "tools": true, "vision": false, "image_output": false, "streaming": true }
    })).await?;
    let definition = json!({ "type": "comfyui", "inputs": [{ "key": "prompt", "type": "string", "required": true }],
        "input_schema": { "type": "object", "properties": { "prompt": { "type": "string", "minLength": 1 } }, "required": ["prompt"], "additionalProperties": false },
        "outputs": [{ "key": "image", "type": "image" }], "execution": { "node_id": "",
            "workflow": serde_json::from_str::<Value>(include_str!("../tests/data/klein9b-api.json"))?,
            "input_bindings": [{ "key": "prompt", "node_id": "40", "field_path": "text" }],
            "output_bindings": [{ "key": "image", "node_id": "80", "index": 0 }] } });
    let workflow = store.create_workflow(json!({ "name": "Klein9B 输入检查", "category": "image", "enabled": true, "draft": definition })).await?;
    let workflow_id = text(&workflow, "id")?;
    let version = store.create_workflow_version(workflow_id, json!({ "expected_revision": 1, "version": "1.0.0" })).await?;
    let version_id = text(&version, "id")?;
    store.set_default_workflow_version(workflow_id, version_id).await?;
    let session = store.create_session("input-local").await?;
    let queued = store.queue_run(&session.id, "input-local", text(&model, "id")?, "生成图片", "auto_approve").await?;
    let run_id = text(&queued["run"], "id")?;
    assert!(store.claim_run(run_id).await?);
    let tool = store.run_tools(run_id).await?.into_iter().find(|tool| tool.definition.name.as_str() == "studio_run_workflow").ok_or("本地工作流工具缺失")?;
    let call = ToolCall::from_wire("local-input", rig::message::ToolFunction::new(tool.definition.name.clone(), json!({ "workflow_id": workflow_id, "version_id": version_id, "inputs": {} })));
    store.save_tool_part(run_id, 1, 0, &call).await?;
    let record = store.tool_call_records(run_id, 1).await?.remove(0);
    assert!(store.pause_for_workflow_input(run_id, &record, &tool, &call).await?);
    let detail = store.session_detail(&session.id).await?;
    let pending = &detail["pending_clarifications"][0];
    assert_eq!(pending["metadata"]["workflow"]["version_id"], version_id);
    let response = json!({ "interruptId": pending["id"], "status": "resolved", "payload": { "inputs": { "prompt": "海边灯塔" } } });
    store.resolve_run_interrupt(&session.id, &response).await?;
    store.resolve_run_interrupt(&session.id, &response).await?;
    store.close().await;
    let reopened = Store::open(&database).await?;
    assert_eq!(reopened.get_run(run_id).await?["status"], "queued");
    assert!(reopened.claim_run(run_id).await?);
    let records = reopened.tool_call_records(run_id, 1).await?;
    assert_eq!(records.len(), 1);
    assert_eq!(records[0]["approved_arguments"]["inputs"]["prompt"], "海边灯塔");
    let saved = reopened.workflow_for_call(run_id, &tool, &call).await?.ok_or("工作流绑定缺失")?;
    assert_eq!(saved["version_id"], version_id);
    assert_eq!(saved["execution"]["workflow"], definition["execution"]["workflow"]);
    reopened.close().await;
    Ok(())
}
