# Pixoma Studio 独立功能实施计划

日期：2026-10-29

> 执行方式：主会话使用 `executing-plans` 按任务顺序实施。业务行为采用 TDD，使用真实文件、SQLite 和实际服务。下列任务完成后逐项填写验证结果。

**目标：** 在现有 Studio 工程中完成本地业务、工作流、Rust 计算服务和 Go Edge Agent，页面沿用原组件与样式。

**结构：** React 通过 Tauri Commands 和 Channel 调用 Rust。Rust 共用现有 Store、Agent、MCP 与资产功能，使用 Axum 承接节点接口；Go Agent 独立运行并执行 ComfyUI。

**使用的技术：** Tauri、Rust、Tokio、SQLx、SQLite、reqwest、serde、Axum、Rig、rmcp、Go、React、TypeScript、Vite、shadcn/ui。

## 全局要求

- 设计依据为 `docs/design.md`；外部接口依据为 `docs/standalone-research.md`。
- Studio 的业务表、方法、类型与请求移除 `account_id`，业务 ID 直接标识对象。
- 实施仅覆盖功能与代码；使用全新应用目录建立数据库，不转换既有数据，不自动删除用户目录。
- 默认数据目录为 Tauri 本地应用目录下的 `standalone/`，数据库为 `studio-standalone.sqlite`。
- 不新增登录、注册、身份、平台目录、发布、Topic、任务队列设置或平台地址要求。
- 计算服务使用 Rust；ComfyUI 执行与节点采集使用独立 Go Agent。
- 前端从 Pixoma 原组件复制并调整，优先复用已经安装的 shadcn/ui；遵循 Pixoma 令牌、图标、文案与状态规范。
- 全部文件修改使用编辑工具。测试产物使用 `.local-cache/`，格式工具只执行检查。
- 不使用 mock、固定成功响应、截图测试或未执行的通过记录。服务不可用时记录实际阻碍。
- 不修改 Pixoma 的业务数据或计算服务；迁移后的构建、导入与运行依赖全部位于 Studio 工程及其正常依赖中。
- 提交时显式设置作者与提交者为 `李卓洲 <1138099359@qq.com>`。

## 任务 1：本地启动、请求与数据库

当前进度：全部 Rust 业务方法、数据库表和索引均使用本地业务 ID。桌面请求、媒体、运行恢复与工作流工具直接访问本地数据。全新数据目录直接进入 Studio，24 项真实桌面 DOM 页面检查通过。节点 HTTP 服务已接入应用启动，本机 Agent 的管理和打包继续按任务 9 处理。

**修改：** `src-tauri/src/lib.rs`、`desktop_state.rs`、`commands.rs`、`local_api.rs`、`store.rs`、`media.rs`、全部使用账号参数的业务模块及 `src-tauri/migrations/`；`src/routes/`、`src/features/studio/studio-auth.tsx`、`studio-workspace.tsx`、`studio-personal-settings.tsx`、`src/lib/api/operations.ts`、`src/lib/identity-cache.ts`、`src-tauri/build.rs`。

**移除：** 平台 Client、平台配置、发布模块、登录与注册页面，以及实际只被这些入口使用的类型、路由和依赖。按 import 调用关系确认文件范围。

**测试：** `src-tauri/tests/storage.rs`、`local_api.rs`、`credentials.rs`、`media.rs`；同步现有业务检查与 `tests/integration.rs` 的模块注册。

**接口：** `LocalApi::request(&self, request: LocalRequest) -> Result<Value>`；`Store::create_project(&self, name: &str) -> Result<Project>`；`Store::create_session(&self, request_id: &str) -> Result<Session>`。其他业务方法保持原参数顺序并移除账号参数。

