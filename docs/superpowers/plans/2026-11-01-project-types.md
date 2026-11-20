# 项目类型与通用项目

**目标：** 新建和编辑项目时选择通用或视频，未指定类型时保存为通用。类型决定默认工作区，现有内容持续保留。

**设计：** `projects.project_type` 保存 `general` 或 `video`。现有项目迁移为 `general`。通用项目默认展示项目资产，视频项目默认展示视频编辑器；链接明确指定的面板优先。两类项目均支持全部资产、Chat、技能、工作流和会话任务，也可手动切换视频编辑器。移动窗口与桌面使用相同的选中面板。

**通用能力：** 管理共享文档、图片、音频和视频；在不同会话中引用指定资产版本；创建与修改 Markdown 文档；使用技能、专家和工作流生成内容；查看任务状态和产物；使用分类、标签、评分与导出操作组织项目资产。引用的资料通过现有选择机制加入 Chat。

**使用的技术：** React 19、已有 shadcn/ui、TanStack Query、Tauri、Rust、SQLite。

**约束：** 当前工作区修改；使用文件编辑工具；测试数据写入 `.local-cache`；使用真实数据库与原生 DOM 验收；复用已有视频；不调用视频模型；不创建漫画编辑器入口。

## 任务一：类型持久化与 API

- [x] 在 `src-tauri/tests/local_api.rs` 添加真实数据库测试，验证默认通用、视频创建、修改类型、只重命名保留类型、非法类型拒绝和重新打开数据库后的类型。
- [x] 运行 `cargo test --test integration project_types`，确认功能缺失导致断言失败。
- [x] 添加 `src-tauri/migrations/0030_project_types.sql`，新增非空 `project_type` 字段，默认 `general`，数据库限定 `general/video`。
- [x] 修改 `src-tauri/src/projects.rs`：`Project.project_type: String`；保留 `create_project(name)` 默认行为，新增 `create_project_with_type(name, project_type)`；`update_project(id, name, project_type: Option<&str>)` 在同一条 SQL 中保存名称和类型。类型校验在写入前处理。
- [x] 修改 `src-tauri/src/local_api.rs`：POST 省略或传入 null 类型时创建通用项目，PATCH 省略类型时保留，传入 null 时改为通用，未知类型或非字符串值直接拒绝。
- [x] 运行类型与现有项目、资产 API 测试。

## 任务二：项目表单

- [x] 修改 `src/lib/api/studio.ts`，定义 `StudioProjectType = 'general' | 'video'`，校验响应类型，创建和编辑请求提交 `project_type`。
- [x] 使用 shadcn CLI 安装 ToggleGroup；修改 `src/features/studio/studio-project-dialog.tsx`，使用 FieldGroup、Field 和 ToggleGroup 展示项目名称、项目类型，默认通用，保持一项选中；保存失败保留输入。
- [x] 将侧栏项目菜单的编辑入口改为「编辑项目」，支持同时修改名称与类型，保存后刷新共享项目查询。

## 任务三：默认工作区

- [x] 在 `src/features/studio/studio-workspace.tsx` 读取共享项目查询，根据会话所属项目类型计算默认面板；缺省显示资产，视频项目显示视频编辑器，显式路由参数优先。
- [x] 移动工作台使用相同的路由面板。
- [x] 保留 380px Chat 最小宽度、编辑器持续挂载、草稿和宽度保持机制。

## 任务四：运行与验收

- [x] 执行前端测试、生产构建、Rust 相关集成测试。
- [x] 添加原生项目类型验收脚本，通过真实表单创建默认通用与视频项目，编辑类型，检查数据库响应、桌面与移动默认面板、手动切换及原有内容保留。
- [x] 构建 Tauri 生产应用及原生验收程序，检查签名并运行原生脚本。
- [x] 更新验证文档，确认视频任务数量没有增长。
