use crate::{
    Error, Result, Store,
    commands::{AppState, CommandError},
};
use tauri::{
    Manager,
    http::{Request, Response},
};
use tokio::io::{AsyncReadExt, AsyncSeekExt};
use url::Url;

pub struct MediaResponse {
    pub status: u16,
    pub body: Vec<u8>,
    pub mime_type: String,
    pub content_length: u64,
    pub content_range: Option<String>,
}

impl Store {
    pub async fn asset_poster_response(
        &self,
        asset: &str,
        version: &str,
        method: &str,
    ) -> Result<MediaResponse> {
        if !["GET", "HEAD"].contains(&method) {
            return Err(Error::Invalid("媒体请求方法无效"));
        }
        let path: String = sqlx::query_scalar("SELECT b.path FROM asset_versions v JOIN blobs b ON b.digest=json_extract(v.metadata,'$.poster_digest') WHERE v.asset_id=? AND v.id=?")
            .bind(asset).bind(version).fetch_optional(&self.pool).await?.ok_or(Error::NotFound)?;
        let path = std::path::PathBuf::from(path).canonicalize()?;
        let root = self.root.join("assets").canonicalize()?;
        if !path.starts_with(root) {
            return Err(Error::Invalid("文件路径超出授权范围"));
        }
        let content_length = tokio::fs::metadata(&path).await?.len();
        let body = if method == "GET" {
            tokio::fs::read(path).await?
        } else {
            Vec::new()
        };
        Ok(MediaResponse {
            status: 200,
            body,
            mime_type: "image/png".to_owned(),
            content_length,
            content_range: None,
        })
    }

    pub async fn expert_avatar_response(
        &self,
        expert_id: &str,
        version: &str,
        method: &str,
    ) -> Result<MediaResponse> {
        let expert = self.expert_version(expert_id, version).await?;
        let asset = crate::local_api::text(&expert, "avatar_asset_id")?;
        let version = crate::local_api::text(&expert, "avatar_version_id")?;
        if asset.is_empty() || version.is_empty() {
            return Err(Error::NotFound);
        }
        let response = self.asset_response(asset, version, method, None).await?;
        if !response.mime_type.starts_with("image/") {
            return Err(Error::Invalid("专家头像需要图片"));
        }
        Ok(response)
    }

    pub async fn asset_response(
        &self,
        asset_id: &str,
        version_id: &str,
        method: &str,
        range: Option<&str>,
    ) -> Result<MediaResponse> {
        if !["GET", "HEAD"].contains(&method) {
            return Err(Error::Invalid("媒体请求方法无效"));
        }
        let path = self.asset_path(asset_id, version_id).await?;
        let mime_type: String =
            sqlx::query_scalar("SELECT mime_type FROM asset_versions WHERE asset_id=? AND id=?")
                .bind(asset_id)
                .bind(version_id)
                .fetch_one(&self.pool)
                .await?;
        let mut file = tokio::fs::File::open(path).await?;
        let size = file.metadata().await?.len();
        let (start, length, content_range, status) = if let Some(header) = range {
            let ranges = http_range::HttpRange::parse(header, size).map_err(|_| Error::Remote {
                status: 416,
                code: 0,
                message: "媒体读取范围无效".to_owned(),
            })?;
            if ranges.len() != 1 {
                return Err(Error::Invalid("媒体请求需要单个读取范围"));
            }
            let range = &ranges[0];
            (
                range.start,
                range.length,
                Some(format!(
                    "bytes {}-{}/{}",
                    range.start,
                    range.start + range.length - 1,
                    size
                )),
                206,
            )
        } else {
            (0, size, None, 200)
        };
        let mut body = Vec::new();
        if method != "HEAD" {
            file.seek(std::io::SeekFrom::Start(start)).await?;
            file.take(length).read_to_end(&mut body).await?;
            if body.len() as u64 != length {
                return Err(Error::State("媒体文件在读取期间发生变化"));
            }
        }
        Ok(MediaResponse {
            status,
            body,
            mime_type,
            content_length: length,
            content_range,
        })
    }
}

