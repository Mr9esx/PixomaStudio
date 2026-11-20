use pixoma_studio::Store;
use std::path::PathBuf;

fn directory() -> Result<tempfile::TempDir, std::io::Error> {
    let root = PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../.local-cache");
    std::fs::create_dir_all(&root)?;
    tempfile::Builder::new().prefix("assets-").tempdir_in(root)
}

#[tokio::test]
async fn uploads_reuse_project_content_and_preserve_distinct_files() -> Result<(), Box<dyn std::error::Error>> {
    let directory = directory()?;
    let store = Store::open(&directory.path().join("studio.sqlite")).await?;
    let project = store.create_project("导入检查").await?;
    let other = store.create_project("其它项目").await?;
    let first = store.upload_asset(serde_json::json!({"project_id":project.id,"file_name":"剧本.txt","request_id":"first"}), "清晨茶园".as_bytes()).await?;
    let repeated = store.upload_asset(serde_json::json!({"project_id":project.id,"file_name":"剧本副本.txt","request_id":"repeated"}), "清晨茶园".as_bytes()).await?;
    assert_eq!(first["already_exists"], false);
    assert_eq!(repeated["already_exists"], true);
    assert_eq!(repeated["id"], first["id"]);
    assert_eq!(repeated["project_asset_id"], first["project_asset_id"]);
    assert_eq!(repeated["versions"], first["versions"]);
    assert_eq!(repeated["name"], "剧本.txt");
    let changed = store.upload_asset(serde_json::json!({"project_id":project.id,"file_name":"剧本.txt","request_id":"changed"}), "茶杯近景".as_bytes()).await?;
    assert_ne!(changed["id"], first["id"]);
    let separate = store.upload_asset(serde_json::json!({"project_id":other.id,"file_name":"剧本.txt","request_id":"separate"}), "清晨茶园".as_bytes()).await?;
    assert_ne!(separate["id"], first["id"]);
    assert!(store.upload_asset(serde_json::json!({"project_id":project.id,"file_name":"剧本.txt","request_id":"first"}), "茶杯近景".as_bytes()).await.is_err());
    let history_session = store.create_session("version-history").await?;
    store.move_session(&history_session.id, Some(&project.id)).await?;
    let manual = store.create_text_asset(&history_session.id, "手写剧本", "手写原文", "manual").await?;
    store.update_text_asset(&manual.id, "更新后的剧本", "edit").await?;
    let old_version = store.upload_asset(serde_json::json!({"project_id":project.id,"file_name":"旧剧本.md","request_id":"old-version"}), "手写原文".as_bytes()).await?;
    assert_eq!(old_version["id"], manual.id);
    assert_eq!(old_version["current_version"], 1);
    assert_eq!(old_version["versions"][0]["id"], manual.versions[0].id);
    let replay = store.upload_asset(serde_json::json!({"project_id":project.id,"file_name":"旧剧本.md","request_id":"old-version"}), "手写原文".as_bytes()).await?;
    assert_eq!(replay["current_version"], 1);
    let current = store.upload_asset(serde_json::json!({"project_id":project.id,"file_name":"新版剧本.md","request_id":"current-version"}), "更新后的剧本".as_bytes()).await?;
    assert_eq!(current["id"], manual.id);
    assert_eq!(current["current_version"], 2);
    let session = store.create_session("duplicate-session").await?;
    store.move_session(&session.id, Some(&project.id)).await?;
    let imported = store.upload_asset(serde_json::json!({"session_id":session.id,"file_name":"会话剧本.txt","request_id":"session-upload"}), "清晨茶园".as_bytes()).await?;
    assert_eq!(imported["id"], first["id"]);
    assert_eq!(imported["current_version"], 1);
    assert_eq!(store.list_session_assets(&session.id).await?.len(), 1);
    store.close().await;
    Ok(())
}

#[tokio::test]
async fn uploads_validate_media_types_before_reusing_content() -> Result<(), Box<dyn std::error::Error>> {
    let directory = directory()?;
    let store = Store::open(&directory.path().join("studio.sqlite")).await?;
    store.upload_asset(serde_json::json!({"project_id":"","file_name":"说明.txt","request_id":"text"}), b"unreadable audio").await?;
    assert!(store.upload_asset(serde_json::json!({"project_id":"","file_name":"损坏音频.wav","request_id":"audio"}), b"unreadable audio").await.is_err());
    store.close().await;
    Ok(())
}

