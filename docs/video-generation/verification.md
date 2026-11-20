# 视频编辑器验收

## 编辑与对话

- Studio 会话右侧使用视频编辑器，预览位于时间轴上方，属性区域按需展开。
- 固定轨道包含剧本、分镜、参考资产、视频、成片音频和字幕。
- 时间轴支持选择、直接编辑文字、调整时长、改变顺序、复制、分割、删除、撤销和重做。
- 时间轴使用 `@xzdarcy/react-timeline-editor` 1.0.0，播放使用 Remotion Player。时间尺、双边裁剪、分镜拖动、吸附、自动滚动和游标拖动由库组件处理。
- 剧本、分镜、参考资产、视频和字幕共享分镜边界，按项目帧率取整。音频支持独立移动与裁剪。
- 分镜重排使用移除当前分镜后的连续范围计算插入位置，支持不同时长和单帧分镜；素材、工作流、参数与历史随分镜保留。
- 直接文字编辑时保留输入基线。同一字段被其他入口修改后，保留当前输入并提供「使用最新内容 / 保留当前输入」；外部删除编辑中的分镜后，清理输入状态并恢复时间轴操作。
- 项目资产库展示项目内可用素材，支持导入、预览和引用到 Chat。
- 整片对话和分镜对话分别传入项目范围与分镜标识。模型读取最新项目后，通过工具修改、生成、采用和导出。
- 项目保存使用版本检查。存在其他窗口或 Chat 修改时，界面保留尚未保存的输入并提供冲突处理。

## 工作流与参数

- 每个分镜保存工作流、固定版本和独立输入；切换工作流时保留各自参数草稿。
- 参数控件读取工作流的 JSON Schema，支持枚举、数值范围、开关、文本和资产版本引用。
- ComfyUI 工作流定义节点输入、输出绑定以及宽度、高度、帧数等计算方式。
- 在线工作流支持 Seedance 模板和自定义 HTTP JSON 模板。自定义模板定义提交、查询、取消、认证、状态映射和输入／输出字段映射。
- 项目尺寸与生成素材尺寸分别保存；导出时按比例适配项目尺寸。
- 分镜剪辑时长按项目帧率计算。生成素材的时长由各工作流声明。

## 生成状态

- 未选择工作流：选择工作流后填写参数。
- 待生成：提交生成并保存执行编号、输入基线。
- 提交中／等待执行／生成中／保存结果：显示真实任务状态，提供取消操作。
- 首次生成成功：输入未改变时采用结果；输入已经改变时保留为候选。
- 已采用视频：显示「重新生成」。新结果作为候选，采用后保留原视频历史。
- 生成失败／取消：保留原有采用视频，允许重新生成。
- 提交结果未知：持续显示未知状态，提供查询和停止等待；不自动再次提交。
- 视频成功后提取首尾帧并登记独立资产，尾帧可供下一分镜的首帧输入引用。

## 真实验收记录

验收数据目录：`.local-cache/real-video-20261031`。

| 验收内容 | 记录 |
| --- | --- |
| H3 正式执行、视频与首尾帧 | `h3-manifest.json` |
| 四段 Seedance 分镜 | `video-acceptance.json` |
| 自定义 HTTP JSON 模板执行 | `http-video-acceptance.json` |
| AI Chat 创建五个分镜 | `chat-create.json` |
| AI Chat 采用五次生成结果 | `chat-apply.json` |
| 原生页面编辑、撤销重做、引用对话、H3 重新生成、保留历史与导出 | `native-video-result.json` |
| 开源时间轴、五轨联动、真实 Chat、并发修改、已有媒体导出 | `native-timeline-result.json` |
| 成片媒体信息与完整解码 | `final-video-verification.json` |
| 静音素材补音、字幕、指定位置循环音频、重复导出和非法剪辑范围 | `export-cases-verification.json` |
| AI Chat 导出成片 | `chat-export.json` |

字节模型生成共五次，均已结束。时间轴接入验收复用已有五段视频，视频生成调用为零；验收前后 `online_executions` 均为五项，`local_executions` 均为两项。

## 构建与文件

- 前端测试：26 项通过；类型检查和生产构建通过。
- Rust 测试：79 项通过；需要额外外部服务的 17 项测试保持忽略。
- macOS 应用：`src-tauri/target/release/bundle/macos/Pixoma Studio.app`，完整 Release 构建与签名检查通过。
- 媒体程序：FFmpeg、ffprobe 与字体库使用静态构建，只依赖系统动态库；最低系统版本为 macOS 11.0。
- 成片副本：`artifacts/video-editor/山间茶事-15s.mp4`。
- 时间轴验收导出：`artifacts/video-editor/山间茶事-时间轴验证.mp4`。

## 时间轴兼容性验证

