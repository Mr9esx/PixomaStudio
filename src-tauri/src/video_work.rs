use crate::{Error, Result, Store, error::timestamp};
use serde_json::{Value, json};
use uuid::Uuid;

impl Store {
    pub async fn video_editor_state(&self, video_id: &str) -> Result<Value> {
        self.read_video_work(video_id).await?;
        let document: Option<String> = sqlx::query_scalar("SELECT document FROM video_editor_states WHERE video_id=?").bind(video_id).fetch_optional(&self.pool).await?;
        document.map(|document|serde_json::from_str(&document).map_err(Error::from)).transpose().map(|state|state.unwrap_or(Value::Null))
    }

    pub async fn save_video_editor_state(&self, video_id: &str, input: Value) -> Result<Value> {
        self.authorize_video_generation(video_id,crate::local_api::text(&input,"session_id")?).await?;
        let state = input.get("state").filter(|state|state.is_object() && state["version"]==1).ok_or(Error::Invalid("编辑状态格式无效"))?;
        sqlx::query("INSERT INTO video_editor_states(video_id,document,updated_at) VALUES(?,?,?) ON CONFLICT(video_id) DO UPDATE SET document=excluded.document,updated_at=excluded.updated_at")
            .bind(video_id).bind(serde_json::to_string(state)?).bind(timestamp()?).execute(&self.pool).await?;
        Ok(state.clone())
    }

    pub async fn asset_video_usages(&self, asset_id: &str) -> Result<Value> {
        let asset = self.get_asset(asset_id).await?;
        let works: Vec<(String, String)> = sqlx::query_as("SELECT w.id,coalesce(p.name,'未归属项目') FROM video_works w LEFT JOIN projects p ON p.id=w.project_id ORDER BY w.id")
            .fetch_all(&self.pool).await?;
        let mut usages = Vec::new();
        for (id, project_name) in works {
            let work = self.get_asset(&id).await?;
            let current = work.versions.iter().find(|version|version.version == work.current_version).ok_or(Error::State("作品版本缺失"))?;
            let project: Value = serde_json::from_slice(&self.read_asset(&id,&current.id).await?)?;
            let shots = project["shots"].as_array().ok_or(Error::State("作品分镜缺失"))?;
            let mut references = Vec::new();
            for shot in shots {
                if let Some(items) = shot.get("references") {
                    for reference in items.as_array().ok_or(Error::State("分镜素材引用格式无效"))? {
                        if reference["assetId"] != asset_id {continue;}
                        let version = asset.versions.iter().find(|version|reference["versionId"] == version.id).ok_or(Error::State("分镜引用的素材版本缺失"))?;
                        references.push(json!({"shot_id":shot["id"],"shot_name":shot["name"],"reference":reference,"version":version.version}));
                    }
                }
            }
            if !references.is_empty() {usages.push(json!({"video_id":id,"video_name":project["name"],"project_name":project_name,"references":references}));}
        }
        Ok(json!(usages))
    }

    pub async fn list_video_works(&self, project_id: &str) -> Result<Value> {
        if !project_id.is_empty() {self.get_project(project_id).await?;}
        let ids: Vec<String> = sqlx::query_scalar("SELECT w.id FROM video_works w JOIN assets a ON a.id=w.id WHERE coalesce(w.project_id,'')=? ORDER BY a.updated_at DESC,w.id")
            .bind(project_id).fetch_all(&self.pool).await?;
        let mut works = Vec::with_capacity(ids.len());
        for id in ids {
            let work = self.read_video_work(&id).await?;
            works.push(json!({"id":id,"name":work["project"]["name"],"asset_version":work["asset_version"],"main_session_id":work["main_session_id"],"shot_count":work["project"]["shots"].as_array().ok_or(Error::State("作品分镜缺失"))?.len()}));
        }
        Ok(json!(works))
    }

    pub async fn open_video_work(&self, id: &str) -> Result<Value> {
        let work = self.read_video_work(id).await?;
        let mut transaction = self.pool.begin_with("BEGIN IMMEDIATE").await?;
        let (session_id,project_id): (Option<String>,Option<String>) = sqlx::query_as("SELECT main_session_id,project_id FROM video_works WHERE id=?")
            .bind(id).fetch_one(&mut *transaction).await?;
        if session_id.is_none() {
            let session_id = Uuid::new_v4().to_string();
            let now = timestamp()?;
            sqlx::query("INSERT INTO sessions(id,request_id,project_id,title,created_at,updated_at) VALUES(?,?,?,?,?,?)")
                .bind(&session_id).bind(format!("video-main-chat:{session_id}")).bind(project_id).bind(work["project"]["name"].as_str().ok_or(Error::State("作品名称缺失"))?).bind(&now).bind(&now).execute(&mut *transaction).await?;
            sqlx::query("UPDATE video_works SET main_session_id=? WHERE id=?")
                .bind(session_id).bind(id).execute(&mut *transaction).await?;
        }
        transaction.commit().await?;
        self.read_video_work(id).await
    }