#[tokio::test]
async fn concurrent_uploads_reuse_one_project_asset() -> Result<(), Box<dyn std::error::Error>> {
    let directory = directory()?;
    let store = Store::open(&directory.path().join("studio.sqlite")).await?;
    let first = serde_json::json!({"project_id":"","file_name":"清晨.txt","request_id":"first"});
    let second = serde_json::json!({"project_id":"","file_name":"清晨副本.txt","request_id":"second"});
    let (first, second) = tokio::join!(store.upload_asset(first, "同一正文".as_bytes()), store.upload_asset(second, "同一正文".as_bytes()));
    let (first, second) = (first?, second?);
    assert_eq!(first["id"], second["id"]);
    assert_eq!(first["project_asset_id"], second["project_asset_id"]);
    assert_ne!(first["already_exists"], second["already_exists"]);
    assert_eq!(first["versions"].as_array().ok_or("资产版本缺失")?.len(), 1);
    store.close().await;
    Ok(())
}

#[tokio::test]
async fn audio_import_preserves_duration_after_reopening() -> Result<(), Box<dyn std::error::Error>> {
    let directory = directory()?;
    let source = directory.path().join("tone.wav");
    let samples = directory.path().join("tone.pcm");
    let pcm: Vec<u8> = (0..60_000).flat_map(|sample| {
        let value = (f64::sin(sample as f64 * 440.0 * std::f64::consts::TAU / 48_000.0) * 8_000.0) as i16;
        value.to_le_bytes()
    }).collect();
    std::fs::write(&samples, pcm)?;
    let output = tokio::process::Command::new(env!("PIXOMA_FFMPEG_PATH"))
        .args(["-nostdin", "-v", "error", "-f", "s16le", "-ar", "48000", "-ac", "1", "-i"])
        .arg(samples).args(["-c:a", "pcm_s16le"])
        .arg(&source).output().await?;
    assert!(output.status.success(), "{}", String::from_utf8_lossy(&output.stderr));
    let bytes = std::fs::read(source)?;
    let database = directory.path().join("studio.sqlite");
    let store = Store::open(&database).await?;
    let asset = store.create_asset_bytes(None, "配乐.wav", &bytes, "user", "audio-duration").await?;
    assert_eq!(asset.kind, "audio");
    assert_eq!(asset.versions[0].metadata["duration"], 1.25);
    assert!(asset.versions[0].metadata.get("fps").is_none());
    assert_eq!(store.read_asset(&asset.id, &asset.versions[0].id).await?, bytes);
    store.close().await;
    let connection = sqlx::SqlitePool::connect_with(sqlx::sqlite::SqliteConnectOptions::new().filename(&database)).await?;
    sqlx::query("UPDATE asset_versions SET metadata=json_remove(metadata,'$.duration') WHERE id=?")
        .bind(&asset.versions[0].id).execute(&connection).await?;
    let reopened = Store::open(&database).await?;
    let restored = reopened.get_asset(&asset.id).await?;
    assert_eq!(restored.versions[0].metadata, asset.versions[0].metadata);
    assert_eq!(restored.current_version, 1);
    let persisted: f64 = sqlx::query_scalar("SELECT json_extract(metadata,'$.duration') FROM asset_versions WHERE id=?")
        .bind(&asset.versions[0].id).fetch_one(&connection).await?;
    assert_eq!(persisted, 1.25);
    connection.close().await;
    reopened.close().await;
    Ok(())
}

#[tokio::test]
async fn audio_import_rejects_unreadable_media() -> Result<(), Box<dyn std::error::Error>> {
    let directory = directory()?;
    let store = Store::open(&directory.path().join("studio.sqlite")).await?;
    let result = store.create_asset_bytes(None, "损坏音频.wav", b"unreadable audio", "user", "invalid-audio").await;
    assert!(result.is_err(), "无法读取的音频不应成为可用资产");
    store.close().await;
    Ok(())
}

#[tokio::test]
async fn assets_and_library_records_use_business_identifiers()
-> Result<(), Box<dyn std::error::Error>> {
    let directory = directory()?;
    let database = directory.path().join("studio.sqlite");
    let store = Store::open(&database).await?;
    let connection = sqlx::SqlitePool::connect(&format!("sqlite://{}", database.display())).await?;
    for table in [
        "blobs",
        "assets",
        "asset_versions",
        "asset_usages",
        "asset_operations",
        "project_assets",
        "project_asset_tags",
        "categories",
        "tags",
    ] {
        let columns: Vec<String> = sqlx::query_scalar("SELECT name FROM pragma_table_info(?)")
            .bind(table)
            .fetch_all(&connection)
            .await?;
        assert!(
            !columns.iter().any(|column| column == "account_id"),
            "{table}"
        );
    }
    let bytes = include_bytes!("data/klein9b-api.json");
    let asset = store
        .create_asset_bytes(None, "工作流.json", bytes, "workflow", "workflow-output")
        .await?;
    let id = asset.id.clone();
    let version = asset.versions[0].id.clone();
    assert_eq!(store.read_asset(&id, &version).await?, bytes);
    connection.close().await;
    store.close().await;
    let reopened = Store::open(&database).await?;
    assert_eq!(reopened.read_asset(&id, &version).await?, bytes);
    reopened.close().await;
    Ok(())
}

