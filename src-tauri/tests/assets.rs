use pixoma_studio::Store;
use std::path::PathBuf;

fn directory() -> Result<tempfile::TempDir, std::io::Error> {
    let root = PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../.local-cache");
    std::fs::create_dir_all(&root)?;
    tempfile::Builder::new().prefix("assets-").tempdir_in(root)
}

#[tokio::test]
async fn video_posters_and_palettes_keep_actual_frames_after_reopening()
-> Result<(), Box<dyn std::error::Error>> {
    let directory = directory()?;
    let database = directory.path().join("studio.sqlite");
    let store = Store::open(&database).await?;
    let session = store.create_session("account-a", "video").await?;
    let bytes = include_bytes!("fixtures/red.mp4");
    let asset = store
        .create_asset_bytes(
            "account-a",
            Some(&session.id),
            "red.mp4",
            bytes,
            "user",
            "video",
        )
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
    assert_eq!(document["versions"][0]["palette"]["status"], "ready");
    let poster = store
        .asset_poster_response("account-a", &asset.id, &asset.versions[0].id, "GET")
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
        .asset_poster_response("account-a", &asset.id, &asset.versions[0].id, "HEAD")
        .await?;
    assert_eq!(head.content_length, poster.body.len() as u64);
    assert!(head.body.is_empty());
    assert!(
        store
            .asset_poster_response("account-b", &asset.id, &asset.versions[0].id, "GET")
            .await
            .is_err()
    );
    assert!(
        store
            .asset_poster_response("account-a", &asset.id, &asset.versions[0].id, "POST")
            .await
            .is_err()
    );
    assert!(
        store
            .asset_poster_response("account-a", &asset.id, "invalid-version", "GET")
            .await
            .is_err()
    );
    store
        .retry_asset_palette("account-a", &asset.id, &asset.versions[0].id)
        .await?;
    let document = serde_json::to_value(store.get_asset("account-a", &asset.id).await?)?;
    assert_eq!(
        document["versions"][0]["palette"]["colors"][0]["hex"],
        color
    );
    assert!(
        store
            .create_asset_bytes(
                "account-a",
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
        store
            .read_asset("account-a", &asset.id, &asset.versions[0].id)
            .await?,
        bytes
    );
    store.close().await;
    let reopened = Store::open(&database).await?;
    assert_eq!(
        reopened
            .asset_poster_response("account-a", &asset.id, &asset.versions[0].id, "GET")
            .await?
            .body,
        poster.body
    );
    let saved = serde_json::to_value(reopened.get_asset("account-a", &asset.id).await?)?;
    assert_eq!(
        saved["versions"][0]["poster_url"],
        document["versions"][0]["poster_url"]
    );
    assert_eq!(
        saved["versions"][0]["palette"],
        document["versions"][0]["palette"]
    );
    assert!(reopened.get_asset("account-b", &asset.id).await.is_err());
    reopened.close().await;
    Ok(())
}

#[tokio::test]
async fn reopening_cleans_unreferenced_files_and_preserves_all_asset_versions()
-> Result<(), Box<dyn std::error::Error>> {
    let directory = directory()?;
    let database = directory.path().join("studio.sqlite");
    let store = Store::open(&database).await?;
    let session = store.create_session("account-a", "清理检查").await?;
    let asset = store
        .create_text_asset("account-a", &session.id, "正文", "原始正文", "create")
        .await?;
    let asset = store
        .update_text_asset("account-a", &asset.id, "更新正文", "update")
        .await?;
    let parent = store
        .asset_path("account-a", &asset.id, &asset.versions[0].id)
        .await?
        .parent()
        .ok_or("资产目录缺失")?
        .to_path_buf();
    let connection = sqlx::SqlitePool::connect(&format!("sqlite://{}", database.display())).await?;
    sqlx::query("CREATE TRIGGER reject_asset BEFORE INSERT ON assets BEGIN SELECT RAISE(ABORT,'资产写入检查'); END")
        .execute(&connection).await?;
    assert!(
        store
            .create_text_asset(
                "account-a",
                &session.id,
                "中断正文",
                "未引用正文",
                "interrupted"
            )
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
            reopened
                .read_asset("account-a", &asset.id, &version.id)
                .await?,
            content.as_bytes()
        );
    }
    let (created, opened) = tokio::join!(
        reopened.create_text_asset(
            "account-a",
            &session.id,
            "并发正文",
            "并发保存内容",
            "concurrent"
        ),
        Store::open(&database),
    );
    let created = created?;
    let opened = opened?;
    assert_eq!(
        opened
            .read_asset("account-a", &created.id, &created.versions[0].id)
            .await?,
        "并发保存内容".as_bytes()
    );
    opened.close().await;
    reopened.close().await;
    Ok(())
}

