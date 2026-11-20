use tauri::{
    Emitter,
    menu::{Menu, MenuBuilder, SubmenuBuilder},
};

pub(crate) fn build(app: &tauri::AppHandle) -> tauri::Result<Menu<tauri::Wry>> {
    let application = SubmenuBuilder::new(app, "Pixoma Studio")
        .text("settings.models", "配置模型")
        .text("settings.skills", "管理技能")
        .text("settings.mcp", "配置 MCP")
        .text("settings.compute-nodes", "计算节点")
        .separator()
        .quit_with_text("退出 Pixoma Studio")
        .build()?;
    let edit = SubmenuBuilder::new(app, "编辑")
        .undo_with_text("撤销")
        .redo_with_text("重做")
        .separator()
        .cut_with_text("剪切")
        .copy_with_text("复制")
        .paste_with_text("粘贴")
        .select_all_with_text("全选")
        .build()?;
    MenuBuilder::new(app).items(&[&application, &edit]).build()
}

pub(crate) fn on_event(app: &tauri::AppHandle, event: tauri::menu::MenuEvent) {
    let section = match event.id.as_ref() {
        "settings.models" => "models",
        "settings.skills" => "skills",
        "settings.mcp" => "mcp",
        "settings.compute-nodes" => "compute-nodes",
        _ => return,
    };
    app.emit("studio:navigate", section)
        .expect("打开配置页面失败");
}