#[tokio::test]
async fn video_posters_and_palettes_keep_actual_frames_after_reopening()
-> Result<(), Box<dyn std::error::Error>> {
    let directory = directory()?;
    let database = directory.path().join("studio.sqlite");
    let store = Store::open(&database).await?;
    let session = store.create_session("video").await?;
    let bytes = include_bytes!("fixtures/red.mp4");
    let asset = store
        .create_asset_bytes(Some(&session.id), "red.mp4", bytes, "user", "video")
        .await?;
    let document = serde_json::to_value(&asset)?;
    assert_eq!(asset.kind, "video");
    assert!(
        document["versions"][0]["poster_url"]
            .as_str()
            .is_some_and(|url| url.ends_with("/poster"))
    );
    assert_eq!(asset.versions[0].width_px, Some(80));
    assert_eq!(asset.versions[0].height_px, Some(40));
    assert_eq!(asset.versions[0].metadata["duration"], 1.0);
    assert_eq!(asset.versions[0].metadata["fps"], 1.0);
    assert_eq!(document["versions"][0]["palette"]["status"], "ready");
    let poster = store
        .asset_poster_response(&asset.id, &asset.versions[0].id, "GET")
        .await?;
    assert_eq!(poster.mime_type, "image/png");
    let frame = image::load_from_memory(&poster.body)?.to_rgb8();
    assert_eq!(frame.dimensions(), (80, 40));
    let pixel = frame.get_pixel(40, 20);
    assert!(pixel[0] > 240 && pixel[1] < 16 && pixel[2] < 16);
    let color = document["versions"][0]["palette"]["colors"][0]["hex"]
        .as_str()
        .ok_or("视频调色盘颜色缺失")?
        .to_owned();
    assert!(color.starts_with("#f") && color.ends_with("0000"));
    assert_eq!(
        document["versions"][0]["palette"]["colors"][0]["ratio"],
        1.0
    );
    let head = store
        .asset_poster_response(&asset.id, &asset.versions[0].id, "HEAD")
        .await?;
    assert_eq!(head.content_length, poster.body.len() as u64);
    assert!(head.body.is_empty());
    assert!(
        store
            .asset_poster_response("missing-asset", &asset.versions[0].id, "GET")
            .await
            .is_err()
    );
    assert!(
        store
            .asset_poster_response(&asset.id, &asset.versions[0].id, "POST")
            .await
            .is_err()
    );
    assert!(
        store
            .asset_poster_response(&asset.id, "invalid-version", "GET")
            .await
            .is_err()
    );
    store
        .retry_asset_palette(&asset.id, &asset.versions[0].id)
        .await?;
    let document = serde_json::to_value(store.get_asset(&asset.id).await?)?;
    assert_eq!(
        document["versions"][0]["palette"]["colors"][0]["hex"],
        color
    );
    assert!(
        store
            .create_asset_bytes(
                Some(&session.id),
                "invalid.mp4",
                b"invalid video",
                "user",
                "invalid-video"
            )
            .await
            .is_err()
    );
    assert_eq!(
        store.read_asset(&asset.id, &asset.versions[0].id).await?,
        bytes
    );
    store.close().await;
    let reopened = Store::open(&database).await?;
    assert_eq!(
        reopened
            .asset_poster_response(&asset.id, &asset.versions[0].id, "GET")
            .await?
            .body,
        poster.body
    );
    let saved = serde_json::to_value(reopened.get_asset(&asset.id).await?)?;
    assert_eq!(
        saved["versions"][0]["poster_url"],
        document["versions"][0]["poster_url"]
    );
    assert_eq!(
        saved["versions"][0]["palette"],
        document["versions"][0]["palette"]
    );
    assert!(reopened.get_asset("missing-asset").await.is_err());
    reopened.close().await;
    Ok(())
}