#[cfg(unix)]
#[tokio::test]
async fn asset_writes_reject_account_directory_symlinks() -> Result<(), Box<dyn std::error::Error>>
{
    use sha2::{Digest, Sha256};
    let directory = directory()?;
    let database = directory.path().join("studio.sqlite");
    let store = Store::open(&database).await?;
    let first = store.create_session("account-a", "first").await?;
    let second = store.create_session("account-b", "second").await?;
    let asset = store
        .create_text_asset("account-b", &second.id, "正文", "保留正文", "create")
        .await?;
    let parent = store
        .asset_path("account-b", &asset.id, &asset.versions[0].id)
        .await?
        .parent()
        .ok_or("资产目录缺失")?
        .to_path_buf();
    let linked = directory
        .path()
        .join("assets")
        .join(format!("{:x}", Sha256::digest(b"account-a")));
    std::os::unix::fs::symlink(&parent, &linked)?;
    assert!(
        store
            .create_text_asset("account-a", &first.id, "正文", "其他正文", "create")
            .await
            .is_err()
    );
    store.close().await;
    let reopened = Store::open(&database).await?;
    assert_eq!(
        reopened
            .read_asset("account-b", &asset.id, &asset.versions[0].id)
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
    let session = store.create_session("account-a", "palette").await?;
    let pixels = image::RgbImage::from_fn(80, 40, |x, _| {
        if x < 60 {
            image::Rgb([255, 0, 0])
        } else {
            image::Rgb([0, 0, 255])
        }
    });
    let source = directory.path().join("colors.png");
    pixels.save(&source)?;
    let asset = store
        .import_asset("account-a", &session.id, &source)
        .await?;
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
            "account-b",
            LocalRequest {
                method: "POST".to_owned(),
                path: path.clone(),
                body: None
            }
        )
        .await
        .is_err()
    );
    api.request(
        "account-a",
        LocalRequest {
            method: "POST".to_owned(),
            path,
            body: None,
        },
    )
    .await?;
    assert_eq!(
        store
            .read_asset("account-a", &asset.id, &asset.versions[0].id)
            .await?,
        std::fs::read(source)?
    );
    let white = directory.path().join("white.png");
    image::RgbImage::from_pixel(8, 8, image::Rgb([255, 255, 255])).save(&white)?;
    let white = store.import_asset("account-a", &session.id, &white).await?;
    let white: Value = serde_json::to_value(white)?;
    assert_eq!(
        white["versions"][0]["palette"]["colors"],
        json!([{ "hex": "#ffffff", "ratio": 1.0 }])
    );
    store.close().await;
    let reopened = Store::open(&database).await?;
    let saved = serde_json::to_value(reopened.get_asset("account-a", &asset.id).await?)?;
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
    let avatar = store
        .upload_asset("account-a", metadata.clone(), bytes)
        .await?;
    assert_eq!(avatar["kind"], "image");
    assert_eq!(avatar["versions"][0]["mime_type"], "image/png");
    assert_eq!(
        store.upload_asset("account-a", metadata, bytes).await?["id"],
        avatar["id"]
    );
    assert!(store.upload_asset("account-a", json!({ "file_name": "头像.png", "project_id": "", "request_id": "invalid-avatar", "purpose": "expert_avatar" }), b"ordinary text").await.is_err());
    assert!(store.upload_asset("account-a", json!({ "file_name": "头像.png", "project_id": "", "request_id": "large-avatar", "purpose": "expert_avatar" }), &vec![0_u8; (5 << 20) + 1]).await.is_err());
    assert_eq!(
        store
            .library_assets("account-a", &Default::default())
            .await?["total"],
        1
    );
    store.close().await;
    let store = Store::open(&database).await?;
    let id = avatar["id"].as_str().ok_or("头像资产标识缺失")?;
    let version = avatar["versions"][0]["id"]
        .as_str()
        .ok_or("头像版本标识缺失")?;
    assert_eq!(store.read_asset("account-a", id, version).await?, bytes);
    assert!(store.read_asset("account-b", id, version).await.is_err());
    store.close().await;
    Ok(())
}

