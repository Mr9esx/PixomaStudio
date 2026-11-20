# 计算节点与工作流验收

## 页面来源

计算节点详情沿用 Pixoma `features/edges/detail-panel.tsx` 的标题、状态标签、横排元信息、硬件规格栏、统计卡片和操作入口；监控沿用原卡片、折线图、IO 面积图、时间范围和任务表格布局。

ComfyUI 工作流详情沿用 `features/cases/detail-panel.tsx`、`sections/workflow-graph-preview.tsx`、字段表格与节点图查看器。默认显示工作流配置，分别编辑基础信息、工作流、输入与输出。编辑使用 Studio 已确认的页内区域。计算节点放在设置，工作流直接选择执行节点。

## 自动检查

- `pnpm test`：25 个文件、139 项测试通过。
- `pnpm build`：Vite 构建与 TypeScript 检查通过。
- Rust library：33 项通过、5 项需要外部服务的测试未运行；新增媒体 Origin 检查单独通过。
- Rust `compute_nodes`：5 项通过，涵盖真实 HTTP、凭据、配置、统计与引用检查。
- Rust `workflow_definitions`：4 项通过；`execution_recovery`：1 项通过。
- Go `go test -race ./...`、`go vet ./...` 通过。硬件采集读取实际设备；缺少指标时保留空值与采集错误。
- 本次文件的 `git diff --check` 通过。

## 原生页面

使用 Tauri 实际页面、原生控件事件、本地 SQLite 与真实 Go Agent 检查，未使用截图或模拟服务。

- `native_compute_migration.js`：新建两步、部署凭据、平台命令、本机 Agent 启停、持久凭据、硬件编辑与重新采集、监控、工作流配置、启停和版本查看通过。节点 `e6450607-b167-4e92-b725-c616a35149f8`。
- `native_pixoma_details.js`：节点详情、监控自定义时间、基础信息保存、输入输出分别编辑、节点设置往返保留草稿通过；1440、1024、760 像素窗口均无横向越界。
- `native_workflow_pages.js`：创建版本、页内查看、设为默认、复制到草稿、测试版本表单通过。版本 `1.0.6`；默认版本 `b27752cd-8c9c-46c7-8c38-1efc990a0fd9`。
- 端口占用时显示监听错误，其余本地功能仍可访问；连接设置保存与重启提示通过。

## 真实 ComfyUI

ComfyUI 版本为 `0.43.0`，工作流使用提供的 Klein9B API JSON。

- 执行 `7f29ad40-01c0-45af-9f82-2b9b738e010d` 与再次运行 `e17d1672-9878-4b3c-a66d-1fd855949626` 均成功保存并预览 1024×1024 PNG。两次执行和 `prompt_id` 独立，原结果保持不变。
- 执行过程中停止本机 Agent 返回 409。
- 执行 `90760d48-7017-431a-8846-6c0d48066a0a` 通过界面取消；ComfyUI 任务 `20aa1ab1-79c5-46af-8099-6a9033813ea7` 返回已取消。取消请求使用任务编号。
- 重新启动后沿原 `prompt_id` 恢复已提交执行并保存结果。
- 原任务在实际 ComfyUI 队列与历史中均不存在时，执行 `07e41082-4933-43a3-8ed6-32df203667ee` 返回 `result_unknown`，保留原编号与原因，输出数量为零。

## macOS 应用包

`pnpm tauri build --bundles app` 构建通过，`codesign --verify --deep --strict` 通过。包中包含 `edge-agent`、`ffmpeg` 和 `ffprobe`。

独立验收目录中的节点 `91fed52f-186e-4981-98f7-62b6ae87bde5` 使用包内 Agent 连接 ComfyUI，保存实际版本、心跳和监控记录。通过应用正常退出后，主进程、Agent 进程与节点监听端口均已退出。

MiniMax 与火山方舟的外部生成调用按用户要求跳过。