#[tokio::test]
async fn reopening_cleans_unreferenced_files_and_preserves_all_asset_versions()
-> Result<(), Box<dyn std::error::Error>> {
    let directory = directory()?;
    let database = directory.path().join("studio.sqlite");
    let store = Store::open(&database).await?;
    let session = store.create_session("清理检查").await?;
    let asset = store
        .create_text_asset(&session.id, "正文", "原始正文", "create")
        .await?;
    let asset = store
        .update_text_asset(&asset.id, "更新正文", "update")
        .await?;
    let parent = store
        .asset_path(&asset.id, &asset.versions[0].id)
        .await?
        .parent()
        .ok_or("资产目录缺失")?
        .to_path_buf();
    let connection = sqlx::SqlitePool::connect(&format!("sqlite://{}", database.display())).await?;
    sqlx::query("CREATE TRIGGER reject_asset BEFORE INSERT ON assets BEGIN SELECT RAISE(ABORT,'资产写入检查'); END")
        .execute(&connection).await?;
    assert!(
        store
            .create_text_asset(&session.id, "中断正文", "未引用正文", "interrupted")
            .await
            .is_err()
    );
    sqlx::query("DROP TRIGGER reject_asset")
        .execute(&connection)
        .await?;
    let files = std::fs::read_dir(&parent)?.collect::<Result<Vec<_>, _>>()?;
    assert_eq!(files.len(), 3);
    let pending = parent.join(format!("{}.pending", uuid::Uuid::new_v4()));
    std::fs::write(&pending, b"incomplete")?;
    let note = parent.join("notes.txt");
    std::fs::write(&note, b"notes")?;
    connection.close().await;
    store.close().await;
    let reopened = Store::open(&database).await?;
    assert!(!pending.exists());
    assert_eq!(std::fs::read_dir(&parent)?.count(), 3);
    assert_eq!(std::fs::read(&note)?, b"notes");
    for (version, content) in asset.versions.iter().zip(["原始正文", "更新正文"]) {
        assert_eq!(
            reopened.read_asset(&asset.id, &version.id).await?,
            content.as_bytes()
        );
    }
    let (created, opened) = tokio::join!(
        reopened.create_text_asset(&session.id, "并发正文", "并发保存内容", "concurrent"),
        Store::open(&database),
    );
    let created = created?;
    let opened = opened?;
    assert_eq!(
        opened
            .read_asset(&created.id, &created.versions[0].id)
            .await?,
        "并发保存内容".as_bytes()
    );
    opened.close().await;
    reopened.close().await;
    Ok(())
}

#[cfg(unix)]
#[tokio::test]
async fn asset_writes_reject_directory_symlinks() -> Result<(), Box<dyn std::error::Error>> {
    let directory = directory()?;
    let database = directory.path().join("studio.sqlite");
    let store = Store::open(&database).await?;
    let first = store.create_session("first").await?;
    let second = store.create_session("second").await?;
    let asset = store
        .create_text_asset(&second.id, "正文", "保留正文", "create")
        .await?;
    let parent = store
        .asset_path(&asset.id, &asset.versions[0].id)
        .await?
        .parent()
        .ok_or("资产目录缺失")?
        .to_path_buf();
    let preserved = directory.path().join("preserved-assets");
    std::fs::rename(&parent, &preserved)?;
    std::os::unix::fs::symlink(&preserved, &parent)?;
    assert!(
        store
            .create_text_asset(&first.id, "正文", "其他正文", "create-other")
            .await
            .is_err()
    );
    store.close().await;
    std::fs::remove_file(&parent)?;
    std::fs::rename(preserved, &parent)?;
    let reopened = Store::open(&database).await?;
    assert_eq!(
        reopened
            .read_asset(&asset.id, &asset.versions[0].id)
            .await?,
        "保留正文".as_bytes()
    );
    reopened.close().await;
    Ok(())
}

#[tokio::test]
async fn image_palettes_follow_actual_pixels_and_survive_reopening()
-> Result<(), Box<dyn std::error::Error>> {
    use pixoma_studio::{LocalApi, LocalRequest};
    use serde_json::{Value, json};
    let directory = directory()?;
    let database = directory.path().join("studio.sqlite");
    let store = Store::open(&database).await?;
    let session = store.create_session("palette").await?;
    let pixels = image::RgbImage::from_fn(80, 40, |x, _| {
        if x < 60 {
            image::Rgb([255, 0, 0])
        } else {
            image::Rgb([0, 0, 255])
        }
    });
    let source = directory.path().join("colors.png");
    pixels.save(&source)?;
    let asset = store.import_asset(&session.id, &source).await?;
    let document = serde_json::to_value(&asset)?;
    let palette = &document["versions"][0]["palette"];
    assert_eq!(palette["status"], "ready");
    let colors = palette["colors"].as_array().ok_or("调色盘颜色缺失")?;
    assert_eq!(colors.len(), 2);
    assert_eq!(colors[0]["hex"], "#ff0000");
    assert_eq!(colors[1]["hex"], "#0000ff");
    assert!((colors[0]["ratio"].as_f64().ok_or("颜色比例缺失")? - 0.75).abs() < 0.001);
    assert!((colors[1]["ratio"].as_f64().ok_or("颜色比例缺失")? - 0.25).abs() < 0.001);
    let path = format!(
        "/api/v1/studio/assets/{}/versions/{}/palette/retry",
        asset.id, asset.versions[0].id
    );
    let api = LocalApi::new(store.clone());
    assert!(
        api.request(
            LocalRequest {
                method: "POST".to_owned(),
                path: "/api/v1/studio/assets/missing-asset/versions/missing-version/palette/retry"
                    .to_owned(),
                body: None
            }
        )
        .await
        .is_err()
    );
    api.request(
        LocalRequest {
            method: "POST".to_owned(),
            path,
            body: None,
        },
    )
    .await?;
    assert_eq!(
        store.read_asset(&asset.id, &asset.versions[0].id).await?,
        std::fs::read(source)?
    );
    let white = directory.path().join("white.png");
    image::RgbImage::from_pixel(8, 8, image::Rgb([255, 255, 255])).save(&white)?;
    let white = store.import_asset(&session.id, &white).await?;
    let white: Value = serde_json::to_value(white)?;
    assert_eq!(
        white["versions"][0]["palette"]["colors"],
        json!([{ "hex": "#ffffff", "ratio": 1.0 }])
    );
    store.close().await;
    let reopened = Store::open(&database).await?;
    let saved = serde_json::to_value(reopened.get_asset(&asset.id).await?)?;
    assert_eq!(saved["versions"][0]["palette"]["colors"], palette["colors"]);
    reopened.close().await;
    Ok(())
}

