use crate::{Error, Result, Store, error::{required, timestamp}, local_api::text};
use serde::Deserialize;
use serde_json::{Value, json};
use sqlx::Row;
use uuid::Uuid;

#[derive(Deserialize, PartialEq)]
#[serde(deny_unknown_fields)]
struct MaterialChatInput {
    request_id: String,
    title: String,
    material_id: Option<String>,
    asset_id: Option<String>,
    asset_version_id: Option<String>,
    name: Option<String>,
    kind: Option<String>,
    #[serde(default)] brief: String,
    #[serde(default)] shot_ids: Vec<String>,
}

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct MaterialResultInput {
    request_id: String,
    session_id: String,
    asset_id: String,
    asset_version_id: String,
    expected_version: i64,
}

impl Store {
    pub async fn video_material_for_asset(&self, asset_id: &str) -> Result<Value> {
        let id: Option<String> = sqlx::query_scalar("SELECT id FROM video_materials WHERE asset_id=?").bind(asset_id).fetch_optional(&self.pool).await?;
        match id {Some(id) => self.read_video_material(&id).await,None => Ok(Value::Null)}
    }

    pub async fn read_video_material(&self, id: &str) -> Result<Value> {
        let row = sqlx::query("SELECT * FROM video_materials WHERE id=?").bind(id).fetch_optional(&self.pool).await?.ok_or(Error::NotFound)?;
        let asset_id = row.get::<Option<String>,_>("asset_id");
        let asset = match &asset_id { Some(id) => serde_json::to_value(self.get_asset(id).await?)?, None => Value::Null };
        let results = sqlx::query("SELECT * FROM video_material_results WHERE material_id=? ORDER BY created_at,request_id").bind(id).fetch_all(&self.pool).await?;
        Ok(json!({"id":id,"name":row.get::<String,_>("name"),"kind":row.get::<String,_>("kind"),"brief":row.get::<String,_>("brief"),"asset_id":asset_id,"asset":asset,
            "results":results.into_iter().map(|row|json!({"source_session_id":row.get::<Option<String>,_>("source_session_id"),"source_session_title":row.get::<String,_>("source_session_title"),"source_asset_id":row.get::<String,_>("source_asset_id"),"source_version_id":row.get::<String,_>("source_version_id"),"asset_version_id":row.get::<String,_>("asset_version_id"),"created_at":row.get::<String,_>("created_at")})).collect::<Vec<_>>()}))
    }

