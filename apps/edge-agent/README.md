# Studio Edge Agent

Agent 主动连接 Studio 的 `/agent/v1` 接口，领取指定节点的执行，提交 ComfyUI，并将输出文件传回 Studio。

## 配置

| 环境变量 | 用途 |
| --- | --- |
| `CONTROL_PLANE_URL` | Studio 节点接口地址 |
| `AGENT_TOKEN` | 创建节点时生成的凭据 |
| `EDGE_DATA_DIR` | 当前 Agent 的独立数据目录 |
| `EDGE_ID` | 可选，检查凭据对应的节点 ID |
| `COMFYUI_BASE_URL` | 可选，检查 Studio 返回的 ComfyUI 地址 |

ComfyUI 地址由 Studio 节点配置提供。当前数据目录中的 `agent.sqlite` 保存实例编号、任务包、提交阶段、`prompt_id`、事件序号与输出确认记录。同一目录只允许一个 Agent 进程运行。

## 构建与运行

从 Studio 工程根目录创建 `.local-cache/go-tmp` 和 `.local-cache/go-build`，在本目录执行：

```bash
GOTMPDIR="$PWD/../../.local-cache/go-tmp" \
TMPDIR="$PWD/../../.local-cache/go-tmp" \
GOCACHE="$PWD/../../.local-cache/go-build" \
go build -o ../../.local-cache/edge-agent ./cmd/edge-agent
```

设置环境变量后运行生成的可执行文件。进程重新启动时继续处理日志中的原执行。提交结果无法唯一确认时保留原尝试并返回具体错误；更新配置后可使用相同数据目录重新启动。

## 检查

使用同样的构建缓存和临时目录设置，执行 `go test ./...`、`go test -race ./...` 与 `go vet ./...`。Studio 的实际工作流执行记录见 `docs/standalone-verification.md`。
