# 开发验证记录

日期：2026-10-22。

工程目录：`/Users/mr9esx/Documents/PixomaStudio`。

## 已验证的行为

- 项目、会话、资产版本、分类、标签、模型与 MCP 配置保存在独立 SQLite 数据库。
- 账号之间的数据访问相互隔离，项目删除后保留会话和资产；重复的未归属资产条目合并标签。
- 资产库支持分类层级、标签、评分、归档、格式、日期和重复内容筛选，以及六种分组方式。
- 资产复制保留来源记录，各个项目独立选择版本；会话引用的资产加入会话所属项目。
- 文档编辑保留历史版本；并发重复请求只创建一份资产或一个新版本。
- 启动清理仅处理资产管理目录中的未引用内容文件和未完成文件；文件写入与清理共用 SQLite 写入事务，实际检查覆盖写入中断、历史版本、普通文件保留和并发保存。
- 视频使用随包 FFmpeg 生成 PNG 封面、保存尺寸和调色盘。实际 MP4 检查验证画面像素、原始字节、GET、HEAD、账号范围、重新分析、无效内容拒绝，以及重新打开数据库后的封面读取。
- 视频原始文件与封面在同一事务保存，启动清理同时保留封面引用；应用启动取得数据目录的系统文件锁后清理分析临时文件。真实子进程验证互斥及关闭后的释放，临时目录检查验证普通文件和符号链接目标保留。
- FFmpeg 9.0.2 官方源码签名及固定 SHA-256 检查通过，程序仅链接系统组件，安装包配置保存程序、许可证、源码和构建参数。程序的 macOS 最低系统版本为 11.0，已经通过 Mach-O 元数据检查。
- 文件上传保留文件修改日期，导出使用选定版本，ZIP 文件通过实际解压读取验证。
- 本地媒体协议检查账号和资产版本，支持 GET、HEAD 和 Range。
- 流程图保存节点、连线、顺序和位置；批量位置更新使用事务，删除节点同时删除相关连线。
- API Key 使用 macOS 系统凭据存储，返回数据和 SQLite 配置不包含密钥。测试已执行真实凭据写入、读取和删除。
- 模型保存、运行配置保存和会话清理共用并发保护；真实凭据检查覆盖并发密钥更新、保留密钥的配置更新，以及排队运行使用的密钥。清空会话同时清理普通运行和官方专家会话保留的旧凭据，当前模型凭据继续保留。
- 排队运行、普通模型请求和上下文压缩请求检查当前模型的启停与 Agent 权限；停用后不建立新的请求记录。
- Rig 的 Responses、Chat Completions 和 Anthropic Messages 均通过真实模型的连续对话、工具创建、审批恢复、指定版本资料读取和上下文压缩检查。
- rmcp 承担 MCP 协议协商、初始化与工具发现；2026 协议通过逐次请求的 metadata 传递客户端能力，2025 协议通过 initialize 建立会话。关闭会话失效后的请求重发。
- Agent 可以通过实际工具列出会话资产、读取文本、创建文档和保存文档新版本；参数使用 JSON Schema 校验。
- 工具定义、调用、参数、审批和结果保存到 SQLite；工具结果与对应 AG-UI 事件使用同一个事务。
- 真实模型生成的文档调用在实际文件创建后、结果保存前中断，重新打开数据库后保存为 `tool_result_unknown`；自动恢复与直接重复调用均不再次执行，实际文件、资产和版本保持一份。未知工具名称被拒绝。
- 真实模型验证审批等待、批准、拒绝、参数修改、数据库重新打开和重复答复；审批通过之前不创建资产，重复答复不重复执行工具。
- 会话记录保留工具调用与结果标识；原生窗口通过 AG-UI SDK 完成审批暂停与恢复。
- 所选文本资产绑定指定版本，资料保存在 Rig 的消息历史中；真实模型验证其他会话的旧版本资料，以及连续对话中的资料读取。
- 资产引用、消息和运行在同一个事务保存；跨账号引用在创建运行之前被拒绝。
- 上下文接口读取实际模型请求、资料、服务用量与时间记录；运行轨迹提供模型、消息、思考和工具调用的记录与详情。
- 文本用量使用 `cl100k_base` 估算；图片按实际尺寸和模型的图片规则或公开上限独立计算。未识别模型使用原始尺寸的 32 × 32 patch 与 2.46 倍数估算，每份图片资料保留估算方法、尺寸、格式和摘要。服务返回的实际用量单独记录，缺失用量保留为空值。
- 三种协议均通过流式与非流式真实双色 PNG 的上下颜色识别、输入预算、实际服务用量、指定图片版本和重新打开数据库后的检查点读取；请求保留图片原始字节，上下文展示使用图片元数据。
- 三种协议均通过启用图片能力时的流式与非流式思考内容检查，思考事件、正文、检查点与重新打开数据库后的内容保持一致。
- MCP 标准图片内容保留为 Rig 图片内容块；实际 PNG 检查确认完整图片字节和文本结果均保留。图片格式转换失败时，在同一事务保存完整原始响应、明确错误、消息、事件和结束状态；实际 SQLite 检查验证重新启动后的内容保持。
- 较长资料通过真实模型分段生成摘要，缓存保存在对应账号与会话中；重新打开数据库后复用摘要，并保留原始资产文件。真实检查验证审核编号、备用编号，以及各次请求的估算和实际输入用量。
- 当前用户消息与固定请求内容超出输入预算时，在发送模型请求之前结束运行，完整保存用户原文；三种协议均通过此项检查。
- 真实模型通过 rmcp 调用平台 `list_workflows`，返回实际官方工作流目录；连接器的审批策略在 `full_access` 模式下继续生效。
- 个人技能支持本地编辑、ZIP 导入、历史版本和启停；实际 ZIP 与 SQLite 检查覆盖目录路径、文件冲突、大小限制和元数据内容。
- `load_skill` 读取本次运行保存的技能文件，并提供 Unicode 字符位置和目录分页；真实模型已验证指定文件读取，以及修改、停用和重新打开数据库后的版本保持。
- 官方技能保存在独立数据库中，记录来源地址、版本和内容摘要；真实平台验证重复获取与只读保护。
- 个人专家保存内容、SOP 和历史版本；会话绑定指定专家版本与本地模型，真实模型验证版本读取、数据库重新打开、连续消息及版本更换检查。
- 官方专家会话保存创建时本地默认模型的完整配置和凭据引用。真实平台与模型验证修改原模型的地址、名称、限制和密钥之后，原会话继续使用原配置，新会话使用当前默认配置；清空会话清理不再引用的旧密钥。
- 专家任务与步骤保存实际处理记录；输入和结果卡片支持开始、暂缓、跳过、确认与修改。并发相同答复只创建一个运行；错误账号、缺失必填输入和过期卡片被拒绝。
- 专家卡片的真实模型检查覆盖文档创建、版本修改、结果资产引用，以及重新打开数据库后的卡片答复。文档新版本和会话使用记录在同一个事务保存。
- 专家会话中的技能列表显示绑定版本；编辑器可以引用个人技能。头像通过二进制 IPC 上传，校验实际图片内容和 5 MiB 限制；本地头像按专家版本读取，平台头像经已认证的 Rust Client 读取。
- 官方工作流目录保存版本、来源、输入要求与内容摘要；重复获取复用原版本，同一版本的内容变化被拒绝。真实平台验证重新打开数据库、账号隔离、专家绑定版本的能力列表及上下文注入。
- 保存个人专家前获取所引用的官方技能和工作流版本；个人专家的列表与详情共用依赖检查。实际 SQLite 与真实 MCP 检查覆盖技能停用、模型工具能力变化、模型停用、MCP 工具尚未就绪和连接停用。
- 已有专家会话按绑定版本检查依赖；更新后的专家配置不可用时，原会话可以继续使用其可用的绑定版本。
- 真实平台检查包含头像上传、专家发布、读取完整图片和取消发布；无登录状态的读取被拒绝。原生窗口验证头像字节、专家引用个人技能，以及切换账号后的访问隔离。
- 消息与运行请求支持重复请求检查，保存 Rig 0.44.0 的真实检查点和模型配置快照。
- 运行事件保存连续序号，重新打开数据库后可以按序号继续读取。
- 同一会话只允许一个运行进入执行状态；App 启动时记录执行中断，保留排队记录。
- 账号认证和运行订阅恢复排队记录；重新打开实际数据库后，排队运行继续执行并保存实际网络请求结果。
- 账号变化关闭旧订阅并等待原账号的运行结束；前端清除旧账号业务缓存，重新创建页面状态。同一账号的资料更新保留当前业务缓存。
- 清空会话要求确认文字，停止当前账号的运行；保留项目、资产文件、版本、项目资产和使用记录，已经清空的会话标记为不可打开。
- 桌面进程已启动；再次启动 App 会激活已有窗口，第二个进程正常退出。
- 使用独立测试平台完成注册、登录、身份读取、资料修改、密码修改、退出和系统凭据中的登录状态恢复。
- 使用真实模型完成流式回复和连续对话；第二次运行读取前一次运行的持久检查点，返回相同的对话编号。
- 模型地址支持基础地址与完整操作路径，保留代理服务的路径前缀。
- 关闭思考时通过对应协议明确传递关闭设置。DeepSeek Chat 保留 `thinking` 与 `reasoning_content`，启用图片能力时使用 Rig 兼容协议扩展；Agent 与压缩请求分别保留其思考设置。原生窗口确认关闭思考时实际回复不包含思考内容。
- Tauri Channel 的事件读取与 SQLite 中保存的事件顺序一致；网络请求失败和取消均保存结束状态与结束事件。
- 消息接口创建本地会话和运行，重复请求返回原运行；请求内容变化时拒绝复用请求标识。
- 会话、消息和运行使用同一事务保存；无效模型配置不创建空会话。同一会话按提交顺序领取运行。
- 原生窗口通过真实 Commands 和 AG-UI Adapter 验证流式与非流式回复、持续对话、事件续接、取消订阅、取消运行和退出。
- Channel 验证包含超过 8 KiB 的实际文档读取工具结果，核对完整内容、保存记录与接收顺序，订阅等待全部消息送达后结束。
- 达到模型输出限制时保留生成内容与实际用量，运行保存错误状态。
- 后续历史读取包含失败、取消和输出受限运行的用户消息、已保存正文与实际工具结果；工具结果未知保留明确说明。实际文件与数据库恢复检查确认读取历史不会再次执行工具。
- 会话页面从实际保存的流式事件恢复失败和取消运行的未完成正文。重新打开数据库后保留已收到的完整正文，后续完整回复和已保存的正文均只展示一次；账号范围继续受原会话检查保护。
- 原生窗口通过本地媒体协议读取文档，Range 请求返回实际选定的字节。
- 真实平台 MCP 初始化、官方工具发现与连接关闭通过。
- 真实 MCP 拒绝错误凭据；查询不存在的任务返回服务错误，随后可以继续读取工作流目录。
- 平台地址通过 `desktop.json` 保存；实际原生窗口连续两次读取真实平台注册设置，第二次启动没有 `PIXOMA_PLATFORM_URL`。构建地址初始化、启动地址覆盖、无效配置和地址校验检查通过。
- 已引用工作流缺少必填输入时，输入卡片、调用绑定与等待状态在同一个事务保存，远程调用尚未提交。真实模型和 MCP 检查覆盖重新打开数据库、空输入拒绝、账号范围、并发重复跳过，以及结束后的卡片状态。填写输入后将 MCP 策略改为审批，审批展示与批准后的调用参数继续保留已填内容。
- 原生窗口通过 AG-UI 验证工作流连接器审批、输入等待、跳过、运行结束和输入卡片的会话记录。
- Pixoma 平台菜单和快捷搜索已移除 Studio 产品入口，Studio 运营入口继续使用原路径。

