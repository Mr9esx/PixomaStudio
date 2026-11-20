use crate::{Error,Result,Store,error::{required,timestamp},local_api::text};
use serde::Deserialize;
use serde_json::{Value,json};
use sqlx::Row;

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct WorkExportInput {
    session_id: String,
    request: Value,
    project: Value,
    source_version: Option<i64>,
    #[serde(default)]
    register_only: bool,
}

fn same_number(left: &Value,right: &Value) -> bool {left.as_f64().zip(right.as_f64()).is_some_and(|(left,right)|left.is_finite() && left==right)}

fn validate_snapshot(request: &Value, project: &Value) -> Result<()> {
    for key in ["width","height","fps"] {if !same_number(&request[key],&project[key]) {return Err(Error::Invalid("导出规格与作品快照不一致"));}}
    let name = text(project,"name")?;
    let name = if name.to_ascii_lowercase().ends_with(".mp4") {&name[..name.len()-4]} else {name};
    if request["name"] != format!("{}.mp4",name.trim()) {return Err(Error::Invalid("导出名称与作品快照不一致"));}
    let shots = project["shots"].as_array().ok_or(Error::Invalid("导出作品缺少分镜"))?;
    let clips = request["shots"].as_array().ok_or(Error::Invalid("导出请求缺少分镜"))?;
    if shots.is_empty() || shots.len()!=clips.len() {return Err(Error::Invalid("导出分镜与作品快照不一致"));}
    for (shot,clip) in shots.iter().zip(clips) {
        if shot["media"]["assetId"]!=clip["asset_id"] || shot["media"]["versionId"]!=clip["asset_version_id"] || !same_number(&shot["sourceIn"],&clip["source_in"]) || !same_number(&shot["duration"],&clip["duration"]) || shot["subtitle"].as_str()!=Some(clip.get("subtitle").and_then(Value::as_str).unwrap_or("")) || !same_number(shot.get("volume").unwrap_or(&json!(1)),clip.get("volume").unwrap_or(&json!(1))) {return Err(Error::Invalid("导出媒体或剪辑与作品快照不一致"));}
    }
    let audio = project.get("audio").map(|value|value.as_array().ok_or(Error::Invalid("作品音频格式无效"))).transpose()?.cloned().unwrap_or_default();
    let clips = request.get("audio").map(|value|value.as_array().ok_or(Error::Invalid("导出音频格式无效"))).transpose()?.cloned().unwrap_or_default();
    if audio.len()!=clips.len() {return Err(Error::Invalid("导出音频与作品快照不一致"));}
    for (audio,clip) in audio.iter().zip(&clips) {
        if audio["assetId"]!=clip["asset_id"] || audio["versionId"]!=clip["asset_version_id"] || ["start","duration","volume"].iter().any(|key|!same_number(&audio[*key],&clip[*key])) || !same_number(&audio["sourceIn"],&clip["source_in"]) || audio["loop"].as_bool()!=Some(clip.get("loop").and_then(Value::as_bool).unwrap_or(false)) {return Err(Error::Invalid("导出音频剪辑与作品快照不一致"));}
    }
    Ok(())
}

