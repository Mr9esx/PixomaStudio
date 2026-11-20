use crate::{Error, Result, Store, local_api::text, video_project::validate_project_script};
use serde_json::{Value, json};
use std::collections::HashSet;

fn fields(value: &Value, allowed: &[&str]) -> Result<()> {
    if value.as_object().is_none_or(|object| object.keys().any(|key| !allowed.contains(&key.as_str()))) {
        return Err(Error::Invalid("创作提案字段无效"));
    }
    Ok(())
}

pub(crate) fn validate_video_proposals(project: &Value) -> Result<()> {
    let Some(value) = project.get("proposals") else { return Ok(()) };
    let proposals = value.as_array().ok_or(Error::Invalid("创作提案需要列表"))?;
    let mut ids = HashSet::new();
    for proposal in proposals {
        if !ids.insert(text(proposal, "id")?) { return Err(Error::Invalid("创作提案标识重复")); }
        match text(proposal, "kind")? {
            "script" => {
                fields(proposal, &["id", "kind", "baseline", "script"])?;
                for script in [&proposal["baseline"], &proposal["script"]] {
                    validate_project_script(&json!({"script":script}))?;
                }
            }
            "storyboard" => {
                fields(proposal, &["id", "kind", "changes", "order", "reorder"])?;
                let changes = proposal["changes"].as_array().ok_or(Error::Invalid("分镜提案需要列表"))?;
                if (changes.is_empty() && proposal.get("reorder").is_none()) || changes.len() > 100 { return Err(Error::Invalid("分镜提案需要修改内容或顺序建议")); }
                if let Some(reorder) = proposal.get("reorder") {
                    fields(reorder,&["baseline","proposed"])?;
                    for key in ["baseline","proposed"] {
                        let ids = reorder[key].as_array().ok_or(Error::Invalid("分镜顺序需要列表"))?;
                        let mut unique = HashSet::new();
                        for id in ids {if !unique.insert(id.as_str().filter(|id|!id.is_empty()).ok_or(Error::Invalid("分镜顺序标识无效"))?) {return Err(Error::Invalid("分镜顺序标识重复"));}}
                    }
                }
                if let Some(order) = proposal.get("order") {
                    let order = order.as_array().ok_or(Error::Invalid("分镜提案顺序需要列表"))?;
                    let mut ordered_ids = HashSet::new();
                    for id in order {
                        let id = id.as_str().filter(|id| !id.is_empty()).ok_or(Error::Invalid("分镜提案顺序标识无效"))?;
                        if !ordered_ids.insert(id) {return Err(Error::Invalid("分镜提案顺序标识重复"));}
                    }
                    if changes.iter().filter(|change| change["action"] == "add").any(|change| !order.contains(&change["shotId"])) {return Err(Error::Invalid("分镜提案顺序缺少目标"));}
                }
                let mut shot_ids = HashSet::new();
                for change in changes {
                    if !shot_ids.insert(text(change, "shotId")?) { return Err(Error::Invalid("分镜提案目标重复")); }
                    text(change, "name")?;
                    if !change["storyboard"].is_string() { return Err(Error::Invalid("分镜画面描述缺失")); }
                    match text(change, "action")? {
                        "add" => fields(change, &["shotId", "action", "name", "storyboard", "sources"] )?,
                        "update" => {
                            fields(change, &["shotId", "action", "name", "storyboard", "sources", "baseline"])?;
                            fields(&change["baseline"], &["name", "storyboard", "scriptSources"])?;
                            text(&change["baseline"], "name")?;
                            if !change["baseline"]["storyboard"].is_string() || !change["baseline"]["scriptSources"].is_array() { return Err(Error::Invalid("分镜提案基线缺失")); }
                            validate_project_script(&json!({"shots":[change["baseline"]]}))?;
                        }
                        "remove" => {
                            fields(change,&["shotId","action","name","storyboard","sources","baseline"])?;
                            if change["baseline"]["id"] != change["shotId"] || !change["baseline"].is_object() {return Err(Error::Invalid("删除分镜的原始内容缺失"));}
                            validate_project_script(&json!({"shots":[change["baseline"]]}))?;
                        }
                        _ => return Err(Error::Invalid("分镜提案操作无效")),
                    }
                    if !change["sources"].is_array() { return Err(Error::Invalid("分镜原文关联缺失")); }
                    validate_project_script(&json!({"shots":[{"scriptSources":change["sources"]}]}))?;
                }
            }
            _ => return Err(Error::Invalid("创作提案类型无效")),
        }
    }
    Ok(())
}