## 执行的检查

| 检查 | 当前结果 |
| --- | --- |
| `pnpm typecheck` | 通过 |
| `pnpm test` | 5 项通过 |
| `pnpm build` | 通过，包含构建文件大小提示 |
| `cargo check --manifest-path src-tauri/Cargo.toml --all-targets` | 通过 |
| `cargo test --manifest-path src-tauri/Cargo.toml` | 55 项通过，24 项真实服务检查单独执行 |
| `cargo test --manifest-path src-tauri/Cargo.toml --test integration platform:: -- --ignored` | 真实账号流程与官方技能保存通过 |
| `cargo test --manifest-path src-tauri/Cargo.toml --test integration agent::real_model_streams_text -- --ignored` | 三种协议均通过 |
| `cargo test --manifest-path src-tauri/Cargo.toml --test integration agent::real_model_creates_a_document -- --ignored` | 三种协议均通过 |
| `cargo test --manifest-path src-tauri/Cargo.toml --test integration agent::real_model_reads_the_selected_asset -- --ignored` | 三种协议均通过 |
| `cargo test --manifest-path src-tauri/Cargo.toml --test integration agent::real_model_reads_image_pixels -- --ignored` | 三种协议均通过流式与非流式真实 PNG 颜色识别、图片预算、服务用量及重新打开数据库后的引用检查 |
| `cargo test --manifest-path src-tauri/Cargo.toml --test integration agent::real_model_reasoning -- --ignored` | 三种协议均通过启用图片能力时的流式、非流式思考事件和检查点保存 |
| `cargo test --manifest-path src-tauri/Cargo.toml --lib models::tests::vision_models` | 经过 Rig 历史适配后，真实 PNG 字节与启停思考字段保留 |
| `cargo test --manifest-path src-tauri/Cargo.toml --lib mcp::tests::image_results` | 标准 MCP 图片转换保留实际 PNG 数据，文本 JSON 不包含图片编码 |
| `cargo test --manifest-path src-tauri/Cargo.toml --lib tools::tests::mcp_image_conversion` | 图片格式转换错误时保存完整响应、错误事件和明确结束状态；重新打开数据库检查通过 |
| `cargo test --manifest-path src-tauri/Cargo.toml --test integration agent::real_model_continues -- --ignored` | 三种协议均通过输出受限后的请求状态、原用户消息、已生成正文与连续对话检查 |
| `cargo test --manifest-path src-tauri/Cargo.toml --test integration agent::real_cancelled_text -- --ignored` | 真实模型开始输出后取消；SQLite 重新打开后的会话正文与全部实际文字事件逐字一致。后续完整真实回复和取消正文分别展示一次，错误账号读取被拒绝 |
| `cargo test --manifest-path src-tauri/Cargo.toml --test integration compaction:: -- --ignored` | 三种协议均通过启用图片能力时的资料摘要、缓存恢复、用量与超长当前消息检查 |
| `cargo test --manifest-path src-tauri/Cargo.toml real_connector -- --ignored` | 真实 MCP 发现、错误凭据、任务查询错误及后续调用检查通过 |
| `cargo test --manifest-path src-tauri/Cargo.toml --lib mcp_execution::tests::saved_task_handles` | 实际 SQLite 重新打开后保留任务编号、原始响应及 metadata，输入校验、逐题答复和取消状态检查通过 |
| `cargo test --manifest-path src-tauri/Cargo.toml --lib mcp_execution::tests::real_sdk_task -- --ignored` | 官方 SDK TaskDemo 实际计算 `17 + 23`，本地等待中断、数据库重新打开后继续查询原任务编号并取得 `40` |
| `cargo test --manifest-path src-tauri/Cargo.toml --lib mcp_execution::tests::real_sdk_mrtr -- --ignored` | 官方 TypeScript SDK todos 服务实际创建和完成待办；取消、拒绝和选择否保留记录，确认后删除记录。等待输入、保存答复和准备继续的三个阶段重新打开数据库，原工具继续并返回实际业务结果 |
| `cargo test --manifest-path src-tauri/Cargo.toml --lib mcp_inputs::tests::form_answers` | 实际 SQLite 检查覆盖 MRTR 原始状态、字段 Schema 校验、账号范围、并发重复答复、恢复参数及实时输入中断后的未知结果状态 |
| `cargo test --manifest-path src-tauri/Cargo.toml --lib mcp::tests::cancelled_elicitation` | 实际 SQLite 与 CancellationToken 检查覆盖当前输入取消、排队输入取消、门禁隔离、关闭问题及继续事件。服务撤销输入后的完整工具响应保存由 `form_answers` 检查覆盖 |
| `cargo test --manifest-path src-tauri/Cargo.toml --lib mcp_inputs::tests::real_sdk_enum_form -- --ignored` | 连接官方 rmcp 1.5.0 enum form 服务，实际等待 301 秒后连续提交四项输入，服务返回更新后的选项；Channel 中断事件和数据库重新打开检查通过。App 的 rmcp 保持 3.5.1 |
| `cargo test --manifest-path src-tauri/Cargo.toml --lib tools::tests::real_tool_result -- --ignored` | 真实模型、实际文件与数据库中断恢复检查通过 |
| `cargo test --manifest-path src-tauri/Cargo.toml --test integration skills::` | 2 项实际 SQLite 与 ZIP 检查通过 |
| `cargo test --manifest-path src-tauri/Cargo.toml --test integration skills::real_model -- --ignored` | Chat 协议真实文件读取、版本绑定与恢复检查通过 |
| `cargo test --manifest-path src-tauri/Cargo.toml --test integration experts:: -- --ignored` | 真实专家版本、模型绑定、步骤卡片和文档版本检查通过 |
| `cargo test --manifest-path src-tauri/Cargo.toml --test integration platform::official_experts -- --ignored` | 真实官方专家获取、完整模型配置保存、连续会话和旧凭据清理通过 |
| `cargo test --manifest-path src-tauri/Cargo.toml --lib platform::tests::real_published -- --ignored` | 真实平台头像上传、发布和鉴权读取通过 |
| `cargo test --manifest-path src-tauri/Cargo.toml --lib platform::publications_tests:: -- --ignored` | 真实个人技能和专家发布、撤销发布、发布历史、并发重复请求、版本递增、历史版本发布、数据库重新打开、模型与技能标识转换、头像原始字节通过。实际 SQLite 触发器验证平台创建成功后的本地关联保存中断，以及平台撤销发布成功后的本地结果保存中断；恢复没有重复创建对象或发布记录。纯文本首尾空白、完整技能包、二进制文件和专家空名称引用检查通过，本地版本保留原文 |
| `cargo test --manifest-path src-tauri/Cargo.toml --lib platform::tests::real_workflow_catalog -- --ignored` | 真实目录版本、专家引用准备、会话上下文、账号隔离及 MCP 依赖状态通过 |
| `cargo test --manifest-path src-tauri/Cargo.toml --test integration workflows::real_workflow_missing_inputs -- --ignored` | 真实输入等待、重新打开数据库、校验、重复跳过和审批策略变化后的参数保留通过 |
| `cargo test --manifest-path src-tauri/Cargo.toml --test integration workflows::real_workflow_executes -- --ignored` | 真实图片字节、MCP 提交、连续重启后的原任务恢复、两个结果的像素、资产、流程图、取消后继续对话、并发刷新、专家结果确认及计算失败状态通过。真实任务提交后以 SQLite 触发器阻止任务编号保存，结果未知、任务候选、账号范围、重复核对及拒绝重新执行通过 |
| `cargo test --manifest-path src-tauri/Cargo.toml --lib workflow_execution::tests::workflow_outputs` | 实际 JSON、文本与二进制文件按 MIME 和字节类型保存，原始内容保持一致；无效 MIME 被拒绝 |
| `cargo test --manifest-path src-tauri/Cargo.toml --test integration assets::image_palettes` | 实际双色图片的 75%/25% 比例、纯白图片、重新分析、账号范围、原始字节与数据库重新打开检查通过 |
| `cargo test --manifest-path src-tauri/Cargo.toml --test integration assets::reopening_cleans` | 写入中断后的内容文件、未完成文件、普通文件保留、所有历史版本及启动清理与保存并发检查通过 |
| `cargo test --manifest-path src-tauri/Cargo.toml --test integration assets::video_posters` | 实际 MP4 的封面像素、尺寸、调色盘、GET、HEAD、账号限制、重新分析、无效视频与数据库重新打开检查通过 |
| `cargo test --manifest-path src-tauri/Cargo.toml --lib store::tests::application_directory_lock` | 真实子进程拒绝重复取得应用目录锁，文件关闭后可以重新取得锁 |
| `cargo test --manifest-path src-tauri/Cargo.toml --lib asset_analysis::tests::startup_cleans` | 清理实际分析临时文件，普通目录内容与符号链接目标保留 |
| `cargo test --manifest-path src-tauri/Cargo.toml --lib expert_runtime::tests::skipped_expert` | 真实 SQLite 步骤状态验证开始前、开始后和跳过后的写入权限 |
| `cargo test --manifest-path src-tauri/Cargo.toml --test integration agent::` | 请求错误、取消、Channel 事件一致性通过 |
| `cargo test --manifest-path src-tauri/Cargo.toml --test integration runs::` | 消息重复请求、事务、检查点、事件序号、排队顺序和重新启动通过 |
| `cargo clippy --manifest-path src-tauri/Cargo.toml --all-targets -- -D warnings` | 通过 |
| `cargo fmt --manifest-path src-tauri/Cargo.toml -- --check` | 全部 Rust 文件通过 |
| `pnpm tauri dev --no-watch` | 已实际启动并验证单进程运行 |
| `cargo run --manifest-path src-tauri/Cargo.toml --example native_journey` | 真实原生窗口 IPC、模型、Channel、AG-UI、文档资源、工作流输入等待与跳过、账号切换、清空会话与退出检查通过。原有前端 API 的个人技能和专家发布、重复请求与撤销发布通过；真实资料压缩与摘要复用、收到正文后取消，以及取消和输出限制后的连续对话通过 |
| `cargo run --manifest-path src-tauri/Cargo.toml --example native_recovery` | 顺序启动创建和恢复两个 App 阶段。恢复阶段移除平台地址、账号密码和模型配置环境变量，读取保存的登录、项目、原始资料和资料摘要，继续原审批运行。未批准时没有文档，批准后只有一份，重复批准不重复执行；最后退出登录。父进程同时核对本次启动编号、阶段和成功结果；实际占用应用目录锁后启动的子进程不能形成成功结果，其他启动编号不能使用已有结果 |
| `cargo run --manifest-path src-tauri/Cargo.toml --example native_platform_config` | 两次原生启动均读取真实平台，第二次启动只使用保存的配置 |
| `CI=true PIXOMA_PLATFORM_URL=http://127.0.0.1:18082 pnpm tauri build --ci` | 完整构建通过，生成 77.50 MiB 的 `.app` 和 37.46 MiB 的 `.dmg`，使用 ad-hoc 签名 |
| `codesign --verify --deep --strict` | 最新应用、随包 FFmpeg 及 DMG 镜像内的应用均通过签名完整性检查 |
| `hdiutil verify` | 最新 DMG 的全部校验和通过 |
| 原工程目录访问限制 | 使用 `sandbox-exec` 禁止读取 `/Users/mr9esx/Documents/Pixoma`，读取原工程 `AGENTS.md` 的校准检查返回 `Operation not permitted`。在相同限制下，`pnpm install --frozen-lockfile`、FFmpeg 准备和 `CI=true pnpm tauri build --ci` 全部通过，构建包含前端、类型、Rust、应用签名与 DMG 生成。检查使用本机已有依赖缓存 |
| 源码符号链接 | `src/`、`public/`、`scripts/`、Rust 源码、测试、示例和数据库迁移目录均没有符号链接 |
| 安装包实际页面 | 从 DMG 复制应用并指定独立数据目录，没有平台地址环境变量时读取并保存构建地址；登录、项目、文档、图片和视频导入、图片预览、视频播放、单项及 ZIP 导出通过。`bundle-final.CFap2e` 副本的登录恢复、模型选择、连续对话、工具批准与拒绝、正文生成后取消、取消后继续、第二版本文档引用和实际用量读取通过 |
| 平台 `pnpm test src/config/menu.test.ts` | 5 项通过 |
| 平台 `pnpm exec vitest run --config vite.config.ts src/context/search-provider.test.tsx --browser.headless` | 7 项通过，关闭失败截图 |