#[tokio::test]
async fn expert_avatar_uploads_validate_actual_images_and_keep_owned_bytes()
-> Result<(), Box<dyn std::error::Error>> {
    use serde_json::json;
    let directory = directory()?;
    let database = directory.path().join("studio.sqlite");
    let store = Store::open(&database).await?;
    let metadata = json!({ "file_name": "头像.png", "project_id": "", "request_id": "avatar", "purpose": "expert_avatar" });
    let bytes = include_bytes!("../../public/images/pixoma-ghost.png");
    let avatar = store.upload_asset(metadata.clone(), bytes).await?;
    assert_eq!(avatar["kind"], "image");
    assert_eq!(avatar["versions"][0]["mime_type"], "image/png");
    assert_eq!(
        store.upload_asset(metadata, bytes).await?["id"],
        avatar["id"]
    );
    assert!(store.upload_asset(json!({ "file_name": "头像.png", "project_id": "", "request_id": "invalid-avatar", "purpose": "expert_avatar" }), b"ordinary text").await.is_err());
    assert!(store.upload_asset(json!({ "file_name": "头像.png", "project_id": "", "request_id": "large-avatar", "purpose": "expert_avatar" }), &vec![0_u8; (5 << 20) + 1]).await.is_err());
    assert_eq!(store.library_assets(&Default::default()).await?["total"], 1);
    store.close().await;
    let store = Store::open(&database).await?;
    let id = avatar["id"].as_str().ok_or("头像资产标识缺失")?;
    let version = avatar["versions"][0]["id"]
        .as_str()
        .ok_or("头像版本标识缺失")?;
    assert_eq!(store.read_asset(id, version).await?, bytes);
    assert!(store.read_asset(id, "missing-version").await.is_err());
    store.close().await;
    Ok(())
}

#[tokio::test]
async fn exporting_fixed_asset_versions_preserves_contents_and_destination_failures()
-> Result<(), Box<dyn std::error::Error>> {
    let directory = directory()?;
    let store = Store::open(&directory.path().join("studio.sqlite")).await?;
    let session = store.create_session("fixed-export-session").await?;
    let asset = store.create_text_asset(&session.id, "剧本.md", "最初的内容", "fixed-export-source").await?;
    let first = &asset.versions[0].id;
    let changed = store.update_text_asset(&asset.id, "更新的内容", "fixed-export-change").await?;
    let second = &changed.versions.iter().find(|version| version.version == 2).ok_or("更新版本缺失")?.id;
    let library = store.library_assets(&Default::default()).await?;
    let placement = library["items"].as_array().ok_or("资产列表缺失")?.iter().find(|item| item["asset_id"] == asset.id).ok_or("项目资产缺失")?;
    store.patch_project_asset(placement["id"].as_str().ok_or("项目资产标识缺失")?, &serde_json::json!({"archived":true})).await?;
    assert!(store.library_assets(&Default::default()).await?["items"].as_array().ok_or("资产列表缺失")?.iter().all(|item| item["asset_id"] != asset.id));
    let destination = directory.path().join("保存的剧本.md");
    std::fs::write(&destination, "已有文件")?;
    let exported = store.export_asset_version(&asset.id, first).await?;
    assert_eq!(exported.name, "剧本.md");
    exported.save_to(&destination)?;
    assert_eq!(std::fs::read_to_string(&destination)?, "最初的内容");
    store.export_asset_version(&asset.id, second).await?.save_to(&destination)?;
    assert_eq!(std::fs::read_to_string(&destination)?, "更新的内容");
    assert!(store.export_asset_version(&asset.id, "missing-version").await.is_err());
    let other = store.create_text_asset(&session.id, "其它剧本.md", "其它内容", "fixed-export-other").await?;
    assert!(store.export_asset_version(&other.id, first).await.is_err());
    let occupied = directory.path().join("existing-directory");
    std::fs::create_dir(&occupied)?;
    std::fs::write(occupied.join("保留.txt"), "保留内容")?;
    assert!(store.export_asset_version(&asset.id, first).await?.save_to(&occupied).is_err());
    assert_eq!(std::fs::read_to_string(occupied.join("保留.txt"))?, "保留内容");
    assert_eq!(std::fs::read_to_string(&destination)?, "更新的内容");
    store.close().await;
    Ok(())
}

