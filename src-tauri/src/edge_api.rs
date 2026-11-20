use crate::{
    Error, Result, Store,
    compute_nodes::NodePresence,
    executions::{ExecutionCompletion, ExecutionEvent},
};
use axum::{
    Json, Router,
    body::Body,
    extract::{DefaultBodyLimit, Path, State, rejection::JsonRejection},
    http::{HeaderMap, StatusCode},
    response::{IntoResponse, Response},
    routing::{get, post, put},
};
use futures_util::StreamExt;
use headers::{Authorization, HeaderMapExt, authorization::Bearer};
use serde_json::{Value, json};
use sha2::{Digest, Sha256};
use sqlx::Row;
use tokio::io::AsyncWriteExt;

impl IntoResponse for Error {
    fn into_response(self) -> Response {
        let status = match &self {
            Self::Invalid(_) | Self::Validation(_) => StatusCode::BAD_REQUEST,
            Self::NodeUnauthorized => StatusCode::UNAUTHORIZED,
            Self::NotFound => StatusCode::NOT_FOUND,
            Self::Conflict(_) | Self::Cancelled => StatusCode::CONFLICT,
            Self::Http(_) => StatusCode::BAD_GATEWAY,
            Self::Remote { status, .. } => {
                StatusCode::from_u16(*status).unwrap_or(StatusCode::INTERNAL_SERVER_ERROR)
            }
            _ => StatusCode::INTERNAL_SERVER_ERROR,
        };
        (status, Json(json!({ "message": self.to_string() }))).into_response()
    }
}

pub fn compute_router(store: Store) -> Router {
    Router::new()
        .route("/agent/v1/presence", post(presence))
        .route("/agent/v1/jobs/claim", get(claim))
        .route("/agent/v1/executions/{id}/package", get(package))
        .route("/agent/v1/executions/{id}/heartbeat", post(heartbeat))
        .route("/agent/v1/executions/{id}/events", post(event))
        .route(
            "/agent/v1/executions/{id}/inputs/{file_id}",
            get(input_file),
        )
        .route(
            "/agent/v1/executions/{id}/outputs/{output_id}",
            put(output_file),
        )
        .route("/agent/v1/executions/{id}/complete", post(complete))
        .layer(DefaultBodyLimit::max(16 << 20))
        .with_state(store)
}

fn token(headers: &HeaderMap) -> Result<String> {
    Ok(headers
        .typed_get::<Authorization<Bearer>>()
        .ok_or(Error::NodeUnauthorized)?
        .token()
        .to_owned())
}

fn header<'a>(headers: &'a HeaderMap, name: &str) -> Result<&'a str> {
    headers
        .get(name)
        .and_then(|value| value.to_str().ok())
        .ok_or_else(|| Error::Validation(format!("缺少请求字段 {name}")))
}

async fn claim(State(store): State<Store>, headers: HeaderMap) -> Result<Response> {
    match store.claim_local_execution(&token(&headers)?).await? {
        Some(claim) => Ok(Json(claim).into_response()),
        None => Ok(StatusCode::NO_CONTENT.into_response()),
    }
}

async fn package(
    State(store): State<Store>,
    Path(id): Path<String>,
    headers: HeaderMap,
) -> Result<Json<Value>> {
    Ok(Json(
        store
            .local_execution_package(&token(&headers)?, &id)
            .await?,
    ))
}

#[derive(serde::Deserialize)]
#[serde(deny_unknown_fields)]
struct Heartbeat {
    attempt_id: String,
}

async fn heartbeat(
    State(store): State<Store>,
    Path(id): Path<String>,
    headers: HeaderMap,
    Json(input): Json<Heartbeat>,
) -> Result<Json<Value>> {
    Ok(Json(
        store
            .local_execution_heartbeat(&token(&headers)?, &id, &input.attempt_id)
            .await?,
    ))
}

async fn event(
    State(store): State<Store>,
    Path(id): Path<String>,
    headers: HeaderMap,
    Json(document): Json<Value>,
) -> Result<Json<Value>> {
    let event: ExecutionEvent = serde_json::from_value(document.clone())
        .map_err(|error| Error::Validation(error.to_string()))?;
    Ok(Json(
        store
            .local_execution_event(&token(&headers)?, &id, event, document)
            .await?,
    ))
}