开发命令使用 Node.js 22：

```sh
PATH="/opt/homebrew/opt/node@22/bin:$PATH" pnpm tauri dev --no-watch
```

## 真实联调环境

平台地址为 `http://127.0.0.1:18082`，使用专用测试数据目录与测试账号。模型使用本机已有的 DeepSeek V4.1 Flash 配置，通过 Rig 的 Chat Completions、Responses 和 Anthropic Messages 协议发送真实请求。凭据仅在本机进程内读取，并由 App 保存到系统凭据存储；文档与普通数据库配置不包含凭据。

真实服务检查明确标记为 `ignored`，通过环境变量 `PIXOMA_TEST_PLATFORM_URL`、`PIXOMA_TEST_MODEL_CONFIG`、`PIXOMA_TEST_MCP_URL` 和 `PIXOMA_TEST_MCP_TOKEN` 单独执行。原生窗口联调使用同一套 App Builder、Commands、前端 API 和 AG-UI Adapter；测试账号由测试平台的管理接口创建，使用 `PIXOMA_TEST_STUDIO_USERNAME` 和 `PIXOMA_TEST_STUDIO_PASSWORD` 登录并修改初始密码。

原生重新启动检查使用 `PIXOMA_STUDIO_DATA_DIR` 指定独立测试目录，保存不含密钥的检查记录。创建阶段保存实际资料摘要并在工具审批时退出；恢复阶段读取同一目录，通过系统凭据存储恢复账号和模型配置。父进程只接受带有本次启动编号和阶段名称的成功结果，同时验证目录锁占用与已有结果隔离。检查结束后，辅助命令删除专用账号与本次模型、MCP 凭据。