- [x] 编写实际 SQLite 检查，创建项目与会话后重新打开仍可读取；重复 `request_id` 返回同一会话。
- [ ] 运行检查并确认当前签名无法通过；记录命令和失败原因。
- [x] 重建单一空间的 SQL 结构，更新所有调用、索引、唯一约束、缓存键、媒体读取与运行恢复入口。请求入口直接调用本地业务。
- [x] 删除身份查询、页面门禁和发布入口；启动直接打开 Studio，构建不读取平台地址。
- [x] 使用 `PRAGMA table_info` 和 `sqlite_master` 检查全部业务表与索引，确认不存在账号列或占位账号。
- [x] 运行 storage、local_api、media 检查及前端类型检查，用全新目录启动真实 App。

检查代码使用新签名：

```rust
let project = store.create_project("角色与分镜").await?;
let first = store.create_session("request-one").await?;
let again = store.create_session("request-one").await?;
assert_eq!(first.id, again.id);
assert_eq!(store.list_projects().await?[0].id, project.id);
```

**通过条件：** 无账号参数与平台 Client 的本地业务可以运行；项目、会话和媒体经过真实读写；没有启动身份请求。

## 任务 2：本地技能、专家、MCP 与原有业务

**修改：** `skills.rs`、`experts.rs`、`expert_runtime.rs`、`expert_cards.rs`、`expert_tasks.rs`、`mcp.rs`、`mcp_execution.rs`、`mcp_inputs.rs`、`settings.rs`、`models.rs`；`src/features/agent-assets/`、`src/features/studio/studio-market.tsx`、技能、专家与设置入口。

**测试：** `tests/skills.rs`、`experts.rs`、`settings.rs`、`assets.rs`、`library.rs`、`runs.rs`、`agent.rs`。

**接口：** 继续通过 `LocalApi::request` 维护技能、专家、模型和 MCP。对象只引用本地 ID 与具体版本，头像引用本地资产。

- [ ] 为实际技能目录、ZIP 导入、历史版本与专家 SOP 编写直接本地读写检查，运行并记录依赖缺失的失败结果。
- [x] 将列表与编辑入口连接到本地管理，移除目录获取、发布状态、平台资源 URL 与相关查询。
- [ ] 保留技能文件、专家版本、步骤、模型协议、工具审批、上下文、轨迹和 MCP 继续执行能力。
- [x] 对项目、资产、文档版本、预览、导出和批量操作执行已有真实存储检查；更新账号相关的调用与断言。
- [ ] 使用实际对话模型与独立 MCP 服务验证工具发现、凭据错误、审批与连接释放；结果未知时拒绝重复执行。

技能版本检查直接比较文件内容：

```rust
assert_eq!(std::fs::read(&exported_skill_file)?, original_skill_bytes);
assert_eq!(bound_skill_version, version_before_edit);
assert_eq!(pending_tool_call_id, restored_tool_call_id);
```

**通过条件：** 本地创建的技能和专家可在真实会话使用，既有媒体、审批与 MCP 功能通过对应检查。

## 任务 3：工作流定义、草稿与版本

当前进度：`workflow_definitions.rs`、独立业务表与本地 API 已实现，4 项真实数据库和接口检查通过；原附件已保存为版本并由独立 Go Agent 成功执行。页面复制和会话调用分别继续按任务 7、任务 8 处理。

**修改：** `src-tauri/src/workflows.rs`、`workflow_inputs.rs`、`local_api.rs`、`lib.rs` 与本地 SQL 结构。

**新增：** `src-tauri/tests/workflow_definitions.rs`、`src/lib/api/workflows.ts`；在 `tests/integration.rs` 中注册检查模块。

**接口：**

| 本地请求 | 内容 |
| --- | --- |
| `GET /api/v1/studio/workflows` | 名称、类型、分类与启停筛选 |
| `POST /api/v1/studio/workflows` | 名称、说明、分类、标签与完整草稿 |
| `GET/PATCH/DELETE /api/v1/studio/workflows/{id}` | 读取、按修订保存、引用检查后删除 |
| `GET/POST /api/v1/studio/workflows/{id}/versions` | 版本列表、创建不可变版本 |
| `GET /api/v1/studio/workflows/{id}/versions/{version_id}` | 取得指定版本 |
| `POST /api/v1/studio/workflows/{id}/default-version` | 设置默认版本 |

