use pixoma_studio::Store;
use serde_json::json;

#[tokio::test]
async fn editor_state_survives_reopening_and_main_chat_removal() -> Result<(), Box<dyn std::error::Error>> {
    let root = std::path::PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../.local-cache");
    std::fs::create_dir_all(&root)?;
    let directory = tempfile::Builder::new().prefix("video-editor-state-").tempdir_in(root)?;
    let path = directory.path().join("studio.sqlite");
    let store = Store::open(&path).await?;
    let owner = store.create_session("editor-state-owner").await?;
    let project = json!({"version":2,"name":"编辑状态验证","width":1280,"height":720,"fps":30,"audio":[],"shots":[{"id":"opening","name":"开场","script":"","storyboard":"茶园晨光","references":[],"subtitle":"","duration":0,"plannedDuration":3,"sourceIn":0,"inputs":{}}]});
    let saved = store.write_video_project(&owner.id,"editor-state-project",json!({"expected_version":0,"project":project})).await?;
    let video_id = saved["asset_id"].as_str().ok_or("作品标识缺失")?.to_owned();
    let api = pixoma_studio::LocalApi::new(store.clone());
    let endpoint = format!("/api/v1/studio/video-projects/{video_id}/editor-state");
    let read = ||pixoma_studio::LocalRequest {method:"GET".into(),path:endpoint.clone(),body:None};
    assert!(api.request(read()).await?.is_null());
    let mut first = project["shots"][0].clone(); first["id"] = json!("first"); first["name"] = json!("");
    first.as_object_mut().ok_or("草稿格式无效")?.remove("plannedDuration");
    let mut second = first.clone(); second["id"] = json!("second"); second["name"] = json!("后半部分"); second["storyboard"] = json!("尚未采用的画面");
    let state = json!({"version":1,"selectedId":"opening","playheadFrame":45,"structureDrafts":{"[\"split\",\"opening\"]":{"request":{"mode":"split","shotId":"opening","project":project},"selectedIds":["opening"],"settingsId":"opening","drafts":[first,second],"referenceChoices":[]}}});
    let write = |session: &str,state|pixoma_studio::LocalRequest {method:"PUT".into(),path:endpoint.clone(),body:Some(json!({"session_id":session,"state":state}))};
    let unrelated = store.create_session("editor-state-unrelated").await?;
    assert!(api.request(write(&unrelated.id,state.clone())).await.is_err());
    assert!(api.request(write(&owner.id,json!([]))).await.is_err());
    assert_eq!(api.request(write(&owner.id,state.clone())).await?,state);
    assert_eq!(api.request(read()).await?,state);
    assert_eq!(store.read_video_work(&video_id).await?["asset_version"],1);
    assert_eq!(store.read_video_work(&video_id).await?["project"],project);
    store.clear_sessions().await?;
    store.close().await;
    let reopened = Store::open(&path).await?;
    let api = pixoma_studio::LocalApi::new(reopened.clone());
    assert_eq!(api.request(read()).await?,state);
    let work = reopened.open_video_work(&video_id).await?;
    let new_owner = work["main_session_id"].as_str().ok_or("新主对话缺失")?;
    let changed = json!({"version":1,"selectedId":"ending","playheadFrame":60,"structureDrafts":{}});
    assert_eq!(api.request(write(new_owner,changed.clone())).await?,changed);
    assert_eq!(api.request(read()).await?,changed);
    assert_eq!(reopened.read_video_work(&video_id).await?["project"],project);
    reopened.close().await;
    Ok(())
}

#[tokio::test]
async fn input_sources_survive_saved_chat_edits_and_validate_drafts() -> Result<(), Box<dyn std::error::Error>> {
    let root = std::path::PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../.local-cache");
    std::fs::create_dir_all(&root)?;
    let directory = tempfile::Builder::new().prefix("video-input-sources-").tempdir_in(root)?;
    let store = Store::open(&directory.path().join("studio.sqlite")).await?;
    let session = store.create_session("input-source-session").await?;
    let sources = json!({"prompt":{"field":"storyboard","revision":1,"sourceText":"人物进入茶园","value":"人物进入茶园","edited":false}});
    let project = json!({"version":2,"name":"输入来源验证","width":1280,"height":720,"fps":30,"shots":[{"id":"opening","name":"开场","script":"","storyboard":"人物进入茶园","contentRevisions":{"script":1,"storyboard":1},"subtitle":"","duration":0,"sourceIn":0,"inputs":{"prompt":"人物进入茶园"},"inputSources":sources,"references":[],"workflowDrafts":{"workflow:version":{"inputs":{"prompt":"手工输入"},"inputSources":sources,"referenceInputs":{}}}}]});
    let saved = store.write_video_project(&session.id,"save-input-source",json!({"expected_version":0,"project":project})).await?;
    let work_id = saved["asset_id"].as_str().ok_or("作品标识缺失")?;
    let mut incoming = project.clone();
    incoming["shots"][0].as_object_mut().ok_or("分镜格式无效")?.remove("inputSources");
    incoming["shots"][0]["storyboard"] = json!("人物离开茶园");
    let changed = store.write_video_project_from_chat(&session.id,"change-description",json!({"expected_version":1,"project":incoming})).await?;
    assert_eq!(changed["project"]["shots"][0]["inputSources"],sources);
    assert_eq!(changed["project"]["shots"][0]["inputs"]["prompt"],"人物进入茶园");
    assert_eq!(changed["project"]["shots"][0]["contentRevisions"],json!({"script":1,"storyboard":2}));
    let edited = store.update_video_shot(&session.id,"edit-input",&json!({"expected_version":2,"shot_id":"opening","changes":{"inputs":{"prompt":"手工修改"}}})).await?;
    assert_eq!(edited["project"]["shots"][0]["inputSources"],sources);
    assert_eq!(edited["project"]["shots"][0]["inputs"]["prompt"],"手工修改");
    assert_eq!(edited["project"]["shots"][0]["contentRevisions"]["storyboard"],2);
    for (field,invalid) in [("inputSources",json!({"prompt":{"field":"unknown","sourceText":"茶园","value":"茶园","edited":false}})),("workflowDrafts",json!({"workflow:version":{"inputs":[]}}))] {
        let mut invalid_project = edited["project"].clone();
        invalid_project["shots"][0][field] = invalid;
        assert!(store.write_video_project(&session.id,&format!("invalid-{field}"),json!({"expected_version":3,"project":invalid_project})).await.is_err());
    }
    let restored = store.update_video_shot(&session.id,"restore-description",&json!({"expected_version":3,"shot_id":"opening","changes":{"storyboard":"人物进入茶园"}})).await?;
    assert_eq!(restored["project"]["shots"][0]["contentRevisions"]["storyboard"],3);
    assert_eq!(restored["project"]["shots"][0]["inputSources"],sources);
    store.clear_sessions().await?;
    assert_eq!(store.read_video_work(work_id).await?["project"]["shots"][0]["inputSources"],sources);
    store.close().await;
    Ok(())
}