#[tokio::test]
async fn uploads_keep_project_scope_file_dates_and_export_selected_versions()
-> Result<(), Box<dyn std::error::Error>> {
    use serde_json::json;
    use std::io::{Cursor, Read};
    let directory = directory()?;
    let store = Store::open(&directory.path().join("studio.sqlite")).await?;
    let project = store.create_project("account-a", "素材").await?;
    let metadata = json!({ "file_name": "正文.txt", "project_id": project.id, "request_id": "upload", "source_modified_at": "2026-10-17T10:00:00Z" });
    let first = store
        .upload_asset("account-a", metadata.clone(), "正文".as_bytes())
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
        store.project_asset("account-a", first_id).await?["project_id"],
        project.id
    );
    assert_eq!(
        store
            .upload_asset("account-a", metadata, "正文".as_bytes())
            .await?["id"],
        first["id"]
    );
    assert!(store.upload_asset("account-b", json!({ "file_name": "正文.txt", "project_id": project.id, "request_id": "upload" }), "正文".as_bytes()).await.is_err());
    let exported = store
        .export_assets("account-a", &[first_id.to_owned()])
        .await?;
    assert_eq!(exported.name, "正文.txt");
    assert_eq!(exported.bytes, "正文".as_bytes());
    let second = store.upload_asset("account-a", json!({ "file_name": "../正文.txt", "project_id": project.id, "request_id": "upload-second" }), "其他正文".as_bytes()).await?;
    let second_id = second["project_asset_id"]
        .as_str()
        .ok_or("项目资产标识缺失")?;
    let exported = store
        .export_assets("account-a", &[first_id.to_owned(), second_id.to_owned()])
        .await?;
    let mut archive = zip::ZipArchive::new(Cursor::new(exported.bytes))?;
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
            .export_assets("account-b", &[first_id.to_owned()])
            .await
            .is_err()
    );
    store.close().await;
    Ok(())
}