async fn response(app: &tauri::AppHandle, request: Request<Vec<u8>>) -> Result<Response<Vec<u8>>> {
    let url = Url::parse(&request.uri().to_string()).map_err(|_| Error::Invalid("媒体地址无效"))?;
    let parts: Vec<_> = url
        .path_segments()
        .ok_or(Error::Invalid("媒体路径无效"))?
        .collect();
    let state = app.state::<AppState>();
    let access = state.access().await?;
    let range = request
        .headers()
        .get("Range")
        .map(|value| {
            value
                .to_str()
                .map_err(|_| Error::Invalid("媒体范围编码无效"))
        })
        .transpose()?;
    let media = access
        .execute(async {
            match parts.as_slice() {
                ["expert-avatars", "studio", id] => {
                    uuid::Uuid::parse_str(id).map_err(|_| Error::Invalid("专家标识无效"))?;
                    let version = url
                        .query_pairs()
                        .find(|(key, _)| key == "version")
                        .map(|(_, version)| version.into_owned())
                        .ok_or(Error::Invalid("头像缺少专家版本"))?;
                    state.store.expert_avatar_response(id, &version, request.method().as_str()).await
                }
                [asset, version] | [asset, version, "poster"] => {
                    uuid::Uuid::parse_str(asset).map_err(|_| Error::Invalid("资产标识无效"))?;
                    uuid::Uuid::parse_str(version).map_err(|_| Error::Invalid("版本标识无效"))?;
                    if parts.len() == 3 {
                        state
                            .store
                            .asset_poster_response(asset, version, request.method().as_str())
                            .await
                    } else {
                        state
                            .store
                            .asset_response(asset, version, request.method().as_str(), range)
                            .await
                    }
                }
                _ => Err(Error::Invalid("媒体标识无效")),
            }
        })
        .await?;
    let mut builder = Response::builder()
        .status(media.status)
        .header("Content-Type", media.mime_type)
        .header("Content-Length", media.content_length)
        .header("Accept-Ranges", "bytes")
        .header("Cache-Control", "private, no-store");
    if let Some(origin) = request.headers().get("Origin") {
        let origin = origin
            .to_str()
            .map_err(|_| Error::Invalid("请求来源编码无效"))?;
        if !allowed_origin(origin, app.config().build.dev_url.as_ref()) {
            return Err(Error::Invalid("媒体请求来源无效"));
        }
        builder = builder.header("Access-Control-Allow-Origin", origin);
    }
    if let Some(range) = media.content_range {
        builder = builder.header("Content-Range", range);
    }
    Ok(builder.body(media.body).expect("构造媒体响应失败"))
}

fn allowed_origin(origin: &str, dev_url: Option<&Url>) -> bool {
    ["tauri://localhost", "http://tauri.localhost", "https://tauri.localhost"].contains(&origin)
        || cfg!(debug_assertions) && dev_url.is_some_and(|url| url.origin().ascii_serialization() == origin)
}

pub(crate) fn handle(
    context: tauri::UriSchemeContext<'_, tauri::Wry>,
    request: Request<Vec<u8>>,
    responder: tauri::UriSchemeResponder,
) {
    let app = context.app_handle().clone();
    tauri::async_runtime::spawn(async move {
        let origin = request.headers().get("Origin").and_then(|value|value.to_str().ok()).filter(|origin|allowed_origin(origin, app.config().build.dev_url.as_ref())).map(str::to_owned);
        let response = match response(&app, request).await {
            Ok(response) => response,
            Err(error) => {
                let error = CommandError::from(error);
                let mut response = Response::builder()
                    .status(if error.status >= 100 {
                        error.status
                    } else {
                        503
                    })
                    .header("Content-Type", "text/plain; charset=utf-8");
                if let Some(origin) = origin {response = response.header("Access-Control-Allow-Origin", origin);}
                response
                    .body(error.message.into_bytes())
                    .expect("构造媒体错误响应失败")
            }
        };
        responder.respond(response);
    });
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn media_origin_uses_the_configured_development_page_only() -> Result<()> {
        let page = Url::parse("http://127.0.0.1:4179/studio").map_err(|_|Error::State("页面地址无效"))?;
        assert!(allowed_origin("tauri://localhost", None));
        assert!(allowed_origin("http://127.0.0.1:4179", Some(&page)));
        assert!(!allowed_origin("http://127.0.0.1:1420", Some(&page)));
        assert!(!allowed_origin("https://example.com", Some(&page)));
        assert!(!allowed_origin("http://127.0.0.1:4179", None));
        Ok(())
    }
}