    pub async fn create_video_material_chat(&self, video_id: &str, input: Value) -> Result<Value> {
        let encoded = serde_json::to_string(&input)?;
        let input: MaterialChatInput = serde_json::from_value(input)?;
        required(&input.request_id,"请求标识不能为空")?;
        required(&input.title,"填写素材对话名称")?;
        if input.material_id.is_some() && input.asset_id.is_some() {return Err(Error::Invalid("素材对象与已有资产只能选择一项"));}
        let work = self.read_video_work(video_id).await?;
        let material = match &input.material_id {Some(id) => Some(self.read_video_material(id).await?),None => None};
        let asset_id = material.as_ref().and_then(|value|value["asset_id"].as_str()).or(input.asset_id.as_deref());
        let asset = match asset_id {Some(id) => Some(self.get_asset(id).await?),None => None};
        let version_id = match (&asset,&input.asset_version_id) {
            (Some(asset), selected) => Some(asset.versions.iter().find(|version|selected.as_ref().map_or(version.version==asset.current_version,|id|version.id==*id)).ok_or(Error::Invalid("素材版本不存在"))?.id.clone()),
            (None,Some(_)) => return Err(Error::Invalid("选择素材后指定版本")),
            (None,None) => None,
        };
        let kind = input.kind.as_deref().unwrap_or("reference");
        if !["character","scene","style","voice","reference"].contains(&kind) {return Err(Error::Invalid("素材类型无效"));}
        let name = input.name.as_deref().or(asset.as_ref().map(|asset|asset.name.as_str())).unwrap_or(&input.title).trim();
        required(name,"填写素材名称")?;
        if input.brief.len()>65536 {return Err(Error::Invalid("素材创作要求超过 65536 字节"));}
        let request_id = format!("video-material-chat:{video_id}:{}",input.request_id);
        let mut transaction = self.pool.begin_with("BEGIN IMMEDIATE").await?;
        let previous: Option<(String,Option<String>,Option<String>)> = sqlx::query_as("SELECT s.id,c.material_id,c.creation_input FROM sessions s JOIN video_work_chats c ON c.session_id=s.id WHERE s.request_id=? AND c.video_id=?")
            .bind(&request_id).bind(video_id).fetch_optional(&mut *transaction).await?;
        if let Some((session_id,material_id,previous)) = previous {
            let mut previous: Value = serde_json::from_str(previous.as_deref().ok_or(Error::State("素材对话请求内容缺失"))?)?;
            if previous.get("request_id").is_none() {previous["request_id"] = json!(input.request_id);}
            if serde_json::from_value::<MaterialChatInput>(previous)?!=input {return Err(Error::Conflict("请求标识已用于其他素材对话内容".into()));}
            transaction.commit().await?;
            return Ok(json!({"session_id":session_id,"material_id":material_id,"work":self.read_video_work(video_id).await?}));
        }
        let mut shots = Vec::new();
        let mut selected = std::collections::HashSet::new();
        for id in &input.shot_ids {
            if !selected.insert(id) {return Err(Error::Invalid("分镜范围重复"));}
            let shot = work["project"]["shots"].as_array().and_then(|shots|shots.iter().find(|shot|shot["id"]==*id)).ok_or(Error::Invalid("素材创作来源分镜已删除"))?;
            shots.push(json!({"id":id,"name":shot["name"],"storyboard":shot["storyboard"],"scriptSources":shot["scriptSources"],"references":shot["references"]}));
        }
        let context = json!({"video_id":video_id,"video_name":work["project"]["name"],"project_version":work["asset_version"],"shots":shots});
        let now = timestamp()?;
        let material_id = if let Some(material) = &material {text(material,"id")?.to_owned()} else {
            let id = Uuid::new_v4().to_string();
            sqlx::query("INSERT INTO video_materials(id,name,kind,brief,asset_id,created_at) VALUES(?,?,?,?,?,?) ON CONFLICT(asset_id) DO NOTHING")
                .bind(&id).bind(name).bind(kind).bind(&input.brief).bind(asset_id).bind(&now).execute(&mut *transaction).await?;
            match asset_id {Some(asset) => {
                let existing = sqlx::query("SELECT id,name,kind,brief FROM video_materials WHERE asset_id=?").bind(asset).fetch_one(&mut *transaction).await?;
                if (input.name.is_some() && existing.get::<String,_>("name")!=name) || (input.kind.is_some() && existing.get::<String,_>("kind")!=kind) || (!input.brief.is_empty() && existing.get::<String,_>("brief")!=input.brief) {return Err(Error::Conflict("此资产已关联素材对象，请选择该对象继续创作".into()));}
                existing.get::<String,_>("id")
            },None => id}
        };
        let session_id = Uuid::new_v4().to_string();
        sqlx::query("INSERT INTO sessions(id,request_id,project_id,title,created_at,updated_at) VALUES(?,?,?,?,?,?)")
            .bind(&session_id).bind(request_id).bind(work["project_id"].as_str()).bind(input.title.trim()).bind(&now).bind(&now).execute(&mut *transaction).await?;
        sqlx::query("INSERT INTO video_work_chats(video_id,session_id,asset_id,creation_input,material_id,selected_version_id,source_context) VALUES(?,?,?,?,?,?,?)")
            .bind(video_id).bind(&session_id).bind(asset_id).bind(encoded).bind(&material_id).bind(version_id).bind(context.to_string()).execute(&mut *transaction).await?;
        sqlx::query("INSERT INTO video_work_materials(video_id,material_id) VALUES(?,?) ON CONFLICT DO NOTHING").bind(video_id).bind(&material_id).execute(&mut *transaction).await?;
        transaction.commit().await?;
        Ok(json!({"session_id":session_id,"material_id":material_id,"work":self.read_video_work(video_id).await?}))
    }

