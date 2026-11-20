mod agent;
mod approvals;
mod asset_analysis;
mod assets;
mod commands;
mod compaction;
mod compute_nodes;
mod context;
mod context_records;
mod desktop_state;
mod edge_api;
mod error;
mod executions;
mod expert_cards;
mod expert_runtime;
mod expert_tasks;
mod experts;
mod file_transfer;
mod library;
mod library_operations;
mod library_tree;
mod local_api;
mod local_agents;
mod mcp;
mod mcp_execution;
mod mcp_inputs;
mod media;
mod menu;
mod models;
mod node_observation;
mod http_generation;
mod generation_providers;
mod online_generation;
mod projects;
mod run_storage;
mod runs;
mod sessions;
mod settings;
mod skills;
mod store;
mod taxonomy;
mod tools;
mod trajectory;
mod video_export;
mod video_work_exports;
mod video_project;
mod video_creation;
mod video_work;
mod video_generations;
mod video_generation_batches;
mod video_generation_dependencies;
mod video_frames;
mod video_materials;
mod workflow_definitions;
mod workflow_execution;
mod workflow_inputs;
mod workflows;

pub use agent::AgentRuntime;
pub use assets::{Asset, AssetVersion};
pub use edge_api::compute_router;
pub use error::{Error, Result};
pub use file_transfer::AssetExport;
pub use library::LibraryQuery;
pub use local_api::{LocalApi, LocalRequest};
pub use projects::Project;
pub use sessions::Session;
pub use store::Store;

#[derive(Default)]
struct DesktopExitState(std::sync::atomic::AtomicBool);

pub fn desktop_builder() -> tauri::Builder<tauri::Wry> {
    use tauri::{Emitter, Listener, Manager};

    tauri::Builder::default()
        .plugin(tauri_plugin_single_instance::init(|app, _args, _cwd| {
            let window = app.get_webview_window("main").expect("Studio 主窗口不存在");
            window.unminimize().expect("恢复 Studio 窗口失败");
            window.show().expect("显示 Studio 窗口失败");
            window.set_focus().expect("激活 Studio 窗口失败");
        }))
        .plugin(tauri_plugin_dialog::init())
        .plugin(tauri::plugin::Builder::<tauri::Wry, ()>::new("local-agents").on_event(|app, event| {
            if matches!(event, tauri::RunEvent::Exit) && let Some(service) = app.try_state::<local_agents::LocalAgentService>() {
                service.stop().expect("停止本机 Agent 服务失败");
            }
        }).build())
        .setup(|app| {
            app.manage(DesktopExitState::default());
            let handle = app.handle().clone();
            app.listen("studio:exit-guard-ready", move |event| {
                let ready: bool = serde_json::from_str(event.payload()).expect("退出保护状态无效");
                handle.state::<DesktopExitState>().0.store(ready, std::sync::atomic::Ordering::SeqCst);
            });
            let root = match std::env::var_os("PIXOMA_STUDIO_DATA_DIR") {
                Some(path) => {
                    std::path::PathBuf::from(path)
                }
                None => app.path().app_local_data_dir()?.join("standalone"),
            };
            let Some(directory_lock) = Store::try_lock_application_directory(&root)? else {
                std::process::exit(0);
            };
            app.manage(directory_lock);
            asset_analysis::remove_interrupted_video_analysis(&root)?;
            let store = tauri::async_runtime::block_on(Store::open(&root.join("studio-standalone.sqlite")))?;
            tauri::async_runtime::block_on(store.recover_interrupted_runs())?;
            let compute_store = store.clone();
            let compute_address = store.compute_address;
            let runtime_store = store.clone();
            let compute_server = tauri::async_runtime::block_on(async move {
                let listener = tokio::net::TcpListener::bind(compute_address).await?;
                Ok::<_, std::io::Error>(tokio_util::task::AbortOnDropHandle::new(tokio::spawn(async move {
                    if let Err(error) = axum::serve(listener, compute_router(compute_store)).await {
                        runtime_store.set_compute_service_error(format!("计算节点服务已停止：{error}")).expect("保存节点服务错误失败");
                    }
                })))
            });
            match compute_server {
                Ok(server) => {
                    app.manage(server);
                    let agent_store = store.clone();
                    app.manage(tauri::async_runtime::block_on(async move { local_agents::start(agent_store) }));
                }
                Err(error) => store.set_compute_service_error(format!("无法监听 {compute_address}：{error}"))?,
            }
            let state = commands::AppState::new(store);
            state.resume_pending();
            app.manage(state);
            Ok(())
        })
        .on_window_event(|window, event| {
            if window.label() == "main" && window.state::<DesktopExitState>().0.load(std::sync::atomic::Ordering::SeqCst)
                && let tauri::WindowEvent::CloseRequested { api, .. } = event {
                api.prevent_close();
                window.emit("studio:exit-requested", ()).expect("发送关闭请求失败");
            }
        })
        .menu(menu::build)
        .on_menu_event(menu::on_event)
        .invoke_handler(tauri::generate_handler![
            commands::studio_request,
            commands::studio_confirm_exit,
            commands::studio_upload,
            commands::studio_inspect_skill,
            commands::studio_export,
            commands::studio_export_asset_version,
            commands::studio_run,
            commands::studio_subscribe,
            commands::studio_unsubscribe
        ])
        .register_asynchronous_uri_scheme_protocol("pixoma-media", media::handle)
}

pub fn desktop_context() -> tauri::Context<tauri::Wry> {
    tauri::generate_context!()
}

pub fn run() {
    use tauri::{Emitter, Manager};
    desktop_builder()
        .build(desktop_context())
        .expect("启动 Pixoma Studio 失败")
        .run(|app, event| {
            if let tauri::RunEvent::ExitRequested { code: None, api, .. } = event
                && app.state::<DesktopExitState>().0.load(std::sync::atomic::Ordering::SeqCst)
                && app.get_webview_window("main").is_some() {
                api.prevent_exit();
                app.emit("studio:exit-requested", ()).expect("发送退出请求失败");
            }
        });
}
