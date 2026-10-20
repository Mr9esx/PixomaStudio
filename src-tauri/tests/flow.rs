use pixoma_studio::{LocalApi, LocalRequest, Store};
use serde_json::{Value, json};
use std::path::PathBuf;

async fn request(
    api: &LocalApi,
    session: &str,
    method: &str,
    operation: &str,
    body: Value,
) -> pixoma_studio::Result<Value> {
    api.request(
        "account-a",
        LocalRequest {
            method: method.to_owned(),
            path: format!("/api/v1/studio/sessions/{session}/flow{operation}"),
            body: Some(body),
        },
    )
    .await
}

#[tokio::test]
async fn flow_edits_are_atomic_and_node_deletion_removes_connected_edges()
-> Result<(), Box<dyn std::error::Error>> {
    let root = PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../.local-cache");
    std::fs::create_dir_all(&root)?;
    let directory = tempfile::Builder::new().prefix("flow-").tempdir_in(root)?;
    let store = Store::open(&directory.path().join("studio.sqlite")).await?;
    let api = LocalApi::new(store.clone());
    let session = store.create_session("account-a", "session").await?;
    let other = store.create_session("account-a", "other").await?;
    let first = request(
        &api,
        &session.id,
        "POST",
        "/nodes",
        json!({ "type": "plan", "title": "编写文稿", "position": { "x": 10, "y": 20 } }),
    )
    .await?;
    let second = request(
        &api,
        &session.id,
        "POST",
        "/nodes",
        json!({ "type": "stage", "title": "审核文稿", "position": { "x": 30, "y": 40 } }),
    )
    .await?;
    let source = first["id"].as_str().ok_or("节点标识缺失")?;
    let target = second["id"].as_str().ok_or("节点标识缺失")?;
    assert!(
        request(
            &api,
            &session.id,
            "POST",
            "/nodes",
            json!({ "type": "asset", "title": "资产", "position": { "x": 0, "y": 0 } })
        )
        .await
        .is_err()
    );
    assert!(
        request(
            &api,
            &other.id,
            "POST",
            "/edges",
            json!({ "source": source, "target": target })
        )
        .await
        .is_err()
    );
    assert!(
        request(
            &api,
            &session.id,
            "POST",
            "/edges",
            json!({ "source": source, "target": source })
        )
        .await
        .is_err()
    );
    let edge = request(
        &api,
        &session.id,
        "POST",
        "/edges",
        json!({ "source": source, "target": target, "label": "审核" }),
    )
    .await?;
    assert_eq!(edge["label"], "审核");
    assert!(
        request(
            &api,
            &session.id,
            "POST",
            "/edges",
            json!({ "source": source, "target": target })
        )
        .await
        .is_err()
    );
    assert!(
        request(
            &api,
            &session.id,
            "PATCH",
            "",
            json!({ "nodes": [
        { "id": source, "position": { "x": 500, "y": 200 }, "sort_order": 20 },
        { "id": "missing", "position": { "x": 0, "y": 0 }, "sort_order": 10 }
    ] })
        )
        .await
        .is_err()
    );
    let detail = store.session_detail("account-a", &session.id).await?;
    assert_eq!(detail["flow"]["nodes"][0]["position"]["x"], 10.0);
    request(
        &api,
        &session.id,
        "PATCH",
        "",
        json!({ "nodes": [
        { "id": source, "position": { "x": 500, "y": 200 }, "sort_order": 20 }
    ] }),
    )
    .await?;
    store.close().await;
    let reopened = Store::open(&directory.path().join("studio.sqlite")).await?;
    let detail = reopened.session_detail("account-a", &session.id).await?;
    let nodes = detail["flow"]["nodes"].as_array().ok_or("节点列表缺失")?;
    assert_eq!(nodes[0]["id"], target);
    assert_eq!(
        nodes
            .iter()
            .find(|node| node["id"] == source)
            .ok_or("节点缺失")?["position"]["x"],
        500.0
    );
    request(
        &LocalApi::new(reopened.clone()),
        &session.id,
        "DELETE",
        &format!("/nodes/{source}"),
        json!(null),
    )
    .await?;
    let detail = reopened.session_detail("account-a", &session.id).await?;
    assert_eq!(
        detail["flow"]["nodes"]
            .as_array()
            .ok_or("节点列表缺失")?
            .len(),
        1
    );
    assert!(
        detail["flow"]["edges"]
            .as_array()
            .ok_or("连线列表缺失")?
            .is_empty()
    );
    reopened.close().await;
    Ok(())
}