官方专家和工作流目录检查使用管理接口创建的专用账号，并在检查结束后删除账号和本次 MCP 凭据。平台注册流程使用独立检查覆盖。

真实计算使用官方 ComfyUI 0.41.3（Git commit `8713616fcc321624e306bbd36be5906203dcb0dc`）和真实 Edge Agent。测试工作流执行 `LoadImage`、`ImageInvert` 与两个 `SaveImage`，通过 MCP 提交和读取结果。输入图片为 12 × 9，检查原图像素 `[20,60,100]` 与处理后像素 `[235,195,155]`，同时核对保存的资产版本、输入连线和输出记录。计算节点、路由、工作流、账号和 MCP 凭据均使用专用检查对象。

## 独立构建检查

在新工程目录执行以下检查。系统限制只作用于该命令及其子进程，禁止读取 Pixoma 原工程目录，保留新工程和系统依赖的正常访问。

```sh
sandbox-exec -p '(version 1) (allow default) (deny file-read* (subpath "/Users/mr9esx/Documents/Pixoma"))' \
  /usr/bin/env CI=true PIXOMA_PLATFORM_URL=http://127.0.0.1:18082 CARGO_BUILD_JOBS=2 CARGO_INCREMENTAL=0 \
  PATH=/opt/homebrew/opt/node@22/bin:/opt/homebrew/bin:/usr/bin:/bin pnpm tauri build --ci
```