#[tokio::test]
async fn imported_files_are_owned_copies_and_versions_enforce_account_scope()
-> Result<(), Box<dyn std::error::Error>> {
    let directory = directory()?;
    let input = directory.path().join("input.png");
    let original = include_bytes!("../../public/images/pixoma-ghost.png");
    std::fs::write(&input, original)?;
    let path = directory.path().join("studio.sqlite");
    let store = Store::open(&path).await?;
    let session = store.create_session("account-a", "session-a").await?;
    let asset = store.import_asset("account-a", &session.id, &input).await?;
    assert_eq!(asset.kind, "image");
    assert_eq!(asset.versions[0].mime_type, "image/png");
    assert_eq!(asset.versions[0].width_px, Some(512));
    std::fs::remove_file(input)?;
    assert_eq!(
        store
            .read_asset("account-a", &asset.id, &asset.versions[0].id)
            .await?,
        original
    );
    assert!(
        store
            .read_asset("account-b", &asset.id, &asset.versions[0].id)
            .await
            .is_err()
    );
    assert!(
        store
            .read_asset("account-a", &asset.id, "../../studio.sqlite")
            .await
            .is_err()
    );
    store.close().await;
    let reopened = Store::open(&path).await?;
    assert_eq!(
        reopened
            .read_asset("account-a", &asset.id, &asset.versions[0].id)
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
    let session = store.create_session("account-a", "session-a").await?;
    let first = store
        .create_text_asset(
            "account-a",
            &session.id,
            "脚本.md",
            "第一段正文",
            "create-a",
        )
        .await?;
    assert_eq!(first.versions[0].content_origin, "manual");
    let repeated = store
        .create_text_asset(
            "account-a",
            &session.id,
            "脚本.md",
            "第一段正文",
            "create-a",
        )
        .await?;
    assert_eq!(first.id, repeated.id);
    let edited = store
        .update_text_asset("account-a", &first.id, "第二段正文", "update-a")
        .await?;
    assert_eq!(edited.current_version, 2);
    assert_eq!(edited.versions.len(), 2);
    assert_eq!(edited.versions[1].content_origin, "manual");
    assert_eq!(
        store
            .read_asset("account-a", &first.id, &first.versions[0].id)
            .await?,
        "第一段正文".as_bytes()
    );
    assert_eq!(
        store
            .read_asset("account-a", &first.id, &edited.versions[1].id)
            .await?,
        "第二段正文".as_bytes()
    );
    let second = store
        .update_text_asset("account-a", &first.id, "第二段正文", "update-a")
        .await?;
    assert_eq!(second.current_version, 2);
    assert!(
        store
            .update_text_asset("account-b", &first.id, "其他内容", "update-b")
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
    let session = store.create_session("account-a", "session").await?;
    let asset = store
        .create_text_asset("account-a", &session.id, "正文", "正文", "document")
        .await?;
    assert_eq!(asset.name, "正文.md");
    assert!(
        store
            .create_text_asset("account-a", &session.id, "空白", "   ", "empty")
            .await
            .is_err()
    );
    assert!(
        store
            .update_text_asset("account-a", &asset.id, "   ", "empty-update")
            .await
            .is_err()
    );
    let upload = store.upload_asset("account-a", serde_json::json!({ "session_id": session.id, "file_name": "文本.txt", "request_id": "upload" }), b"text").await?;
    assert!(
        store
            .update_text_asset(
                "account-a",
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
    let session = store.create_session("account-a", "session").await?;
    let (left, right) = tokio::join!(
        store.create_text_asset("account-a", &session.id, "正文.md", "正文", "create"),
        store.create_text_asset("account-a", &session.id, "正文.md", "正文", "create"),
    );
    let left = left?;
    assert_eq!(left.id, right?.id);
    let (left, right) = tokio::join!(
        store.update_text_asset("account-a", &left.id, "更新正文", "update"),
        store.update_text_asset("account-a", &left.id, "更新正文", "update"),
    );
    assert_eq!(left?.current_version, 2);
    assert_eq!(right?.current_version, 2);
    assert_eq!(
        store
            .list_session_assets("account-a", &session.id)
            .await?
            .len(),
        1
    );
    store.close().await;
    Ok(())
}

#[tokio::test]
async fn asset_references_require_owned_sessions_and_keep_the_selected_version()
-> Result<(), Box<dyn std::error::Error>> {
    let directory = directory()?;
    let store = Store::open(&directory.path().join("studio.sqlite")).await?;
    let source = store.create_session("account-a", "source").await?;
    let target = store.create_session("account-a", "target").await?;
    let project = store.create_project("account-a", "引用项目").await?;
    store
        .move_session("account-a", &target.id, Some(&project.id))
        .await?;
    let other = store.create_session("account-b", "other").await?;
    let asset = store
        .create_text_asset("account-a", &source.id, "文档.md", "正文", "create")
        .await?;
    assert!(
        store
            .reference_asset(
                "account-a",
                &other.id,
                &asset.id,
                &asset.versions[0].id,
                "reference-b"
            )
            .await
            .is_err()
    );
    store
        .reference_asset(
            "account-a",
            &target.id,
            &asset.id,
            &asset.versions[0].id,
            "reference-a",
        )
        .await?;
    let views = store.list_session_assets("account-a", &target.id).await?;
    assert_eq!(views.len(), 1);
    assert_eq!(views[0].id, asset.id);
    assert_eq!(views[0].versions[0].id, asset.versions[0].id);
    let placements = store
        .library_assets(
            "account-a",
            &pixoma_studio::LibraryQuery {
                project_id: project.id,
                ..Default::default()
            },
        )
        .await?;
    assert_eq!(placements["total"], 1);
    assert_eq!(placements["items"][0]["asset_id"], asset.id);
    assert!(
        store
            .reference_asset(
                "account-a",
                &source.id,
                &asset.id,
                &asset.versions[0].id,
                "reference-a"
            )
            .await
            .is_err()
    );
    store.close().await;
    Ok(())
}
