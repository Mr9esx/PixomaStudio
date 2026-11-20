use crate::{Error, Result, Store, local_api::text};
use serde_json::{Value, json};
use std::collections::HashSet;

const PROJECT_NAME: &str = "video-project.json.md";

fn validate_input_sources(value: &Value) -> Result<()> {
    let sources = value.as_object().ok_or(Error::Invalid("输入来源格式无效"))?;
    for (key,source) in sources {
        if key.is_empty() || !matches!(source["field"].as_str(),Some("script" | "storyboard")) || !source["sourceText"].is_string() || !source["value"].is_string() || !source["edited"].is_boolean() || source.get("revision").is_some_and(|revision|revision.as_u64().is_none_or(|revision|revision==0)) {
            return Err(Error::Invalid("输入来源格式无效"));
        }
    }
    Ok(())
}

fn preserve_chat_input_sources(previous: &Value, shot: &mut Value) -> Result<()> {
    if previous["workflowId"] == shot["workflowId"] && previous["versionId"] == shot["versionId"] {
        if shot.get("inputSources").is_none() {
            if let Some(sources) = previous.get("inputSources") {shot["inputSources"] = sources.clone();}
        }
    } else {
        if let (Some(workflow),Some(version)) = (previous["workflowId"].as_str(),previous["versionId"].as_str()) {
            if shot.get("workflowDrafts").is_none() {shot["workflowDrafts"] = json!({});}
            let mut draft = json!({"inputs":previous["inputs"]});
            if let Some(sources) = previous.get("inputSources") {draft["inputSources"] = sources.clone();}
            let mut references = serde_json::Map::new();
            for reference in previous["references"].as_array().into_iter().flatten() {
                if let (Some(id),Some(key)) = (reference["id"].as_str(),reference["inputKey"].as_str()) {references.insert(id.into(),json!(key));}
            }
            draft["referenceInputs"] = Value::Object(references);
            shot["workflowDrafts"].as_object_mut().ok_or(Error::Invalid("工作流草稿格式无效"))?.insert(format!("{workflow}:{version}"),draft);
        }
        if shot.get("inputSources").is_none() || shot["inputSources"] == previous["inputSources"] {shot["inputSources"] = json!({});}
        if shot["references"] == previous["references"] {
            for reference in shot.get_mut("references").and_then(Value::as_array_mut).into_iter().flatten() {
                if let Some(reference) = reference.as_object_mut() {reference.remove("inputKey");}
            }
        }
    }
    Ok(())
}

fn same_script_field(left: Option<&Value>, right: Option<&Value>) -> bool {
    left == right || left.and_then(Value::as_f64).zip(right.and_then(Value::as_f64)).is_some_and(|(left,right)|left==right)
}

fn adopt_result_media(shot: &mut Value, media: Value, source_in: f64, duration: f64, fps: f64) -> Result<()> {
    let available = media["duration"].as_f64().ok_or(Error::Invalid("视频时长尚未就绪"))?;
    if !available.is_finite() || available <= 0.0 || !source_in.is_finite() || !duration.is_finite() || source_in < 0.0 || duration <= 0.0 || source_in + duration > available + 1e-6 {
        return Err(Error::Invalid("选择有效的视频使用范围"));
    }
    if [source_in,duration].iter().any(|value|(value*fps-(value*fps).round()).abs()>1e-6) {
        return Err(Error::Invalid("使用范围需要符合项目帧间隔"));
    }
    let mut versions = match shot.get("versions") {
        Some(value) => value.as_array().ok_or(Error::Invalid("分镜视频版本格式无效"))?.clone(),
        None => Vec::new(),
    };
    versions.retain(|record|record["media"]["versionId"] != media["versionId"]);
    if let Some(previous) = shot.get("media").filter(|previous|!previous.is_null() && previous["versionId"] != media["versionId"]).cloned() {
        versions.retain(|record|record["media"]["versionId"] != previous["versionId"]);
        versions.push(json!({"id":previous["versionId"],"media":previous,"status":"history","sourceIn":shot["sourceIn"],"duration":shot["duration"]}));
    }
    if shot.get("media").is_none_or(Value::is_null) && shot.get("plannedDuration").is_none() && shot["duration"].as_f64().is_some_and(|value|value > 0.0) {
        shot["plannedDuration"] = shot["duration"].clone();
    }
    shot["versions"] = json!(versions);
    shot["media"] = media;
    shot["sourceIn"] = json!(source_in);
    shot["duration"] = json!(duration);
    Ok(())
}