#[tokio::test]
async fn uploads_keep_project_scope_file_dates_and_export_selected_versions()
-> Result<(), Box<dyn std::error::Error>> {
    use serde_json::json;
    use std::io::Read;
    let directory = directory()?;
    let store = Store::open(&directory.path().join("studio.sqlite")).await?;
    let project = store.create_project("素材").await?;
    let metadata = json!({ "file_name": "正文.txt", "project_id": project.id, "request_id": "upload", "source_modified_at": "2026-10-17T10:00:00Z" });
    let first = store
        .upload_asset(metadata.clone(), "正文".as_bytes())
        .await?;
    let first_id = first["project_asset_id"]
        .as_str()
        .ok_or("项目资产标识缺失")?;
    assert_eq!(
        first["versions"][0]["source_modified_at"],
        "2026-10-17T10:00:00Z"
    );
    assert_eq!(first["versions"][0]["content_origin"], "upload");
    assert_eq!(
        store.project_asset(first_id).await?["project_id"],
        project.id
    );
    assert_eq!(
        store.upload_asset(metadata, "正文".as_bytes()).await?["id"],
        first["id"]
    );
    assert!(store.upload_asset(json!({ "file_name": "正文.txt", "project_id": "missing-project", "request_id": "upload-missing-project" }), "正文".as_bytes()).await.is_err());
    let mut exported = store.export_assets(&[first_id.to_owned()]).await?;
    assert_eq!(exported.name, "正文.txt");
    let mut contents = Vec::new();
    exported.file.read_to_end(&mut contents)?;
    assert_eq!(contents, "正文".as_bytes());
    let second = store.upload_asset(json!({ "file_name": "../正文.txt", "project_id": project.id, "request_id": "upload-second" }), "其他正文".as_bytes()).await?;
    let second_id = second["project_asset_id"]
        .as_str()
        .ok_or("项目资产标识缺失")?;
    let exported = store
        .export_assets(&[first_id.to_owned(), second_id.to_owned()])
        .await?;
    let mut archive = zip::ZipArchive::new(exported.file)?;
    assert_eq!(archive.len(), 2);
    for index in 0..2 {
        let mut file = archive.by_index(index)?;
        assert!(file.enclosed_name().is_some());
        let mut content = String::new();
        file.read_to_string(&mut content)?;
        assert_eq!(content, if index == 0 { "正文" } else { "其他正文" });
    }
    assert!(
        store
            .export_assets(&["missing-placement".to_owned()])
            .await
            .is_err()
    );
    store.close().await;
    Ok(())
}

#[tokio::test]
async fn imported_files_are_owned_copies_and_versions_match_their_assets()
-> Result<(), Box<dyn std::error::Error>> {
    let directory = directory()?;
    let input = directory.path().join("input.png");
    let original = include_bytes!("../../public/images/pixoma-ghost.png");
    std::fs::write(&input, original)?;
    let path = directory.path().join("studio.sqlite");
    let store = Store::open(&path).await?;
    let session = store.create_session("session-a").await?;
    let asset = store.import_asset(&session.id, &input).await?;
    assert_eq!(asset.kind, "image");
    assert_eq!(asset.versions[0].mime_type, "image/png");
    assert_eq!(asset.versions[0].width_px, Some(512));
    std::fs::remove_file(input)?;
    assert_eq!(
        store.read_asset(&asset.id, &asset.versions[0].id).await?,
        original
    );
    assert!(
        store
            .read_asset("missing-asset", &asset.versions[0].id)
            .await
            .is_err()
    );
    assert!(
        store
            .read_asset(&asset.id, "../../studio.sqlite")
            .await
            .is_err()
    );
    store.close().await;
    let reopened = Store::open(&path).await?;
    assert_eq!(
        reopened
            .read_asset(&asset.id, &asset.versions[0].id)
            .await?,
        original
    );
    reopened.close().await;
    Ok(())
}

