use pixoma_studio::{LocalApi, LocalRequest, Store};
use serde_json::{Value, json};
use std::io::{Cursor, Write};
use std::path::PathBuf;

fn archive(files: &[(&str, &[u8])]) -> Result<Vec<u8>, Box<dyn std::error::Error>> {
    let mut output = zip::ZipWriter::new(Cursor::new(Vec::new()));
    for (name, bytes) in files {
        output.start_file(*name, zip::write::SimpleFileOptions::default())?;
        output.write_all(bytes)?;
    }
    Ok(output.finish()?.into_inner())
}

#[test]
fn skill_import_reads_real_zip_files_and_rejects_unsafe_entries()
-> Result<(), Box<dyn std::error::Error>> {
    let manifest = b"---\nname: review-document\ndescription: Review documents\n---\nRead references/rules.md.";
    let bytes = archive(&[
        ("review-document/SKILL.md", manifest),
        ("review-document/references/rules.md", b"PX73916"),
    ])?;
    let inspected = Store::inspect_skill_zip(&bytes)?;
    assert_eq!(inspected["name"], "review-document");
    let files = inspected["files"].as_array().ok_or("文件列表缺失")?;
    assert_eq!(files.len(), 2);
    assert!(
        files
            .iter()
            .any(|file| file["path"] == "references/rules.md" && file["content"] == "PX73916")
    );
    for invalid in [
        "../escape.md",
        "/absolute.md",
        "folder/../escape.md",
        "folder\\escape.md",
    ] {
        let bytes = archive(&[("SKILL.md", manifest), (invalid, b"PX73916")])?;
        assert!(
            Store::inspect_skill_zip(&bytes).is_err(),
            "路径应被拒绝：{invalid}"
        );
    }
    let bytes = archive(&[
        ("SKILL.md", manifest),
        ("large.md", &vec![b'x'; (256 << 10) + 1]),
    ])?;
    assert!(Store::inspect_skill_zip(&bytes).is_err());
    Ok(())
}

async fn request(
    api: &LocalApi,
    account: &str,
    method: &str,
    path: &str,
    body: Value,
) -> pixoma_studio::Result<Value> {
    api.request(
        account,
        LocalRequest {
            method: method.to_owned(),
            path: path.to_owned(),
            body: Some(body),
        },
    )
    .await
}

#[tokio::test]
async fn personal_skills_keep_immutable_versions_and_validate_package_paths()
-> Result<(), Box<dyn std::error::Error>> {
    let root = PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../.local-cache");
    std::fs::create_dir_all(&root)?;
    let directory = tempfile::Builder::new()
        .prefix("skills-")
        .tempdir_in(root)?;
    let database = directory.path().join("studio.sqlite");
    let store = Store::open(&database).await?;
    let api = LocalApi::new(store.clone());
    let manifest = "---\nname: review-document\ndescription: 检查用户文档\n---\n读取 references/rules.md，按照其中的说明检查文档。";
    let input = json!({
        "name": "review-document", "description": "检查用户文档", "version": "1.0.0", "enabled": true,
        "files": [{ "path": "SKILL.md", "content": manifest }, { "path": "references/rules.md", "content": "保留编号 PX73916。" }],
        "notes": "创建技能",
    });
    let created = request(
        &api,
        "account-a",
        "POST",
        "/api/v1/studio/skills",
        input.clone(),
    )
    .await?;
    let id = created["id"].as_str().ok_or("技能标识缺失")?;
    let path = format!("/api/v1/studio/skills/{id}");
    assert_eq!(created["prompt"], manifest);
    assert_eq!(created["source"], "user");
    assert!(!created["published"].as_bool().ok_or("发布状态缺失")?);
    assert_eq!(
        request(
            &api,
            "account-a",
            "GET",
            &format!("{path}/publications"),
            Value::Null
        )
        .await?,
        json!([])
    );
    assert!(
        request(
            &api,
            "account-b",
            "GET",
            &format!("{path}/publications"),
            Value::Null
        )
        .await
        .is_err()
    );
    assert_eq!(
        request(
            &api,
            "account-a",
            "GET",
            "/api/v1/studio/skills",
            Value::Null
        )
        .await?
        .as_array()
        .ok_or("技能列表缺失")?
        .len(),
        1
    );
    assert!(
        request(&api, "account-b", "GET", &path, Value::Null)
            .await
            .is_err()
    );
    let mut changed = input.clone();
    changed["version"] = json!("1.1.0");
    changed["updated_at"] = created["updated_at"].clone();
    changed["files"][1]["content"] = json!("保留编号 PX84027。");
    let updated = request(&api, "account-a", "PATCH", &path, changed.clone()).await?;
    assert_eq!(updated["version"], "1.1.0");
    assert!(
        request(&api, "account-a", "PATCH", &path, changed)
            .await
            .is_err()
    );
    let original = request(
        &api,
        "account-a",
        "GET",
        &format!("{path}/versions/1.0.0"),
        Value::Null,
    )
    .await?;
    assert_eq!(original["files"][1]["content"], "保留编号 PX73916。");
    let versions = request(
        &api,
        "account-a",
        "GET",
        &format!("{path}/versions"),
        Value::Null,
    )
    .await?;
    assert_eq!(versions.as_array().ok_or("版本列表缺失")?.len(), 2);
    for invalid in [
        "../rules.md",
        "/rules.md",
        "a\\rules.md",
        "a//rules.md",
        "a/./rules.md",
        "C:/rules.md",
    ] {
        let mut unsafe_input = input.clone();
        unsafe_input["files"][1]["path"] = json!(invalid);
        assert!(
            request(
                &api,
                "account-a",
                "POST",
                "/api/v1/studio/skills",
                unsafe_input
            )
            .await
            .is_err(),
            "路径应被拒绝：{invalid}"
        );
    }
    let mut conflict = input.clone();
    conflict["files"]
        .as_array_mut()
        .ok_or("文件列表缺失")?
        .push(json!({ "path": "references", "content": "文件与目录冲突" }));
    assert!(
        request(&api, "account-a", "POST", "/api/v1/studio/skills", conflict)
            .await
            .is_err()
    );
    let mut mismatch = input.clone();
    mismatch["name"] = json!("other-name");
    assert!(
        request(&api, "account-a", "POST", "/api/v1/studio/skills", mismatch)
            .await
            .is_err()
    );
    for field in ["version", "notes"] {
        let mut invalid = input.clone();
        invalid[field] = json!(3);
        assert!(
            request(&api, "account-a", "POST", "/api/v1/studio/skills", invalid)
                .await
                .is_err(),
            "字段类型应被拒绝：{field}"
        );
    }
    let disabled = request(
        &api,
        "account-a",
        "PATCH",
        &format!("{path}/enabled"),
        json!({ "enabled": false }),
    )
    .await?;
    assert_eq!(disabled["enabled"], false);
    store.close().await;
    drop(api);
    let reopened = Store::open(&database).await?;
    let api = LocalApi::new(reopened.clone());
    let saved = request(&api, "account-a", "GET", &path, Value::Null).await?;
    assert_eq!(saved["version"], "1.1.0");
    assert_eq!(saved["enabled"], false);
    assert_eq!(saved["files"][1]["content"], "保留编号 PX84027。");
    reopened.close().await;
    Ok(())
}