    pub async fn video_material_chat_context(&self, video_id: &str, session_id: &str, selected_version_id: Option<&str>) -> Result<Value> {
        let row = sqlx::query("SELECT material_id,selected_version_id,source_context FROM video_work_chats WHERE video_id=? AND session_id=?")
            .bind(video_id).bind(session_id).fetch_optional(&self.pool).await?.ok_or(Error::NotFound)?;
        let material_id = row.get::<Option<String>,_>("material_id").ok_or(Error::Invalid("对话尚未关联素材对象"))?;
        let material = self.read_video_material(&material_id).await?;
        let saved_version = row.get::<Option<String>,_>("selected_version_id");
        let asset_version = if material["asset_id"].is_string() {
            let requested = selected_version_id.or(saved_version.as_deref());
            Some(material["asset"]["versions"].as_array().ok_or(Error::State("素材版本缺失"))?.iter().find(|version|requested.map_or(version["version"]==material["asset"]["current_version"],|id|version["id"]==id)).ok_or(Error::Invalid("素材上下文版本不存在"))?.clone())
        } else {if selected_version_id.is_some() {return Err(Error::Invalid("素材尚未登记结果"));} None};
        Ok(json!({"video_id":video_id,"material_id":material_id,"name":material["name"],"kind":material["kind"],"brief":material["brief"],"asset_id":material["asset_id"],"asset_version":asset_version,"source":serde_json::from_str::<Value>(&row.get::<String,_>("source_context"))?}))
    }