#[tokio::test]
async fn text_updates_keep_previous_versions_and_repeated_requests_do_not_duplicate()
-> Result<(), Box<dyn std::error::Error>> {
    let directory = directory()?;
    let store = Store::open(&directory.path().join("studio.sqlite")).await?;
    let session = store.create_session("session-a").await?;
    let first = store
        .create_text_asset(&session.id, "脚本.md", "第一段正文", "create-a")
        .await?;
    assert_eq!(first.versions[0].content_origin, "manual");
    let repeated = store
        .create_text_asset(&session.id, "脚本.md", "第一段正文", "create-a")
        .await?;
    assert_eq!(first.id, repeated.id);
    let edited = store
        .update_text_asset(&first.id, "第二段正文", "update-a")
        .await?;
    assert_eq!(edited.current_version, 2);
    assert_eq!(edited.versions.len(), 2);
    assert_eq!(edited.versions[1].content_origin, "manual");
    assert_eq!(
        store.read_asset(&first.id, &first.versions[0].id).await?,
        "第一段正文".as_bytes()
    );
    assert_eq!(
        store.read_asset(&first.id, &edited.versions[1].id).await?,
        "第二段正文".as_bytes()
    );
    let second = store
        .update_text_asset(&first.id, "第二段正文", "update-a")
        .await?;
    assert_eq!(second.current_version, 2);
    assert!(
        store
            .update_text_asset("missing-asset", "其他内容", "update-b")
            .await
            .is_err()
    );
    store.close().await;
    Ok(())
}

#[tokio::test]
async fn manual_documents_validate_content_and_keep_markdown_names()
-> Result<(), Box<dyn std::error::Error>> {
    let directory = directory()?;
    let store = Store::open(&directory.path().join("studio.sqlite")).await?;
    let session = store.create_session("session").await?;
    let asset = store
        .create_text_asset(&session.id, "正文", "正文", "document")
        .await?;
    assert_eq!(asset.name, "正文.md");
    assert!(
        store
            .create_text_asset(&session.id, "空白", "   ", "empty")
            .await
            .is_err()
    );
    assert!(
        store
            .update_text_asset(&asset.id, "   ", "empty-update")
            .await
            .is_err()
    );
    let upload = store.upload_asset(serde_json::json!({ "session_id": session.id, "file_name": "文本.txt", "request_id": "upload" }), b"text").await?;
    assert!(
        store
            .update_text_asset(
                upload["id"].as_str().ok_or("资产标识缺失")?,
                "更新",
                "update-upload"
            )
            .await
            .is_err()
    );
    store.close().await;
    Ok(())
}

#[tokio::test]
async fn concurrent_asset_requests_share_one_asset_and_one_new_version()
-> Result<(), Box<dyn std::error::Error>> {
    let directory = directory()?;
    let store = Store::open(&directory.path().join("studio.sqlite")).await?;
    let session = store.create_session("session").await?;
    let (left, right) = tokio::join!(
        store.create_text_asset(&session.id, "正文.md", "正文", "create"),
        store.create_text_asset(&session.id, "正文.md", "正文", "create"),
    );
    let left = left?;
    assert_eq!(left.id, right?.id);
    let (left, right) = tokio::join!(
        store.update_text_asset(&left.id, "更新正文", "update"),
        store.update_text_asset(&left.id, "更新正文", "update"),
    );
    assert_eq!(left?.current_version, 2);
    assert_eq!(right?.current_version, 2);
    assert_eq!(store.list_session_assets(&session.id).await?.len(), 1);
    store.close().await;
    Ok(())
}