async fn complete(
    State(store): State<Store>,
    Path(id): Path<String>,
    headers: HeaderMap,
    Json(document): Json<Value>,
) -> Result<Json<Value>> {
    let input: ExecutionCompletion = serde_json::from_value(document.clone())
        .map_err(|error| Error::Validation(error.to_string()))?;
    Ok(Json(
        store
            .complete_local_execution(&token(&headers)?, &id, input, document)
            .await?,
    ))
}

async fn input_file(
    State(store): State<Store>,
    Path((id, file_id)): Path<(String, String)>,
    headers: HeaderMap,
) -> Result<Response> {
    let package = store
        .local_execution_package(&token(&headers)?, &id)
        .await?;
    let file = package["input_files"]
        .as_array()
        .ok_or(Error::State("输入文件列表缺失"))?
        .iter()
        .find(|file| file["file_id"] == file_id)
        .ok_or(Error::NotFound)?;
    let range = headers
        .get("range")
        .map(|value| value.to_str())
        .transpose()
        .map_err(|_| Error::Invalid("Range 请求无效"))?;
    let result = store
        .asset_response(
            crate::local_api::text(file, "asset_id")?,
            crate::local_api::text(file, "asset_version_id")?,
            "GET",
            range,
        )
        .await?;
    let mut response = Response::builder()
        .status(result.status)
        .header("content-type", result.mime_type)
        .header("content-length", result.content_length)
        .header("accept-ranges", "bytes")
        .header("x-content-sha256", crate::local_api::text(file, "sha256")?);
    if let Some(range) = result.content_range {
        response = response.header("content-range", range);
    }
    response
        .body(Body::from(result.body))
        .map_err(|_| Error::State("构造文件响应失败"))
}

