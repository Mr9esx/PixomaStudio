use pixoma_studio::Store;
use serde_json::json;
use std::path::PathBuf;

#[tokio::main]
async fn main() -> Result<(), Box<dyn std::error::Error>> {
    let root = PathBuf::from(std::env::var_os("PIXOMA_STUDIO_DATA_DIR").ok_or("需要独立验收数据库目录")?);
    let path = root.join("studio-standalone.sqlite");
    let store = Store::open(&path).await?;
    let pool = sqlx::SqlitePool::connect_with(sqlx::sqlite::SqliteConnectOptions::new().filename(&path).read_only(true)).await?;
    let waiting: Vec<(String,i64)> = sqlx::query_as("SELECT e.request_id,e.waiting FROM online_executions e JOIN video_generations g ON g.execution_request_id=e.request_id UNION ALL SELECT e.request_id,e.waiting FROM local_executions e JOIN video_generations g ON g.execution_request_id=e.request_id").fetch_all(&pool).await?;
    if !waiting.iter().any(|(_,value)|*value==1) {return Err("需要已有作品任务的等待记录".into());}
    store.clear_sessions().await?;
    for (request,before) in &waiting {
        let after: i64 = sqlx::query_scalar("SELECT waiting FROM online_executions WHERE request_id=? UNION ALL SELECT waiting FROM local_executions WHERE request_id=?").bind(request).bind(request).fetch_one(&pool).await?;
        assert_eq!(after,*before,"清空对话改变了作品任务的等待设置：{request}");
    }
    pool.close().await;
    store.close().await;
    println!("{}",json!({"success":true,"verified_executions":waiting.len(),"video_waiting_preserved":true}));
    Ok(())
}
