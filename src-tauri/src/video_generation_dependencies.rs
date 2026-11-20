use crate::{Error,Result,Store,local_api::text};
use serde::{Deserialize,Serialize};
use serde_json::{Value,json};
use sqlx::Row;

#[derive(Clone,Debug,Deserialize,Serialize)]
#[serde(deny_unknown_fields)]
pub(crate) struct VideoGenerationDependency {
    pub shot_id: String,
    pub source_shot_id: String,
    pub input_key: String,
    pub position: String,
    pub fps: u32,
    pub frame: Option<u64>,
    pub output: Option<VideoDependencyOutput>,
}

#[derive(Clone,Debug,Deserialize,Serialize)]
#[serde(deny_unknown_fields)]
pub(crate) struct VideoDependencyOutput { pub field: String, pub index: u32 }

pub(crate) fn validate_dependencies(ids: &[String], dependencies: &[VideoGenerationDependency]) -> Result<()> {
    let mut keys = std::collections::HashSet::new();
    for dependency in dependencies {
        if !ids.contains(&dependency.shot_id) || !ids.contains(&dependency.source_shot_id) {return Err(Error::Invalid("画面依赖的来源与目标需要同时选入批次"));}
        if !keys.insert((&dependency.shot_id,&dependency.input_key)) {return Err(Error::Invalid("同一生成参数只能指定一个画面依赖"));}
        if dependency.input_key.is_empty() || !(1..=60).contains(&dependency.fps) || !["first","last","fixed"].contains(&dependency.position.as_str()) || (dependency.position=="fixed") != dependency.frame.is_some() {return Err(Error::Invalid("画面依赖位置或参数无效"));}
        if dependency.output.as_ref().is_some_and(|output|output.field.is_empty()) {return Err(Error::Invalid("选择上游视频输出字段"));}
    }
    let mut remaining = ids.to_vec();
    loop {
        let ready: Vec<String> = remaining.iter().filter(|id|!dependencies.iter().any(|dependency|dependency.shot_id==**id && remaining.contains(&dependency.source_shot_id))).cloned().collect();
        if ready.is_empty() {break;}
        remaining.retain(|id|!ready.contains(id));
    }
    if !remaining.is_empty() {return Err(Error::Validation(format!("分镜依赖存在循环：{}",remaining.join("、"))));}
    Ok(())
}

impl Store {
    pub(crate) async fn resume_video_generation_dependencies(&self) -> Result<()> {
        let pending: Vec<(String,String)> = sqlx::query_as("SELECT video_id,id FROM video_generations WHERE dependency_state IN ('waiting','waiting_output') ORDER BY created_at,id").fetch_all(&self.pool).await?;
        for (video,id) in pending {
            if let Err(error) = self.resolve_video_generation_dependencies(&video,&id).await {
                if matches!(error,Error::Http(_)) {
                    sqlx::query("UPDATE video_generations SET dependency_error=? WHERE video_id=? AND id=? AND dependency_state IN ('waiting','waiting_output')")
                        .bind(format!("来源视频状态暂未确认：{error}")).bind(&video).bind(&id).execute(&self.pool).await?;
                    continue;
                }
                if !matches!(error,Error::Invalid(_) | Error::Validation(_) | Error::NotFound) {return Err(error);}
                sqlx::query("UPDATE video_generations SET dependency_state='blocked',dependency_error=? WHERE video_id=? AND id=? AND dependency_state IN ('waiting','waiting_output')").bind(error.to_string()).bind(&video).bind(&id).execute(&self.pool).await?;
            }
        }
        Ok(())
    }