fn preserve_chat_project_state(current: &Value, incoming: &mut Value) -> Result<()> {
    let project = incoming.as_object_mut().ok_or(Error::Invalid("视频项目需要对象"))?;
    if current["version"] == 2 {project.insert("version".into(),json!(2));}
    if !project.contains_key("audio") {
        if let Some(audio) = current.get("audio") {project.insert("audio".into(),audio.clone());}
    }
    for key in ["script", "proposals"] {
        if !project.contains_key(key) {
            if let Some(value) = current.get(key) {project.insert(key.into(),value.clone());}
        }
    }
    if let Some(blocks) = project.get_mut("script").and_then(|script|script.get_mut("blocks")).and_then(Value::as_array_mut) {
        for block in blocks {
            if block.get("ancestorIds").is_none() {
                let ancestors = current["script"]["blocks"].as_array().and_then(|blocks|blocks.iter().find(|previous|previous["id"] == block["id"])).and_then(|previous|previous.get("ancestorIds"));
                if let Some(ancestors) = ancestors {block["ancestorIds"] = ancestors.clone();}
            }
        }
    }
    let shots = project.get_mut("shots").and_then(Value::as_array_mut).ok_or(Error::Invalid("分镜需要列表"))?;
    for shot in shots {
        let Some(previous) = current["shots"].as_array().and_then(|shots|shots.iter().find(|previous|previous["id"]==shot["id"])) else {continue};
        let fields = shot.as_object_mut().ok_or(Error::Invalid("分镜需要对象"))?;
        for key in ["media","versions","executionId","executionInputs","workflowDrafts","volume","workflowId","versionId","storyboard","references","generationRequest","scriptSources","plannedDuration"] {
            if !fields.contains_key(key) {
                if let Some(value) = previous.get(key) {fields.insert(key.into(),value.clone());}
            }
        }
        preserve_chat_input_sources(previous,shot)?;
    }
    Ok(())
}