#[tokio::test]
async fn asset_video_usages_follow_saved_references_across_works() -> Result<(), Box<dyn std::error::Error>> {
    let root = std::path::PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../.local-cache");
    std::fs::create_dir_all(&root)?;
    let directory = tempfile::Builder::new().prefix("video-material-usages-").tempdir_in(root)?;
    let store = Store::open(&directory.path().join("studio.sqlite")).await?;
    let api = pixoma_studio::LocalApi::new(store.clone());
    let owner = store.create_session("usages-owner").await?;
    let asset = store.create_text_asset(&owner.id,"人物.md","蓝色外套","usages-character").await?;
    let mut ids = Vec::new();
    for name in ["茶园","茶馆"] {
        let session = store.create_session(&format!("usages-{name}")).await?;
        let project = json!({"version":2,"name":name,"width":1280,"height":720,"fps":30,"audio":[],"shots":[{"id":"opening","name":"开场","script":"","storyboard":"人物走入画面","subtitle":"","duration":0,"sourceIn":0,"inputs":{},"references":[{"id":"character","role":"reference","kind":"document","assetId":asset.id,"versionId":asset.versions[0].id,"url":asset.versions[0].content_url}]}]});
        let work = store.write_video_project(&session.id,&format!("work-{name}"),json!({"expected_version":0,"project":project})).await?;
        ids.push((work["asset_id"].as_str().ok_or("作品标识缺失")?.to_owned(),session.id));
    }
    let path = format!("/api/v1/studio/assets/{}/video-usages",asset.id);
    let usages = api.request(pixoma_studio::LocalRequest {method:"GET".into(),path:path.clone(),body:None}).await?;
    let entries = usages.as_array().ok_or("作品使用位置缺失")?;
    assert_eq!(entries.len(),2);
    for entry in entries {
        assert_eq!(entry["references"][0]["shot_id"],"opening");
        assert_eq!(entry["references"][0]["reference"]["versionId"],asset.versions[0].id);
        assert_eq!(entry["references"][0]["version"],1);
    }
    let work = store.read_video_work(&ids[0].0).await?;
    let mut changed = work["project"].clone();
    changed["shots"][0]["references"] = json!([]);
    store.write_video_work(&ids[0].0,&ids[0].1,"remove-reference",json!({"expected_version":1,"project":changed})).await?;
    let remaining = api.request(pixoma_studio::LocalRequest {method:"GET".into(),path:path.clone(),body:None}).await?;
    assert_eq!(remaining.as_array().ok_or("作品使用位置缺失")?.len(),1);
    assert_eq!(remaining[0]["video_id"],ids[1].0);
    store.clear_sessions().await?;
    assert_eq!(remaining,api.request(pixoma_studio::LocalRequest {method:"GET".into(),path,body:None}).await?);
    store.close().await;
    Ok(())
}

#[tokio::test]
async fn material_objects_keep_identity_across_chats_results_and_chat_removal() -> Result<(), Box<dyn std::error::Error>> {
    let root = std::path::PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../.local-cache");
    std::fs::create_dir_all(&root)?;
    let directory = tempfile::Builder::new().prefix("video-material-object-").tempdir_in(root)?;
    let store = Store::open(&directory.path().join("studio.sqlite")).await?;
    let api = pixoma_studio::LocalApi::new(store.clone());
    let main = store.create_session("material-main").await?;
    let project = store.read_video_project(&main.id).await?["project"].clone();
    let saved = store.write_video_project(&main.id,"material-initial",json!({"expected_version":0,"project":project})).await?;
    let id = saved["asset_id"].as_str().ok_or("作品标识缺失")?;
    let path = format!("/api/v1/studio/video-projects/{id}/material-chats");
    let input = json!({"request_id":"character-chat","title":"小林 · 造型","name":"小林","kind":"character","brief":"青年茶农，蓝色外套"});
    let created = api.request(pixoma_studio::LocalRequest { method:"POST".into(),path:path.clone(),body:Some(input.clone()) }).await?;
    let material_id = created["material_id"].as_str().ok_or("素材对象标识缺失")?;
    assert_eq!(created,api.request(pixoma_studio::LocalRequest { method:"POST".into(),path:path.clone(),body:Some(input) }).await?);
    let session = created["session_id"].as_str().ok_or("素材对话缺失")?;
    let continued = api.request(pixoma_studio::LocalRequest { method:"POST".into(),path,body:Some(json!({"request_id":"character-next","title":"小林 · 服装","material_id":material_id})) }).await?;
    assert_eq!(continued["material_id"],material_id);
    assert_ne!(continued["session_id"],session);
    assert_eq!(continued["work"]["materials"][0]["kind"],"character");
    assert_eq!(continued["work"]["materials"][0]["brief"],"青年茶农，蓝色外套");
    assert_eq!(continued["work"]["chats"][0]["material_id"],material_id);
    let candidate = store.create_text_asset(session,"小林设定.md","蓝色外套","character-result").await?;
    assert!(candidate.usages.iter().any(|usage|usage["session_id"]==session && usage["asset_version_id"]==candidate.versions[0].id));
    let result_path = format!("/api/v1/studio/video-materials/{material_id}/results");
    let publish = json!({"request_id":"adopt-character","session_id":session,"asset_id":candidate.id,"asset_version_id":candidate.versions[0].id,"expected_version":0});
    let adopted = api.request(pixoma_studio::LocalRequest { method:"POST".into(),path:result_path.clone(),body:Some(publish.clone()) }).await?;
    assert_eq!(adopted["asset_id"],candidate.id);
    assert_eq!(adopted,api.request(pixoma_studio::LocalRequest { method:"POST".into(),path:result_path.clone(),body:Some(publish) }).await?);
    let next_session = continued["session_id"].as_str().ok_or("后续对话缺失")?;
    let second = store.create_text_asset(next_session,"小林服装.md","绿色外套","character-second").await?;
    let updated = api.request(pixoma_studio::LocalRequest { method:"POST".into(),path:result_path,body:Some(json!({"request_id":"adopt-next","session_id":next_session,"asset_id":second.id,"asset_version_id":second.versions[0].id,"expected_version":1})) }).await?;
    assert_eq!(updated["asset_id"],candidate.id);
    assert_eq!(updated["asset"]["current_version"],2);
    assert_eq!(store.read_asset(&candidate.id,&candidate.versions[0].id).await?,"蓝色外套".as_bytes());
    let next_version = updated["asset"]["versions"][1]["id"].as_str().ok_or("新素材版本缺失")?;
    assert_eq!(store.read_asset(&candidate.id,next_version).await?,"绿色外套".as_bytes());
    assert_eq!(store.video_material_chat_context(id,session,None).await?["asset_version"]["id"],candidate.versions[0].id);
    assert_eq!(store.video_material_chat_context(id,next_session,None).await?["asset_version"]["id"],next_version);
    assert_eq!(store.video_material_chat_context(id,next_session,Some(&candidate.versions[0].id)).await?["asset_version"]["id"],candidate.versions[0].id);
    assert!(store.video_material_chat_context(id,next_session,Some(&second.versions[0].id)).await.is_err());
    let conflict = json!({"request_id":"stale-result","session_id":next_session,"asset_id":second.id,"asset_version_id":second.versions[0].id,"expected_version":1});
    assert!(store.adopt_video_material_result(material_id,conflict).await.is_err());
    let unrelated = store.create_session("unrelated-material").await?;
    assert!(store.adopt_video_material_result(material_id,json!({"request_id":"unrelated","session_id":unrelated.id,"asset_id":second.id,"asset_version_id":second.versions[0].id,"expected_version":2})).await.is_err());
    assert_eq!(store.read_video_work(id).await?["project"],project);
    assert_eq!(store.video_material_for_asset(&candidate.id).await?["id"],material_id);
    let other_main = store.create_session("material-other-main").await?;
    let other = store.write_video_project(&other_main.id,"other-material-work",json!({"expected_version":0,"project":project})).await?;
    let other_id = other["asset_id"].as_str().ok_or("另一个作品缺失")?;
    let reused = store.create_video_material_chat(other_id,json!({"request_id":"reuse-character","title":"小林 · 另一作品","material_id":material_id,"asset_version_id":candidate.versions[0].id})).await?;
    assert_eq!(reused["material_id"],material_id);
    assert_eq!(reused["work"]["materials"][0]["asset_id"],candidate.id);
    store.bind_video_chat(other_id,session,None).await?;
    assert_eq!(store.video_material_chat_context(other_id,session,None).await?["material_id"],material_id);
    store.clear_sessions().await?;
    let retained = store.read_video_work(id).await?;
    assert_eq!(retained["materials"][0]["id"],material_id);
    assert_eq!(retained["materials"][0]["asset_id"],candidate.id);
    assert_eq!(store.read_asset(&candidate.id,next_version).await?,"绿色外套".as_bytes());
    store.close().await;
    Ok(())
}