草稿结构与两类 `execution` 字段遵循设计第四节。PATCH 接收 `expected_revision`，版本创建接收版本号和当前草稿修订。

- [ ] 使用实际 ComfyUI JSON 与在线模型定义编写保存、重新读取、版本不可变、修订冲突和无效绑定检查。
- [ ] 执行 `cargo test --manifest-path src-tauri/Cargo.toml --test integration workflow_definitions::`，记录当前缺失接口的失败结果。
- [ ] 实现有类型的工作流定义、输入 Schema、输入输出绑定、草稿、版本、默认版本与启停检查。
- [ ] 将原字段类型、图参数位置、输出位置与 Rust 检查逐项核对，错误必须指向具体输入或绑定。
- [ ] 实现对应本地 API 与 TypeScript DTO，DTO 中的执行类型必须为可区分联合类型。
- [ ] 重新打开真实数据库，确认原版本内容摘要与定义保持一致。

核心断言使用完整保存对象：

```rust
assert_eq!(saved_version["content_digest"], reread_version["content_digest"]);
assert_eq!(reread_version["definition"]["type"], "comfyui");
assert!(invalid_input_result.is_err());
assert!(stale_revision_result.is_err());
```

**通过条件：** 两类定义都可管理与固定版本，输入检查在创建外部执行之前完成。

## 任务 4：Rust 执行记录、计算节点与文件接口

当前进度：`compute_nodes.rs`、`executions.rs`、`edge_api.rs` 已提供节点管理、presence、领取、任务包、输入下载、心跳、事件、结果流式上传与完成确认；重复请求、领取互斥、事件冲突、结果去重和重新打开读取已有真实 HTTP 检查。完整生命周期和其他中断阶段尚在实施。

**修改：** `workflow_execution.rs`、`assets.rs`、`file_transfer.rs`、`desktop_state.rs`、`commands.rs`、`local_api.rs`、`lib.rs`、`Cargo.toml` 与 SQL 结构。

**新增：** `src-tauri/src/compute_nodes.rs`、`edge_api.rs`、`execution_files.rs`；`tests/compute_nodes.rs`、`execution_recovery.rs`。

**接口：**

- `/api/v1/studio/compute-nodes` 及 `/{id}` 管理节点，`/{id}/credential` 生成或重置凭据。
- `POST /api/v1/studio/workflow-executions` 接收 `request_id`、`workflow_id`、`version_id` 或明确的草稿测试修订、`inputs` 与可选会话关系。
- `GET /api/v1/studio/workflow-executions` 支持按工作流与会话读取；`/{id}` 返回实际执行，`/{id}/cancel` 与 `/{id}/stop-waiting` 分别处理取消和会话等待。
- `/agent/v1` 的路径、字段、协议版本、序号与文件确认规则遵循设计第六节；JSON 参数使用 serde 校验。

- [ ] 启动实际 Axum Router 与 SQLite，编写节点凭据、领取互斥、重复请求、事件冲突、文件流、输出去重与取消竞争检查。
- [ ] 执行对应检查并记录接口缺失时的失败结果。
- [ ] 实现执行快照、节点指定、事务领取、单个 Agent 单执行、状态与连接健康的分别保存。
- [ ] 实现 10 秒心跳、35 秒连接异常判定与原尝试恢复；断线不产生新尝试。
- [ ] 实现实际输入下载、Range、摘要与长度检查、流式结果上传、输出确认和最终资产导入。
- [ ] 验证相同 `request_id` 与相同内容取得同一执行，内容变化返回冲突；相同输出上传多次只有一个资产版本。
- [ ] 验证服务监听失败返回明确错误，没有随机端口替代；远程监听按明确配置启用。

核心一致性断言：

```rust
assert_eq!(first_execution["id"], repeated_execution["id"]);
assert_eq!(claimed_count_for_execution, 1);
assert_eq!(asset_versions_for_output, 1);
assert_eq!(std::fs::read(&saved_output_path)?, uploaded_bytes);
```

**通过条件：** 实际 HTTP 请求完成领取、文件与状态操作；重复操作与进程重启保持同一执行和输出。