pub(crate) fn validate_project_script(project: &Value) -> Result<()> {
    if let Some(script) = project.get("script") {
        let revision = script["revision"].as_u64().ok_or(Error::Invalid("脚本修订号无效"))?;
        let blocks = script["blocks"].as_array().ok_or(Error::Invalid("脚本文本块需要列表"))?;
        let mut ids = HashSet::new();
        for block in blocks {
            if !ids.insert(text(block,"id")?) {return Err(Error::Invalid("脚本文本块标识重复"));}
            if !block["text"].is_string() || block["revision"].as_u64().is_none_or(|value|value == 0 || value > revision) {return Err(Error::Invalid("脚本内容或修订号无效"));}
        }
        for block in blocks {
            if let Some(value) = block.get("ancestorIds") {
                let ancestors = value.as_array().ok_or(Error::Invalid("脚本来源标识需要列表"))?;
                let mut visited = HashSet::new();
                for ancestor in ancestors {
                    let id = ancestor.as_str().filter(|id|!id.is_empty()).ok_or(Error::Invalid("脚本来源标识无效"))?;
                    if ids.contains(id) || !visited.insert(id) {return Err(Error::Invalid("脚本来源标识重复或包含当前文本块"));}
                }
            }
        }
    }
    if let Some(shots) = project["shots"].as_array() {
        for shot in shots {
            if let Some(sources) = shot.get("scriptSources") {
                let sources = sources.as_array().ok_or(Error::Invalid("分镜原文关联需要列表"))?;
                let mut ids = HashSet::new();
                for source in sources {
                    if !ids.insert(text(source,"blockId")?) {return Err(Error::Invalid("分镜原文关联重复"));}
                    if !source["text"].is_string() || source["revision"].as_u64().is_none_or(|value|value == 0) {return Err(Error::Invalid("分镜原文关联无效"));}
                }
            }
        }
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn chat_workflow_changes_preserve_prior_input_sources_in_drafts() -> Result<()> {
        let previous = json!({"workflowId":"workflow","versionId":"first","inputs":{"prompt":"旧描述"},"inputSources":{"prompt":{"field":"storyboard","sourceText":"旧描述","value":"旧描述","edited":false}},"references":[{"id":"person","inputKey":"image"}]});
        let mut next = json!({"workflowId":"workflow","versionId":"second","inputs":{"prompt":"新输入"},"references":previous["references"]});
        preserve_chat_input_sources(&previous,&mut next)?;
        assert_eq!(next["inputSources"],json!({}));
        assert_eq!(next["workflowDrafts"]["workflow:first"]["inputs"],previous["inputs"]);
        assert_eq!(next["workflowDrafts"]["workflow:first"]["inputSources"],previous["inputSources"]);
        assert_eq!(next["workflowDrafts"]["workflow:first"]["referenceInputs"]["person"],"image");
        assert!(next["references"][0].get("inputKey").is_none());
        let mut same = json!({"workflowId":"workflow","versionId":"first","inputs":{"prompt":"手工修改"}});
        preserve_chat_input_sources(&previous,&mut same)?;
        assert_eq!(same["inputSources"],previous["inputSources"]);
        assert_eq!(same["inputs"]["prompt"],"手工修改");
        Ok(())
    }

    #[test]
    fn adopting_media_requires_valid_explicit_range_and_retains_history() -> Result<()> {
        let previous = json!({"assetId":"old","versionId":"old-version","duration":8});
        let media = json!({"assetId":"new","versionId":"new-version","duration":2});
        let original = json!({"media":previous,"sourceIn":1,"duration":4,"plannedDuration":5,"versions":[]});
        for (start,duration) in [(0.0,4.0),(-1.0,1.0),(1.0,2.0),(0.0,0.0),(0.0,1.01)] {
            let mut shot = original.clone();
            assert!(adopt_result_media(&mut shot,media.clone(),start,duration,30.0).is_err());
            assert_eq!(shot,original);
        }
        let mut shot = original.clone();
        adopt_result_media(&mut shot,media.clone(),0.0,2.0,30.0)?;
        assert_eq!(shot["media"],media);
        assert_eq!(shot["duration"],2.0);
        assert_eq!(shot["plannedDuration"],5);
        assert_eq!(shot["versions"],json!([{"id":"old-version","media":previous,"status":"history","sourceIn":1,"duration":4}]));
        let mut legacy = json!({"sourceIn":0,"duration":3,"inputs":{"duration":8}});
        adopt_result_media(&mut legacy,media,0.0,2.0,30.0)?;
        assert_eq!(legacy["plannedDuration"],3);
        assert_eq!(legacy["duration"],2.0);
        assert_eq!(legacy["inputs"]["duration"],8);
        Ok(())
    }

    #[test]
    fn chat_project_edits_preserve_omitted_shot_state_and_audio() -> Result<()> {
        let current = json!({"audio":[{"id":"music"}],"shots":[{"id":"shot-1","media":{"assetId":"video-asset","versionId":"video-version"},"versions":[],"executionId":"execution","executionInputs":{"prompt":"晨雾"},"workflowDrafts":{},"volume":0.4}]});
        let mut incoming = json!({"shots":[{"id":"shot-1","name":"山间晨光"},{"id":"shot-2","name":"茶叶露珠"}]});
        preserve_chat_project_state(&current,&mut incoming)?;
        for field in ["media","versions","executionId","executionInputs","workflowDrafts","volume"] {assert_eq!(incoming["shots"][0][field],current["shots"][0][field]);}
        assert_eq!(incoming["audio"],current["audio"]);
        assert!(incoming["shots"][1].get("media").is_none());
        incoming["shots"][0]["media"] = Value::Null;
        incoming["audio"] = json!([]);
        preserve_chat_project_state(&current,&mut incoming)?;
        assert!(incoming["shots"][0]["media"].is_null());
        assert_eq!(incoming["audio"],json!([]));
        assert_eq!(incoming["shots"][0]["name"],"山间晨光");
        Ok(())
    }
}

impl Store {
    pub async fn write_video_script_project_from_chat(&self, session: &str, request: &str, input: &Value, context: &Value) -> Result<Value> {
        if context["scope"] != "project" {return Err(Error::Invalid("整片剧本生成范围缺失"));}
        let baseline = context.get("project").filter(|value|value.is_object()).ok_or(Error::Invalid("整片剧本原始内容缺失"))?;
        let incoming = input.get("project").and_then(Value::as_object).ok_or(Error::Invalid("视频项目需要对象"))?;
        if incoming.get("version").is_some_and(|version|!matches!(version.as_u64(),Some(1 | 2))) {return Err(Error::Invalid("视频项目格式无效"));}
        let record = self.read_video_project(session).await?;
        if input["expected_version"] != record["asset_version"] {return Err(Error::Conflict("视频项目已更新，重新读取后编辑".into()));}
        let current = &record["project"];
        if input["project"] == *current {return Ok(record);}
        if baseline["fps"] != current["fps"] {return Err(Error::Conflict("项目帧率已改变，请在对话中保留生成内容".into()));}
        for (key,value) in incoming {
            if ["version","name","shots"].contains(&key.as_str()) {continue;}
            if !["width","height","fps","audio","script"].contains(&key.as_str()) || (!same_script_field(Some(value),baseline.get(key)) && !same_script_field(Some(value),current.get(key))) {return Err(Error::Invalid("分镜生成不能修改完整脚本、规格或已有音频"));}
        }
        let original_shots = baseline["shots"].as_array().ok_or(Error::Invalid("原始分镜列表缺失"))?;
        let current_shots = current["shots"].as_array().ok_or(Error::Invalid("当前分镜列表缺失"))?;
        let generated_shots = incoming.get("shots").and_then(Value::as_array).ok_or(Error::Invalid("生成分镜列表缺失"))?;
        if original_shots.iter().map(|shot|&shot["id"]).collect::<Vec<_>>() != current_shots.iter().map(|shot|&shot["id"]).collect::<Vec<_>>() {return Err(Error::Conflict("分镜数量或顺序已改变，请在对话中保留生成内容".into()));}
        if current_shots.iter().any(|shot|!generated_shots.iter().any(|generated|generated["id"]==shot["id"])) {return Err(Error::Invalid("整片剧本生成需要保留已有分镜"));}
        let mut project = current.clone();
        if let Some(name) = incoming.get("name") {
            if name != &baseline["name"] {
                if current["name"] != baseline["name"] && name != &current["name"] {return Err(Error::Conflict("视频名称已被修改，请保留生成内容".into()));}
                project["name"] = name.clone();
            }
        }
        let creative_fields = ["name","script","storyboard","duration"];
        let mut shots = Vec::new();
        for generated in generated_shots {
            let id = text(generated,"id")?;
            let fields = generated.as_object().ok_or(Error::Invalid("生成分镜需要对象"))?;
            let mut shot = if let Some(previous) = current_shots.iter().find(|shot|shot["id"]==id) {
                let original = original_shots.iter().find(|shot|shot["id"]==id).ok_or(Error::Invalid("原始分镜缺失"))?;
                let mut shot = previous.clone();
                for (key,value) in fields {
                    if key == "id" {continue;}
                    if creative_fields.contains(&key.as_str()) {
                        if !same_script_field(Some(value),original.get(key)) {
                            if !same_script_field(previous.get(key),original.get(key)) && !same_script_field(Some(value),previous.get(key)) {return Err(Error::Conflict(format!("分镜 {} 的 {} 已被修改，请在对话中保留生成内容",id,key)));}
                            shot[key] = value.clone();
                        }
                    } else if !same_script_field(Some(value),original.get(key)) && !same_script_field(Some(value),previous.get(key)) {return Err(Error::Invalid("整片剧本请求只能修改分镜名称、剧本、画面描述和时长"));}
                }
                shot
            } else {
                if ["name","script","duration"].iter().any(|key|!fields.contains_key(*key)) {return Err(Error::Invalid("新分镜需要名称、剧本和时长"));}
                let mut shot = json!({"id":id,"name":"","script":"","storyboard":"","duration":3,"sourceIn":0,"subtitle":"","inputs":{},"references":[],"volume":1,"versions":[]});
                for (key,value) in fields {
                    if key == "id" || creative_fields.contains(&key.as_str()) {shot[key] = value.clone();}
                    else if !same_script_field(Some(value),shot.get(key)) {return Err(Error::Invalid("新剧本分镜不能包含素材、字幕或生成配置"));}
                }
                shot
            };
            let fields = shot.as_object_mut().ok_or(Error::Invalid("分镜需要对象"))?;
            fields.entry("storyboard").or_insert(json!(""));
            fields.entry("references").or_insert(json!([]));
            shots.push(shot);
        }
        project["version"] = json!(2);
        project["shots"] = Value::Array(shots);
        if project.get("audio").is_none() {project["audio"] = json!([]);}
        self.write_video_project(session,request,json!({"expected_version":record["asset_version"],"project":project})).await
    }

    pub async fn update_video_script_from_chat(&self, session: &str, request: &str, input: &Value, context: &Value) -> Result<Value> {
        let shot_id = text(context,"shot_id")?;
        if input["shot_id"] != shot_id {return Err(Error::Invalid("剧本生成只能修改请求中的分镜"));}
        let changes = input["changes"].as_object().ok_or(Error::Invalid("剧本修改需要对象"))?;
        if changes.len() != 1 || !changes.get("script").is_some_and(Value::is_string) {return Err(Error::Invalid("剧本生成只能修改 script 字段"));}
        let baseline = context["script"].as_str().ok_or(Error::Invalid("剧本原始内容缺失"))?;
        let current = self.read_video_project(session).await?;
        let shot = current["project"]["shots"].as_array().and_then(|shots|shots.iter().find(|shot|shot["id"]==shot_id)).ok_or(Error::Conflict("目标分镜已被删除，请在对话中保留生成内容".into()))?;
        if shot["script"] != baseline && shot["script"] != changes["script"] {return Err(Error::Conflict("剧本已被修改，请在对话中保留生成内容，等待重新采用".into()));}
        if input["expected_version"] != current["asset_version"] {return Err(Error::Conflict("视频项目已更新，重新读取后编辑".into()));}
        self.update_video_shot(session,request,input).await
    }

    pub async fn write_video_project_from_chat(&self, session: &str, request: &str, mut input: Value) -> Result<Value> {
        let current = self.read_video_project(session).await?;
        if input["expected_version"] != current["asset_version"] {return Err(Error::Conflict("视频项目已更新，重新读取后编辑".into()));}
        preserve_chat_project_state(&current["project"],input.get_mut("project").ok_or(Error::Invalid("视频项目缺失"))?)?;
        self.write_video_project(session,request,input).await
    }

    pub async fn update_video_shot(&self, session: &str, request: &str, input: &Value) -> Result<Value> {
        let current = self.read_video_project(session).await?;
        if input["expected_version"] != current["asset_version"] {return Err(Error::Conflict("视频项目已更新，重新读取后编辑".into()));}
        let shot_id = text(input,"shot_id")?;
        let changes = input["changes"].as_object().ok_or(Error::Invalid("分镜修改需要对象"))?;
        if changes.is_empty() || changes.keys().any(|key|!["name","script","storyboard","references","subtitle","duration","plannedDuration","sourceIn","inputs","workflowId","versionId","volume"].contains(&key.as_str())) {return Err(Error::Invalid("分镜修改字段无效"));}
        let mut project = current["project"].clone();
        let previous = project["shots"].as_array().and_then(|shots|shots.iter().find(|shot|shot["id"]==shot_id)).cloned().ok_or(Error::Invalid("分镜不存在"))?;
        let shot = project["shots"].as_array_mut().ok_or(Error::Invalid("视频项目缺少分镜"))?.iter_mut().find(|shot|shot["id"]==shot_id).ok_or(Error::Invalid("分镜不存在"))?.as_object_mut().ok_or(Error::Invalid("分镜需要对象"))?;
        for (key,value) in changes {
            if key == "plannedDuration" && value.is_null() {shot.remove(key);}
            else {shot.insert(key.clone(),value.clone());}
        }
        let updated = project["shots"].as_array_mut().ok_or(Error::Invalid("视频项目缺少分镜"))?.iter_mut().find(|shot|shot["id"]==shot_id).ok_or(Error::Invalid("分镜不存在"))?;
        if previous["workflowId"] != updated["workflowId"] || previous["versionId"] != updated["versionId"] {updated.as_object_mut().ok_or(Error::Invalid("分镜需要对象"))?.remove("inputSources");}
        preserve_chat_input_sources(&previous,updated)?;
        self.write_video_project(session,request,json!({"expected_version":input["expected_version"],"project":project})).await
    }

    pub async fn read_video_project(&self, session: &str) -> Result<Value> {
        self.get_session(session).await?;
        let work: Option<String> = sqlx::query_scalar("SELECT id FROM video_works WHERE main_session_id=?")
            .bind(session).fetch_optional(&self.pool).await?;
        if let Some(id) = work {
            let work = self.read_video_work(&id).await?;
            return Ok(json!({"asset_id":id,"asset_version":work["asset_version"],"project":work["project"]}));
        }
        let asset_id: Option<String> = sqlx::query_scalar("SELECT asset_id FROM asset_operations WHERE request_id=? AND operation='create'")
            .bind(format!("video-project:init:{session}")).fetch_optional(&self.pool).await?;
        let asset = match asset_id {
            Some(id) => Some(self.get_asset(&id).await?),
            None => {
                let others: Vec<String> = sqlx::query_scalar("SELECT id FROM video_works WHERE main_session_id IS NULL OR main_session_id!=?")
                    .bind(session).fetch_all(&self.pool).await?;
                self.list_session_assets(session).await?.into_iter().find(|asset|asset.name == PROJECT_NAME && !others.contains(&asset.id))
            },
        };
        let Some(asset) = asset else {
            return Ok(json!({"asset_id":null,"asset_version":0,"project":{"version":1,"name":"未命名视频","width":1280,"height":720,"fps":30,"shots":[]}}));
        };
        let version = asset.versions.iter().find(|version| version.version == asset.current_version).ok_or(Error::State("视频项目版本缺失"))?;
        Ok(json!({"asset_id":asset.id,"asset_version":asset.current_version,"project":serde_json::from_slice::<Value>(&self.read_asset(&asset.id,&version.id).await?)?}))
    }

    pub async fn write_video_project(&self, session: &str, request: &str, input: Value) -> Result<Value> {
        self.write_video_record(session,request,input,None).await
    }

    pub(crate) async fn write_video_record(&self, session: &str, request: &str, input: Value, video_id: Option<&str>) -> Result<Value> {
        let expected = input["expected_version"].as_i64().ok_or(Error::Invalid("视频项目版本缺失"))?;
        let mut project = input.get("project").ok_or(Error::Invalid("视频项目缺失"))?.clone();
        let current = match video_id {
            Some(id) => self.read_video_work(id).await?,
            None => self.read_video_project(session).await?,
        };
        if current["asset_version"] != expected { return Err(Error::Conflict("视频项目已更新，重新读取后编辑".into())); }
        for shot in project.get_mut("shots").and_then(Value::as_array_mut).into_iter().flatten() {
            let previous = current["project"]["shots"].as_array().and_then(|shots|shots.iter().find(|item|item["id"]==shot["id"]));
            if shot.get("contentRevisions").is_some() || previous.is_some_and(|previous|previous.get("contentRevisions").is_some()) {
                if let Some(revisions) = shot.get("contentRevisions") {
                    if ["script","storyboard"].iter().any(|field|revisions[field].as_u64().is_none_or(|value|value==0)) {return Err(Error::Invalid("分镜文字修订格式无效"));}
                }
                let mut revisions = json!({});
                for field in ["script","storyboard"] {
                    let before = previous.and_then(|previous|previous["contentRevisions"][field].as_u64()).unwrap_or(1);
                    let supplied = shot["contentRevisions"][field].as_u64().unwrap_or(before);
                    let changed = previous.is_some_and(|previous|previous[field].as_str().unwrap_or("")!=shot[field].as_str().unwrap_or(""));
                    revisions[field] = json!(if changed && supplied==before {before.checked_add(1).ok_or(Error::Invalid("分镜文字修订超出范围"))?} else {supplied});
                }
                shot["contentRevisions"] = revisions;
            }
        }
        let project = &project;
        if !matches!(project["version"].as_u64(), Some(1 | 2)) || project["name"].as_str().is_none_or(|name| name.trim().is_empty()) { return Err(Error::Invalid("视频项目格式无效")); }
        validate_project_script(project)?;
        crate::video_creation::validate_video_proposals(project)?;
        for key in ["width","height","fps"] {
            let value = project[key].as_u64().ok_or(Error::Invalid("视频尺寸或帧率无效"))?;
            let valid = if key == "fps" { (1..=60).contains(&value) } else { (16..=4096).contains(&value) && value % 2 == 0 };
            if !valid { return Err(Error::Invalid("视频尺寸或帧率超出范围")); }
        }
        let shots = project["shots"].as_array().ok_or(Error::Invalid("分镜需要列表"))?;
        if shots.len() > 100 { return Err(Error::Invalid("分镜数量超过一百")); }
        let mut ids = HashSet::new();
        let mut total = 0.0;
        let mut unknown_timing = false;
        for shot in shots {
            if !ids.insert(text(shot,"id")?) { return Err(Error::Invalid("分镜标识重复")); }
            text(shot,"name")?;
            if !shot["script"].is_string() || !shot["subtitle"].is_string() || !shot["inputs"].is_object() { return Err(Error::Invalid("分镜剧本、字幕或参数格式无效")); }
            if let Some(sources) = shot.get("inputSources") {validate_input_sources(sources)?;}
            if let Some(drafts) = shot.get("workflowDrafts") {
                for (key,draft) in drafts.as_object().ok_or(Error::Invalid("工作流草稿格式无效"))? {
                    if key.is_empty() || !draft["inputs"].is_object() {return Err(Error::Invalid("工作流草稿格式无效"));}
                    if let Some(sources) = draft.get("inputSources") {validate_input_sources(sources)?;}
                    if let Some(references) = draft.get("referenceInputs") {
                        if references.as_object().is_none_or(|references|references.iter().any(|(id,key)|id.is_empty() || key.as_str().is_none_or(str::is_empty))) {return Err(Error::Invalid("工作流草稿格式无效"));}
                    }
                }
            }
            if project["version"] == 2 && (!shot["storyboard"].is_string() || !shot["references"].is_array()) {return Err(Error::Invalid("分镜画面描述或参考格式无效"));}
            if let Some(references) = shot.get("references") {
                let references = references.as_array().ok_or(Error::Invalid("参考资产需要列表"))?;
                let mut reference_ids = HashSet::new();
                let mut reference_uses = HashSet::new();
                for reference in references {
                    if !reference_ids.insert(text(reference,"id")?) {return Err(Error::Invalid("参考资产标识重复"));}
                    let kind = text(reference,"kind")?;
                    let role = text(reference,"role")?;
                    if !["image","audio","video","document"].contains(&kind) || !["storyboard","character","scene","style","first_frame","last_frame","voice","rhythm","reference"].contains(&role) {return Err(Error::Invalid("参考资产格式无效"));}
                    if (["storyboard","character","scene","style","first_frame","last_frame"].contains(&role) && kind != "image") || (["voice","rhythm"].contains(&role) && kind != "audio") {return Err(Error::Invalid("参考用途与资产类型不匹配"));}
                    let asset_id = text(reference,"assetId")?;
                    let version_id = text(reference,"versionId")?;
                    if !reference_uses.insert((asset_id,version_id,role,reference["inputKey"].as_str())) {return Err(Error::Invalid("参考资产用途重复"));}
                    if reference.get("sourceShotId").is_some() != reference.get("sourceMediaVersionId").is_some() {return Err(Error::Invalid("参考来源信息不完整"));}
                    if let Some(position) = reference.get("sourcePosition") {
                        text(reference,"sourceShotId")?;
                        if !matches!(position.as_str(),Some("first" | "last" | "fixed")) {return Err(Error::Invalid("参考画面类型无效"));}
                        if position == "fixed" && reference.get("sourceFrame").is_none() {return Err(Error::Invalid("指定画面位置缺失"));}
                    }
                    for (frame_key,fps_key) in [("sourceFrame","sourceFps"),("confirmedSourceFrame","confirmedSourceFps")] {
                        if reference.get(frame_key).is_some() || reference.get(fps_key).is_some() {
                            text(reference,"sourceShotId")?;
                            if reference[frame_key].as_u64().is_none() || reference[fps_key].as_u64().is_none_or(|fps|!(1..=60).contains(&fps)) {return Err(Error::Invalid("参考画面位置或帧率无效"));}
                        }
                    }
                    if self.get_asset(asset_id).await?.kind != kind {return Err(Error::Invalid("参考资产类型无效"));}
                    self.asset_path(asset_id,version_id).await?;
                }
            }
            let duration = shot["duration"].as_f64().ok_or(Error::Invalid("分镜时长缺失"))?;
            let source_in = shot["sourceIn"].as_f64().ok_or(Error::Invalid("剪辑开始位置缺失"))?;
            let has_media = shot.get("media").is_some_and(|media|!media.is_null());
            if !duration.is_finite() || duration < 0.0 || (has_media && duration == 0.0) || duration > 3600.0 || !source_in.is_finite() || source_in < 0.0 { return Err(Error::Invalid("分镜剪辑范围无效")); }
            let frames = duration * project["fps"].as_f64().ok_or(Error::Invalid("视频帧率无效"))?;
            if (frames-frames.round()).abs()>1e-6 {return Err(Error::Invalid("分镜时长需要按项目帧率设置"));}
            let planned = shot.get("plannedDuration").map(|value| value.as_f64().ok_or(Error::Invalid("计划时长无效"))).transpose()?;
            if let Some(planned) = planned {
                let frames = planned * project["fps"].as_f64().ok_or(Error::Invalid("视频帧率无效"))?;
                if !planned.is_finite() || planned <= 0.0 || planned > 3600.0 || (frames-frames.round()).abs()>1e-6 {return Err(Error::Invalid("计划时长需要符合项目帧间隔且不超过一小时"));}
            }
            let effective = if has_media {duration} else {planned.unwrap_or(duration)};
            unknown_timing |= effective == 0.0;
            total += effective;
            if total > 3600.0 { return Err(Error::Invalid("视频时长超过一小时")); }
            if shot.get("volume").is_some_and(|volume|volume.as_f64().is_none_or(|volume|!(0.0..=2.0).contains(&volume))) {return Err(Error::Invalid("分镜音量无效"));}
            match (shot["workflowId"].as_str(), shot["versionId"].as_str()) {
                (Some(id),Some(version)) => {
                    let workflow = self.get_workflow_version(id,version).await?;
                    let mut schema = workflow["definition"]["input_schema"].clone();
                    schema.as_object_mut().ok_or(Error::State("工作流Schema无效"))?.remove("required");
                    jsonschema::validator_for(&schema).map_err(|error| Error::Validation(error.to_string()))?.validate(&shot["inputs"]).map_err(|error| Error::Validation(error.to_string()))?;
                },
                (None,None) => {},
                _ => return Err(Error::Invalid("工作流与版本需要同时选择")),
            }
            if let Some(media) = shot.get("media").filter(|media|!media.is_null()) {
                if self.get_asset(text(media,"assetId")?).await?.kind != "video" {return Err(Error::Invalid("分镜素材需要视频"));}
                self.asset_path(text(media,"assetId")?,text(media,"versionId")?).await?;
                let metadata = self.ensure_video_metadata(text(media,"assetId")?,text(media,"versionId")?).await?;
                let available = metadata["duration"].as_f64().ok_or(Error::State("视频素材时长缺失"))?;
                if source_in + duration > available + 1.0 / project["fps"].as_f64().ok_or(Error::Invalid("视频帧率无效"))? {return Err(Error::Invalid("剪辑范围超出源视频"));}
            }
        }
        if let Some(audio) = project.get("audio") {
            let clips = audio.as_array().ok_or(Error::Invalid("音频需要列表"))?;
            if clips.len() > 100 {return Err(Error::Invalid("音频数量超过一百"));}
            let mut audio_ids = HashSet::new();
            for clip in clips {
                if !audio_ids.insert(text(clip,"id")?) {return Err(Error::Invalid("音频标识重复"));}
                let values: Vec<f64> = ["start","duration","sourceIn","volume"].iter().map(|key|clip[key].as_f64().ok_or(Error::Invalid("音频剪辑参数无效"))).collect::<Result<_>>()?;
                if !values.iter().all(|value|value.is_finite()) || values[0] < 0.0 || (!unknown_timing && values[0] >= total) || values[1] <= 0.0 || values[2] < 0.0 || !(0.0..=2.0).contains(&values[3]) || !clip["loop"].is_boolean() {return Err(Error::Invalid("音频剪辑范围或音量无效"));}
                let asset = self.get_asset(text(clip,"assetId")?).await?;
                if !["audio","video"].contains(&asset.kind.as_str()) {return Err(Error::Invalid("音频素材格式无效"));}
                self.asset_path(&asset.id,text(clip,"versionId")?).await?;
            }
        }
        if let Some(id) = current["asset_id"].as_str() {self.preserve_legacy_video_generations(id,&current).await?;}
        let content = serde_json::to_string(project)?;
        let asset = match current["asset_id"].as_str() {
            Some(id) => self.update_text_asset_expected(session,id,&content,request,expected).await?,
            None => self.create_text_asset(session,PROJECT_NAME,&content,&format!("video-project:init:{session}")).await?,
        };
        if video_id.is_none() {
            let owner = self.get_session(session).await?;
            sqlx::query("INSERT INTO video_works(id,main_session_id,project_id) VALUES(?,?,?) ON CONFLICT(id) DO NOTHING")
                .bind(&asset.id).bind(session).bind(owner.project_id).execute(&self.pool).await?;
        }
        Ok(json!({"asset_id":asset.id,"asset_version":asset.current_version,"project":project}))
    }

    pub(crate) async fn apply_video_result(&self, session: &str, request: &str, input: &Value) -> Result<Value> {
        let source_in = input["source_in"].as_f64().ok_or(Error::Invalid("选择视频开始位置"))?;
        let selected_duration = input["duration"].as_f64().ok_or(Error::Invalid("选择视频使用时长"))?;
        let selected_version = text(input,"asset_version_id")?;
        let execution = self.get_local_execution(text(input,"execution_id")?).await?;
        if execution["session_id"] != session || execution["status"] != "succeeded" { return Err(Error::Invalid("工作流结果尚未就绪或不属于当前会话")); }
        let output = execution["outputs"].as_array().ok_or(Error::State("执行结果缺失"))?.iter().find(|output| output["asset_version_id"] == selected_version && output["media_type"].as_str().is_some_and(|mime| mime.starts_with("video/"))).ok_or(Error::Invalid("所选视频不属于本次生成结果"))?;
        let asset = self.get_asset(text(output,"asset_id")?).await?;
        let version = asset.versions.iter().find(|version| Some(version.id.as_str()) == output["asset_version_id"].as_str()).ok_or(Error::NotFound)?;
        let current = self.read_video_project(session).await?;
        let mut project = current["project"].clone();
        let fps = project["fps"].as_f64().ok_or(Error::Invalid("视频帧率无效"))?;
        let shot = project["shots"].as_array_mut().ok_or(Error::Invalid("视频项目缺少分镜"))?.iter_mut().find(|shot| shot["id"] == input["shot_id"]).ok_or(Error::Invalid("分镜不存在"))?;
        let metadata = self.ensure_video_metadata(&asset.id,&version.id).await?;
        let duration = metadata["duration"].as_f64().ok_or(Error::State("生成视频时长缺失"))?;
        let mut media = json!({"assetId":asset.id,"versionId":version.id,"url":version.content_url,"duration":duration});
        if !execution["inputs"].is_object() || !execution["workflow_version_id"].is_string() {return Err(Error::State("生成任务缺少工作流版本或输入快照"));}
        media["generation"] = json!({"executionId":execution["id"],"workflowId":execution["workflow_id"],"versionId":execution["workflow_version_id"],"inputs":execution["inputs"]});
        for (field,key) in [("first_frame","firstFrame"),("last_frame","lastFrame")] {
            if let Some(output) = execution["outputs"].as_array().and_then(|outputs|outputs.iter().find(|frame|frame["field_key"] == field && frame["index"] == output["index"])) {
                let frame = self.get_asset(text(output,"asset_id")?).await?;
                let version = frame.versions.iter().find(|version|Some(version.id.as_str()) == output["asset_version_id"].as_str()).ok_or(Error::NotFound)?;
                media[key] = json!({"assetId":frame.id,"versionId":version.id,"url":version.content_url});
            }
        }
        adopt_result_media(shot,media,source_in,selected_duration,fps)?;
        self.write_video_project(session,request,json!({"expected_version":input["expected_version"],"project":project})).await
    }

    pub(crate) async fn export_current_video(&self, session: &str, request: &str) -> Result<Value> {
        let current = self.read_video_project(session).await?;
        let video_id = text(&current,"asset_id")?;
        let (project,export_request,source_version) = match self.get_video_work_export(video_id,request).await {
            Ok(saved) => (saved["project"].clone(),saved["request"].clone(),saved["sourceVersion"].clone()),
            Err(Error::NotFound) => {
                let project = &current["project"];
                let shots: Vec<_> = project["shots"].as_array().ok_or(Error::Invalid("视频项目缺少分镜"))?.iter().map(|shot| json!({"asset_id":shot["media"]["assetId"],"asset_version_id":shot["media"]["versionId"],"source_in":shot["sourceIn"],"duration":shot["duration"],"subtitle":shot["subtitle"].as_str().unwrap_or(""),"volume":shot["volume"].as_f64().unwrap_or(1.0)})).collect();
                let audio: Vec<_> = match project["audio"].as_array() {Some(clips)=>clips.iter().map(|clip|json!({"asset_id":clip["assetId"],"asset_version_id":clip["versionId"],"start":clip["start"],"duration":clip["duration"],"source_in":clip["sourceIn"],"volume":clip["volume"],"loop":clip["loop"]})).collect(),None=>vec![]};
                let name = text(project,"name")?;
                let name = if name.to_ascii_lowercase().ends_with(".mp4") {&name[..name.len()-4]} else {name};
                (project.clone(),json!({"session_id":session,"request_id":request,"name":format!("{}.mp4",name.trim()),"width":project["width"],"height":project["height"],"fps":project["fps"],"shots":shots,"audio":audio}),current["asset_version"].clone())
            },
            Err(error) => return Err(error),
        };
        let exported = self.export_video_work(video_id,json!({"session_id":session,"request":export_request,"project":project,"source_version":source_version})).await?;
        let asset = self.get_asset(text(&exported["result"],"assetId")?).await?;
        let mut result = serde_json::to_value(asset)?;
        result["asset_version_id"] = exported["result"]["versionId"].clone();
        result["video_id"] = json!(video_id);
        result["export_request_id"] = json!(request);
        Ok(result)
    }
}
