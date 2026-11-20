use pixoma_studio::{Error, Store};
use serde_json::{Value, json};
use std::{path::PathBuf, time::Duration};

#[tokio::main]
async fn main() -> Result<(), Box<dyn std::error::Error>> {
    let root = PathBuf::from(std::env::var_os("PIXOMA_STUDIO_DATA_DIR").ok_or("需要独立验收数据目录")?);
    let manifest = PathBuf::from(std::env::var_os("PIXOMA_TEST_BATCH_MANIFEST").ok_or("需要已有真实批次记录")?);
    let manifest: Value = serde_json::from_slice(&std::fs::read(manifest)?)?;
    let video = manifest["video_id"].as_str().ok_or("作品标识缺失")?;
    let batch = manifest["batch_id"].as_str().ok_or("批次标识缺失")?;
    let session = manifest["session_id"].as_str().ok_or("主对话标识缺失")?;
    let path = root.join("studio-standalone.sqlite");
    let store = Store::open(&path).await?;
    let pool = sqlx::SqlitePool::connect_with(sqlx::sqlite::SqliteConnectOptions::new().filename(&path)).await?;
    let (execution, connection, original_base): (String, String, String) = sqlx::query_as("SELECT e.id,e.connection_id,c.base_url FROM video_generations g JOIN video_generation_batch_items i ON i.video_id=g.video_id AND i.generation_id=g.id JOIN online_executions e ON e.request_id=g.execution_request_id JOIN generation_services c ON c.id=e.connection_id WHERE i.video_id=? AND i.batch_id=? AND i.position=0 AND e.status IN ('pending','running')")
        .bind(video).bind(batch).fetch_one(&pool).await?;
    let child: String = sqlx::query_scalar("SELECT generation_id FROM video_generation_batch_items WHERE video_id=? AND batch_id=? AND position=1")
        .bind(video).bind(batch).fetch_one(&pool).await?;
    let before = store.read_video_work(video).await?;
    let stopped = store.cancel_local_execution(&execution, true).await?;
    if stopped["waiting"] != false { return Err("停止等待没有保存".into()); }
    sqlx::query("UPDATE generation_services SET base_url='https://127.0.0.1:9' WHERE id=?").bind(&connection).execute(&pool).await?;
    let interrupted: pixoma_studio::Result<()> = async {
        store.resume_video_generations().await?;
        let (state, error): (String, Option<String>) = sqlx::query_as("SELECT dependency_state,dependency_error FROM video_generations WHERE video_id=? AND id=?")
            .bind(video).bind(&child).fetch_one(&pool).await?;
        if state != "waiting" || error.as_deref().is_none_or(str::is_empty) { return Err(Error::State("连接中断后没有保留依赖等待和错误原因")); }
        let (state, error): (String, String) = sqlx::query_as("SELECT status,error FROM online_executions WHERE id=?").bind(&execution).fetch_one(&pool).await?;
        if !["pending", "running"].contains(&state.as_str()) || error.is_empty() { return Err(Error::State("连接中断后没有保存任务状态与查询错误")); }
        Ok(())
    }.await;
    sqlx::query("UPDATE generation_services SET base_url=? WHERE id=?").bind(&original_base).bind(&connection).execute(&pool).await?;
    interrupted?;
    store.cancel_video_dependency_wait(video, &child, json!({"session_id":session})).await?;
    let deadline = tokio::time::Instant::now() + Duration::from_secs(180);
    loop {
        store.resume_video_generations().await?;
        let status: String = sqlx::query_scalar("SELECT status FROM online_executions WHERE id=?").bind(&execution).fetch_one(&pool).await?;
        if status == "succeeded" { break; }
        if ["failed", "cancelled", "result_unknown"].contains(&status.as_str()) { return Err(format!("原提供方任务没有完成：{status}").into()); }
        if tokio::time::Instant::now() > deadline { return Err("原任务尚未结束，保留原批次继续查询".into()); }
        tokio::time::sleep(Duration::from_secs(2)).await;
    }
    let result = store.get_local_execution(&execution).await?;
    if result["waiting"] != false || result["error"] != "" || result["can_cancel"] != false { return Err("完成后的等待或取消状态无效".into()); }
    let outputs = result["outputs"].as_array().ok_or("真实输出缺失")?;
    if !outputs.iter().any(|output| output["media_type"].as_str().is_some_and(|kind| kind.starts_with("video/"))) { return Err("真实视频输出缺失".into()); }
    for output in outputs {
        let bytes = store.read_asset(output["asset_id"].as_str().ok_or("结果资产缺失")?, output["asset_version_id"].as_str().ok_or("结果版本缺失")?).await?;
        if bytes.len() < 1000 { return Err("实际媒体内容缺失".into()); }
    }
    let child_record = store.get_video_generation(video, &child).await?;
    if child_record["status"] != "cancelled" || !child_record["execution_id"].is_null() { return Err("取消依赖等待后提交了下游".into()); }
    if store.read_video_work(video).await?["project"] != before["project"] { return Err("恢复查询改变了作品内容".into()); }
    pool.close().await;
    store.close().await;
    println!("{}", json!({"success":true,"video_id":video,"batch_id":batch,"execution_id":execution,"realConnectionFailureRecorded":true,"dependencyWaitPreserved":true,"backgroundCollectionAfterStopWaiting":true,"actualMediaRead":true,"adoptedVideoPreserved":true}));
    Ok(())
}
