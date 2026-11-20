# 视频时间轴组件接入计划

执行方式：当前工作区，主会话顺序执行，使用 executing-plans 与 TDD。

目标：使用 @xzdarcy/react-timeline-editor 1.0.0 接管时间轴交互，保留紧凑布局、现有编辑功能、分镜联动与 Chat 保存同步。仅使用已有视频验证。

设计：VideoProject 为唯一持久化数据；组件视图由项目生成，所有编辑回调转为项目操作。Remotion Player 提供播放时间，时间轴拖动游标通过 seekTo 定位。时间按项目帧率取整，同一分镜的剧本、分镜、参考、视频和字幕共享边界。

约束：保留已有修改；使用语义令牌与现有 shadcn 控件；不新增字节模型调用、不使用模拟服务、不使用截图验证；中间文件使用已忽略的 .local-cache。

- [x] 编写时间轴数据与编辑规则测试，确认失败；实现固定轨道映射、重排、左右裁剪、音频移动与裁剪，验证帧间隔、媒体边界和输入不被修改。
  - 文件：src/features/studio/video-timeline-model.ts、video-timeline-model.test.ts。
- [x] 接入库组件，复用现有播放、属性、文字直接编辑、撤销重做和保存逻辑；替换自定义时间尺与指针处理。
  - 文件：src/features/studio/video-timeline.tsx、video-editor.tsx、video-editor.css、package.json、pnpm-lock.yaml。
  - 验证：pnpm test、pnpm typecheck、pnpm build；依赖源码与 React 19 运行错误必须核实处理。
- [x] 用已有五个分镜执行原生 Tauri DOM 验收：库组件挂载、真实视频播放、游标同步、双边裁剪、分镜联动与重排、音频编辑、直接文字编辑、撤销重做、外部项目修改与冲突保护、Chat 引用、保存恢复和导出。
  - 文件：src-tauri/examples/native_timeline.rs、native_timeline.js、docs/video-generation/verification.md。
  - 记录真实项目与资产编号、修改前后边界和导出媒体信息，核对在线生成任务数量未增加；完整构建与签名检查。

完成验证：26 项前端测试、类型检查、生产构建与 macOS 签名检查通过。React 19.3.1、Tauri 2.12.4 的生产资源协议验收通过，覆盖单分镜与整片真实 Chat、连续键盘裁剪、并发文字冲突和外部删除分镜。已有五段视频导出为十五秒、450 帧，完整解码通过。online_executions 保持五项，local_executions 保持两项，本次视频生成调用为零。