#[tokio::test]
async fn asset_references_require_existing_sessions_and_keep_the_selected_version()
-> Result<(), Box<dyn std::error::Error>> {
    let directory = directory()?;
    let store = Store::open(&directory.path().join("studio.sqlite")).await?;
    let source = store.create_session("source").await?;
    let target = store.create_session("target").await?;
    let project = store.create_project("引用项目").await?;
    store.move_session(&target.id, Some(&project.id)).await?;
    let asset = store
        .create_text_asset(&source.id, "文档.md", "正文", "create")
        .await?;
    assert!(
        store
            .reference_asset(
                "missing-session",
                &asset.id,
                &asset.versions[0].id,
                "reference-b"
            )
            .await
            .is_err()
    );
    store
        .reference_asset(&target.id, &asset.id, &asset.versions[0].id, "reference-a")
        .await?;
    let views = store.list_session_assets(&target.id).await?;
    assert_eq!(views.len(), 1);
    assert_eq!(views[0].id, asset.id);
    assert_eq!(views[0].versions[0].id, asset.versions[0].id);
    let placements = store
        .library_assets(&pixoma_studio::LibraryQuery {
            project_id: project.id,
            ..Default::default()
        })
        .await?;
    assert_eq!(placements["total"], 1);
    assert_eq!(placements["items"][0]["asset_id"], asset.id);
    assert!(
        store
            .reference_asset(&source.id, &asset.id, &asset.versions[0].id, "reference-a")
            .await
            .is_err()
    );
    store.close().await;
    Ok(())
}
#[tokio::test]
async fn video_frame_extracts_actual_position_and_preserves_fixed_source() -> Result<(), Box<dyn std::error::Error>> {
    let root = std::path::PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../.local-cache");
    std::fs::create_dir_all(&root)?;
    let directory = tempfile::Builder::new().prefix("video-frame-position-").tempdir_in(root)?;
    let store = pixoma_studio::Store::open(&directory.path().join("studio.sqlite")).await?;
    let session = store.create_session("frame-owner").await?;
    let video = store.create_asset_bytes(Some(&session.id),"茶园.mp4",include_bytes!("fixtures/red.mp4"),"user","frame-source").await?;
    let version_id = &video.versions[0].id;
    let input = serde_json::json!({"session_id":session.id,"frame":10,"fps":30});
    let frame = store.create_video_frame(&video.id,version_id,input.clone()).await?;
    assert_eq!(frame.kind,"image");
    let version = &frame.versions[0];
    assert_eq!(version.metadata["source_video"],serde_json::json!({"asset_id":video.id,"version_id":version_id,"frame":10,"fps":30}));
    let decoded = image::load_from_memory(&store.read_asset(&frame.id,&version.id).await?)?.to_rgb8();
    let pixel = decoded.get_pixel(0,0);
    assert!(pixel[0]>200 && pixel[1]<30 && pixel[2]<30);
    assert_eq!(store.create_video_frame(&video.id,version_id,input).await?.id,frame.id);
    let another = store.create_video_frame(&video.id,version_id,serde_json::json!({"session_id":session.id,"frame":11,"fps":30})).await?;
    assert_ne!(another.id,frame.id);
    let project = serde_json::json!({"version":2,"name":"画面引用","width":1280,"height":720,"fps":30,"audio":[],"shots":[{"id":"target","name":"茶园","script":"","storyboard":"晨光","subtitle":"","duration":0,"sourceIn":0,"inputs":{},"references":[{"id":"frame-reference","kind":"image","role":"first_frame","assetId":frame.id,"versionId":version.id,"url":version.content_url,"sourceShotId":"source","sourceMediaVersionId":version_id,"sourcePosition":"fixed","sourceFrame":10,"sourceFps":30,"confirmedSourceVersionId":version_id,"confirmedSourceFrame":10,"confirmedSourceFps":30}]}]});
    let saved = store.write_video_project(&session.id,"frame-project",serde_json::json!({"expected_version":0,"project":project})).await?;
    assert_eq!(saved["project"],project);
    for (key,value) in [("sourceFrame",serde_json::json!(-1)),("sourceFps",serde_json::json!(0)),("confirmedSourceFrame",serde_json::json!(0.5)),("sourcePosition",serde_json::json!("unknown"))] {
        let mut invalid = project.clone();
        invalid["shots"][0]["references"][0][key] = value;
        assert!(store.write_video_project(&session.id,key,serde_json::json!({"expected_version":1,"project":invalid})).await.is_err());
    }
    for invalid in [serde_json::json!({"session_id":session.id,"frame":30000,"fps":30}),serde_json::json!({"session_id":session.id,"frame":1,"fps":0}),serde_json::json!({"session_id":session.id,"frame":-1,"fps":30})] {
        assert!(store.create_video_frame(&video.id,version_id,invalid).await.is_err());
    }
    store.close().await;
    Ok(())
}

#[tokio::test]
async fn video_frame_conversion_reports_unavailable_fractional_end_positions() -> Result<(), Box<dyn std::error::Error>> {
    let directory = directory()?;
    let destination = directory.path().join("fractional-video.mp4");
    let encoder = if cfg!(target_os="macos") {"h264_videotoolbox"} else {"libopenh264"};
    let output = tokio::process::Command::new(env!("PIXOMA_FFMPEG_PATH"))
        .args(["-nostdin","-hide_banner","-loglevel","error","-stream_loop","-1","-i"])
        .arg(PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("tests/fixtures/red.mp4"))
        .args(["-vf","fps=24","-frames:v","97","-an","-c:v",encoder]).arg(&destination).output().await?;
    assert!(output.status.success(),"{}",String::from_utf8_lossy(&output.stderr));
    let store = Store::open(&directory.path().join("studio.sqlite")).await?;
    let session = store.create_session("fractional-frame-owner").await?;
    let video = store.create_asset_bytes(Some(&session.id),"fractional.mp4",&std::fs::read(destination)?,"user","fractional-source").await?;
    let version = &video.versions[0];
    assert!((version.metadata["duration"].as_f64().ok_or("视频时长缺失")? - 97.0/24.0).abs()<0.00001);
    let frame = store.create_video_frame(&video.id,&version.id,serde_json::json!({"session_id":session.id,"frame":120,"fps":30})).await?;
    assert_eq!(frame.kind,"image");
    let error = store.create_video_frame(&video.id,&version.id,serde_json::json!({"session_id":session.id,"frame":121,"fps":30})).await.err().ok_or("不存在的视频画面需要返回明确错误")?;
    assert!(error.to_string().contains("指定位置没有可用的视频画面"));
    store.close().await;
    Ok(())
}
