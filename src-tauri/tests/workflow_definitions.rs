use pixoma_studio::{LocalApi, LocalRequest, Store};
use serde_json::{Value, json};
use std::path::PathBuf;

fn directory() -> Result<tempfile::TempDir, std::io::Error> {
    let root = PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../.local-cache");
    std::fs::create_dir_all(&root)?;
    tempfile::Builder::new()
        .prefix("definitions-")
        .tempdir_in(root)
}

pub(super) fn comfy_definition() -> Value {
    json!({
        "type": "comfyui", "description": "Klein9B 文生图", "usage": "输入画面描述", "examples": [],
        "inputs": [{ "key": "prompt", "type": "string", "required": true, "description": "画面描述" }],
        "input_schema": { "type": "object", "properties": { "prompt": { "type": "string", "minLength": 1 } }, "required": ["prompt"], "additionalProperties": false },
        "outputs": [{ "key": "image", "type": "image", "media_type": "image/png" }],
        "execution": {
            "node_id": "", "workflow_filename": "MiniMax_Klein9B_文生图_本地.json",
            "workflow": serde_json::from_str::<Value>(include_str!("data/klein9b-api.json")).expect("验收工作流 JSON 无效"),
            "input_bindings": [{ "key": "prompt", "node_id": "40", "field_path": "text" }],
            "output_bindings": [{ "key": "image", "node_id": "80", "index": 0 }]
        }
    })
}

#[tokio::test]
async fn imported_comfy_definition_keeps_immutable_versions_and_checks_revisions()
-> Result<(), Box<dyn std::error::Error>> {
    let directory = directory()?;
    let path = directory.path().join("studio.sqlite");
    let store = Store::open(&path).await?;
    let mut definition = comfy_definition();
    definition["preview"] = json!("pixoma-media://localhost/asset/version#result.png");
    let saved = store.create_workflow(json!({ "name": "Klein9B 文生图", "category": "image", "tags": ["文生图"], "enabled": true, "draft": definition })).await?;
    let id = saved["id"].as_str().ok_or("工作流标识缺失")?;
    assert_eq!(saved["revision"], 1);
    assert_eq!(saved["draft"]["preview"], definition["preview"]);
    assert_eq!(
        saved["draft"]["execution"]["workflow"],
        definition["execution"]["workflow"]
    );
    let version = store
        .create_workflow_version(
            id,
            json!({ "expected_revision": 1, "version": "1.0.0", "notes": "保存导入内容" }),
        )
        .await?;
    let version_id = version["id"].as_str().ok_or("版本标识缺失")?;
    assert!(
        store
            .create_workflow_version(id, json!({ "expected_revision": 1, "version": "1.0.0" }))
            .await
            .is_err()
    );
    let mut changed = definition.clone();
    changed["execution"]["workflow"]["60"]["inputs"]["seed"] = json!(43);
    store
        .update_workflow(id, json!({ "expected_revision": 1, "draft": changed }))
        .await?;
    assert!(
        store
            .update_workflow(id, json!({ "expected_revision": 1, "name": "过期编辑" }))
            .await
            .is_err()
    );
    store.set_default_workflow_version(id, version_id).await?;
    let saved_id = id.to_owned();
    let saved_version = version_id.to_owned();
    store.close().await;
    let reopened = Store::open(&path).await?;
    let reread = reopened
        .get_workflow_version(&saved_id, &saved_version)
        .await?;
    assert_eq!(reread, version);
    assert_eq!(
        reread["definition"]["execution"]["workflow"]["60"]["inputs"]["seed"],
        42
    );
    assert_eq!(
        reopened.get_workflow(&saved_id).await?["draft"]["execution"]["workflow"]["60"]["inputs"]["seed"],
        43
    );
    assert_eq!(
        reopened
            .list_workflow_versions(&saved_id)
            .await?
            .as_array()
            .ok_or("版本列表缺失")?
            .len(),
        1
    );
    reopened.delete_workflow(&saved_id).await?;
    assert!(reopened.get_workflow(&saved_id).await.is_err());
    reopened.close().await;
    Ok(())
}