#[tokio::test]
async fn material_image_versions_preserve_media_bytes_and_source_metadata() -> Result<(), Box<dyn std::error::Error>> {
    let root = std::path::PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../.local-cache");
    std::fs::create_dir_all(&root)?;
    let directory = tempfile::Builder::new().prefix("video-material-image-").tempdir_in(root)?;
    let store = Store::open(&directory.path().join("studio.sqlite")).await?;
    let main = store.create_session("material-image-main").await?;
    let project = store.read_video_project(&main.id).await?["project"].clone();
    let saved = store.write_video_project(&main.id,"image-material-work",json!({"expected_version":0,"project":project})).await?;
    let created = store.create_video_material_chat(saved["asset_id"].as_str().ok_or("作品缺失")?,json!({"request_id":"image-material","title":"茶园画面","name":"茶园","kind":"scene"})).await?;
    let session = created["session_id"].as_str().ok_or("素材对话缺失")?;
    let material = created["material_id"].as_str().ok_or("素材对象缺失")?;
    let video = store.create_asset_bytes(Some(session),"来源.mp4",include_bytes!("fixtures/red.mp4"),"user","material-image-source").await?;
    let mut frames = Vec::new();
    for frame in [10,20] {frames.push(store.create_video_frame(&video.id,&video.versions[0].id,json!({"session_id":session,"frame":frame,"fps":30})).await?);}
    for (index,frame) in frames.iter().enumerate() {
        store.adopt_video_material_result(material,json!({"request_id":format!("material-frame-{index}"),"session_id":session,"asset_id":frame.id,"asset_version_id":frame.versions[0].id,"expected_version":index})).await?;
    }
    let asset = store.get_asset(&frames[0].id).await?;
    assert_eq!(asset.kind,"image");
    assert_eq!(asset.current_version,2);
    assert_eq!(asset.versions[0].metadata["source_video"]["frame"],10);
    assert_eq!(asset.versions[1].metadata["source_video"]["frame"],20);
    assert_eq!(asset.versions[1].width_px,frames[1].versions[0].width_px);
    assert_eq!(store.read_asset(&asset.id,&asset.versions[1].id).await?,store.read_asset(&frames[1].id,&frames[1].versions[0].id).await?);
    let repeated = store.adopt_video_material_result(material,json!({"request_id":"same-image-another-request","session_id":session,"asset_id":frames[1].id,"asset_version_id":frames[1].versions[0].id,"expected_version":2})).await?;
    assert_eq!(repeated["asset"]["current_version"],2);
    store.close().await;
    Ok(())
}

#[tokio::test]
async fn storyboard_proposals_capture_removal_and_order_without_changing_shots() -> Result<(), Box<dyn std::error::Error>> {
    let root = std::path::PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../.local-cache");
    std::fs::create_dir_all(&root)?;
    let directory = tempfile::Builder::new().prefix("video-structure-proposal-").tempdir_in(root)?;
    let store = Store::open(&directory.path().join("studio.sqlite")).await?;
    let session = store.create_session("structure-proposal").await?;
    let shots = ["opening","middle","ending"].map(|id|json!({"id":id,"name":id,"script":"","storyboard":"原始描述","references":[],"scriptSources":[],"subtitle":"","duration":0,"sourceIn":0,"inputs":{}}));
    let project = json!({"version":2,"name":"茶园","width":1280,"height":720,"fps":30,"shots":shots,"audio":[],"script":{"revision":0,"blocks":[]}});
    store.write_video_project(&session.id,"initial",json!({"expected_version":0,"project":project})).await?;
    let input = json!({"expected_version":1,"changes":[{"shotId":"middle","action":"remove","name":"middle","storyboard":"","sourceBlockIds":[]}],"order":["ending","opening"]});
    let context = json!({"scope":"storyboard","project":project});
    let saved = store.propose_video_creation(&session.id,"structure",&input,&context).await?;
    assert_eq!(saved["project"]["shots"],project["shots"]);
    assert_eq!(saved["project"]["proposals"][0]["changes"][0]["baseline"],shots[1]);
    assert_eq!(saved["project"]["proposals"][0]["reorder"],json!({"baseline":["opening","middle","ending"],"proposed":["ending","opening"]}));
    let only_order = json!({"expected_version":2,"changes":[],"order":["ending","middle","opening"]});
    let ordered = store.propose_video_creation(&session.id,"order-only",&only_order,&context).await?;
    assert_eq!(ordered["project"]["shots"],project["shots"]);
    let scoped = json!({"scope":"storyboard","project":project,"shot_ids":["middle"]});
    let mut invalid = input.clone(); invalid["expected_version"] = json!(3);
    assert!(store.propose_video_creation(&session.id,"outside-order",&invalid,&scoped).await.is_err());
    invalid["order"] = json!(["opening","missing"]);
    assert!(store.propose_video_creation(&session.id,"invalid-order",&invalid,&context).await.is_err());
    store.close().await;
    Ok(())
}