相同限制下执行 `pnpm install --frozen-lockfile` 通过。Rust 发布编译耗时 1 分 41 秒，应用及 DMG 均正常生成；新应用与随包 FFmpeg 的签名完整性检查通过。

## 当前安装包

当前安装包包含 MCP 协议、取消正文恢复和模型缓存刷新。发布编译耗时 7 分 20 秒，命令退出码为 0，生成 77.57 MiB 的应用与 37.49 MiB 的 DMG。版本为 `0.1.0`，默认平台地址为 `http://127.0.0.1:30808`，目标为 macOS ARM64，最低系统版本声明为 11.0，使用 ad-hoc 签名。

DMG 位于 `artifacts/Pixoma Studio_0.1.0_aarch64.dmg`，SHA-256 为 `20abcb50bcedc7e5eddf6ce6b9898978ecdd3d7d48ee31723bb992799d1742e2`，主程序 SHA-256 为 `fb9fed7cb39ad5950a2e96de92acd08a2694191e2b7fc083d095d5f9f95131a3`。应用和随包 FFmpeg 的严格签名检查、保存后的 DMG 全部校验和均通过。当前前端 6 项行为检查、类型检查和构建通过。

不设置 `PIXOMA_PLATFORM_URL`，从本次镜像启动应用后，独立目录中的 `config/desktop.json` 保存内置地址 `http://127.0.0.1:30808`，SQLite 的 `quick_check` 通过。实际平台收到未登录身份查询并返回 HTTP 401，原生窗口显示登录页面，应用菜单包含模型、技能和 MCP 设置。最新包的登录提交和模型即时刷新未取得完整页面证据，保留待验证状态。

交付检查结束后停止本轮应用和隔离平台，退出镜像挂载，执行 Cargo 清理并删除前端依赖、构建目录、临时凭据文档、诊断文件和独立测试数据。`src-tauri/target/`、`node_modules/`、`dist/` 均不存在，`.local-cache/` 为空；`artifacts/` 只保留当前一份 DMG。

此前 MCP 安装副本已验证账号、项目、会话、文档第二版本和引用记录恢复，页面验收按清单记录状态。测试完成后清理编译缓存、测试安装副本、独立测试数据及测试服务环境；本节以外的测试路径仅记录当时操作。当前仅保留 `artifacts/` 中的一份安装包。

## 安装包项目与技能页面检查

2026-10-22 直接运行当前 DMG 中的应用，指定 `.local-cache/ui-check/data` 为本轮独立测试目录。账号检查使用本机已有的 `bin/pixoma` 和隔离数据库，地址为 `http://127.0.0.1:18083`；平台注册接口和安装包内登录成功，登录时没有勾选保存登录状态。本轮没有重新编译桌面应用。

