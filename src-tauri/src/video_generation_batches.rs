use crate::{Error,Result,Store,error::{required,timestamp},local_api::text,video_generations::check_active};
use serde::Deserialize;
use serde_json::{Value,json};
use sqlx::Row;
use uuid::Uuid;
use crate::video_generation_dependencies::{VideoGenerationDependency,validate_dependencies};

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct BatchInput {
    request_id: String,
    session_id: String,
    expected_version: i64,
    shot_ids: Vec<String>,
    #[serde(default)]
    dependencies: Vec<VideoGenerationDependency>,
}

impl Store {
    pub async fn prepare_video_generation_batch(&self, video_id: &str, request: Value) -> Result<Value> {
        let input: BatchInput = serde_json::from_value(request.clone())?;
        required(&input.request_id,"请求标识不能为空")?;
        let work = self.authorize_video_generation(video_id,&input.session_id).await?;
        let existing: Option<String> = sqlx::query_scalar("SELECT request FROM video_generation_batches WHERE video_id=? AND id=?").bind(video_id).bind(&input.request_id).fetch_optional(&self.pool).await?;
        if let Some(existing) = existing {
            if serde_json::from_str::<Value>(&existing)? != request {return Err(Error::Conflict("请求标识已用于其他批量生成内容".into()));}
            return self.get_video_generation_batch(video_id,&input.request_id).await;
        }
        if input.shot_ids.is_empty() || input.shot_ids.len()>100 {return Err(Error::Invalid("选择一至一百个分镜"));}
        let selected: std::collections::HashSet<_> = input.shot_ids.iter().collect();
        if selected.len()!=input.shot_ids.len() {return Err(Error::Invalid("分镜选择重复"));}
        validate_dependencies(&input.shot_ids,&input.dependencies)?;
        if work["asset_version"] != input.expected_version {return Err(Error::Conflict("视频作品已更新，请读取最新内容后生成".into()));}
        let shots = work["project"]["shots"].as_array().ok_or(Error::State("视频分镜缺失"))?;
        if input.shot_ids.iter().any(|id|!shots.iter().any(|shot|shot["id"]==*id)) {return Err(Error::Invalid("选定分镜已删除"));}
        let mut prepared = Vec::new();
        for shot in shots.iter().filter(|shot|shot["id"].as_str().is_some_and(|id|input.shot_ids.iter().any(|selected|selected==id))) {
            if shot.get("generationRequest").is_some_and(|request|!request.is_null()) {return Err(Error::Invalid("恢复当前生成请求后再批量生成"));}
            let id = Uuid::new_v4().to_string();
            let mut snapshot = self.video_generation_snapshot(&work,text(shot,"id")?,&id,input.expected_version).await?;
            let version = self.get_workflow_version(text(&snapshot,"workflow_id")?,text(&snapshot,"version_id")?).await?;
            let mut schema = version["definition"]["input_schema"].clone();
            for dependency in input.dependencies.iter().filter(|dependency|shot["id"]==dependency.shot_id) {
                if !snapshot["input_fields"].as_array().is_some_and(|fields|fields.iter().any(|field|field["key"]==dependency.input_key && matches!(field["type"].as_str(),Some("image"|"file")))) {return Err(Error::Invalid("画面依赖需要图片或文件输入参数"));}
                if schema["properties"][&dependency.input_key]["type"]=="array" {return Err(Error::Invalid("画面依赖需要单个图片输入参数"));}
                snapshot["inputs"].as_object_mut().ok_or(Error::State("生成输入缺失"))?.remove(&dependency.input_key);
                if let Some(required) = schema["required"].as_array_mut() {required.retain(|key|key!=&dependency.input_key);}
                if let Some(output) = &dependency.output {
                    let source = shots.iter().find(|shot|shot["id"]==dependency.source_shot_id).ok_or(Error::Invalid("上游分镜已删除"))?;
                    let source = self.get_workflow_version(text(source,"workflowId")?,text(source,"versionId")?).await?;
                    if !source["definition"]["outputs"].as_array().is_some_and(|fields|fields.iter().any(|field|field["key"]==output.field && field["type"]=="video")) {return Err(Error::Invalid("选择上游的视频输出字段"));}
                }
            }
            let name = text(shot,"name")?;
            jsonschema::validator_for(&schema).map_err(|_|Error::State("输入 Schema 无效"))?.validate(&snapshot["inputs"]).map_err(|error|Error::Validation(format!("{name}：{error}")))?;
            self.workflow_transport_inputs(&json!({"input_fields":snapshot["input_fields"]}),&snapshot["inputs"]).await?;
            prepared.push((id,snapshot));
        }
        self.preserve_legacy_video_generations(video_id,&work).await?;
        let mut transaction = self.pool.begin_with("BEGIN IMMEDIATE").await?;
        let existing: Option<String> = sqlx::query_scalar("SELECT request FROM video_generation_batches WHERE video_id=? AND id=?").bind(video_id).bind(&input.request_id).fetch_optional(&mut *transaction).await?;
        if let Some(existing) = existing {
            if serde_json::from_str::<Value>(&existing)? != request {return Err(Error::Conflict("请求标识已用于其他批量生成内容".into()));}
            transaction.commit().await?;
            return self.get_video_generation_batch(video_id,&input.request_id).await;
        }
        let current: i64 = sqlx::query_scalar("SELECT current_version FROM assets WHERE id=?").bind(video_id).fetch_one(&mut *transaction).await?;
        if current!=input.expected_version {return Err(Error::Conflict("视频作品已更新，请读取最新内容后生成".into()));}
        let created = timestamp()?;
        sqlx::query("INSERT INTO video_generation_batches(video_id,id,session_id,request,created_at) VALUES(?,?,?,?,?)").bind(video_id).bind(&input.request_id).bind(&input.session_id).bind(request.to_string()).bind(&created).execute(&mut *transaction).await?;
        for (position,(id,snapshot)) in prepared.iter().enumerate() {
            let shot_id = text(&snapshot["shot"],"id")?;
            check_active(&mut transaction,video_id,shot_id).await?;
            sqlx::query("INSERT INTO video_generations(video_id,id,shot_id,session_id,execution_request_id,snapshot,created_at) VALUES(?,?,?,?,?,?,?)").bind(video_id).bind(id).bind(shot_id).bind(&input.session_id).bind(format!("video-generation:{video_id}:{id}")).bind(snapshot.to_string()).bind(&created).execute(&mut *transaction).await?;
            sqlx::query("INSERT INTO video_generation_batch_items(video_id,batch_id,generation_id,position) VALUES(?,?,?,?)").bind(video_id).bind(&input.request_id).bind(id).bind(position as i64).execute(&mut *transaction).await?;
        }
        for (id,snapshot) in &prepared {
            let dependencies: Vec<Value> = input.dependencies.iter().filter(|dependency|snapshot["shot"]["id"]==dependency.shot_id).map(|dependency| {
                let source = prepared.iter().find(|(_,source)|source["shot"]["id"]==dependency.source_shot_id).expect("已校验批次依赖范围");
                json!({"source_generation_id":source.0,"binding":dependency})
            }).collect();
            if !dependencies.is_empty() {sqlx::query("UPDATE video_generations SET dependency_state='waiting',dependencies=? WHERE video_id=? AND id=?").bind(json!(dependencies).to_string()).bind(video_id).bind(id).execute(&mut *transaction).await?;}
        }
        transaction.commit().await?;
        self.get_video_generation_batch(video_id,&input.request_id).await
    }