#[tokio::test]
async fn video_work_reopens_after_main_chat_is_cleared() -> Result<(), Box<dyn std::error::Error>> {
    let root = std::path::PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../.local-cache");
    std::fs::create_dir_all(&root)?;
    let directory = tempfile::Builder::new().prefix("video-reopen-").tempdir_in(root)?;
    let store = Store::open(&directory.path().join("studio.sqlite")).await?;
    let folder = store.create_project("茶园项目").await?;
    let session = store.create_session("original-main-chat").await?;
    store.move_session(&session.id,Some(&folder.id)).await?;
    let project = json!({"version":2,"name":"茶园短片","width":1280,"height":720,"fps":30,"shots":[],"audio":[],"script":{"revision":1,"blocks":[{"id":"opening","text":"晨光穿过茶树","revision":1}]}});
    let saved = store.write_video_project(&session.id,"initial",json!({"expected_version":0,"project":project})).await?;
    let id = saved["asset_id"].as_str().ok_or("作品标识缺失")?;
    assert_eq!(store.open_video_work(id).await?["main_session_id"],session.id);
    store.clear_sessions().await?;
    let listed = store.list_video_works(&folder.id).await?;
    assert_eq!(listed.as_array().ok_or("作品列表缺失")?.len(),1);
    assert_eq!(listed[0]["id"],id);
    assert_eq!(listed[0]["name"],"茶园短片");
    assert_eq!(store.list_video_works("").await?,json!([]));
    let (left,right) = tokio::join!(store.open_video_work(id),store.open_video_work(id));
    let opened = left?;
    assert_eq!(opened["main_session_id"],right?["main_session_id"]);
    let next_session = opened["main_session_id"].as_str().ok_or("恢复对话缺失")?;
    assert_ne!(next_session,session.id);
    assert_eq!(store.get_session(next_session).await?.project_id,Some(folder.id));
    assert_eq!(store.read_video_project(next_session).await?,saved);
    let mut edited = project.clone(); edited["name"] = json!("茶园晨光");
    store.write_video_project(next_session,"renamed",json!({"expected_version":1,"project":edited})).await?;
    assert_eq!(store.read_video_work(id).await?["project"],edited);
    assert_eq!(store.open_video_work(id).await?["main_session_id"],next_session);
    assert_eq!(store.read_video_work(id).await?["asset_version"],2);
    store.close().await;
    Ok(())
}

#[tokio::test]
async fn unplanned_shots_persist_without_changing_generation_duration() -> Result<(), Box<dyn std::error::Error>> {
    let root = std::path::PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../.local-cache");
    std::fs::create_dir_all(&root)?;
    let directory = tempfile::Builder::new().prefix("video-timing-").tempdir_in(root)?;
    let store = Store::open(&directory.path().join("studio.sqlite")).await?;
    let session = store.create_session("video-timing").await?;
    let project = json!({"version":2,"name":"晨光","width":1280,"height":720,"fps":30,"audio":[],"shots":[{"id":"opening","name":"开场","script":"","storyboard":"远景","references":[],"subtitle":"","duration":0,"sourceIn":0,"inputs":{"duration":8}}]});
    let saved = store.write_video_project(&session.id,"unplanned",json!({"expected_version":0,"project":project})).await?;
    assert_eq!(saved["project"],project);
    let planned = store.update_video_shot(&session.id,"planned",&json!({"expected_version":1,"shot_id":"opening","changes":{"plannedDuration":3}})).await?;
    assert_eq!(planned["project"]["shots"][0]["duration"],0);
    assert_eq!(planned["project"]["shots"][0]["plannedDuration"],3);
    assert_eq!(planned["project"]["shots"][0]["inputs"]["duration"],8);
    for invalid in [json!(0),json!(-1),json!(0.01),json!(3601)] {
        assert!(store.update_video_shot(&session.id,"invalid-plan",&json!({"expected_version":2,"shot_id":"opening","changes":{"plannedDuration":invalid}})).await.is_err());
    }
    let cleared = store.update_video_shot(&session.id,"clear-plan",&json!({"expected_version":2,"shot_id":"opening","changes":{"plannedDuration":null}})).await?;
    assert!(cleared["project"]["shots"][0].get("plannedDuration").is_none());
    assert_eq!(cleared["project"]["shots"][0]["inputs"]["duration"],8);
    store.close().await;
    Ok(())
}

#[tokio::test]
async fn creative_proposals_preserve_project_and_capture_request_baseline() -> Result<(), Box<dyn std::error::Error>> {
    let root = std::path::PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../.local-cache");
    std::fs::create_dir_all(&root)?;
    let directory = tempfile::Builder::new().prefix("video-proposal-").tempdir_in(root)?;
    let store = Store::open(&directory.path().join("studio.sqlite")).await?;
    let session = store.create_session("creative-proposal").await?;
    let project = json!({"version":2,"name":"晨光","width":1280,"height":720,"fps":30,"audio":[],
        "script":{"revision":1,"blocks":[{"id":"opening","text":"清晨薄雾散开","revision":1,"ancestorIds":["earlier-opening"]}]},
        "shots":[{"id":"first","name":"开场","script":"","storyboard":"远景","references":[],"subtitle":"","duration":3,"sourceIn":0,"inputs":{}}]});
    store.write_video_project(&session.id,"initial",json!({"expected_version":0,"project":project})).await?;
    let context = json!({"scope":"storyboard","project":project,"shot_ids":["first"]});
    let input = json!({"expected_version":1,"changes":[{"shotId":"first","action":"update","name":"茶园","storyboard":"阳光穿过薄雾","sourceBlockIds":["opening"]}]});
    let saved = store.propose_video_creation(&session.id,"storyboard-proposal",&input,&context).await?;
    assert_eq!(saved["project"]["shots"],project["shots"]);
    assert_eq!(saved["project"]["script"],project["script"]);
    let proposal = &saved["project"]["proposals"][0];
    assert_eq!(proposal["changes"][0]["baseline"]["storyboard"],"远景");
    assert_eq!(proposal["changes"][0]["sources"][0]["text"],"清晨薄雾散开");
    assert_eq!(store.propose_video_creation(&session.id,"storyboard-proposal",&input,&context).await?,saved);
    let outside = json!({"expected_version":2,"changes":[{"shotId":"new","action":"add","name":"茶杯","storyboard":"近景","sourceBlockIds":["opening"]}]});
    assert!(store.propose_video_creation(&session.id,"outside-scope",&outside,&context).await.is_err());
    let script_input = json!({"expected_version":2,"blocks":[{"id":"opening","text":"茶园迎来晨光"}]});
    let next = store.propose_video_creation(&session.id,"script-proposal",&script_input,&json!({"scope":"script","project":project})).await?;
    assert_eq!(next["project"]["script"],project["script"]);
    assert_eq!(next["project"]["proposals"][1]["script"]["blocks"][0]["revision"],2);
    assert_eq!(next["project"]["proposals"][1]["baseline"],project["script"]);
    let mut invalid = next["project"].clone();
    invalid["proposals"][0]["changes"][0]["media"] = json!({"url":"invalid"});
    assert!(store.write_video_project(&session.id,"invalid-proposal",json!({"expected_version":3,"project":invalid})).await.is_err());
    let empty = store.create_session("empty-creation").await?;
    let baseline = store.read_video_project(&empty.id).await?;
    let initial = store.propose_video_creation(&empty.id,"initial-script",&json!({"expected_version":0,"blocks":[{"id":"opening","text":"茶园晨光"}]}),&json!({"scope":"script","project":baseline["project"]})).await?;
    assert_eq!(initial["project"]["proposals"][0]["baseline"],json!({"revision":0,"blocks":[]}));
    assert_eq!(initial["project"]["shots"],json!([]));
    store.close().await;
    Ok(())
}