#[tokio::test]
async fn invalid_graph_bindings_and_schemas_do_not_create_workflows()
-> Result<(), Box<dyn std::error::Error>> {
    let directory = directory()?;
    let store = Store::open(&directory.path().join("studio.sqlite")).await?;
    let mut invalid = comfy_definition();
    invalid["execution"]["input_bindings"][0]["field_path"] = json!("missing");
    let error = store
        .create_workflow(json!({ "name": "无效绑定", "category": "image", "draft": invalid }))
        .await
        .expect_err("不存在的输入位置应被拒绝");
    assert!(error.to_string().contains("prompt"));
    let mut invalid = comfy_definition();
    invalid["execution"]["workflow"]["60"]["inputs"]["model"] = json!(["missing", 0]);
    assert!(
        store
            .create_workflow(json!({ "name": "无效连线", "category": "image", "draft": invalid }))
            .await
            .is_err()
    );
    let mut invalid = comfy_definition();
    invalid["input_schema"]["properties"]["prompt"]["type"] = json!("unknown");
    assert!(
        store
            .create_workflow(
                json!({ "name": "无效 Schema", "category": "image", "draft": invalid })
            )
            .await
            .is_err()
    );
    let mut cycle = comfy_definition();
    cycle["execution"]["workflow"]["10"]["inputs"]["model"] = json!(["60", 0]);
    assert!(
        store
            .create_workflow(json!({ "name": "循环连线", "category": "image", "draft": cycle }))
            .await
            .is_err()
    );
    assert!(
        store
            .list_workflows(None, None, None, None)
            .await?
            .as_array()
            .ok_or("工作流列表缺失")?
            .is_empty()
    );
    store.close().await;
    Ok(())
}

#[tokio::test]
async fn online_definitions_keep_provider_operations_and_filter_independently()
-> Result<(), Box<dyn std::error::Error>> {
    let directory = directory()?;
    let store = Store::open(&directory.path().join("studio.sqlite")).await?;
    let mut online = comfy_definition();
    online["type"] = json!("online");
    online["execution"] = json!({ "provider": "seedream", "operation": "image", "connection_id": "", "model": "doubao-seedream-4-0-250828", "capability": "seedream_4", "defaults": { "size": "2K" }, "input_mapping": { "prompt": "prompt" }, "output_mapping": { "image": "images" } });
    let saved = store.create_workflow(json!({ "name": "Seedream 图片", "category": "image", "enabled": false, "draft": online })).await?;
    assert_eq!(saved["type"], "online");
    assert_eq!(
        store
            .list_workflows(Some("online"), Some("image"), Some(false), Some("Seedream"))
            .await?
            .as_array()
            .ok_or("工作流列表缺失")?
            .len(),
        1
    );
    assert!(
        store
            .list_workflows(Some("comfyui"), None, None, None)
            .await?
            .as_array()
            .ok_or("工作流列表缺失")?
            .is_empty()
    );
    online["execution"]["provider"] = json!("unknown");
    assert!(
        store
            .create_workflow(json!({ "name": "未知服务", "category": "image", "draft": online }))
            .await
            .is_err()
    );
    store
        .delete_workflow(saved["id"].as_str().ok_or("工作流标识缺失")?)
        .await?;
    store.close().await;
    Ok(())
}

#[tokio::test]
async fn workflow_routes_save_and_read_the_imported_graph() -> Result<(), Box<dyn std::error::Error>>
{
    let directory = directory()?;
    let store = Store::open(&directory.path().join("studio.sqlite")).await?;
    let api = LocalApi::new(store.clone());
    let saved = api
        .request(
            LocalRequest {
                method: "POST".to_owned(),
                path: "/api/v1/studio/workflows".to_owned(),
                body: Some(
                    json!({ "name": "Klein9B", "category": "image", "draft": comfy_definition() }),
                ),
            },
        )
        .await?;
    let id = saved["id"].as_str().ok_or("工作流标识缺失")?;
    let reread = api
        .request(
            LocalRequest {
                method: "GET".to_owned(),
                path: format!("/api/v1/studio/workflows/{id}"),
                body: None,
            },
        )
        .await?;
    assert_eq!(saved, reread);
    let list = api
        .request(
            LocalRequest {
                method: "GET".to_owned(),
                path: "/api/v1/studio/workflows?type=comfyui&category=image&enabled=false"
                    .to_owned(),
                body: None,
            },
        )
        .await?;
    assert_eq!(list[0]["id"], id);
    store.close().await;
    Ok(())
}