- 依赖：`@xzdarcy/react-timeline-editor` 1.0.0、`react-virtualized` 9.22.6、React / React DOM 19.3.1、Tauri 2.12.4。应用包包含时间轴组件及新增依赖的 MIT 许可声明。
- 原生验收使用 macOS WebView，分别检查开发资源和 `tauri:` 生产资源协议。生产验收使用正式前端产物及 CSP。
- 指针操作覆盖双边裁剪、分镜重排、音频移动与裁剪、游标拖动和播放定位；游标拖出片尾后定位到最后一帧，游标位置与预览保持一致。五个业务轨道共享边界，源素材开始位置与采用版本保持正确。
- 键盘操作覆盖连续逐帧裁剪、焦点保持和不同时长分镜重排；文字编辑覆盖保存、撤销、重做、同字段并发修改的两项选择，以及外部删除正在编辑的分镜。
- 单分镜 Chat 修改第二个分镜字幕；整片 Chat 同时修改第一个和第五个分镜字幕。两种范围均通过真实对话和业务工具执行，确认时间轴同步、其他分镜内容与音频完整保留。
- 复制、分割、删除、新增、分镜循环、缩放、并发项目版本冲突处理、保存恢复与导出均在原生页面执行。
- 导出视频为 1280×720、30 FPS、450 帧，保留音频；使用随应用分发的 ffprobe 检查媒体信息，并通过 FFmpeg 完整解码转码检查。
- 结果保存在 `.local-cache/real-video-20261031/native-timeline-result.json`；最终记录同时包含 `success: true`、`mode: production`、`protocol: tauri:` 和 `media_verification.full_decode: true`。

在项目根目录执行生产协议验收：

```sh
MACOSX_DEPLOYMENT_TARGET=11.0 CARGO_INCREMENTAL=0 cargo build --manifest-path src-tauri/Cargo.toml --release --example native_timeline --features tauri/custom-protocol
mkdir -p .local-cache/timeline-production
ditto 'src-tauri/target/release/bundle/macos/Pixoma Studio.app' '.local-cache/timeline-production/Pixoma Timeline Acceptance.app'
cp src-tauri/target/release/examples/native_timeline '.local-cache/timeline-production/Pixoma Timeline Acceptance.app/Contents/MacOS/pixoma-studio'
codesign --force --deep --sign - '.local-cache/timeline-production/Pixoma Timeline Acceptance.app'
PIXOMA_STUDIO_DATA_DIR=/Users/mr9esx/Documents/PixomaStudio/.local-cache/real-video-20261031 PIXOMA_STUDIO_AGENT_LISTEN=127.0.0.1:19429 '.local-cache/timeline-production/Pixoma Timeline Acceptance.app/Contents/MacOS/pixoma-studio'
```

## Chat 与工作台宽度

- 左右区域使用已有 shadcn Resizable 组件，支持分隔线拖动、键盘调整和双击恢复默认宽度。
- 桌面端 Chat 默认 380px，最小 380px；右侧最小 400px。切换工作台标签保留当前宽度，双击分隔线恢复 380px。
- 右侧收起与展开保留编辑器挂载和 Chat 输入；窗口小于 1280px 时使用工作台 Sheet，Chat 使用完整可用宽度。
- 输入区参与正常布局，消息区域使用剩余高度；容器宽度小于 640px 时工具栏分为两行，模型选择与发送按钮保持同行。
- 视频编辑器在切换标签和收起面板时保留挂载，隐藏时暂停成片与资产媒体预览。
- 原生验收脚本：`src-tauri/examples/native_workspace_resize.js`，通过 `PIXOMA_LAYOUT_SCRIPT` 交给 `native_layout` 运行。验收使用真实会话和已有视频，未发送消息或调用视频模型。
- 原生实测：默认与最小 Chat 宽度 380px、键盘调整至 438.25px、右侧最小 400px、1000px 窗口内 Chat 966px；恢复 1440px 窗口后 Chat 保持 438.25px。双击恢复、四个标签切换、收起展开、草稿保留、输入区边界与媒体暂停检查通过。右侧最小宽度下四个标签与收起按钮的几何检查通过。
- 显示规则由会话页面、右侧开关、项目类型和选中标签决定。通用项目默认显示项目资产，视频项目默认显示视频编辑器；链接明确指定的面板优先。`VideoProject` 按会话保存，项目用于会话归属与资产共享。

## 项目类型

- 新建与编辑项目提供「通用 / 视频」选择，默认通用；既有项目迁移为通用。省略或传入 null 类型的新建请求使用通用，编辑请求省略类型时保留当前类型，非法类型在写入前拒绝。
- 通用项目支持共享资产、文档版本、Chat 引用、技能与专家、工作流和会话任务，也支持手动打开视频编辑器。
- 类型修改只更新项目元数据，保留全部会话、资产和视频工程；显式选择的工作区保持不变。桌面与移动窗口使用相同的路由面板。
- 原生验收脚本为 `src-tauri/examples/native_project_types.js`，使用真实创建和编辑表单、已有五个分镜以及项目文档。
- 验证结果：前端 26 项测试通过，Rust 集成测试 54 项通过、12 项需要外部服务的测试按配置跳过。原生表单创建、重复选择、修改类型、五个分镜保留、通用项目手动打开编辑器与移动面板同步全部通过；Chat 默认宽度保持 380px。在线任务维持 5 个，本地任务维持 2 个。

## 验收条件

1. 五个分镜均采用真实视频，总视频轨道为十五秒、450 帧；成片为 1280×720、30 FPS、H.264 与 AAC，完整解码通过。
2. 时间轴直接编辑字幕后能够自动保存，撤销和重做的结果与持久化项目一致。
3. 单分镜 Chat 能重新生成并采用 H3 视频，保留旧视频历史，其他四个分镜完整保持。
4. 在线工作流与 ComfyUI 工作流使用各自的参数定义；相同在线请求返回相同任务，参数冲突和非法参数在提交前拒绝。
5. 首尾帧分别作为可读取资产保存，版本冲突不会覆盖其他修改；导出保留字幕、视频声音和项目音频设置。