#[tokio::test]
async fn complete_script_and_source_snapshots_survive_chat_edits() -> Result<(), Box<dyn std::error::Error>> {
    let root = std::path::PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../.local-cache");
    std::fs::create_dir_all(&root)?;
    let directory = tempfile::Builder::new().prefix("video-script-document-").tempdir_in(root)?;
    let store = Store::open(&directory.path().join("studio.sqlite")).await?;
    let session = store.create_session("script-document").await?;
    let project = json!({"version":2,"name":"晨光","width":1280,"height":720,"fps":30,"audio":[],
        "script":{"revision":1,"blocks":[{"id":"opening","text":"清晨薄雾散开","revision":1,"ancestorIds":["earlier-opening"]}]},
        "shots":[{"id":"first","name":"开场","script":"清晨薄雾散开","storyboard":"远景","references":[],"subtitle":"","duration":3,"sourceIn":0,"inputs":{},
            "scriptSources":[{"blockId":"opening","text":"清晨薄雾散开","revision":1}]}]});
    store.write_video_project(&session.id,"initial",json!({"expected_version":0,"project":project})).await?;
    let mut incoming = project.clone();
    incoming.as_object_mut().ok_or("项目缺失")?.remove("script");
    incoming["shots"][0].as_object_mut().ok_or("分镜缺失")?.remove("scriptSources");
    incoming["shots"][0]["name"] = json!("茶园晨光");
    let saved = store.write_video_project_from_chat(&session.id,"rename",json!({"expected_version":1,"project":incoming})).await?;
    assert_eq!(saved["project"]["script"],project["script"]);
    assert_eq!(saved["project"]["shots"][0]["scriptSources"],project["shots"][0]["scriptSources"]);
    let mut invalid = project.clone(); invalid["script"]["blocks"][0]["revision"] = json!(2);
    assert!(store.write_video_project(&session.id,"invalid-revision",json!({"expected_version":2,"project":invalid})).await.is_err());
    invalid = project.clone(); invalid["script"]["blocks"] = json!([project["script"]["blocks"][0],project["script"]["blocks"][0]]);
    assert!(store.write_video_project(&session.id,"duplicate-block",json!({"expected_version":2,"project":invalid})).await.is_err());
    assert_eq!(store.read_video_project(&session.id).await?,saved);
    for ancestors in [json!(["opening"]),json!(["earlier-opening","earlier-opening"]),json!([""]),json!([1])] {
        let mut invalid = project.clone(); invalid["script"]["blocks"][0]["ancestorIds"] = ancestors;
        assert!(store.write_video_project(&session.id,&uuid::Uuid::new_v4().to_string(),json!({"expected_version":2,"project":invalid})).await.is_err());
    }
    let proposed = store.propose_video_creation(&session.id,"rewrite",&json!({"expected_version":2,"blocks":[{"id":"opening","text":"晨光穿过茶园"}]}),&json!({"scope":"script","project":saved["project"]})).await?;
    assert_eq!(proposed["project"]["proposals"][0]["script"]["blocks"][0]["ancestorIds"],json!(["earlier-opening"]));
    let separated = store.propose_video_creation(&session.id,"separate",&json!({"expected_version":3,"blocks":[{"id":"morning","text":"晨光","sourceBlockIds":["opening"]},{"id":"garden","text":"茶园","sourceBlockIds":["opening"]}]}),&json!({"scope":"script","project":saved["project"]})).await?;
    let blocks = separated["project"]["proposals"][1]["script"]["blocks"].as_array().ok_or("提案文本缺失")?;
    assert!(blocks.iter().all(|block|block["ancestorIds"] == json!(["earlier-opening","opening"])));
    assert_eq!(separated["project"]["shots"],saved["project"]["shots"]);
    let mut incoming = separated["project"].clone();
    incoming["script"]["blocks"][0].as_object_mut().ok_or("文本块缺失")?.remove("ancestorIds");
    let retained = store.write_video_project_from_chat(&session.id,"retain-source",json!({"expected_version":4,"project":incoming})).await?;
    assert_eq!(retained["project"]["script"]["blocks"][0]["ancestorIds"],json!(["earlier-opening"]));
    store.close().await;
    Ok(())
}

#[tokio::test]
async fn video_work_remains_readable_after_chats_are_cleared() -> Result<(), Box<dyn std::error::Error>> {
    let root = std::path::PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../.local-cache");
    std::fs::create_dir_all(&root)?;
    let directory = tempfile::Builder::new().prefix("video-work-identity-").tempdir_in(root)?;
    let store = Store::open(&directory.path().join("studio.sqlite")).await?;
    let main = store.create_session("video-main").await?;
    let project = store.read_video_project(&main.id).await?["project"].clone();
    let saved = store.write_video_project(&main.id,"initial",json!({"expected_version":0,"project":project})).await?;
    let id = saved["asset_id"].as_str().ok_or("作品标识缺失")?;
    let material = store.create_session("character-chat").await?;
    store.bind_video_chat(id,&material.id,None).await?;
    let mut next = project.clone(); next["name"] = json!("茶山来信");
    let updated = store.write_video_work(id,&material.id,"material-edit",json!({"expected_version":1,"project":next})).await?;
    assert_eq!(updated["asset_id"],id);
    assert_eq!(store.read_video_project(&main.id).await?["project"]["name"],"茶山来信");
    assert!(store.read_video_project(&material.id).await?["asset_id"].is_null());
    assert_eq!(store.read_video_work(id).await?["main_session_id"],main.id);
    store.clear_sessions().await?;
    let retained = store.read_video_work(id).await?;
    assert_eq!(retained["project"],next);
    assert!(retained["main_session_id"].is_null());
    assert_eq!(retained["chats"],json!([]));
    store.close().await;
    Ok(())
}

#[tokio::test]
async fn video_chat_binding_checks_project_and_write_membership() -> Result<(), Box<dyn std::error::Error>> {
    let root = std::path::PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../.local-cache");
    std::fs::create_dir_all(&root)?;
    let directory = tempfile::Builder::new().prefix("video-work-bindings-").tempdir_in(root)?;
    let store = Store::open(&directory.path().join("studio.sqlite")).await?;
    let project = store.create_project_with_type("春日茶事","video").await?;
    let main = store.create_session("main").await?;
    store.move_session(&main.id,Some(&project.id)).await?;
    let content = store.read_video_project(&main.id).await?["project"].clone();
    let saved = store.write_video_project(&main.id,"initial",json!({"expected_version":0,"project":content})).await?;
    let id = saved["asset_id"].as_str().ok_or("作品标识缺失")?;
    let chat = store.create_session("material").await?;
    assert!(store.bind_video_chat(id,&chat.id,None).await.is_err());
    assert!(store.write_video_work(id,&chat.id,"unbound",json!({"expected_version":1,"project":content})).await.is_err());
    store.move_session(&chat.id,Some(&project.id)).await?;
    store.bind_video_chat(id,&chat.id,None).await?;
    store.bind_video_chat(id,&chat.id,None).await?;
    let work = store.read_video_work(id).await?;
    assert_eq!(work["chats"].as_array().ok_or("对话列表缺失")?.len(),1);
    let api = pixoma_studio::LocalApi::new(store.clone());
    let encoded_material = format!("chat%3A{}",chat.id);
    let material = api.request(pixoma_studio::LocalRequest {method:"GET".into(),path:format!("/api/v1/studio/video-materials/{encoded_material}"),body:None}).await?;
    assert_eq!(material["id"],format!("chat:{}",chat.id));
    assert!(store.bind_video_chat(id,&chat.id,Some("missing-asset")).await.is_err());
    assert!(store.write_video_work(id,&chat.id,"stale",json!({"expected_version":0,"project":content})).await.is_err());
    store.close().await;
    Ok(())
}

