# Pixoma Studio 功能迁移调研

日期：2026-10-29

本文记录源码与官方接口核查，以及据此确定的实施方式。接口文档核查不表示真实生成已经通过；本次没有提交付费生成请求。

## 1. 已确定的工程入口

| 功能 | Studio 文件 | 实施内容 |
| --- | --- | --- |
| 应用初始化 | `src-tauri/src/lib.rs` | 直接打开本地 Store，启动本地运行和节点接口 |
| 业务 Commands | `src-tauri/src/commands.rs`、`desktop_state.rs` | 方法直接接收业务 ID；保留运行取消与订阅管理 |
| 本地 API | `src-tauri/src/local_api.rs` | 技能、专家、资产与设置继续使用现有本地功能；增加工作流与计算节点接口 |
| 数据结构 | `src-tauri/migrations/`、`src-tauri/src/store.rs` | 按单一应用空间建立新数据库结构 |
| 工作流 | `workflows.rs`、`workflow_inputs.rs`、`workflow_execution.rs` | 本地定义、版本、校验、执行、输出导入与会话等待 |
| 资产 | `assets.rs`、`file_transfer.rs`、`media.rs`、`library.rs` | 统一保存本机、远程节点和在线服务的实际结果 |
| Agent | `agent.rs`、`tools.rs`、`expert_runtime.rs` | 注册本地工作流工具，保留技能与专家版本 |
| 技能与 MCP | `skills.rs`、`mcp.rs`、`mcp_execution.rs`、`mcp_inputs.rs` | 本地管理、工具发现、审批与既有继续执行能力 |
| 构建 | `src-tauri/build.rs`、`tauri.conf.json` | 打包 FFmpeg 与本机 Go Agent，构建不要求平台地址 |

对话、资产、技能、专家和 MCP 已有可复用的 Rust 业务。工作流的本地编辑与执行服务需要接入同一 Store，避免另外建立一套会话或资产实现。

## 2. Pixoma 可迁移的功能代码

| 来源 | 需要保留的能力 | Studio 中的负责位置 |
| --- | --- | --- |
| `web/admin/src/features/cases/` | JSON 导入、图预览、字段生成与绑定、介绍、示例、版本 | 原页面组件复制到 `src/features/cases/` |
| `web/admin/src/features/edges/` | 列表、详情、表单、部署、指标、状态 | 原组件复制到 `src/features/edges/`，嵌入设置 |
| `internal/cases/domain/document.go` | 输入输出类型、JSON Schema、ComfyUI 参数与输出位置 | Rust 工作流定义与校验 |
| `internal/tasks/infrastructure/actuator/snapshot.go`、`job.go` | 固定版本、输入替换与任务包 | Rust 创建任务包，Go 消费已校验任务包 |
| `internal/tasks/infrastructure/comfyui/` | 上传、提交、等待、历史、结果字节、系统状态 | 独立 Go Agent 的 `internal/comfyui/` |
| `apps/edge-agent/internal/collect/` | 硬件、运行指标与采样 | 独立 Go Agent 中保留实现 |
| `apps/edge-agent/internal/controlplane/` | 主动连接、心跳、状态与任务领取 | 按 Studio 节点协议调整 |

现有 Agent 从平台 Blob 读取任务包与素材，并向 Topic Publisher 发送结果。Studio 使用按执行授权的 HTTP 文件接口和状态接口，任务包与输出都由本地应用保存。Agent 独立 module 只包含实际使用的执行、采集、协议和日志代码。

## 3. ComfyUI 官方接口核查