页面创建项目 `页面验收项目`，随后重命名为 `项目重命名检查`；SQLite 中项目 `b5ac7fad-915c-4ad6-84ec-1ebf3631032d` 保存该名称。在该项目通过原生文件选择器导入当前 `README.md`，资产 `027e0f4b-c075-4604-90be-2b84b2e1a5d8` 的版本为 `e5e777fe-ecf8-4f01-9e72-4a59592b272d`。页面保存评分 4 和标签 `版本检查`，数据库中的项目引用、评分和标签一致。名称搜索 `不存在的验收文件` 返回 0 项，标签搜索 `版本检查` 返回该文档，共 1 项。

从原生菜单打开技能管理，创建 `desktop-validation`，在编辑器保存 `SKILL.md` 和 `references/steps.md`，版本为 `1.0.0`。重新打开后，正文与附带文件内容完整；版本历史列出 `1.0.0`。修改技能说明后保存 `1.0.1`，页面列表显示新说明，SQLite 的两个版本分别保留各自说明和实际文件。

后续页面检查使用相同安装包和重新建立的隔离数据。测试账号 `desktop-ui-20261022-2143` 在页面保存昵称 `桌面资料检查` 和测试邮箱，平台数据库记录一致；退出确认、返回登录页、重新登录及读取原昵称通过。官方技能 `h3-prompt-writing`、专家 `专家对话验证` 和工作流 `文生视频` 的列表与详情可读取，详情展示实际版本、说明、步骤或输入要求。

通过原生文件选择器导入 `document-steps.zip`，其中包含 `SKILL.md` 和 `references/requirements.md`。正文、附带文件和检查结果正常，保存 `1.0.0` 后完成停用与启用；修改说明并保存 `1.0.1` 后，在历史版本页面打开 `1.0.0`，原说明、正文及附带文件保持原文。SQLite 中技能 `5839e503-5e66-4afb-af13-fca80b593daf` 的两个版本与页面一致。

在项目 `资产保留检查` 中导入 86 字节的 `资料检查.md` 和 705,644 字节的 `播放检查.wav`。WAV 使用标准库 `wave` 写入实际 44,100 Hz 单声道 PCM 音频，时长 8 秒；页面显示正确大小与时长。播放后控件显示 `Pause`，暂停后恢复 `Play`；进度控件定位后显示 `Elapsed: 4 seconds`，继续播放和暂停后显示第 5 秒。项目删除确认显示两项资产，删除后均移入未归属项目，SQLite 的 `project_id` 为空，原资产与版本编号保持不变。

文档资产 `a5bc2307-1197-41d9-abbd-39cbe9327f1a`、版本 `23f5da80-8cd2-4c36-aa89-7da0ae0cb056` 通过详情页面添加到项目 `资产引用检查`，新引用 `a02e130f-9780-47ce-9cdd-2d04acff0012` 与原引用使用相同资产和版本，页面显示复制来源。归档未归属项目中的原引用后，普通列表仅显示音频，已归档筛选仅显示文档；恢复后两项资产重新显示。复制后的项目引用保持可用。随后创建分类 `文稿` 并分配给复制引用，未分类筛选返回 0 项，`文稿` 筛选返回该文档 1 项。

资产类型选择图片后返回 0 项，清除筛选恢复原列表。文件树六种分组分别显示实际资产、`无来源对话 2`、`未分类 2`、`md 1` 与 `wav 1`、`未评分 2`、`无标签 2`，切换结果与当前数据一致。分页、日期、格式、重复内容筛选及其他待验收项目继续按清单记录。

文档和音频保存文件的 SHA-256 分别为 `12be3d7499c92b81e98440478814d4a44cb5de0c97b28274d5385e46b1f06fb0`、`fb819449258d6edfd8e076a259e1fe5643685874f9374627b26bcc801beaeb0b`，与导入文件一致。

在隔离平台通过管理接口创建 30 分钟有效的专用 MCP 凭据，从原生菜单新建 `平台 MCP 页面检查`，地址为 `http://127.0.0.1:18083/mcp`。保存前和保存后的工具发现均取得 `get_task`、`get_workflow`、`list_tasks`、`list_workflows`、`run_workflow` 五个真实工具；页面保存凭据掩码、请求确认策略和工具数量。停用后显示已停用，重新启用后显示配置正常、已启用。

原生菜单退出应用后，原应用进程退出码为 0。页面检查结束时停止本轮应用和隔离平台，退出 DMG 挂载，删除独立测试数据、ZIP、音频文件、辅助脚本和专用系统凭据；保留源码、验证资料和 `artifacts/` 中的一份安装包。

## 安装包资产分页检查

通过原生文件选择器逐项导入 52 份实际 Markdown 文档：`文稿-01.md` 至 `文稿-51.md` 使用相同正文，`独立文稿.md` 使用不同正文。SQLite 保存 52 项资产引用，内容目录只保存两份文件；正文 SHA-256 分别为 `26580d0f1fbe0887b089f0bba9b3ab0b933cb6ec3284fd8bad03365c1969b4e7` 和 `d8517933152ad94aa38ed0500f717d4e2daf45870b28f370b08dab685661e27c`。

资产列表每页读取 50 项，页面显示总数 52 和「加载更多资产」。点击后可以读取第二页的 `文稿-02.md` 与 `文稿-01.md`，总数保持 52，加载按钮消失。文件树通过键盘定位项目节点，使用 End、Tab 和 Return 操作加载下一页；加载后实际显示 52 个文档节点，`文稿-02.md` 与 `文稿-01.md` 均可读取。

重复内容筛选返回 51 项，独立文稿未显示；保持该筛选时搜索 `独立文稿` 返回 0 项。清除筛选后恢复 52 项。再导入 83 字节的 `补充资料.txt`，全部资产增加到 53 项；选择 MD 格式返回 52 项，选择 TXT 格式返回 1 项并显示对应文件。日期起点选择 `2026-10-23` 返回 0 项，改为 `2026-10-22` 返回 53 项。仅设置日期终点时，选择 `2026-10-21` 返回 0 项，改为 `2026-10-22` 返回 53 项。起点与终点均为 `2026-10-22` 时返回 53 项；将终点改为 `2026-10-21` 后显示「范围起点不能晚于终点」，筛选浮层保持打开，修正终点后恢复 53 项。