impl Store {
    pub async fn propose_video_creation(&self, session: &str, request: &str, input: &Value, context: &Value) -> Result<Value> {
        let mut baseline = context.get("project").filter(|value| value.is_object()).ok_or(Error::Invalid("创作请求缺少作品快照"))?.clone();
        if baseline.get("script").is_none() {
            let shots = baseline["shots"].as_array_mut().ok_or(Error::Invalid("作品分镜列表缺失"))?;
            let mut blocks = Vec::new();
            for shot in shots {
                let id = format!("script-{}",text(shot,"id")?);
                let content = shot["script"].as_str().ok_or(Error::Invalid("分镜脚本内容缺失"))?;
                let sources = if content.trim().is_empty() { json!([]) } else {
                    blocks.push(json!({"id":id,"text":content,"revision":1}));
                    json!([{"blockId":id,"text":content,"revision":1}])
                };
                if shot.get("scriptSources").is_none() {shot["scriptSources"] = sources;}
            }
            baseline["script"] = json!({"revision":if blocks.is_empty(){0}else{1},"blocks":blocks});
        }
        validate_project_script(&baseline)?;
        let script = baseline.get("script").ok_or(Error::Invalid("创作请求缺少完整脚本"))?;
        let blocks = script["blocks"].as_array().ok_or(Error::Invalid("脚本文本块需要列表"))?;
        let proposal = match text(context, "scope")? {
            "script" => {
                fields(input, &["expected_version", "blocks"])?;
                let generated = input["blocks"].as_array().ok_or(Error::Invalid("生成脚本需要文本块列表"))?;
                let revision = script["revision"].as_u64().ok_or(Error::Invalid("脚本修订号无效"))?.checked_add(1).ok_or(Error::Invalid("脚本修订号超出范围"))?;
                let mut next = Vec::new();
                for block in generated {
                    fields(block, &["id", "text", "sourceBlockIds"])?;
                    let id = text(block, "id")?;
                    let content = block["text"].as_str().ok_or(Error::Invalid("脚本文本缺失"))?;
                    let previous = blocks.iter().find(|item| item["id"] == id);
                    let block_revision = match previous {
                        Some(previous) => previous["revision"].as_u64().ok_or(Error::Invalid("脚本文本块修订号无效"))?.checked_add(u64::from(previous["text"] != content)).ok_or(Error::Invalid("脚本修订号超出范围"))?,
                        None => 1,
                    };
                    let mut item = json!({"id":id,"text":content,"revision":block_revision});
                    if let Some(ancestors) = previous.and_then(|block|block.get("ancestorIds")) {item["ancestorIds"] = ancestors.clone();}
                    if let Some(sources) = block.get("sourceBlockIds") {
                        if previous.is_some() {return Err(Error::Invalid("已有脚本文本块沿用来源关系"));}
                        let sources = sources.as_array().filter(|sources|!sources.is_empty()).ok_or(Error::Invalid("脚本来源需要非空标识列表"))?;
                        let mut source_ids = HashSet::new();
                        let mut ancestors = Vec::new();
                        for source in sources {
                            let source_id = source.as_str().ok_or(Error::Invalid("脚本来源标识无效"))?;
                            if !source_ids.insert(source_id) {return Err(Error::Invalid("脚本来源标识重复"));}
                            let source = blocks.iter().find(|block|block["id"] == source_id).ok_or(Error::Invalid("脚本来源文本块不存在"))?;
                            if let Some(values) = source["ancestorIds"].as_array() { for value in values { if !ancestors.contains(value) {ancestors.push(value.clone());} } }
                        }
                        for source in sources {if !ancestors.contains(source) {ancestors.push(source.clone());}}
                        item["ancestorIds"] = json!(ancestors);
                    }
                    if previous.is_none() && (blocks.iter().any(|block|block["ancestorIds"].as_array().is_some_and(|ids|ids.contains(&json!(id)))) || baseline["shots"].as_array().is_some_and(|shots|shots.iter().any(|shot|shot["scriptSources"].as_array().is_some_and(|sources|sources.iter().any(|source|source["blockId"] == id))))) {
                        return Err(Error::Invalid("新增脚本文本块不能复用历史来源标识"));
                    }
                    next.push(item);
                }
                json!({"id":request,"kind":"script","baseline":script,"script":{"revision":revision,"blocks":next}})
            }
            "storyboard" => {
                fields(input, &["expected_version", "changes", "order"])?;
                let changes = input["changes"].as_array().ok_or(Error::Invalid("分镜提案需要列表"))?;
                let shots = baseline["shots"].as_array().ok_or(Error::Invalid("作品分镜列表缺失"))?;
                let restricted = context.get("shot_ids").map(|value| value.as_array().ok_or(Error::Invalid("创作分镜范围无效"))).transpose()?;
                let mut next = Vec::new();
                for change in changes {
                    fields(change, &["shotId", "action", "name", "storyboard", "sourceBlockIds"])?;
                    let id = text(change, "shotId")?;
                    if restricted.is_some_and(|ids| !ids.iter().any(|value| value == id)) { return Err(Error::Invalid("分镜提案超出请求范围")); }
                    let source_ids = change["sourceBlockIds"].as_array().ok_or(Error::Invalid("分镜原文标识需要列表"))?;
                    let mut sources = Vec::new();
                    for source_id in source_ids {
                        let block = blocks.iter().find(|block| block["id"] == *source_id).ok_or(Error::Invalid("分镜原文不存在"))?;
                        sources.push(json!({"blockId":block["id"],"text":block["text"],"revision":block["revision"]}));
                    }
                    let mut item = json!({"shotId":id,"action":change["action"],"name":change["name"],"storyboard":change["storyboard"],"sources":sources});
                    let previous = shots.iter().find(|shot| shot["id"] == id);
                    match text(change, "action")? {
                        "update" => {
                            let shot = previous.ok_or(Error::Invalid("目标分镜不存在"))?;
                            item["baseline"] = json!({"name":shot["name"],"storyboard":shot.get("storyboard").cloned().unwrap_or(json!("")),"scriptSources":shot.get("scriptSources").cloned().unwrap_or(json!([]))});
                        }
                        "add" if previous.is_none() => {}
                        "remove" => {
                            let shot = previous.ok_or(Error::Invalid("目标分镜不存在"))?;
                            item["baseline"] = shot.clone();
                            item["name"] = shot["name"].clone();
                            item["storyboard"] = shot.get("storyboard").cloned().unwrap_or(json!(""));
                            item["sources"] = shot.get("scriptSources").cloned().unwrap_or(json!([]));
                        }
                        _ => return Err(Error::Invalid("分镜提案操作与目标不匹配")),
                    }
                    next.push(item);
                }
                let mut proposal = json!({"id":request,"kind":"storyboard","changes":next});
                if let Some(order) = input.get("order") {
                    let order = order.as_array().ok_or(Error::Invalid("分镜顺序需要列表"))?;
                    let baseline_order: Vec<Value> = shots.iter().map(|shot|shot["id"].clone()).collect();
                    let mut expected: Vec<Value> = baseline_order.iter().filter(|id|!changes.iter().any(|change|change["action"]=="remove" && change["shotId"]==**id)).cloned().collect();
                    expected.extend(changes.iter().filter(|change|change["action"]=="add").map(|change|change["shotId"].clone()));
                    if expected.len()!=order.len() || expected.iter().any(|id|order.iter().filter(|value|*value==id).count()!=1) {return Err(Error::Invalid("顺序建议需要包含修改后的全部分镜，且不能重复"));}
                    if let Some(restricted) = restricted {
                        let outside = |ids: &[Value]| ids.iter().filter(|id|!restricted.contains(id)).cloned().collect::<Vec<_>>();
                        if outside(&baseline_order) != outside(order) {return Err(Error::Invalid("顺序建议超出请求中的分镜范围"));}
                        for id in &baseline_order {
                            if restricted.contains(id) {continue;}
                            let before: Vec<_> = baseline_order.iter().take_while(|value|*value!=id).filter(|value|expected.contains(value)).collect();
                            let after: Vec<_> = order.iter().take_while(|value|*value!=id).collect();
                            if before.len()!=after.len() {return Err(Error::Invalid("顺序建议改变了范围之外的分镜位置"));}
                        }
                    }
                    proposal["order"] = json!(order);
                    if *order != expected {proposal["reorder"] = json!({"baseline":baseline_order,"proposed":order});}
                }
                proposal
            }
            _ => return Err(Error::Invalid("创作请求范围无效")),
        };
        validate_video_proposals(&json!({"proposals":[proposal]}))?;
        let video_id = context.get("video_id").map(|value| value.as_str().ok_or(Error::Invalid("作品标识无效"))).transpose()?;
        let record = match video_id { Some(id) => self.read_video_work(id).await?, None => self.read_video_project(session).await? };
        let mut project = record["project"].clone();
        let mut proposals = project.get("proposals").map(|value| value.as_array().cloned().ok_or(Error::Invalid("创作提案需要列表"))).transpose()?.unwrap_or_default();
        if let Some(previous) = proposals.iter().find(|item| item["id"] == request) {
            if previous != &proposal { return Err(Error::Conflict("重复请求的创作内容不同".into())); }
            return Ok(record);
        }
        if input["expected_version"] != record["asset_version"] { return Err(Error::Conflict("作品已更新，读取最新版本后提交提案".into())); }
        proposals.push(proposal);
        project["proposals"] = json!(proposals);
        let update = json!({"expected_version":record["asset_version"],"project":project});
        match video_id { Some(id) => self.write_video_work(id,session,request,update).await, None => self.write_video_project(session,request,update).await }
    }
}
