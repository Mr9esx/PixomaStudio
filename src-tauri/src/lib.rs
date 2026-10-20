mod agent;
mod approvals;
mod asset_analysis;
mod assets;
mod commands;
mod compaction;
mod context;
mod context_records;
mod credentials;
mod desktop_config;
mod desktop_state;
mod error;
mod expert_cards;
mod expert_runtime;
mod expert_tasks;
mod experts;
mod file_transfer;
mod flow;
mod library;
mod library_operations;
mod library_tree;
mod local_api;
mod mcp;
mod mcp_execution;
mod mcp_inputs;
mod media;
mod menu;
mod models;
mod platform;
mod projects;
mod publications;
mod run_storage;
mod runs;
mod sessions;
mod settings;
mod skills;
mod store;
mod taxonomy;
mod tools;
mod trajectory;
mod workflow_execution;
mod workflow_inputs;
mod workflows;

pub use agent::AgentRuntime;
pub use assets::{Asset, AssetVersion};
pub use error::{Error, Result};
pub use file_transfer::AssetExport;
pub use library::LibraryQuery;
pub use local_api::{LocalApi, LocalRequest};
pub use platform::PlatformClient;
pub use projects::Project;
pub use sessions::Session;
pub use store::Store;

pub fn desktop_builder() -> tauri::Builder<tauri::Wry> {
    use std::sync::Arc;
    use tauri::Manager;

    tauri::Builder::default()
        .plugin(tauri_plugin_single_instance::init(|app, _args, _cwd| {
            let window = app.get_webview_window("main").expect("Studio 主窗口不存在");
            window.unminimize().expect("恢复 Studio 窗口失败");
            window.show().expect("显示 Studio 窗口失败");
            window.set_focus().expect("激活 Studio 窗口失败");
        }))
        .plugin(tauri_plugin_dialog::init())
        .setup(|app| {
            let (root, configuration) = match std::env::var_os("PIXOMA_STUDIO_DATA_DIR") {
                Some(path) => {
                    let root = std::path::PathBuf::from(path);
                    let configuration = root.join("config");
                    (root, configuration)
                }
                None => (
                    app.path().app_local_data_dir()?,
                    app.path().app_config_dir()?,
                ),
            };
            let Some(directory_lock) = Store::try_lock_application_directory(&root)? else {
                std::process::exit(0);
            };
            app.manage(directory_lock);
            asset_analysis::remove_interrupted_video_analysis(&root)?;
            let store = tauri::async_runtime::block_on(Store::open(&root.join("studio.sqlite")))?;
            tauri::async_runtime::block_on(store.recover_interrupted_runs())?;
            let address = std::env::var("PIXOMA_PLATFORM_URL")
                .map(Some)
                .or_else(|error| match error {
                    std::env::VarError::NotPresent => Ok(None),
                    error => Err(error),
                })?;
            let platform = desktop_config::load_platform(
                &configuration,
                address.as_deref(),
                option_env!("PIXOMA_PLATFORM_URL"),
            )?
            .map(Arc::new);
            app.manage(commands::AppState::new(store, platform));
            Ok(())
        })
        .menu(menu::build)
        .on_menu_event(menu::on_event)
        .invoke_handler(tauri::generate_handler![
            commands::studio_request,
            commands::studio_upload,
            commands::studio_inspect_skill,
            commands::studio_export,
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
    desktop_builder()
        .run(desktop_context())
        .expect("启动 Pixoma Studio 失败");
}