检查结束后停止应用和隔离平台，退出镜像挂载，删除独立测试数据库、资产文件和临时归档。编译目录、前端依赖和测试安装副本已清理，`artifacts/` 仅保留一份安装包。

## 安装包模型、文档与专家检查

隔离平台地址为 `http://127.0.0.1:18083`，本轮账号为 `desktop-ui-20261022-2248`。通过原生菜单创建 `桌面页面检查` 模型，使用现有平台配置对应的真实服务；页面连接测试成功，耗时 973 ms。随后修改名称为 `桌面默认模型`、最大输出为 3072 Token，再添加 `桌面备用模型` 并设为默认。SQLite 中两项配置的默认标识分别为 0 和 1；重新启动后，新会话选择备用模型，专家可以选择默认模型。凭据未输出到检查记录，保密传递使用的临时文件已删除。

保存本地模型后同步刷新设置、对话及专家编辑中的模型缓存。使用实际 `QueryClient` 验证三类模型查询失效、项目查询保持原状态；全部 6 项前端检查、类型检查和构建通过。该调整已包含在当前安装包，即时刷新页面操作保留待验证。

会话 `164fbcf5-28f2-402c-848a-7bf69d397d95` 通过页面创建 `版本检查.md`，第一版本内容包含 `VERSION-ALPHA-2248`，第二版本包含 `VERSION-BETA-2248`。资产库先保留第一版本，选择第二版本并更新项目引用后显示第二版正文；再次选择第一版本并更新引用后，第一版原文继续可读。新会话 `ac95c0f1-8657-4690-9832-d0c380763e5d` 从全局资料选择第一版本，真实模型返回 `VERSION-ALPHA-2248`；消息保存的 `asset_version_id` 为第一版本 `a176bbe5-2136-4ae5-9579-3f9b4004b8a2`。

在同一会话通过页面创建「准备资料」计划节点和「确认结果」操作节点。选中计划节点后按右方向键，SQLite 保存的横坐标由 72 更新为 77；使用 Fit View 定位操作节点并通过 Delete 删除，页面及数据库保留计划节点。连线创建及输出资产打开继续按页面清单验收。

通过页面创建「文档核对专家」，上传 `public/images/pixoma-ghost.png` 头像，绑定本地模型并保存两个工作步骤。会话 `421ad310-6816-4303-a641-844a7d333d89` 实际生成专家任务与输入卡片；选择「继续补充」后保存 defer，重新请求卡片并提交 `DOC-2248-ALPHA` 后保存 start。批准创建 `编号核对.md` 后出现结果确认卡片；填写修改要求后选择「修改结果」，批准文档更新，第二版本正文包含 `REVIEW-2248`。工具详情显示实际参数、结果和资产 ID。退出应用后重新启动，第二版本文档、已答复卡片与待确认结果卡片均恢复；点击「确认结果」后保存 accept，第一步骤状态为 completed，第二步骤保持 pending。

本轮结束后停止应用与隔离平台，退出镜像挂载，删除本轮独立数据库、资产、前端依赖与构建目录，并删除两项专用模型凭据。

## 安装包技能发布与资产批量检查

本轮使用隔离平台 `http://127.0.0.1:18083` 和账号 `desktop-ui-20261022-2311`，通过页面创建技能 `publication-check`，本地 ID 为 `1600765e-7f30-4921-af56-22e6b7e962fd`。两个保存版本分别包含 `PUBLICATION-ONE-2311` 和 `PUBLICATION-TWO-2311`。

本地保存至 v1.0.1 后，发布对话框选择 v1.0.0，列表同时显示「已保存 v1.0.1」与「已发布 v1.0.0」，用户目录详情仅显示第一版本标记。重复发布第一版本后，平台保留同一个技能 ID `4c23548d-9d43-4459-abc5-e87a5fe7286c`。更新上架版本为 v1.0.1 后，平台保存第二版本正文；下架后用户目录显示「暂无内容」，本地两个版本继续可读。详情页面显示三次上架和一次下架记录，平台数据库记录一致。重复请求编号的幂等检查仍以原有接口检查为依据，本轮页面操作使用不同请求编号。

原生文件选择器导入 `public/images/pixoma-ghost.png` 和 `public/images/logo.png`。选中两项资产后，依次通过批量操作将分类设为「批量图片」、添加「批量检查」标签、移除该标签、设置三星评分，并添加到「批量操作项目」。SQLite 确认两项原引用的分类及评分正确，标签关联从两项变为零项；项目副本引用相同资产及版本，共四项项目引用、两项资产和两份内容文件。

批量归档确认框选择取消后，两项原资产与选择状态保持；再次确认归档后，普通列表显示零项，已归档筛选显示两项。批量恢复后，已归档筛选显示零项，清除筛选恢复两项；目标项目的两项副本持续可读。数据库的 `quick_check` 通过。

使用现有 FFmpeg 将实际视频样本延长为八秒，生成 6,702 字节的 MP4 并通过原生文件选择器导入。应用显示 80 × 40、视频封面、调色盘和八秒时长。点击原生时间轴中点后，控件显示 `Elapsed: 4 seconds`、`Duration: 8 seconds` 和位置 `0.5`；继续播放后暂停，位置更新为 `0.624`。

本轮结束后停止应用和隔离平台，退出镜像挂载，删除专用账号数据库、导入资产及生成的视频文件，`.local-cache/` 为空。

## 安装包资产页面检查

本轮页面操作使用 `.local-cache/bundle-check.BqONOZ/installed/Pixoma Studio.app`，数据目录为 `.local-cache/bundle-check.BqONOZ/data`。重新启动后，页面读取原账号、项目和文档。文档在项目中的名称改为 `安装包文稿检查.md`，评分保存为 4，标签保存为 `安装包验收`；实际 SQLite 记录与页面一致。

