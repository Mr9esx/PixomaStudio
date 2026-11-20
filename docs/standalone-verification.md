# 独立运行验证记录

日期：2026-10-31。

## 验收文件与节点

附件：`/Users/mr9esx/Downloads/MiniMax_Klein9B_文生图_本地.json`。

SHA-256：`b22aa58ef2607b8d9734021b0e765f334b98326214340621edf117796c587607`。

文件使用 ComfyUI API 格式，包含 9 个节点。参数为 1024 × 1024、20 steps、seed 42、Euler、simple scheduler、CFG 1；输出为 `80 / SaveImage`。

| 模型类型 | 文件 |
| --- | --- |
| UNET | `darkBeast30BF16INT8_dbkleinv2BFS.safetensors` |
| CLIP | `qwen_3_8b_fp8mixed.safetensors` |
| VAE | `flux2-vae.safetensors` |

节点采用现有 `Linux-ComfyUI` 的地址 `http://192.168.31.131:8188`。实际 `/system_stats` 返回 ComfyUI 0.34.0、Linux、NVIDIA GeForce RTX 4070 SUPER。所需节点类型、模型文件与枚举参数均通过实际 `/object_info` 查询。

## Rust 与 Go 实际计算

最近一次执行使用当前 Rust 业务结构，程序为 `src-tauri/examples/standalone_acceptance.rs`。独立 Go Agent 在 Mac 上运行，通过网络访问 Linux ComfyUI。

| 项目 | 结果 |
| --- | --- |
| 工作流 ID | `5e9f58e1-94ed-4d25-bca9-9fbd9b187261` |
| 版本 ID | `0d138fb0-fb49-4df5-b795-3112ef6d2201` |
| 执行 ID | `c4bd521e-05f4-4182-af2c-d348917a3041` |
| 尝试 ID | `fa8c1436-3138-4f53-be1c-6b40d6ddbdc5` |
| ComfyUI `prompt_id` | `ea1236de-36ec-4c57-99c3-a1050710530f` |
| 最终状态 | `succeeded` |
| 图片 | PNG，1024 × 1024，1,485,298 bytes |
| SHA-256 | `8b7377a8973009768c6e9d9f773b1afcd07b7cf4a783cf7055124b32db1ab8a2` |

程序在取得 `prompt_id` 后终止 Agent、停止 Rust 服务并关闭数据库，随后重新启动两者，沿用原执行编号、尝试编号和 ComfyUI 编号取得结果。重复请求返回同一执行，输出字节与 ComfyUI `/view` 返回内容一致，重新打开数据库仍可读取相同资产。

结果位于 `.local-cache/acceptance-20261031-03/result.json` 与 `result.png`。该检查已经通过三次，均使用原附件和实际计算节点。

## 本地业务检查

| 命令 | 当前结果 |
| --- | --- |
| `CARGO_INCREMENTAL=0 cargo test --offline --lib --test integration` | 19 项单元检查、50 项集成检查通过；17 项需要模型或 MCP 服务的检查未执行 |
| `CARGO_INCREMENTAL=0 cargo clippy --offline --all-targets -- -D warnings` | 通过 |
| `CARGO_INCREMENTAL=0 cargo check --offline --examples` | 通过 |
| `pnpm test` | 6 个文件、11 项检查通过 |
| `pnpm build` | Vite 构建与 TypeScript 检查通过；部分 JavaScript 文件超过 500 kB |
| `cargo fmt --check` | 存在需要手工调整的格式 |

数据库检查遍历全部业务表和索引，确认没有 `account_id` 或账号索引。技能 ZIP、技能版本、专家能力、专家头像、模型和 MCP 凭据、项目、会话、资产版本、媒体范围、导出、节点凭据及执行恢复均使用实际 SQLite、文件或 HTTP 连接验证。

新增的会话工作流检查覆盖固定字符串版本 ID、专家依赖、工作流输入卡片、重复答复和重新打开数据库。前端检查覆盖工作流引用经过编辑器转换后保留本地版本 ID。

## 桌面页面

桌面请求和媒体直接访问本地数据。技能、专家与工作流引用从本地接口读取。登录、注册、身份门禁和发布页面已移除。

`native_layout` 使用 `.local-cache/desktop-standalone-20261031-01/` 中的新数据库启动真实 App。1440px 与 760px 两种宽度下，共 24 项页面测量通过：新对话、已有对话、资产库、工作流、技能、专家、模型设置、技能设置、MCP 设置、计算节点设置、会话管理和专家编辑。页面主体边距、边框、圆角与底色一致，顶部高度、窄窗口菜单和原生窗口双击缩放通过 DOM 与原生事件检查。

Rust 节点接口随 App 启动，默认监听 `127.0.0.1:18083`，显式环境配置使用 `PIXOMA_STUDIO_AGENT_LISTEN`。绑定失败返回实际错误；不选择替代端口。

`native_workflow` 使用显式监听地址 `127.0.0.1:18084` 和独立目录 `.local-cache/native-workflow-20261031-03/`，通过真实页面操作导入原附件、添加输入输出字段、绑定 `40.text` 和 `80[0]`、选择计算节点、保存、填写运行输入并提交。页面成功读取生成的实际资产，图片自然尺寸为 1024 × 1024。

| 项目 | 桌面执行结果 |
| --- | --- |
| 工作流 ID | `b3fabe3d-6471-4da6-b6ed-c583e3358327` |
| 执行 ID | `c56f6005-28c7-4db0-beb6-acc6aeb61fb4` |
| ComfyUI `prompt_id` | `01ecb8af-ee85-443b-9911-1240f1893a91` |
| 图片 SHA-256 | `8b7377a8973009768c6e9d9f773b1afcd07b7cf4a783cf7055124b32db1ab8a2` |
| 页面结果 | `success: true`，实际图片 1024 × 1024 |

桌面验收程序的进程退出码为 0，记录保存在上述目录的 `result.json`。检查结束后 Agent 与验收端口均已退出。该检查使用独立启动的真实 Go Agent；本机 Agent 自动管理和安装包验证继续处理。

## 尚需验证的范围

- 本机 Agent 自动启动、退出、重新启动和随安装包构建。
- Go Agent 在 Linux 机器运行，使用独立目录传输输入与输出。
- 其他执行阶段的中断、取消及恢复。
- 实际模型在会话和专家中调用本地工作流。
- Seedream 图片与 MiniMax 视频的生成服务配置和实际执行。
- 远程监听设置、节点部署和指标页面。
- 当前工程的安装包构建与实际安装检查。