#[tokio::test]
async fn creating_material_chat_is_idempotent_and_keeps_video_owner() -> Result<(), Box<dyn std::error::Error>> {
    let root = std::path::PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../.local-cache");
    std::fs::create_dir_all(&root)?;
    let directory = tempfile::Builder::new().prefix("video-material-chat-").tempdir_in(root)?;
    let store = Store::open(&directory.path().join("studio.sqlite")).await?;
    let main = store.create_session("main").await?;
    let content = store.read_video_project(&main.id).await?["project"].clone();
    let saved = store.write_video_project(&main.id,"initial",json!({"expected_version":0,"project":content})).await?;
    let id = saved["asset_id"].as_str().ok_or("作品标识缺失")?;
    let created = store.create_video_material_chat(id,json!({"request_id":"character-request","title":"小林 · 人物创作"})).await?;
    assert_eq!(created,store.create_video_material_chat(id,json!({"request_id":"character-request","title":"小林 · 人物创作"})).await?);
    let session_id = created["session_id"].as_str().ok_or("素材对话缺失")?;
    assert_ne!(session_id,main.id);
    assert_eq!(store.get_session(session_id).await?.title,"小林 · 人物创作");
    assert_eq!(created["work"]["main_session_id"],main.id);
    assert_eq!(created["work"]["chats"].as_array().ok_or("对话关联缺失")?.len(),1);
    assert!(store.create_video_material_chat(id,json!({"request_id":"character-request","title":"不同目标"})).await.is_err());
    let mut with_shot = content.clone();
    with_shot["shots"] = json!([{"id":"opening","name":"开场","script":"","storyboard":"小林进入茶园","subtitle":"","duration":0,"sourceIn":0,"inputs":{}}]);
    store.write_video_work(id,&main.id,"add-source-shot",json!({"expected_version":1,"project":with_shot})).await?;
    let input = json!({"request_id":"source-shot-request","title":"小林 · 分镜创作","shot_ids":["opening"]});
    let source_chat = store.create_video_material_chat(id,input.clone()).await?;
    store.write_video_work(id,&main.id,"remove-source-shot",json!({"expected_version":2,"project":content})).await?;
    let repeated = store.create_video_material_chat(id,input).await?;
    assert_eq!(repeated["session_id"],source_chat["session_id"]);
    assert_eq!(repeated["material_id"],source_chat["material_id"]);
    let context = store.video_material_chat_context(id,repeated["session_id"].as_str().ok_or("素材对话缺失")?,None).await?;
    assert_eq!(context["source"]["shots"][0]["storyboard"],"小林进入茶园");
    store.close().await;
    Ok(())
}

#[tokio::test]
async fn project_dimensions_follow_export_limits() -> Result<(), Box<dyn std::error::Error>> {
    let root = std::path::PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../.local-cache");
    std::fs::create_dir_all(&root)?;
    let directory = tempfile::Builder::new().prefix("video-dimensions-").tempdir_in(root)?;
    let store = Store::open(&directory.path().join("studio.sqlite")).await?;
    for field in ["width", "height"] {
        for value in [0, 14, 15, 17, 4095, 4097] {
            let request = format!("{field}-{value}");
            let session = store.create_session(&request).await?;
            let mut project = store.read_video_project(&session.id).await?["project"].clone();
            project[field] = json!(value);
            assert!(store.write_video_project(&session.id, &request, json!({"expected_version":0,"project":project})).await.is_err(), "{field}={value}");
            assert_eq!(store.read_video_project(&session.id).await?["asset_version"], 0);
        }
        for value in [16, 4096] {
            let request = format!("{field}-{value}");
            let session = store.create_session(&request).await?;
            let mut project = store.read_video_project(&session.id).await?["project"].clone();
            project[field] = json!(value);
            let saved = store.write_video_project(&session.id, &request, json!({"expected_version":0,"project":project})).await?;
            assert_eq!(saved["project"][field], value);
        }
    }
    store.close().await;
    Ok(())
}

#[tokio::test]
async fn whole_script_requests_create_shots_and_preserve_unrelated_edits() -> Result<(), Box<dyn std::error::Error>> {
    let root = std::path::PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../.local-cache");
    std::fs::create_dir_all(&root)?;
    let directory = tempfile::Builder::new().prefix("video-whole-script-").tempdir_in(root)?;
    let store = Store::open(&directory.path().join("studio.sqlite")).await?;
    let session = store.create_session("whole-script").await?;
    let baseline = json!({"version":2,"name":"晨光","width":1280,"height":720,"fps":30,"audio":[],"shots":[{"id":"first","name":"开场","script":"茶园","storyboard":"远景","references":[],"subtitle":"原字幕","duration":3,"sourceIn":0,"inputs":{"prompt":"保持参数"}}]});
    store.write_video_project(&session.id,"initial",json!({"expected_version":0,"project":baseline})).await?;
    let mut context = json!({"scope":"project","project":baseline});
    context["project"]["shots"][0]["duration"] = json!(3.0);
    store.update_video_shot(&session.id,"manual-subtitle",&json!({"expected_version":1,"shot_id":"first","changes":{"subtitle":"手动字幕"}})).await?;
    let mut generated = baseline.clone();
    generated["shots"][0]["script"] = json!("晨雾散开，人物走向茶园");
    generated["shots"][0]["duration"] = json!(4);
    generated["shots"][0]["sourceIn"] = json!(0.0);
    generated["shots"].as_array_mut().ok_or("分镜列表缺失")?.push(json!({"id":"second","name":"结尾","script":"人物停步望向朝阳","storyboard":"近景","duration":4,"sourceIn":0.0,"volume":1.0}));
    let saved = store.write_video_script_project_from_chat(&session.id,"whole-script",&json!({"expected_version":2,"project":generated}),&context).await?;
    assert_eq!(saved["project"]["shots"].as_array().ok_or("分镜列表缺失")?.len(),2);
    assert_eq!(saved["project"]["shots"][0]["script"],generated["shots"][0]["script"]);
    assert_eq!(saved["project"]["shots"][0]["subtitle"],"手动字幕");
    assert_eq!(saved["project"]["shots"][0]["inputs"],baseline["shots"][0]["inputs"]);
    assert_eq!(saved["project"]["shots"][1]["inputs"],json!({}));
    assert_eq!(saved["project"]["shots"][1]["references"],json!([]));
    assert_eq!(saved["project"]["shots"][1]["subtitle"],"");
    let repeated = store.write_video_script_project_from_chat(&session.id,"whole-script-repeat",&json!({"expected_version":3,"project":saved["project"]}),&context).await?;
    assert_eq!(repeated,saved);
    store.close().await;
    Ok(())
}