    pub async fn adopt_video_material_result(&self, material_id: &str, input: Value) -> Result<Value> {
        let encoded = serde_json::to_string(&input)?;
        let input: MaterialResultInput = serde_json::from_value(input)?;
        required(&input.request_id,"请求标识不能为空")?;
        let session = self.get_session(&input.session_id).await?;
        let bound: bool = sqlx::query_scalar("SELECT EXISTS(SELECT 1 FROM video_work_chats WHERE material_id=? AND session_id=?)").bind(material_id).bind(&input.session_id).fetch_one(&self.pool).await?;
        if !bound {return Err(Error::Invalid("当前对话没有关联此素材对象"));}
        let source = self.get_asset(&input.asset_id).await?;
        self.asset_path(&input.asset_id,&input.asset_version_id).await?;
        let belongs: bool = sqlx::query_scalar("SELECT EXISTS(SELECT 1 FROM asset_usages WHERE session_id=? AND asset_id=? AND asset_version_id=?)")
            .bind(&input.session_id).bind(&input.asset_id).bind(&input.asset_version_id).fetch_one(&self.pool).await?;
        if !belongs {return Err(Error::Invalid("所选结果不属于当前素材对话"));}
        let mut transaction = self.pool.begin_with("BEGIN IMMEDIATE").await?;
        let previous: Option<(String,String)> = sqlx::query_as("SELECT material_id,request_input FROM video_material_results WHERE request_id=?").bind(&input.request_id).fetch_optional(&mut *transaction).await?;
        if let Some((id,previous)) = previous {
            if id!=material_id || previous!=encoded {return Err(Error::Conflict("请求标识已用于其他素材结果".into()));}
            transaction.commit().await?;
            let material = self.read_video_material(material_id).await?;
            let version_id = material["results"].as_array().and_then(|results|results.iter().find(|result|result["source_asset_id"]==input.asset_id && result["source_version_id"]==input.asset_version_id)).and_then(|result|result["asset_version_id"].as_str()).ok_or(Error::State("素材采用记录缺失"))?;
            self.place_asset(session.project_id.as_deref().unwrap_or(""),text(&material,"asset_id")?,version_id).await?;
            return self.read_video_material(material_id).await;
        }
        let existing: Option<String> = sqlx::query_scalar("SELECT asset_id FROM video_materials WHERE id=?").bind(material_id).fetch_one(&mut *transaction).await?;
        let (asset_id,version_id) = if let Some(id) = existing {
            let (kind,current): (String,i64) = sqlx::query_as("SELECT kind,current_version FROM assets WHERE id=?").bind(&id).fetch_one(&mut *transaction).await?;
            if current!=input.expected_version {return Err(Error::Conflict("素材版本已更新，请重新选择结果".into()));}
            if kind!=source.kind {return Err(Error::Invalid("结果类型与素材资产不一致"));}
            let previous_version: Option<String> = sqlx::query_scalar("SELECT asset_version_id FROM video_material_results WHERE material_id=? AND source_asset_id=? AND source_version_id=? ORDER BY created_at LIMIT 1")
                .bind(material_id).bind(&input.asset_id).bind(&input.asset_version_id).fetch_optional(&mut *transaction).await?;
            if let Some(version_id) = previous_version {(id,version_id)} else if id==source.id { (id,input.asset_version_id.clone()) } else {
                let version_id = Uuid::new_v4().to_string();
                sqlx::query("INSERT INTO asset_versions(id,asset_id,version,digest,mime_type,size_bytes,format,width_px,height_px,metadata,content_origin,created_at) SELECT ?,?,?,digest,mime_type,size_bytes,format,width_px,height_px,metadata,content_origin,? FROM asset_versions WHERE id=? AND asset_id=?")
                    .bind(&version_id).bind(&id).bind(current+1).bind(timestamp()?).bind(&input.asset_version_id).bind(&source.id).execute(&mut *transaction).await?;
                sqlx::query("UPDATE assets SET current_version=?,updated_at=? WHERE id=?").bind(current+1).bind(timestamp()?).bind(&id).execute(&mut *transaction).await?;
                (id,version_id)
            }
        } else {
            if input.expected_version!=0 {return Err(Error::Conflict("素材尚未登记结果".into()));}
            sqlx::query("UPDATE video_materials SET asset_id=? WHERE id=?").bind(&source.id).bind(material_id).execute(&mut *transaction).await?;
            (source.id.clone(),input.asset_version_id.clone())
        };
        let now = timestamp()?;
        sqlx::query("INSERT INTO video_material_results(request_id,request_input,material_id,source_session_id,source_session_title,source_asset_id,source_version_id,asset_version_id,created_at) VALUES(?,?,?,?,?,?,?,?,?)")
            .bind(input.request_id).bind(encoded).bind(material_id).bind(&session.id).bind(&session.title).bind(&source.id).bind(&input.asset_version_id).bind(&version_id).bind(&now).execute(&mut *transaction).await?;
        sqlx::query("UPDATE video_work_chats SET asset_id=?,selected_version_id=CASE WHEN session_id=? THEN ? ELSE selected_version_id END WHERE material_id=?")
            .bind(&asset_id).bind(&session.id).bind(&version_id).bind(material_id).execute(&mut *transaction).await?;
        sqlx::query("INSERT INTO asset_usages(id,session_id,asset_id,asset_version_id,session_title_snapshot,usage_kind,created_at) VALUES(?,?,?,?,?,'created',?) ON CONFLICT DO NOTHING")
            .bind(Uuid::new_v4().to_string()).bind(&session.id).bind(&asset_id).bind(&version_id).bind(&session.title).bind(&now).execute(&mut *transaction).await?;
        transaction.commit().await?;
        self.place_asset(session.project_id.as_deref().unwrap_or(""),&asset_id,&version_id).await?;
        self.read_video_material(material_id).await
    }
}