async fn output_file(
    State(store): State<Store>,
    Path((id, output_id)): Path<(String, String)>,
    headers: HeaderMap,
    body: Body,
) -> Result<Json<Value>> {
    let token = token(&headers)?;
    let package = store.local_execution_package(&token, &id).await?;
    let attempt = header(&headers, "x-attempt-id")?;
    if package["attempt_id"] != attempt {
        return Err(Error::Conflict("执行尝试编号不匹配".to_owned()));
    }
    if output_id.is_empty()
        || output_id.len() > 128
        || !output_id
            .bytes()
            .all(|byte| byte.is_ascii_alphanumeric() || matches!(byte, b'-' | b'_'))
    {
        return Err(Error::Invalid("输出标识无效"));
    }
    let key = header(&headers, "x-output-field")?;
    let index = header(&headers, "x-output-index")?
        .parse::<i64>()
        .map_err(|_| Error::Invalid("输出序号无效"))?;
    if index < 0 {
        return Err(Error::Invalid("输出序号无效"));
    }
    let field = package["output_fields"]
        .as_array()
        .ok_or(Error::State("输出字段缺失"))?
        .iter()
        .find(|field| field["key"] == key)
        .ok_or(Error::Invalid("输出字段不存在"))?;
    let expected = header(&headers, "x-content-sha256")?;
    if expected.len() != 64
        || !expected
            .bytes()
            .all(|byte| byte.is_ascii_digit() || (b'a'..=b'f').contains(&byte))
    {
        return Err(Error::Invalid("输出摘要无效"));
    }
    let length = header(&headers, "content-length")?
        .parse::<u64>()
        .map_err(|_| Error::Invalid("输出长度无效"))?;
    if length == 0 || length > 256 << 20 {
        return Err(Error::Invalid("输出文件大小无效或超过 256 MiB"));
    }
    let media_type = header(&headers, "content-type")?;
    let current = store.get_local_execution(&id).await?;
    let already_saved = current["outputs"]
        .as_array()
        .ok_or(Error::State("已保存输出缺失"))?
        .iter()
        .any(|output| output["output_id"] == output_id);
    if !already_saved && !matches!(current["status"].as_str(), Some("running" | "collecting")) {
        return Err(Error::Conflict("当前阶段不接受输出".to_owned()));
    }
    let directory = store.root.join("execution-transfers");
    if directory.is_symlink() {
        return Err(Error::Invalid("结果目录不能使用符号链接"));
    }
    std::fs::create_dir_all(&directory)?;
    let staging = tempfile::NamedTempFile::new_in(&directory)?;
    let mut file = tokio::fs::File::from_std(staging.reopen()?);
    let mut stream = body.into_data_stream();
    let mut size = 0u64;
    let mut hash = Sha256::new();
    while let Some(chunk) = stream.next().await {
        let chunk = chunk.map_err(|error| Error::Validation(format!("结果上传中断：{error}")))?;
        size = size
            .checked_add(chunk.len() as u64)
            .ok_or(Error::Invalid("输出长度超过范围"))?;
        if size > length {
            return Err(Error::Invalid("输出长度超过声明值"));
        }
        hash.update(&chunk);
        file.write_all(&chunk).await?;
    }
    file.flush().await?;
    file.sync_all().await?;
    if size != length || format!("{:x}", hash.finalize()) != expected {
        return Err(Error::Invalid("输出长度或摘要校验失败"));
    }
    let bytes = tokio::fs::read(staging.path()).await?;
    let detected = infer::get(&bytes);
    let extension = if let Some(kind) = detected {
        if kind.mime_type() != media_type {
            return Err(Error::Invalid("结果格式与 Content-Type 不一致"));
        }
        kind.extension()
    } else if field["type"] == "text" && media_type.starts_with("text/") {
        std::str::from_utf8(&bytes).map_err(|_| Error::Invalid("文本输出需要 UTF-8"))?;
        "txt"
    } else if field["type"] == "file" {
        "bin"
    } else {
        return Err(Error::Invalid("结果文件格式无法识别"));
    };
    let valid_kind = match field["type"].as_str() {
        Some("image") => media_type.starts_with("image/"),
        Some("video") => media_type.starts_with("video/"),
        Some("audio") => media_type.starts_with("audio/"),
        Some("text") => media_type.starts_with("text/"),
        Some("file") => true,
        _ => false,
    };
    if !valid_kind {
        return Err(Error::Invalid("结果类型与输出字段不一致"));
    }
    let asset = store
        .create_asset_bytes(
            current["session_id"].as_str(),
            &format!("{output_id}.{extension}"),
            &bytes,
            "workflow",
            &format!("execution:{id}:{output_id}"),
        )
        .await?;
    let mut transaction = store.pool.begin_with("BEGIN IMMEDIATE").await?;
    let node: Option<String> =
        sqlx::query_scalar("SELECT id FROM compute_nodes WHERE credential_hash=?")
            .bind(crate::assets::digest(token.as_bytes()))
            .fetch_optional(&mut *transaction)
            .await?;
    if node.as_deref() != current["node_id"].as_str() {
        return Err(Error::NodeUnauthorized);
    }
    let version = asset
        .versions
        .first()
        .ok_or(Error::State("输出资产版本缺失"))?;
    sqlx::query("INSERT INTO local_execution_outputs(execution_id,output_id,field_key,output_index,sha256,size_bytes,media_type,asset_id,asset_version_id) VALUES(?,?,?,?,?,?,?,?,?) ON CONFLICT(execution_id,output_id) DO NOTHING")
        .bind(&id).bind(&output_id).bind(key).bind(index).bind(expected).bind(size as i64).bind(media_type).bind(&asset.id).bind(&version.id).execute(&mut *transaction).await?;
    let saved =
        sqlx::query("SELECT * FROM local_execution_outputs WHERE execution_id=? AND output_id=?")
            .bind(&id)
            .bind(&output_id)
            .fetch_one(&mut *transaction)
            .await?;
    if saved.get::<String, _>("sha256") != expected
        || saved.get::<String, _>("field_key") != key
        || saved.get::<i64, _>("output_index") != index
        || saved.get::<String, _>("media_type") != media_type
    {
        return Err(Error::Conflict("输出标识已用于其他结果".to_owned()));
    }
    transaction.commit().await?;
    store.place_video_generation_output(&id,&asset.id,&version.id).await?;
    if asset.kind == "video" {
        store.ensure_video_boundary_outputs(current["session_id"].as_str(), &id, &asset.id, &version.id, index).await?;
    }
    Ok(Json(
        json!({ "output_id": output_id, "sha256": expected, "asset_id": saved.get::<String, _>("asset_id"), "asset_version_id": saved.get::<String, _>("asset_version_id"), "confirmed": true }),
    ))
}

async fn presence(
    State(store): State<Store>,
    headers: HeaderMap,
    payload: std::result::Result<Json<NodePresence>, JsonRejection>,
) -> Result<Json<Value>> {
    let authorization = headers
        .typed_get::<Authorization<Bearer>>()
        .ok_or(Error::NodeUnauthorized)?;
    let Json(payload) = payload.map_err(|error| Error::Validation(error.body_text()))?;
    Ok(Json(
        store
            .record_compute_presence(authorization.token(), payload)
            .await?,
    ))
}