    async fn resolve_video_generation_dependencies(&self, video: &str, id: &str) -> Result<()> {
        let row = sqlx::query("SELECT * FROM video_generations WHERE video_id=? AND id=?").bind(video).bind(id).fetch_one(&self.pool).await?;
        if !matches!(row.get::<String,_>("dependency_state").as_str(),"waiting"|"waiting_output") {return Ok(());}
        let raw: String = row.get("dependencies");
        let dependencies: Vec<Value> = serde_json::from_str(&raw)?;
        let mut snapshot: Value = serde_json::from_str(&row.get::<String,_>("snapshot"))?;
        let work = self.read_video_work(video).await?;
        let session: Option<String> = row.get("session_id");
        let mut resolved = Vec::new();
        for dependency in dependencies {
            let binding: VideoGenerationDependency = serde_json::from_value(dependency["binding"].clone())?;
            let source = self.get_video_generation(video,text(&dependency,"source_generation_id")?).await?;
            let status = text(&source,"status")?;
            if ["failed","submission_failed","cancelled","blocked_dependencies","result_unknown"].contains(&status) {
                let reason = match status {"cancelled"=>"已取消","blocked_dependencies"=>"依赖尚未完成","result_unknown"=>"结果尚未确认",_=>"生成失败"};
                return Err(Error::Validation(format!("上游分镜「{}」{reason}，此任务尚未提交",text(&source["snapshot"]["shot"],"name")?)));
            }
            if status!="succeeded" {
                sqlx::query("UPDATE video_generations SET dependency_error=NULL WHERE video_id=? AND id=? AND dependency_state IN ('waiting','waiting_output')")
                    .bind(video).bind(id).execute(&self.pool).await?;
                return Ok(());
            }
            let outputs: Vec<&Value> = source["execution"]["outputs"].as_array().ok_or(Error::State("上游任务输出缺失"))?.iter().filter(|output|output["media_type"].as_str().is_some_and(|kind|kind.starts_with("video/"))).collect();
            let output = if let Some(selection) = &binding.output {
                outputs.iter().find(|output|output["field_key"]==selection.field && output["index"]==selection.index).copied().ok_or(Error::Invalid("指定的上游视频输出不存在"))?
            } else {
                if outputs.is_empty() {return Err(Error::Invalid("上游任务没有视频输出"));}
                if outputs.len()>1 {
                    sqlx::query("UPDATE video_generations SET dependency_state='waiting_output',dependency_error='选择上游输出视频' WHERE video_id=? AND id=? AND dependency_state IN ('waiting','waiting_output')").bind(video).bind(id).execute(&self.pool).await?;
                    return Ok(());
                }
                outputs[0]
            };
            let asset_id = text(output,"asset_id")?;
            let version_id = text(output,"asset_version_id")?;
            let metadata = self.ensure_video_metadata(asset_id,version_id).await?;
            let duration = metadata["duration"].as_f64().filter(|duration|duration.is_finite() && *duration>0.0).ok_or(Error::Invalid("上游视频时长无效"))?;
            let frame = match binding.position.as_str() {"first"=>0,"last"=>{
                let count = (duration*f64::from(binding.fps)).round() as u64;
                count.checked_sub(1).ok_or(Error::Invalid("当前帧率下没有可用的视频画面"))?
            },"fixed"=>binding.frame.ok_or(Error::Invalid("指定画面位置缺失"))?,_=>return Err(Error::Invalid("画面位置无效"))};
            let image = self.create_video_frame_owned(asset_id,version_id,frame,binding.fps,session.as_deref(),&format!("video:{video}")).await?;
            let image_version = image.versions.iter().find(|version|version.version==1).ok_or(Error::State("衍生画面版本缺失"))?;
            self.place_asset(work["project_id"].as_str().unwrap_or(""),&image.id,&image_version.id).await?;
            snapshot["inputs"][&binding.input_key] = json!({"asset_id":image.id,"asset_version_id":image_version.id});
            resolved.push(json!({"input_key":binding.input_key,"source_generation_id":source["id"],"source_shot_id":binding.source_shot_id,"output_field":output["field_key"],"output_index":output["index"],"asset_id":image.id,"asset_version_id":image_version.id,"source_asset_id":asset_id,"source_asset_version_id":version_id,"frame":frame,"fps":binding.fps}));
        }
        snapshot["resolved_dependencies"] = json!(resolved);
        let workflow = self.get_workflow_version(text(&snapshot,"workflow_id")?,text(&snapshot,"version_id")?).await?;
        jsonschema::validator_for(&workflow["definition"]["input_schema"]).map_err(|_|Error::State("输入 Schema 无效"))?.validate(&snapshot["inputs"]).map_err(|error|Error::Validation(error.to_string()))?;
        self.workflow_transport_inputs(&json!({"input_fields":snapshot["input_fields"]}),&snapshot["inputs"]).await?;
        sqlx::query("UPDATE video_generations SET snapshot=?,dependency_state='ready',dependency_error=NULL WHERE video_id=? AND id=? AND dependencies=? AND dependency_state IN ('waiting','waiting_output')").bind(snapshot.to_string()).bind(video).bind(id).bind(raw).execute(&self.pool).await?;
        Ok(())
    }