## 任务 5：独立 Go Edge Agent 与 ComfyUI

当前进度：独立 module 已建立，实际 SQLite 日志、跨进程目录互斥、提交阶段、原编号恢复、输入下载、ComfyUI 提交、历史查询与输出上传已接入。原附件在 `Linux-ComfyUI` 生成结果，并通过服务与 Agent 提交后重启检查；Agent 在 Linux 上部署和其余中断阶段尚待验证。

**源文件：** Pixoma 的 `apps/edge-agent/`、`internal/tasks/infrastructure/comfyui/`、`actuator/job.go`、`worker.go`、实际使用的硬件与指标类型。

**目标文件：** `apps/edge-agent/go.mod`、`cmd/edge-agent/main.go`、`internal/application/`、`internal/controlplane/`、`internal/comfyui/`、`internal/collect/`、`internal/journal/` 与对应检查文件。

**接口：** Go module 使用 `github.com/Mr9esx/PixomaStudio/apps/edge-agent`。配置保留 `CONTROL_PLANE_URL`、`EDGE_ID`、`AGENT_TOKEN`、`COMFYUI_BASE_URL`，增加 `EDGE_DATA_DIR`。结构化日志清除凭据。任务包直接采用设计第六节的 JSON 字段。

- [ ] 将 ComfyUI Client、采集与执行所需代码复制到独立 module，核对全部 import 与依赖许可证。
- [ ] 编写真实 SQLite 日志检查，覆盖目录锁、提交意图、编号保存、事件序号、结果确认和重新打开读取；执行检查并记录失败结果。
- [ ] 实现 Studio HTTP 文件传输与事件上报，移除平台 Blob、Topic Publisher 与原工程内部包依赖。
- [ ] 保存提交阶段与实际 `prompt_id`，恢复时查询原任务，响应未知时核对关联标识；无法唯一核对则停止再次提交。
- [ ] 使用实际 ComfyUI 验证输入上传、图参数替换、实际输出、进度和定向取消能力。
- [ ] 在结果上传前、部分输出确认后、最终确认前分别终止并重新启动 Agent，核对结果字节与执行次数。
- [ ] 在独立远程机器运行相同检查，确认 Studio 与 Agent 没有共享素材目录。
- [ ] 执行 `go test ./...`、`go test -race ./...`、`go vet ./...` 与 `go build ./cmd/edge-agent`；设置 `GOTMPDIR`、`TMPDIR` 和 `GOCACHE` 到 `.local-cache/` 下的相应目录。

恢复检查必须核对实际编号：

```go
if resumed.PromptID != beforeRestart.PromptID {
    t.Fatalf("恢复执行的 prompt_id 发生变化")
}
if !bytes.Equal(uploadedOutput, comfyOutput) {
    t.Fatalf("上传结果与 ComfyUI 输出不一致")
}
```

**通过条件：** 本机与远程实际执行均完成，Agent 独立构建，原执行恢复不重复提交。

## 任务 6：Seedream 与 MiniMax 生成服务

**新增：** `src-tauri/src/generation/mod.rs`、`seedream.rs`、`minimax.rs`、`src-tauri/tests/generation.rs`、`src/lib/api/generation.ts`。

**修改：** `workflow_execution.rs`、`local_api.rs`、`lib.rs`、`assets.rs`、`media.rs` 与连接修订结构。

**接口：** `/api/v1/studio/generation-connections` 及 `/{id}` 管理连接，`/{id}/test` 验证连接；`GET /api/v1/studio/generation-providers` 返回内置服务、操作与模型系列能力。执行入口统一使用 `workflow-executions`。