#[tokio::test]
async fn whole_script_requests_reject_field_changes_and_edit_conflicts() -> Result<(), Box<dyn std::error::Error>> {
    let root = std::path::PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../.local-cache");
    std::fs::create_dir_all(&root)?;
    let directory = tempfile::Builder::new().prefix("video-whole-conflict-").tempdir_in(root)?;
    let store = Store::open(&directory.path().join("studio.sqlite")).await?;
    let session = store.create_session("whole-conflict").await?;
    let baseline = json!({"version":2,"name":"晨光","width":1280,"height":720,"fps":30,"audio":[],"shots":[{"id":"first","name":"开场","script":"茶园","storyboard":"远景","references":[],"subtitle":"原字幕","duration":3,"sourceIn":0,"inputs":{}}]});
    store.write_video_project(&session.id,"initial",json!({"expected_version":0,"project":baseline})).await?;
    let context = json!({"scope":"project","project":baseline});
    let mut candidate = baseline.clone(); candidate["width"] = json!(1920);
    assert!(store.write_video_script_project_from_chat(&session.id,"wrong-format",&json!({"expected_version":1,"project":candidate}),&context).await.is_err());
    candidate = baseline.clone(); candidate["shots"][0]["subtitle"] = json!("越过范围");
    assert!(store.write_video_script_project_from_chat(&session.id,"wrong-field",&json!({"expected_version":1,"project":candidate}),&context).await.is_err());
    candidate = baseline.clone(); candidate["shots"] = json!([]);
    assert!(store.write_video_script_project_from_chat(&session.id,"delete-shot",&json!({"expected_version":1,"project":candidate}),&context).await.is_err());
    store.update_video_shot(&session.id,"manual-script",&json!({"expected_version":1,"shot_id":"first","changes":{"script":"手动剧情"}})).await?;
    candidate = baseline.clone(); candidate["shots"][0]["script"] = json!("生成剧情");
    assert!(store.write_video_script_project_from_chat(&session.id,"conflict",&json!({"expected_version":2,"project":candidate}),&context).await.is_err());
    assert_eq!(store.read_video_project(&session.id).await?["project"]["shots"][0]["script"],"手动剧情");
    store.close().await;
    Ok(())
}

#[tokio::test]
async fn editor_script_requests_only_update_the_original_unchanged_script() -> Result<(), Box<dyn std::error::Error>> {
    let root = std::path::PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../.local-cache");
    std::fs::create_dir_all(&root)?;
    let directory = tempfile::Builder::new().prefix("video-script-scope-").tempdir_in(root)?;
    let store = Store::open(&directory.path().join("studio.sqlite")).await?;
    let session = store.create_session("video-script-scope").await?;
    let mut project = store.read_video_project(&session.id).await?["project"].clone();
    project["shots"] = json!([
        {"id":"first","name":"开场","script":"原剧本","subtitle":"保留字幕","duration":3,"sourceIn":0,"inputs":{}},
        {"id":"second","name":"结尾","script":"结尾剧情","subtitle":"","duration":3,"sourceIn":0,"inputs":{}}
    ]);
    store.write_video_project(&session.id,"initial",json!({"expected_version":0,"project":project})).await?;
    let context = json!({"shot_id":"first","script":"原剧本"});
    let input = json!({"expected_version":1,"shot_id":"first","changes":{"script":"生成的剧情"}});
    let mut invalid = input.clone();
    invalid["shot_id"] = json!("second");
    assert!(store.update_video_script_from_chat(&session.id,"wrong-target",&invalid,&context).await.is_err());
    invalid = input.clone(); invalid["changes"]["subtitle"] = json!("不允许改写");
    assert!(store.update_video_script_from_chat(&session.id,"wrong-field",&invalid,&context).await.is_err());
    store.update_video_shot(&session.id,"subtitle-edit",&json!({"expected_version":1,"shot_id":"first","changes":{"subtitle":"新的字幕"}})).await?;
    let mut fresh = input.clone(); fresh["expected_version"] = json!(2);
    let saved = store.update_video_script_from_chat(&session.id,"apply-script",&fresh,&context).await?;
    assert_eq!(saved["project"]["shots"][0]["script"],"生成的剧情");
    assert_eq!(saved["project"]["shots"][0]["subtitle"],"新的字幕");
    assert_eq!(saved["project"]["shots"][1],project["shots"][1]);
    fresh["expected_version"] = json!(3); fresh["changes"]["script"] = json!("另一段生成内容");
    assert!(store.update_video_script_from_chat(&session.id,"stale-script",&fresh,&context).await.is_err());
    project = saved["project"].clone(); project["shots"] = json!([project["shots"][1]]);
    store.write_video_project(&session.id,"delete-target",json!({"expected_version":3,"project":project})).await?;
    fresh["expected_version"] = json!(4);
    assert!(store.update_video_script_from_chat(&session.id,"deleted-target",&fresh,&context).await.is_err());
    store.close().await;
    Ok(())
}

#[tokio::test]
async fn editor_references_and_storyboard_survive_chat_updates() -> Result<(), Box<dyn std::error::Error>> {
    let root = std::path::PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../.local-cache");
    std::fs::create_dir_all(&root)?;
    let directory = tempfile::Builder::new().prefix("video-references-").tempdir_in(root)?;
    let store = Store::open(&directory.path().join("studio.sqlite")).await?;
    let session = store.create_session("video-references").await?;
    let asset = store.create_text_asset(&session.id,"角色设定.md","角色穿着蓝色外套","reference-document").await?;
    let version = asset.versions.iter().find(|version| version.version == asset.current_version).ok_or("资产版本缺失")?;
    let reference = json!({"id":"reference-1","assetId":asset.id,"versionId":version.id,"url":version.content_url,"kind":"document","role":"reference"});
    let project = json!({"version":2,"name":"角色短片","width":1280,"height":720,"fps":30,"audio":[],"shots":[{"id":"shot-1","name":"开场","script":"人物进入画面","storyboard":"中景，缓慢推进","references":[reference],"subtitle":"","duration":3,"sourceIn":0,"inputs":{}}]});
    let saved = store.write_video_project(&session.id,"save-reference",json!({"expected_version":0,"project":project})).await?;
    assert_eq!(saved["project"],project);
    let changed = store.update_video_shot(&session.id,"edit-storyboard",&json!({"expected_version":1,"shot_id":"shot-1","changes":{"storyboard":"近景，固定机位"}})).await?;
    assert_eq!(changed["project"]["shots"][0]["references"],project["shots"][0]["references"]);
    let mut incoming = changed["project"].clone();
    let shot = incoming["shots"][0].as_object_mut().ok_or("分镜格式无效")?;
    shot.remove("storyboard");
    shot.remove("references");
    shot.insert("script".into(),json!("人物望向窗外"));
    let written = store.write_video_project_from_chat(&session.id,"chat-preserve-reference",json!({"expected_version":2,"project":incoming})).await?;
    assert_eq!(written["project"]["shots"][0]["references"],project["shots"][0]["references"]);
    assert_eq!(written["project"]["shots"][0]["storyboard"],"近景，固定机位");
    let mut invalid = written["project"].clone();
    invalid["shots"][0]["references"][0]["role"] = json!("first_frame");
    assert!(store.write_video_project(&session.id,"invalid-reference-role",json!({"expected_version":3,"project":invalid})).await.is_err());
    invalid["shots"][0]["references"][0]["role"] = json!("reference");
    invalid["shots"][0]["references"][0]["versionId"] = json!("missing-version");
    assert!(store.write_video_project(&session.id,"missing-reference-version",json!({"expected_version":3,"project":invalid})).await.is_err());
    assert_eq!(store.read_video_project(&session.id).await?,written);
    store.close().await;
    Ok(())
}

