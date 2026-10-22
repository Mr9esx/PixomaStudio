<img src="public/images/Pixoma.svg" alt="Pixoma" width="64" />

# Pixoma Studio

使用 Tauri 2 和 Rust 的本地创作应用，保留 Studio 的 React 界面。项目、会话、资产、模型设置和 Agent 运行记录保存在本机；Pixoma 平台提供账号、官方内容与远程工作流。

## 启动开发环境

需要 Node.js 22.14 以上版本、pnpm 10.12.4、Rust 与 Cargo，以及对应系统的 Tauri 2 构建依赖。当前验证环境为 macOS。

视频解码程序从 FFmpeg 9.0.2 官方源码构建，需要 C 编译器、Make、curl、tar 和 zlib 开发文件。macOS 使用 Xcode Command Line Tools 提供编译环境。

在本目录安装依赖：

```sh
pnpm install --frozen-lockfile
```

安装依赖时会校验 FFmpeg 源码并构建本机程序。生成文件保存在 `.local-cache/` 和 `src-tauri/binaries/`；需要重新准备时运行 `pnpm prepare:media`。安装包包含视频解码程序、LGPL 2.1 许可证、对应源码和构建参数。

将命令中的 `平台地址` 替换为实际 Pixoma 平台地址，启动原生窗口：

```sh
PIXOMA_PLATFORM_URL='平台地址' pnpm tauri dev
```

登录后，在应用菜单中配置模型与 MCP 连接。模型配置支持 `openai_chat_compatible`、`openai_responses` 和 `anthropic_messages_compatible`。远程工作流使用独立的 MCP 凭据。

## 本地数据

SQLite 数据库、资产文件和应用配置分别使用系统应用目录。`PIXOMA_STUDIO_DATA_DIR` 可以指定单独的数据目录，用于开发与真实服务检查。

平台地址保存到应用配置目录中的 `desktop.json`。设置 `PIXOMA_PLATFORM_URL` 可以更新地址；后续启动读取已保存的地址。模型 API Key、MCP 凭据和平台登录状态使用系统凭据存储。

官方专家使用创建会话时的本地默认模型，并随会话保存完整模型配置。已有会话保留其绑定的专家版本与模型配置。

工作流任务取得编号后，重新启动会继续查询相同任务。停止本地等待后，远程任务保留实际计算状态。提交结果未知时保存当前凭据的任务候选，等待核对。

## 执行检查

```sh
pnpm typecheck
pnpm test
pnpm build
cargo test --manifest-path src-tauri/Cargo.toml
cargo clippy --manifest-path src-tauri/Cargo.toml --all-targets -- -D warnings
cargo fmt --manifest-path src-tauri/Cargo.toml --check
```

需要真实平台、模型或计算节点的检查使用 `#[ignore]` 标记，按对应检查声明配置环境变量后单独执行。检查使用真实服务、SQLite、系统凭据与资产文件。结果和验证范围见 [验证记录](docs/verification.md)。

每轮测试完成后，停止本轮启动的服务，清理编译缓存、测试安装副本、独立测试数据和下载的测试环境。需要保留的安装包统一存放在 `artifacts/`，仅保留最新一份；使用 `cargo clean --manifest-path src-tauri/Cargo.toml` 清理 Rust 构建产物。源码、验证记录、正式应用数据和随包资源继续保留。

## 构建安装包

当前交付的 macOS 安装包默认连接本机 Pixoma 平台 `http://127.0.0.1:30808`。

将 `平台地址` 替换为安装包要连接的平台地址：

```sh
PIXOMA_PLATFORM_URL='平台地址' pnpm tauri build
```

构建产物位于 `src-tauri/target/release/bundle/`。平台地址随安装包保存，用户首次启动后写入应用配置目录。

macOS 构建使用 ad-hoc 签名。需要 Apple 签名与公证时，按照 [Tauri macOS 签名文档](https://v2.tauri.app/distribute/sign/macos/)设置构建环境变量。

自动化环境使用 `CI=true PIXOMA_PLATFORM_URL='平台地址' pnpm tauri build --ci`。此模式直接生成 DMG，保留应用和 Applications 目录链接。

## 工程目录

| 目录 | 内容 |
| --- | --- |
| `src/` | Studio 页面、组件、路由与 Tauri 传输 |
| `public/` | 品牌资源与页面静态文件 |
| `src-tauri/src/` | 本地业务、Agent、平台账号与 MCP |
| `src-tauri/migrations/` | SQLite 迁移 |
| `src-tauri/tests/` | 真实文件、数据库与服务检查 |
| `scripts/` | 随包视频解码程序的构建 |
| `docs/` | 设计、开发计划与验证记录 |

工程使用独立的 pnpm 和 Cargo 依赖，运行与构建均从本目录执行。
