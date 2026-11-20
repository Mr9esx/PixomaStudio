<img src="public/images/Pixoma.svg" alt="Pixoma" width="64" />

# Pixoma Studio

使用 Tauri 2 和 Rust 的独立创作应用，保留 Studio 的 React 界面。项目、会话、资产、模型、技能、专家、MCP 和工作流保存在本机，独立 Go Edge Agent 连接 ComfyUI 计算节点。

## 启动开发环境

需要 Node.js 22.14 以上版本、pnpm 10.12.4、Rust 与 Cargo，以及对应系统的 Tauri 2 构建依赖。当前验证环境为 macOS。

工程根目录提供 Makefile，运行 `make` 或 `make help` 查看命令：

- `make install`：安装前端依赖并准备 FFmpeg。
- `make dev`：启动桌面应用开发环境。
- `make build`：构建桌面应用安装包。
- `make dev-web`、`make build-web`：单独启动或构建前端。
- `make test`：运行前端、Rust 和 Edge Agent 测试；也可以分别运行 `make test-web`、`make test-rust`、`make test-agent`。Edge Agent 测试启用数据竞争检查，需要 Go 1.25.13 以上版本和 C 编译器。
- `make typecheck`、`make lint`、`make fmt-check`：检查 TypeScript 类型、Rust 与 Go 代码及格式。
- `make fmt`：格式化 Rust 和 Go 代码。
- `make check`：检查格式、类型、代码并运行全部测试。
- `make prepare-media`：准备 FFmpeg 和许可证文件。
- `make agent-build`、`make agent-run`：编译或启动 Edge Agent，配置见 [Edge Agent 说明](apps/edge-agent/README.md)。
- `make clean`：删除 `dist/`、`.tanstack/` 和 Rust 构建产物。

Make 命令使用 `.local-cache/` 保存临时文件和 Go 构建缓存。Tauri 附加参数通过 `ARGS` 传入，例如 `make build ARGS=--debug`。

视频解码程序从 FFmpeg 9.0.2 官方源码构建，需要 C 编译器、Make、curl、tar 和 zlib 开发文件。macOS 使用 Xcode Command Line Tools 提供编译环境。

在本目录安装依赖：

```sh
pnpm install --frozen-lockfile
```

安装依赖时会校验 FFmpeg 源码并构建本机程序。生成文件保存在 `.local-cache/` 和 `src-tauri/binaries/`；需要重新准备时运行 `pnpm prepare:media`。安装包包含视频解码程序、LGPL 2.1 许可证、对应源码和构建参数。

启动原生窗口：

```sh
pnpm tauri dev
```

在设置中配置模型、MCP 和计算节点。模型配置支持 `openai_chat_compatible`、`openai_responses` 和 `anthropic_messages_compatible`。在工作流页面导入 ComfyUI JSON，绑定输入输出字段并选择计算节点。

## 本地数据

默认数据目录为系统应用目录下的 `standalone/`，数据库为 `studio-standalone.sqlite`。`PIXOMA_STUDIO_DATA_DIR` 可以指定独立目录，用于开发与真实服务检查。

模型 API Key 与 MCP 凭据分别保存在 SQLite 的对应记录中。计算节点使用独立凭据，Rust 保存摘要。节点服务默认监听 `127.0.0.1:18083`，通过 `PIXOMA_STUDIO_AGENT_LISTEN` 显式指定其他监听地址。

专家选择本地模型、技能和工作流版本。已有会话保留绑定的专家版本、模型配置和能力内容。

Go Agent 保存执行阶段和 ComfyUI 编号，重新启动后继续查询原编号。停止会话等待保留实际计算状态；无法确定提交结果时保留未知状态。

## 执行检查

```sh
pnpm typecheck
pnpm test
pnpm build
cargo test --manifest-path src-tauri/Cargo.toml
cargo clippy --manifest-path src-tauri/Cargo.toml --all-targets -- -D warnings
cargo fmt --manifest-path src-tauri/Cargo.toml --check
```

需要模型或 MCP 服务的检查使用 `#[ignore]` 标记，按对应声明配置环境变量后单独执行。计算验收使用原附件、实际 ComfyUI、SQLite 和资产文件。结果和验证范围见 [独立运行验证记录](docs/standalone-verification.md)。

每轮测试完成后，停止本轮启动的服务，清理编译缓存、测试安装副本、独立测试数据和下载的测试环境。需要保留的安装包统一存放在 `artifacts/`，仅保留最新一份；使用 `cargo clean --manifest-path src-tauri/Cargo.toml` 清理 Rust 构建产物。源码、验证记录、正式应用数据和随包资源继续保留。

## 构建安装包

构建 macOS 安装包：

```sh
pnpm tauri build
```

构建产物位于 `src-tauri/target/release/bundle/`。当前完整安装包与 Go Agent 随包管理的验证进度见 [前端与安装包清单](docs/frontend-checklist.md)。

macOS 构建使用 ad-hoc 签名。需要 Apple 签名与公证时，按照 [Tauri macOS 签名文档](https://v2.tauri.app/distribute/sign/macos/)设置构建环境变量。

自动化环境使用 `CI=true pnpm tauri build --ci`。此模式直接生成 DMG，保留应用和 Applications 目录链接。

## 工程目录

| 目录 | 内容 |
| --- | --- |
| `src/` | Studio 页面、组件、路由与 Tauri 传输 |
| `public/` | 品牌资源与页面静态文件 |
| `src-tauri/src/` | 本地业务、Rust 计算服务、Agent 与 MCP |
| `apps/edge-agent/` | 独立 Go Agent、ComfyUI 调用与执行日志 |
| `src-tauri/migrations/` | SQLite 迁移 |
| `src-tauri/tests/` | 真实文件、数据库与服务检查 |
| `scripts/` | 随包视频解码程序的构建 |
| `docs/` | 设计、开发计划与验证记录 |

工程使用独立的 pnpm 和 Cargo 依赖，运行与构建均从本目录执行。