    pub async fn read_video_work(&self, id: &str) -> Result<Value> {
        let (main_session_id,project_id): (Option<String>,Option<String>) = sqlx::query_as("SELECT main_session_id,project_id FROM video_works WHERE id=?")
            .bind(id).fetch_optional(&self.pool).await?.ok_or(Error::NotFound)?;
        let asset = self.get_asset(id).await?;
        let version = asset.versions.iter().find(|version|version.version == asset.current_version).ok_or(Error::State("作品版本缺失"))?;
        let project: Value = serde_json::from_slice(&self.read_asset(id,&version.id).await?)?;
        let chats: Vec<(String,Option<String>,String,Option<String>,Option<String>)> = sqlx::query_as("SELECT links.session_id,links.asset_id,sessions.title,links.material_id,links.selected_version_id FROM video_work_chats AS links JOIN sessions ON sessions.id=links.session_id WHERE video_id=? ORDER BY sessions.created_at,links.session_id")
            .bind(id).fetch_all(&self.pool).await?;
        let ids: Vec<String> = sqlx::query_scalar("SELECT material_id FROM video_work_materials WHERE video_id=? ORDER BY material_id").bind(id).fetch_all(&self.pool).await?;
        let mut materials = Vec::new();
        for id in ids {materials.push(self.read_video_material(&id).await?);}
        Ok(json!({"id":id,"asset_id":id,"asset_version":asset.current_version,"project":project,"main_session_id":main_session_id,"project_id":project_id,
            "materials":materials,"chats":chats.into_iter().map(|(session_id,asset_id,title,material_id,selected_version_id)|json!({"session_id":session_id,"asset_id":asset_id,"title":title,"material_id":material_id,"selected_version_id":selected_version_id})).collect::<Vec<_>>()}))
    }

    pub async fn bind_video_chat(&self, video_id: &str, session_id: &str, asset_id: Option<&str>) -> Result<Value> {
        let work = self.read_video_work(video_id).await?;
        let session = self.get_session(session_id).await?;
        if session.project_id.as_deref() != work["project_id"].as_str() {return Err(Error::Invalid("素材对话需要属于当前项目"));}
        if work["main_session_id"] == session_id {return Err(Error::Invalid("视频主对话已经关联此作品"));}
        let asset = match asset_id {Some(id) => Some(self.get_asset(id).await?),None => None};
        let mut transaction = self.pool.begin_with("BEGIN IMMEDIATE").await?;
        let linked: Vec<String> = sqlx::query_scalar("SELECT DISTINCT material_id FROM video_work_chats WHERE session_id=? AND material_id IS NOT NULL LIMIT 2").bind(session_id).fetch_all(&mut *transaction).await?;
        if asset_id.is_none() && linked.len()>1 {return Err(Error::Invalid("当前对话关联多个素材，请指定素材资产"));}
        let candidate = asset_id.map(|id|format!("asset:{id}")).unwrap_or_else(||format!("chat:{session_id}"));
        if asset_id.is_some() || linked.is_empty() {
            sqlx::query("INSERT INTO video_materials(id,name,kind,brief,asset_id,created_at) VALUES(?,?,'reference','',?,?) ON CONFLICT DO NOTHING")
                .bind(&candidate).bind(asset.as_ref().map_or(session.title.as_str(),|asset|asset.name.as_str())).bind(asset_id).bind(timestamp()?).execute(&mut *transaction).await?;
        }
        let material_id: String = match asset_id {
            Some(id) => sqlx::query_scalar("SELECT id FROM video_materials WHERE asset_id=?").bind(id).fetch_one(&mut *transaction).await?,
            None => linked.first().cloned().unwrap_or(candidate),
        };
        let asset_id: Option<String> = sqlx::query_scalar("SELECT asset_id FROM video_materials WHERE id=?").bind(&material_id).fetch_one(&mut *transaction).await?;
        let version_id: Option<String> = sqlx::query_scalar("SELECT coalesce((SELECT selected_version_id FROM video_work_chats WHERE session_id=? AND material_id=? AND selected_version_id IS NOT NULL LIMIT 1),(SELECT v.id FROM asset_versions v JOIN assets a ON a.id=v.asset_id WHERE a.id=? AND v.version=a.current_version))")
            .bind(session_id).bind(&material_id).bind(&asset_id).fetch_one(&mut *transaction).await?;
        sqlx::query("INSERT INTO video_work_chats(video_id,session_id,asset_id,material_id,selected_version_id) VALUES(?,?,?,?,?) ON CONFLICT(video_id,session_id) DO UPDATE SET asset_id=excluded.asset_id,material_id=excluded.material_id,selected_version_id=excluded.selected_version_id")
            .bind(video_id).bind(session_id).bind(asset_id).bind(&material_id).bind(version_id).execute(&mut *transaction).await?;
        sqlx::query("INSERT INTO video_work_materials(video_id,material_id) VALUES(?,?) ON CONFLICT DO NOTHING").bind(video_id).bind(material_id).execute(&mut *transaction).await?;
        transaction.commit().await?;
        self.read_video_work(video_id).await
    }

    pub async fn write_video_work(&self, video_id: &str, session_id: &str, request: &str, input: Value) -> Result<Value> {
        let session = self.get_session(session_id).await?;
        let work = self.read_video_work(video_id).await?;
        let bound: bool = sqlx::query_scalar("SELECT EXISTS(SELECT 1 FROM video_work_chats WHERE video_id=? AND session_id=?)")
            .bind(video_id).bind(session_id).fetch_one(&self.pool).await?;
        if work["main_session_id"] != session_id && !bound {return Err(Error::Invalid("当前对话没有关联此视频作品"));}
        if session.project_id.as_deref() != work["project_id"].as_str() {return Err(Error::Invalid("当前对话与视频作品所属项目不同"));}
        self.write_video_record(session_id,request,input,Some(video_id)).await
    }
}
