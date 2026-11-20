# Studio 连续页面实施计划

执行方式：当前工作区，由主会话使用 executing-plans 顺序执行。

目标：按照已确认的 HTML Demo 调整资产库、工作流、技能与专家的页面结构，复用现有编辑、版本与执行功能。

结构：StudioWorkspace 保留导航与主题。技能编辑、历史版本、工作流运行接入主内容区域；资产详情使用列表旁的属性区域，扩大预览使用同页内容。使用现有 TanStack Router、React Query、shadcn/ui 和 CodeMirror。

约束：代码使用英文 identifier；文案遵循 Pixoma Voice；颜色使用语义令牌；主体保留边框；仅使用文件编辑工具修改源码；验证使用真实应用与独立本地数据库。

## 页面与状态

- 技能：`AssetCatalog` 通过路由查询参数保存当前编辑对象；导入文件由列表组件持有；`SkillWorkspaceDialog` 的编辑内容调整为页面组件。文件、基本信息、版本记录使用标签切换，版本保存和差异在同页展开。
- 专家：保留现有表单与引用编辑器，使用页内版本记录与保存区域，增加路由离开检查。
- 工作流：复用 `WorkflowEditor`、`WorkflowVersions` 与执行表单。版本内容和运行使用页面区域，编辑表单保留节点设置入口和草稿恢复。
- 资产：复用 `AssetDetailsDialog` 的预览与属性内容，增加内嵌呈现方式用于资产库；会话里的资产详情保留原交互。

## 步骤

- [x] 扩展原生页面检查入口，验证技能创建使用主体页面。
- [x] 调整技能列表、编辑、版本保存与历史查看，加入未保存导航检查；验证真实技能创建、更新及历史内容。
- [x] 调整专家的标签、版本和保存区域，保留表单验证、引用和小型确认。
- [x] 调整工作流列表、详情、编辑与运行区域；保存、查看和运行版本在同页处理。
- [x] 调整资产库区域间距、列表边界和内嵌详情；保留筛选、项目树、批量操作、下载与引用能力。
- [x] 运行 TypeScript、现有前端测试、构建与原生交互检查，验证主题和 1440 / 1024 / 760 像素宽度。

## 验证方法

复用 `src-tauri/examples/native_layout.rs`，通过环境参数选择本轮 JavaScript 检查。检查调用真实 `studio_request` 接口创建技能、工作流与资产，再操作渲染后的 DOM。检查技能保存前后的正文和版本内容，确认编辑、版本与执行区域没有大型 Dialog。检查继续编辑、放弃修改和浏览器返回。对每种窗口宽度检查主体与操作按钮边界，保留原有主题与侧栏验证。

检查脚本为 `src-tauri/examples/native_integrated_pages.js`、`src-tauri/examples/native_workflow_pages.js`。验证结果见 `docs/frontend/studio-integrated-pages.md`。

前端命令：`pnpm test`、`pnpm build`。桌面检查：`cargo run --manifest-path src-tauri/Cargo.toml --example native_layout`，设置 `PIXOMA_STUDIO_DATA_DIR`、`PIXOMA_STUDIO_AGENT_LISTEN`、`PIXOMA_LAYOUT_SCRIPT` 和项目内 `TMPDIR`。