- [ ] 编写真实数据库检查，覆盖连接修订、凭据不进入 DTO、模型能力与参数校验；运行并记录失败结果。
- [ ] 实现 `provider` 与 `operation` 枚举分派，固定 Seedream 图片与 MiniMax V2 视频的请求和响应结构。
- [ ] 根据官方文档实现提示词、参考素材、尺寸、时长、比例、模型能力与组合校验；不支持的字段在提交前返回错误。
- [ ] Seedream 保存响应输出与逐项错误；MiniMax 保存 `task_id`、实际状态与用量并查询原任务。
- [ ] 实现连接修订绑定、限流等待、查询恢复、输出下载与资产导入；生成请求不自动重发。
- [ ] 普通取消只取消尚未提交的在线执行；已提交 MiniMax 提供停止等待，保持后续查询和结果保存。
- [ ] 使用明确配置的真实服务分别完成实际图片与视频生成，记录请求编号、任务编号、格式、尺寸、摘要与实际用量。
- [ ] 验证参数拒绝、连接错误、查询中断与下载重试；用实际进程中断验证已知编号的恢复。无法制造并核对的异常保持未验证状态。

实际输出检查：

```rust
assert_eq!(restored_execution["remote_task_id"], original_task_id);
assert!(actual_output_path.is_file());
assert_eq!(execution_output["asset_version_id"], actual_asset_version_id);
assert!(public_connection.get("credential").is_none());
```

**通过条件：** 两个生成服务的真实输出进入资产库，查询与保存重试不会创建新生成任务。

## 任务 7：原页面复制、设置与两类工作流

当前进度：原工作流编辑、JSON 导入、输入输出、介绍、媒体预览、版本列表与共享表格组件已加入 Studio，并连接本地工作流及执行 API。节点列表、表单和规格区域已加入设置；编辑状态由工作区保留。前端 11 项检查、类型检查与构建通过。原附件在真实桌面页面导入、绑定、选择节点、保存、运行并显示 1024 × 1024 图片通过。在线模型表单、节点部署和指标继续处理。

**源文件与目标：** 按设计第十一节清单复制工作流、字段、图预览、版本、节点详情、部署、指标及其实际 import 依赖。

**修改：** `src/features/cases/`、`src/features/edges/`、`src/features/studio/studio-sidebar.tsx`、`studio-workspace.tsx`、`studio-settings.tsx`、`studio-settings-layout.tsx`、工作流列表与详情路由、`src/lib/api/`。

**接口：** 使用任务 3、4、6 提供的本地 DTO 和能力数据。工作流类型使用 `comfyui | online`，节点引用为一个 `node_id`。

- [ ] 记录实际来源组件与依赖清单，连同样式、图标和 i18n 文案复制到 Studio；检查现有目标文件内容后合并修改。
- [ ] 工作流列表按执行类型筛选，分类独立筛选；创建时选择类型，详情配置按类型显示。
- [ ] 保留 ComfyUI 导入、图预览、字段生成、绑定、介绍、示例、版本与草稿测试，执行目标改为单个节点。
- [ ] 将节点管理嵌入设置，保留节点采集、硬件、指标与部署功能；移除 Topic、订阅和任务队列设置。
- [ ] 在现有表单组件中组合生成服务与模型参数，按后端能力显示字段、错误与取消操作。
- [ ] 草稿在进入节点或生成服务设置前保存到页面编辑状态，返回后完整恢复；取消编辑不修改已保存定义。
- [ ] 使用实际本地后端核验列表、详情、类型选择、版本、单节点、部署、指标、设置往返与错误反馈。
- [ ] 执行 `pnpm typecheck`、适用的行为检查与 `pnpm build`；通过 DOM 与可访问性树检查导航和页面结构。

**通过条件：** 页面功能完整，使用原组件与令牌，计算节点只出现在设置及工作流的相关配置入口。

## 任务 8：会话、专家与本地工作流工具

当前进度：`studio_list_workflows`、`studio_get_workflow`、`studio_run_workflow`、`studio_get_workflow_execution` 已注册为内部工具，直接使用本地定义和执行记录。输入卡片、答复幂等、固定版本和重新打开数据库检查通过。专家会话按保存的版本检查依赖，前端引用保留字符串版本 ID。实际模型调用和会话工作流恢复继续验证。

**修改：** `src-tauri/src/tools.rs`、`agent.rs`、`workflow_inputs.rs`、`workflow_execution.rs`、`expert_runtime.rs`、`expert_tasks.rs`、`context.rs`、`runs.rs`、会话轨迹与工作流输入组件。