原生文件选择器导入 `src-tauri/tests/fixtures/red.mp4` 和 `.local-cache/ffmpeg-first-frame.png`，所属项目均为 `安装包创作检查`。视频显示 80 × 40、1,571 字节、原文件修改日期 `2026/10/19 09:45`，封面及调色盘生成成功；点击播放后出现实际媒体控件和暂停按钮，播放结束后显示播放按钮。图片显示 80 × 40、151 字节、原文件修改日期 `2026/10/19 09:51`，详情及调色盘读取通过，放大和恢复详情操作通过。时间定位未获得足够证据，继续保留待验证。

发布程序通过当前可执行程序目录选择随包 FFmpeg，未设置开发用程序路径。实际视频导入和封面生成因此覆盖安装包随附程序的使用。

单项导出通过原生保存对话框选择 `.local-cache/bundle-ui-20261021`，生成 `安装包文稿检查.md`；再次导出后取消保存，正常返回原详情。实际导出文件与原资产内容通过 `cmp` 比较一致。批量选择文档和视频后导出 `assets.zip`，标准库 `zipfile` 读取中文文件名并解压成功；其中 `001-red.mp4` 与 `002-安装包文稿检查.md` 分别与原视频、单项导出文档通过 `cmp` 比较一致。

页面操作结束后，通过应用退出操作正常结束原安装副本，退出码为 0。随后启动 `bundle-final.CFap2e` 安装副本，实际打开同一数据库；完成系统钥匙串授权后恢复账号与原项目。

## 安装包对话页面检查

2026-10-22 使用 `.local-cache/bundle-final.CFap2e/installed/Pixoma Studio.app` 和原独立数据目录完成检查。专用测试 MCP 凭据已经到期，发现工具显示实际 HTTP 401 错误；通过原生菜单进入 MCP 设置并停用该测试连接后，继续验证独立模型对话。MCP 协议调整后的安装包另行验证。

会话 `04065ff0-058c-42e1-bbce-c41536faf86d` 的两次真实模型回复均为 `PX62250`，第二次请求读取第一次消息。轨迹页面显示两个实际模型请求；上下文页面显示历史消息、工具定义、实际输入、缓存读取、输出 Token 与时间记录。

拒绝工具审批后，运行保存为 `cancelled`。随后批准 `create_text_asset`，创建文档 `模型审批检查.md`，资产编号为 `839d7b42-dae6-4430-b4ef-84e1446a4cd6`，第一版本为 `1a9cba9e-e56e-46b6-9957-6964b64f95f0`。编辑表单保存第二版本 `1c0b13f4-b834-4235-82a9-3317573fccb5`，详情页显示第二版本正文。选择该版本作为消息资料，真实模型逐字返回第二版本正文。

科普说明已经开始显示正文后点击停止，运行 `568ee6ef-93cb-4ebf-91f2-ead2c24b3440` 保存为 `cancelled`；随后继续询问资料中的编号，模型回复 `PX62251`。原消息和文档引用继续保留。

## MRTR 真实业务检查

服务使用官方 TypeScript SDK commit `c05225a636c679870b65f8ca4fe83a76ea1cf113` 的 `examples/todos-server`，源码保持原样。安装锁定依赖并构建服务包后，使用 `PORT=18202 pnpm --filter @mcp-examples/todos-server start:http` 启动，只监听 `127.0.0.1`。

检查通过 `PIXOMA_TEST_MRTR_MCP_URL` 连接真实服务，使用 `PIXOMA_TEST_MODEL_CONFIG` 保存本机真实模型的公开配置；本项只执行 MCP 工具，不发送模型请求。工具名称和定义来自实际发现结果。服务看板为空时创建随机标题的待办并标记完成，依次执行取消、拒绝、选择否和确认。每次答复后调用 `list_tasks` 检查实际看板；前三次保留原编号和完成状态，确认后看板为空。四次流程均在等待、答复保存和准备继续时关闭并重新打开 SQLite，工具结果和记录状态一致。

旧版实时输入使用官方 Rust SDK `rmcp-v1.5.0` 的 enum 示例，commit 为 `020a38b6ad3d0f26487c464250a484fad2a06b0e`，示例源码保持原样，服务依赖 `rmcp` 和 `rmcp-macros` 均固定为 1.5.0。App 继续使用 rmcp 3.5.1，协商后建立旧版会话。设置 `PIXOMA_TEST_ELICIT_WAIT_SECONDS=301`，实际等待后提交四项选项，服务保存更新后的内容；完整检查耗时 301.28 秒，退出码为 0。

## 本地 Git 仓库

仓库根目录为 `/Users/mr9esx/Documents/PixomaStudio`，分支为 `codex/desktop-studio`。应用源码与验证资料提交为 `1aec1dff37ae3ea72f49a53d320e75a4055aa2f8`；author 和 committer 均为 `李卓洲 <1138099359@qq.com>`。缓存、依赖目录、构建产物和本地配置没有纳入提交。

## 待完成的验证

- [前端与安装包验收清单](frontend-checklist.md)记录页面操作、已有接口证据与安装包交互状态；Studio 共同业务文件的差异已逐项核查。
- MRTR 补充输入的真实答复、业务状态和恢复检查已经通过；Task 补充输入已有实际 SQLite 状态检查，仍需对应真实服务的输入往返证据。实时输入和异步 TaskDemo 任务已有官方 SDK 服务检查。
- 页面中的完整操作需要在业务接口齐全后继续验证。
- 技能目录与正文分页、官方专家、专家步骤、卡片和个人发布同步已有真实检查；发布操作的完整页面交互仍需验收，原有前端 API 与真实原生 Commands 的往返检查已经通过。
- 图片引用在原生页面中的完整选择、发送与历史读取操作仍需交互验收。
- 文件保存对话框、单项及 ZIP 导出、图片预览和视频播放已完成本轮检查；音频预览与视频时间定位仍需桌面交互验收。
- 安装包完整运行仍待完成。`bundle-final.CFap2e` 副本的登录恢复与真实模型调用已通过，其他页面及 MCP 协议调整后的安装包按清单继续验证。