#[tokio::test]
async fn projects_preserve_versions_and_reject_conflicting_changes() -> Result<(), Box<dyn std::error::Error>> {
    let root = std::path::PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../.local-cache");
    std::fs::create_dir_all(&root)?;
    let directory = tempfile::Builder::new().prefix("video-project-").tempdir_in(root)?;
    let store = Store::open(&directory.path().join("studio.sqlite")).await?;
    let session = store.create_session("video-project").await?;
    let original = store.read_video_project(&session.id).await?;
    assert_eq!(original["asset_version"], 0);
    let mut project = original["project"].clone();
    project["shots"] = json!([{"id":"shot-1","name":"山间晨雾","script":"晨光穿过茶园","subtitle":"茶香入山","duration":3.0,"sourceIn":0.0,"inputs":{}}]);
    let saved = store.write_video_project(&session.id,"create-project",json!({"expected_version":0,"project":project})).await?;
    assert_eq!(saved["asset_version"], 1);
    project["shots"][0]["name"] = json!("茶园晨光");
    let changed = store.write_video_project(&session.id,"change-project",json!({"expected_version":1,"project":project})).await?;
    assert_eq!(changed["asset_version"], 2);
    assert!(store.write_video_project(&session.id,"stale-change",json!({"expected_version":1,"project":project})).await.is_err());
    assert_eq!(store.read_video_project(&session.id).await?["project"], project);
    let asset = store.get_asset(saved["asset_id"].as_str().ok_or("项目资产缺失")?).await?;
    assert_eq!(asset.versions.len(), 2);
    let first = asset.versions.iter().find(|version| version.version == 1).ok_or("原始版本缺失")?;
    let historical: serde_json::Value = serde_json::from_slice(&store.read_asset(&asset.id,&first.id).await?)?;
    assert_eq!(historical["shots"][0]["name"], "山间晨雾");
    project["shots"][0]["duration"] = json!(-1);
    assert!(store.write_video_project(&session.id,"invalid-duration",json!({"expected_version":2,"project":project})).await.is_err());
    project["shots"][0]["duration"] = json!(3);
    let duplicate = project["shots"][0].clone();
    project["shots"].as_array_mut().ok_or("分镜缺失")?.push(duplicate);
    assert!(store.write_video_project(&session.id,"duplicate-shot",json!({"expected_version":2,"project":project})).await.is_err());
    assert_eq!(store.read_video_project(&session.id).await?["asset_version"],2);
    store.close().await;
    Ok(())
}

#[tokio::test]
async fn concurrent_initial_writes_create_one_project() -> Result<(), Box<dyn std::error::Error>> {
    let root = std::path::PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../.local-cache");
    std::fs::create_dir_all(&root)?;
    let directory = tempfile::Builder::new().prefix("video-concurrency-").tempdir_in(root)?;
    let store = Store::open(&directory.path().join("studio.sqlite")).await?;
    let session = store.create_session("video-concurrency").await?;
    let original = store.read_video_project(&session.id).await?;
    let mut second = original["project"].clone();
    second["name"] = json!("另一项修改");
    let (first,second) = tokio::join!(
        store.write_video_project(&session.id,"first-create",json!({"expected_version":0,"project":original["project"]})),
        store.write_video_project(&session.id,"second-create",json!({"expected_version":0,"project":second})),
    );
    assert_ne!(first.is_ok(),second.is_ok());
    let documents = store.list_session_assets(&session.id).await?;
    assert_eq!(documents.iter().filter(|asset| asset.name == "video-project.json.md").count(),1);
    store.close().await;
    Ok(())
}

#[tokio::test]
async fn chat_updates_preserve_shot_state_and_use_project_version_checks() -> Result<(), Box<dyn std::error::Error>> {
    let root = std::path::PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../.local-cache");
    std::fs::create_dir_all(&root)?;
    let directory = tempfile::Builder::new().prefix("video-chat-update-").tempdir_in(root)?;
    let store = Store::open(&directory.path().join("studio.sqlite")).await?;
    let session = store.create_session("video-chat-update").await?;
    let mut project = store.read_video_project(&session.id).await?["project"].clone();
    project["audio"] = json!([]);
    project["shots"] = json!([
        {"id":"shot-1","name":"晨雾","script":"晨雾散去","subtitle":"山间茶香","duration":3,"sourceIn":0,"inputs":{},"volume":0.4,"versions":[],"executionInputs":{"prompt":"晨雾"},"workflowDrafts":{}},
        {"id":"shot-2","name":"茶叶","script":"茶叶露珠","subtitle":"清晨","duration":3,"sourceIn":0,"inputs":{}}
    ]);
    store.write_video_project(&session.id,"chat-initial",json!({"expected_version":0,"project":project})).await?;
    let mut incoming = project.clone();
    incoming.as_object_mut().ok_or("项目格式无效")?.remove("audio");
    for key in ["volume","versions","executionInputs","workflowDrafts"] {incoming["shots"][0].as_object_mut().ok_or("分镜格式无效")?.remove(key);}
    incoming["shots"][0]["script"] = json!("柔和晨光穿过山间");
    let saved = store.write_video_project_from_chat(&session.id,"chat-project-change",json!({"expected_version":1,"project":incoming})).await?;
    assert_eq!(saved["project"]["shots"][0]["volume"],0.4);
    assert_eq!(saved["project"]["shots"][0]["executionInputs"],project["shots"][0]["executionInputs"]);
    assert_eq!(saved["project"]["audio"],json!([]));
    let updated = store.update_video_shot(&session.id,"chat-shot-change",&json!({"expected_version":2,"shot_id":"shot-1","changes":{"name":"山间晨光","duration":2.5}})).await?;
    assert_eq!(updated["asset_version"],3);
    assert_eq!(updated["project"]["shots"][0]["name"],"山间晨光");
    assert_eq!(updated["project"]["shots"][0]["duration"],2.5);
    assert_eq!(updated["project"]["shots"][0]["volume"],0.4);
    assert_eq!(updated["project"]["shots"][1],project["shots"][1]);
    assert!(store.update_video_shot(&session.id,"chat-stale",&json!({"expected_version":2,"shot_id":"shot-1","changes":{"name":"过期修改"}})).await.is_err());
    assert!(store.update_video_shot(&session.id,"chat-protected",&json!({"expected_version":3,"shot_id":"shot-1","changes":{"media":null}})).await.is_err());
    assert!(store.update_video_shot(&session.id,"chat-missing",&json!({"expected_version":3,"shot_id":"missing","changes":{"name":"山间"}})).await.is_err());
    let error = store.update_video_shot(&session.id,"chat-fractional-frame",&json!({"expected_version":3,"shot_id":"shot-1","changes":{"duration":2.51}})).await.expect_err("非整数帧时长应拒绝");
    assert!(error.to_string().contains("分镜时长需要按项目帧率设置"));
    assert_eq!(store.read_video_project(&session.id).await?,updated);
    store.close().await;
    Ok(())
}