    pub async fn get_video_generation_batch(&self, video_id: &str, id: &str) -> Result<Value> {
        let row = sqlx::query("SELECT * FROM video_generation_batches WHERE video_id=? AND id=?").bind(video_id).bind(id).fetch_optional(&self.pool).await?.ok_or(Error::NotFound)?;
        let ids: Vec<String> = sqlx::query_scalar("SELECT generation_id FROM video_generation_batch_items WHERE video_id=? AND batch_id=? ORDER BY position").bind(video_id).bind(id).fetch_all(&self.pool).await?;
        let mut generations = Vec::new();
        for generation in ids {generations.push(self.get_video_generation(video_id,&generation).await?);}
        Ok(json!({"id":id,"video_id":video_id,"session_id":row.get::<Option<String>,_>("session_id"),"request":serde_json::from_str::<Value>(&row.get::<String,_>("request"))?,"created_at":row.get::<String,_>("created_at"),"generations":generations}))
    }

    pub async fn list_video_generation_batches(&self, video_id: &str) -> Result<Value> {
        self.read_video_work(video_id).await?;
        let ids: Vec<String> = sqlx::query_scalar("SELECT id FROM video_generation_batches WHERE video_id=? ORDER BY created_at DESC,id").bind(video_id).fetch_all(&self.pool).await?;
        let mut batches = Vec::new();
        for id in ids {batches.push(self.get_video_generation_batch(video_id,&id).await?);}
        Ok(json!(batches))
    }
}