**测试：** `tests/workflows.rs`、`agent.rs`、`experts.rs`、`execution_recovery.rs`。

**接口：** 注册内部工具 `studio_list_workflows`、`studio_get_workflow`、`studio_run_workflow`、`studio_get_workflow_execution`，界面展示对应工作流与动作。输入和结果通过执行 ID 连接工具调用、审批、专家步骤和资产。

- [ ] 使用实际 SQLite 编写固定版本、补充输入、重复答复、审批修改、执行等待和结果去重检查，执行并记录失败结果。
- [ ] 将 Agent 能力准备与工作流工具连接到本地定义和执行入口，按当前版本输入 Schema 收集与校验。
- [ ] 在提交前保存工具调用关系与请求编号；取得执行后保存等待状态，逐项导入输出并更新对应专家步骤。
- [ ] 会话停止等待与计算状态分别处理，继续聊天不修改已提交计算的真实状态。
- [ ] 用真实模型分别调用本机、远程、在线工作流；核对审批、输入、执行与输出资产。
- [ ] 重新启动 App 后恢复原输入卡片、审批、执行编号与结果，重复答复不会产生第二次计算。

**通过条件：** 对话与专家通过本地工作流完成真实创作过程，版本、审批与输出关系可核验。

## 任务 9：生命周期、打包与完整验收

**修改：** `src-tauri/src/lib.rs`、`desktop_state.rs`、`menu.rs`、`src-tauri/build.rs`、`tauri.conf.json`、实际 Agent 构建脚本、`README.md`。

**新增记录：** `docs/standalone-verification.md`；记录本计划全部任务的实际证据。

- [ ] 使用目标系统构建独立 Go Agent，安装包按目标架构包含 Agent、FFmpeg、许可证与必要资源。
- [ ] Rust 只启动解析后的随包 Agent 文件，设置独立数据目录并保存子进程句柄；退出停止本机 Agent，远程 Agent 保持独立生命周期。
- [ ] 真实验证关闭窗口、退出 App、重新打开、电脑休眠与网络恢复；未结束执行继续使用原 `prompt_id` 或 `task_id`。
- [ ] 核查源码与依赖，移除平台 URL、账号、目录和发布调用；正常运行的网络访问仅来自明确配置的模型、MCP、生成服务与节点。
- [ ] 在不能读取 Pixoma 工程目录的环境中执行依赖安装、前端构建、Rust 发布构建与 Go Agent 构建。
- [ ] 运行 `cargo fmt --manifest-path src-tauri/Cargo.toml --check`、`cargo clippy --manifest-path src-tauri/Cargo.toml --all-targets -- -D warnings`、`cargo test --manifest-path src-tauri/Cargo.toml`。
- [ ] 运行 Go 格式检查、`go test -race ./...`、`go vet ./...` 与构建，前端执行 `pnpm typecheck`、`pnpm test`、`pnpm build`。
- [ ] 执行 `pnpm tauri build --ci`，从实际安装包完成项目、会话、技能、专家、MCP、资产与三条工作流路径。
- [ ] 核对设计第十二节，每项记录真实服务版本、命令、结果摘要和通过范围；未执行或缺少结果证据的项目保持未完成。

**通过条件：** 安装包可独立构建与运行，全部要求获得相应证据，真实执行与中断恢复通过。

## 执行环境与记录规则

使用项目规定的 Node.js 22 及以上版本，pnpm 版本遵循 `package.json`。Go 版本不得低于所迁移代码的 `go.mod` 要求，Rust 使用实际锁文件对应工具链。

每项任务先运行有针对性的检查，修改后再次运行。通过后仅在新增修改或新证据要求下扩大检查范围。需要真实服务的用例明确标识，并单独记录实际执行结果。

本计划中的断言示例由对应检查函数取得实际结果后执行。输入字节、远程任务编号和输出文件均来自实际操作，不使用测试服务替身。所有检查目录、临时目录与构建缓存均位于明确指定的 `.local-cache/` 子目录。