    pub async fn choose_video_dependency_output(&self, video: &str, id: &str, input: Value) -> Result<Value> {
        self.authorize_video_generation(video,text(&input,"session_id")?).await?;
        let key = text(&input,"input_key")?;
        let source_id = text(&input,"source_generation_id")?;
        let output: VideoDependencyOutput = serde_json::from_value(input["output"].clone())?;
        let source = self.get_video_generation(video,source_id).await?;
        if source["status"]!="succeeded" || !source["execution"]["outputs"].as_array().is_some_and(|outputs|outputs.iter().any(|item|item["field_key"]==output.field && item["index"]==output.index && item["media_type"].as_str().is_some_and(|kind|kind.starts_with("video/")))) {return Err(Error::Invalid("选择已生成的视频输出"));}
        let mut transaction = self.pool.begin_with("BEGIN IMMEDIATE").await?;
        let row = sqlx::query("SELECT dependency_state,dependencies FROM video_generations WHERE video_id=? AND id=?").bind(video).bind(id).fetch_optional(&mut *transaction).await?.ok_or(Error::NotFound)?;
        if !matches!(row.get::<String,_>("dependency_state").as_str(),"waiting"|"waiting_output") {return Err(Error::Conflict("此任务已经结束依赖选择".into()));}
        let mut dependencies: Vec<Value> = serde_json::from_str(&row.get::<String,_>("dependencies"))?;
        let dependency = dependencies.iter_mut().find(|dependency|dependency["source_generation_id"]==source_id && dependency["binding"]["input_key"]==key).ok_or(Error::Invalid("画面依赖不存在"))?;
        dependency["binding"]["output"] = serde_json::to_value(output)?;
        sqlx::query("UPDATE video_generations SET dependencies=?,dependency_state='waiting',dependency_error=NULL WHERE video_id=? AND id=?").bind(json!(dependencies).to_string()).bind(video).bind(id).execute(&mut *transaction).await?;
        transaction.commit().await?;
        self.get_video_generation(video,id).await
    }

    pub async fn cancel_video_dependency_wait(&self, video: &str, id: &str, input: Value) -> Result<Value> {
        self.authorize_video_generation(video,text(&input,"session_id")?).await?;
        let changed = sqlx::query("UPDATE video_generations SET dependency_state='cancelled',dependency_error='已取消等待上游视频' WHERE video_id=? AND id=? AND dependency_state IN ('waiting','waiting_output')").bind(video).bind(id).execute(&self.pool).await?;
        if changed.rows_affected()!=1 {return Err(Error::Conflict("任务已经结束依赖等待，请读取最新状态".into()));}
        self.get_video_generation(video,id).await
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    fn dependency(target: &str,source: &str,key: &str) -> VideoGenerationDependency {VideoGenerationDependency {shot_id:target.into(),source_shot_id:source.into(),input_key:key.into(),position:"last".into(),fps:30,frame:None,output:None}}
    #[test]
    fn dependency_graph_checks_range_cycles_and_input_uniqueness() {
        let ids = vec!["opening".into(),"middle".into(),"ending".into()];
        assert!(validate_dependencies(&ids,&[dependency("middle","opening","first"),dependency("ending","middle","first")]).is_ok());
        for invalid in [vec![dependency("opening","opening","first")],vec![dependency("opening","middle","first"),dependency("middle","opening","first")],vec![dependency("ending","removed","first")],vec![dependency("middle","opening","first"),dependency("middle","ending","first")]] {assert!(validate_dependencies(&ids,&invalid).is_err());}
    }
}