impl Store {
    pub async fn export_video_work(&self, video_id: &str, input: Value) -> Result<Value> {
        let input: WorkExportInput = serde_json::from_value(input)?;
        let work = self.authorize_video_generation(video_id,&input.session_id).await?;
        let request_id = text(&input.request,"request_id")?;
        required(request_id,"导出请求标识不能为空")?;
        validate_snapshot(&input.request,&input.project)?;
        if input.register_only {
            let asset_id: String = sqlx::query_scalar("SELECT asset_id FROM asset_operations WHERE request_id=?").bind(request_id).fetch_optional(&self.pool).await?.ok_or(Error::NotFound)?;
            let asset = self.get_asset(&asset_id).await?;
            let fingerprint = crate::assets::digest(&serde_json::to_vec(&input.request)?);
            if !asset.versions.iter().any(|version|version.metadata["video_export_request"]==fingerprint) {return Err(Error::Conflict("原导出结果与保存的请求不一致".into()));}
        }
        if let Some(version) = input.source_version {
            let exists: bool = sqlx::query_scalar("SELECT EXISTS(SELECT 1 FROM asset_versions WHERE asset_id=? AND version=?)").bind(video_id).bind(version).fetch_one(&self.pool).await?;
            if !exists {return Err(Error::Invalid("导出来源作品修订不存在"));}
        }
        let mut transaction = self.pool.begin_with("BEGIN IMMEDIATE").await?;
        let previous = sqlx::query("SELECT video_id,request,project,source_version FROM video_work_exports WHERE request_id=?").bind(request_id).fetch_optional(&mut *transaction).await?;
        if let Some(previous) = previous {
            if previous.get::<String,_>("video_id")!=video_id || serde_json::from_str::<Value>(&previous.get::<String,_>("request"))?!=input.request || serde_json::from_str::<Value>(&previous.get::<String,_>("project"))?!=input.project || previous.get::<Option<i64>,_>("source_version")!=input.source_version {return Err(Error::Conflict("请求标识已用于其他导出快照".into()));}
        } else {
            let now = timestamp()?;
            sqlx::query("INSERT INTO video_work_exports(video_id,request_id,session_id,request,project,source_version,status,created_at,updated_at) VALUES(?,?,?,?,?,?,'exporting',?,?)").bind(video_id).bind(request_id).bind(&input.session_id).bind(input.request.to_string()).bind(input.project.to_string()).bind(input.source_version).bind(&now).bind(&now).execute(&mut *transaction).await?;
        }
        transaction.commit().await?;
        let existing = self.get_video_work_export(video_id,request_id).await?;
        if existing["status"]=="completed" {return Ok(existing);}
        sqlx::query("UPDATE video_work_exports SET status='exporting',error=NULL,session_id=?,updated_at=? WHERE video_id=? AND request_id=?").bind(&input.session_id).bind(timestamp()?).bind(video_id).bind(request_id).execute(&self.pool).await?;
        match self.export_video_project_owned(input.request.clone(),&input.session_id).await {
            Ok(asset) => {
                let version = asset["versions"].as_array().and_then(|versions|versions.iter().find(|version|version["version"]==asset["current_version"])).ok_or(Error::State("导出成片版本缺失"))?;
                let asset_id = text(&asset,"id")?;
                let version_id = text(version,"id")?;
                self.place_asset(work["project_id"].as_str().unwrap_or(""),asset_id,version_id).await?;
                sqlx::query("UPDATE video_work_exports SET status='completed',error=NULL,asset_id=?,asset_version_id=?,updated_at=? WHERE video_id=? AND request_id=?").bind(asset_id).bind(version_id).bind(timestamp()?).bind(video_id).bind(request_id).execute(&self.pool).await?;
                self.get_video_work_export(video_id,request_id).await
            }
            Err(error) => {
                sqlx::query("UPDATE video_work_exports SET status='failed',error=?,updated_at=? WHERE video_id=? AND request_id=?").bind(error.to_string()).bind(timestamp()?).bind(video_id).bind(request_id).execute(&self.pool).await?;
                Err(error)
            }
        }
    }

    pub async fn get_video_work_export(&self, video_id: &str, request_id: &str) -> Result<Value> {
        let work = self.read_video_work(video_id).await?;
        let row = sqlx::query("SELECT * FROM video_work_exports WHERE video_id=? AND request_id=?").bind(video_id).bind(request_id).fetch_optional(&self.pool).await?.ok_or(Error::NotFound)?;
        let request: Value = serde_json::from_str(&row.get::<String,_>("request"))?;
        let mut status: String = row.get("status");
        let mut result = row.get::<Option<String>,_>("asset_id").zip(row.get::<Option<String>,_>("asset_version_id")).map(|(asset,version)|json!({"assetId":asset,"versionId":version}));
        if result.is_none() {
            let asset_id: Option<String> = sqlx::query_scalar("SELECT asset_id FROM asset_operations WHERE request_id=?").bind(request_id).fetch_optional(&self.pool).await?;
            if let Some(asset_id) = asset_id {
                let asset = self.get_asset(&asset_id).await?;
                let fingerprint = crate::assets::digest(&serde_json::to_vec(&request)?);
                if let Some(version) = asset.versions.iter().find(|version|version.metadata["video_export_request"]==fingerprint) {
                    self.place_asset(work["project_id"].as_str().unwrap_or(""),&asset_id,&version.id).await?;
                    sqlx::query("UPDATE video_work_exports SET status='completed',error=NULL,asset_id=?,asset_version_id=?,updated_at=? WHERE video_id=? AND request_id=?").bind(&asset_id).bind(&version.id).bind(timestamp()?).bind(video_id).bind(request_id).execute(&self.pool).await?;
                    result = Some(json!({"assetId":asset_id,"versionId":version.id})); status = "completed".into();
                }
            }
        }
        if status=="exporting" && !self.video_exports.lock().await.get(request_id).is_some_and(|(_,request)|request.strong_count()>0) {status="interrupted".into();}
        let mut document = json!({"version":1,"request":request,"project":serde_json::from_str::<Value>(&row.get::<String,_>("project"))?,"status":status,"submitted":true,"problems":[]});
        if let Some(version) = row.get::<Option<i64>,_>("source_version") {document["sourceVersion"]=json!(version);}
        if let Some(error) = row.get::<Option<String>,_>("error").filter(|_|status!="completed") {document["error"]=json!(error);}
        if let Some(result) = result {document["result"]=result;}
        Ok(document)
    }

    pub async fn list_video_work_exports(&self, video_id: &str) -> Result<Value> {
        self.read_video_work(video_id).await?;
        let ids: Vec<String> = sqlx::query_scalar("SELECT request_id FROM video_work_exports WHERE video_id=? ORDER BY created_at DESC,request_id").bind(video_id).fetch_all(&self.pool).await?;
        let mut records = Vec::new();
        for id in ids {records.push(self.get_video_work_export(video_id,&id).await?);}
        Ok(json!(records))
    }
}