官方路由包含 `/prompt`、`/ws`、`/object_info`、`/upload/image`、`/history/{prompt_id}`、`/view` 与 `/queue`。进度通过 WebSocket 提供，最终结果通过原任务历史读取。[ComfyUI 官方路由文档](https://github.com/Comfy-Org/docs/blob/main/development/comfyui-server/comms_routes.mdx)

官方当前实现允许请求附带 `extra_data` 与 `client_id`，并提供按 `prompt_id` 删除待执行任务和定向中断逻辑。实际支持范围须针对联调版本验证。[ComfyUI 官方 server.py](https://github.com/Comfy-Org/ComfyUI/blob/master/server.py)

据此确定：

- Agent 在提交前保存 Studio 执行 ID 与内容摘要，随请求保存关联标识。
- 编号已知时只查询原编号；响应未知时通过队列与历史核对关联信息，不能把再次提交当作恢复。
- 输入字段以选定节点的实际 `object_info` 校验；缺少自定义节点、模型或字段时返回具体问题。
- 运行中取消要求实际 ComfyUI 版本支持定向中断。Studio 不发送全局清空或全局中断请求。
- ComfyUI 历史被外部清理且 Agent 没有完成日志时，保存明确的结果未知信息；不能根据空历史判定从未提交。

真实验证使用一份可执行的 ComfyUI 工作流、实际输入媒体和明确的输出位置，记录 ComfyUI 版本、`prompt_id`、输入输出 SHA-256，以及 Agent 重新启动后的原任务查询结果。

## 4. Seedream 图片生成

官方图片生成接口为 `POST https://ark.cn-beijing.volces.com/api/v3/images/generations`，使用 API Key。模型参数支持 Model ID 或 Endpoint ID；本地参考图可按接口要求编码后提交。非流式结果含图片数据或下载地址；部分模型支持流式输出和组图，能力需要按模型系列校验。图片 URL 有限时效。[火山方舟图片生成 API](https://docs.volcengine.com/docs/ark/image-generation-api?lang=zh)

Studio 的 `seedream_images` 操作负责提示词、参考图、输出尺寸、格式和服务支持的生成选项。使用真实 Model ID 或 Endpoint ID，另保存所选模型系列。没有声明的能力不显示为可选项。

Rust 在消费响应时保存已取得的输出，结束后记录逐项错误与实际用量。得到部分输出后中断，运行保留成功项并标记其余结果未确认。没有外部任务编号的中断请求不自动重新生成。

图片下载失败时保留原响应与输出位置，允许重试保存已有结果。App 长期关闭造成下载地址失效时，显示结果保存失败；恢复操作不创建新的生成请求。

真实验证覆盖文字生成、实际本地参考图片、支持的多图输出、参数错误、结果保存和响应中断。校验生成图片的实际格式、尺寸、可读取内容与资产引用。

## 5. MiniMax 视频生成

### 创建与查询

国内官方 V2 接口使用 `https://api.minimax.cn`。创建接口为 `POST /v2/video_generation`，接收模型、`content`、分辨率、时长和画面比例，返回 `task_id`。媒体内容具有不同角色与组合规则。[MiniMax 创建视频生成任务](https://platform.minimaxi.com/docs/api-reference/video-generation-v2-create.md)

查询接口为 `GET /v2/query/video_generation/{task_id}`，通过 `task.status` 读取状态，成功后通过 `task.content.url` 取得结果。官方当前说明任务查询覆盖最近七天，下载地址需要及时保存。[MiniMax 查询任务](https://platform.minimaxi.com/docs/api-reference/video-generation-v2-query.md)

Studio 的 `minimax_video` 操作保存 API 版本和实际模型。输入表单区分提示词、首帧、尾帧、参考图、参考视频与参考音频，并校验当前模型的组合要求；分辨率与时长使用对应模型能力。Rust 将字段转换成官方 `content` 对象，不通过对话协议发送视频生成请求。

采用主动查询，默认间隔十秒，网络错误后逐步延长到六十秒；收到服务限流时遵循其重试时间。取得任务编号后立即保存，App 重新启动继续查询原编号，成功后立即下载结果。已经超出可查询时间范围的任务显示查询过期，保留本地记录与已取得输出。

### 取消语义

官方 `DELETE /v2/video_generation/{task_id}` 对排队任务执行取消，对成功或失败任务执行删除；运行中的任务不可取消。[MiniMax 取消或删除任务](https://platform.minimaxi.com/docs/api-reference/video-generation-v2-delete.md)

根据该接口语义，查询后调用 DELETE 存在任务已经结束的时间窗口，普通取消可能删除服务端记录。Studio 对尚未提交的执行提供取消；对已提交的 MiniMax 执行提供停止等待并继续保存真实结果，不把组合 DELETE 接口接到普通取消按钮。

真实验证覆盖文字生成、真实图片输入、参数组合、任务查询、实际 MP4、封面、用量与重新启动恢复。请求超时但未取得编号时保存 `submission_unknown`，不自动重发。

## 6. 节点服务与持久执行

Axum 官方实现支持为 Router 提供应用状态、使用 Tokio Listener 运行服务，以及通过 `with_graceful_shutdown` 结束监听。[Axum 官方生命周期示例](https://github.com/tokio-rs/axum/blob/main/examples/graceful-shutdown/src/main.rs)

据此采用 Tauri 进程中的独立节点 HTTP Router，共用 Store。React 操作继续走 Tauri Commands，节点凭据只授权 `/agent/v1` 的执行与文件范围。应用退出保存执行阶段，Agent 使用 SQLite 保留原任务与待确认输出。

节点服务默认本机监听；远程部署需要 Agent 实际可达的 Studio 地址。运行验证必须包含不同机器间的素材传输与结果回传，不能用共享目录或同机进程替代远程验证。

## 7. 实施验证所需环境

| 环境 | 必须核验的内容 |
| --- | --- |
| 全新应用目录 | SQLite、文件、技能、专家、MCP、项目与会话直接可用 |
| 本机 ComfyUI | 已安装的节点、模型、实际工作流与可核对输入输出 |
| 独立远程机器 | Go Agent、可达 Studio 地址、独立文件目录与网络中断检查 |
| Seedream 配置 | 实际 API Key、可用 Model ID 或 Endpoint ID、模型系列 |
| MiniMax 配置 | 视频服务 API Key、可用模型与对应请求参数 |
| 对话与 MCP | 现有真实对话模型与独立 MCP 服务，验证本地工作流工具与审批 |

验证读取明确配置的凭据，只记录凭据是否存在。文档与源码检查可以在没有服务凭据时完成；真实服务用例只有执行并核对结果后才能记为通过。