struct TestCredential(keyring::Entry);

impl Drop for TestCredential {
    fn drop(&mut self) {
        self.0.delete_credential().expect("删除测试模型凭据失败");
    }
}

#[tokio::test]
#[ignore = "需要 PIXOMA_TEST_MODEL_CONFIG 并连接真实模型服务"]
async fn real_model_uses_the_skill_version_saved_when_the_run_was_created()
-> Result<(), Box<dyn std::error::Error>> {
    let configuration: Value = serde_json::from_str(&std::env::var("PIXOMA_TEST_MODEL_CONFIG")?)?;
    let root = PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../.local-cache");
    std::fs::create_dir_all(&root)?;
    let directory = tempfile::Builder::new()
        .prefix("skill-binding-")
        .tempdir_in(root)?;
    let database = directory.path().join("studio.sqlite");
    let mut store = Store::open(&database).await?;
    let api = LocalApi::new(store.clone());
    let account = "skill-binding";
    let model = request(
        &api,
        account,
        "POST",
        "/api/v1/studio-operations/models",
        configuration,
    )
    .await?;
    let model_id = model["id"].as_str().ok_or("模型标识缺失")?;
    let connection = sqlx::SqlitePool::connect(&format!("sqlite://{}", database.display())).await?;
    let reference: String =
        sqlx::query_scalar("SELECT credential_ref FROM model_configs WHERE account_id=? AND id=?")
            .bind(account)
            .bind(model_id)
            .fetch_one(&connection)
            .await?;
    let _credential = TestCredential(keyring::Entry::new("com.pixoma.studio", &reference)?);
    connection.close().await;
    let expected = uuid::Uuid::new_v4().to_string();
    let replacement = uuid::Uuid::new_v4().to_string();
    let original_content = format!(
        "{}\n审核编号：{expected}",
        "创作资料需要完整读取。\n".repeat(800)
    );
    let mut skill = json!({
        "name": "approval-code", "description": "读取审核编号", "version": "1.0.0", "enabled": true,
        "files": [
            { "path": "SKILL.md", "content": "---\nname: approval-code\ndescription: 读取审核编号\n---\n当用户询问审核编号时，读取 references/code.md，仅回复其中的完整编号。" },
            { "path": "references/code.md", "content": original_content },
        ],
    });
    skill["files"].as_array_mut().ok_or("技能文件列表缺失")?.extend((0..25).map(|index| json!({
        "path": format!("references/notes-{index:02}.md"), "content": format!("创作资料条目 {index}。"),
    })));
    let expected_paths: std::collections::BTreeSet<String> = skill["files"]
        .as_array()
        .ok_or("技能文件列表缺失")?
        .iter()
        .map(|file| {
            file["path"]
                .as_str()
                .ok_or("技能文件路径缺失")
                .map(str::to_owned)
        })
        .collect::<Result<_, _>>()?;
    let created = request(
        &api,
        account,
        "POST",
        "/api/v1/studio/skills",
        skill.clone(),
    )
    .await?;
    let skill_id = created["id"].as_str().ok_or("技能标识缺失")?;
    let session = store.create_session(account, "skills").await?;
    let input = json!({
        "session_id": session.id, "request_id": "skill-binding", "model_config_id": model_id,
        "text": format!("请调用 load_skill，以 path 为 . 完整读取技能 {skill_id} 文件目录的全部分页，再读取 references/code.md 的全部内容，按照所选技能提供审核编号。必须实际调用工具并继续读取所有 next。"), "permission_mode": "auto_approve", "skill_ids": [skill_id],
    });
    let queued = store.submit_message(account, input.clone()).await?;
    let run_id = queued["run"]["id"].as_str().ok_or("运行标识缺失")?;
    assert_eq!(queued["message"]["parts"][0]["version"], "1.0.0");
    let connection = sqlx::SqlitePool::connect(&format!("sqlite://{}", database.display())).await?;
    let context: String =
        sqlx::query_scalar("SELECT context_snapshot FROM runs WHERE account_id=? AND id=?")
            .bind(account)
            .bind(run_id)
            .fetch_one(&connection)
            .await?;
    connection.close().await;
    assert!(
        context.contains(&expected),
        "运行上下文缺少指定技能版本的资料"
    );
    skill["version"] = json!("1.1.0");
    skill["updated_at"] = created["updated_at"].clone();
    skill["files"][1]["content"] = json!(replacement);
    request(
        &api,
        account,
        "PATCH",
        &format!("/api/v1/studio/skills/{skill_id}"),
        skill,
    )
    .await?;
    request(
        &api,
        account,
        "PATCH",
        &format!("/api/v1/studio/skills/{skill_id}/enabled"),
        json!({ "enabled": false }),
    )
    .await?;
    assert_eq!(
        store.submit_message(account, input).await?["run"]["id"],
        run_id
    );
    let denied = json!({
        "session_id": session.id, "request_id": "disabled-skill", "model_config_id": model_id,
        "text": "读取编号", "permission_mode": "auto_approve", "skill_ids": [skill_id],
    });
    assert!(store.submit_message(account, denied).await.is_err());
    drop(api);
    store.close().await;
    store = Store::open(&database).await?;
    let result = pixoma_studio::AgentRuntime::new(store.clone())
        .execute(account, run_id)
        .await?;
    assert_eq!(result["status"], "succeeded", "{}", result["error_message"]);
    let events = store.run_events_after(account, run_id, 0).await?;
    assert!(
        events.iter().any(|event| event["type"] == "TOOL_CALL_START"
            && event["payload"]["toolCallName"] == "load_skill"),
        "模型没有实际读取技能文件"
    );
    assert!(
        events
            .iter()
            .filter(|event| event["type"] == "TOOL_CALL_START"
                && event["payload"]["toolCallName"] == "load_skill")
            .count()
            >= 2,
        "模型没有继续读取技能文件的后续内容"
    );
    let output: String = events
        .iter()
        .filter(|event| event["type"] == "TEXT_MESSAGE_CONTENT")
        .filter_map(|event| event["payload"]["delta"].as_str())
        .collect();
    assert!(
        output.contains(&expected),
        "模型没有使用运行创建时保存的技能资料"
    );
    assert!(!output.contains(&replacement));
    let connection = sqlx::SqlitePool::connect(&format!("sqlite://{}", database.display())).await?;
    let results: Vec<String> = sqlx::query_scalar("SELECT result FROM tool_calls WHERE account_id=? AND run_id=? AND status='succeeded' ORDER BY turn,part")
        .bind(account).bind(run_id).fetch_all(&connection).await?;
    connection.close().await;
    let mut pages = std::collections::BTreeMap::new();
    let mut directories = std::collections::BTreeMap::new();
    for result in results {
        let rig::message::UserContent::ToolResult(result) = serde_json::from_str(&result)? else {
            return Err("工具结果类型无效".into());
        };
        if result.name.as_str() != "load_skill" {
            continue;
        }
        for content in result.content {
            let value = content.as_json().ok_or("技能工具缺少 JSON 内容")?;
            assert_eq!(value["version"], "1.0.0");
            if value["path"] == "references/code.md" {
                pages.insert(
                    value["offset"].as_u64().ok_or("读取位置缺失")?,
                    value["content"]
                        .as_str()
                        .ok_or("技能文件内容缺失")?
                        .to_owned(),
                );
            } else if value["path"] == "." {
                directories.insert(
                    value["offset"].as_u64().ok_or("目录读取位置缺失")?,
                    serde_json::from_value::<Vec<String>>(value["readable_files"].clone())?,
                );
            }
        }
    }
    assert!(pages.len() >= 2);
    assert_eq!(
        pages.values().cloned().collect::<String>(),
        original_content
    );
    assert!(directories.len() >= 2);
    let listed: std::collections::BTreeSet<String> = directories.into_values().flatten().collect();
    assert_eq!(listed, expected_paths);
    store.close().await;
    Ok(())
}
