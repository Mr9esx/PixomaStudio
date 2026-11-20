# H3 自建视频工作流

服务地址：`http://192.168.31.131:8188`。

`tea-h3-api.json` 使用服务器已安装的 MiniMax H3 FL2VA、Qwen3-VL 文本编码器、视频和音频 VAE，以及 H3 Turbo LoRA 与采样器。输入未提供图片时执行文生视频。

## 参数绑定

| 逻辑输入 | 节点 | 参数 | 默认值 |
| --- | --- | --- | --- |
| prompt | 6 | prompt | 晨雾茶园镜头 |
| width | 6 | width | 608 |
| height | 6 | height | 352 |
| length | 6 | length | 124 |
| seed | 8 | noise_seed | 2026103101 |

宽度、高度为 32 的整数倍。帧数符合 `17 × k + 5`，输出帧率为 24。124 帧约为 5.17 秒；时间轴使用其中三秒。当前尺寸适配服务器的显存与内存容量。

主要视频输出绑定节点 `15`，输出索引 `0`。源视频保留声音，成片剪辑可以单独调整音量。

`tea-h3-workflow.json` 为 Pixoma WorkflowDefinition。导入后选择对应计算节点；该文件未包含节点注册凭据。

## 真实生成验证

- 任务：`2f211d31-6976-40f3-8ccc-7a4fbbe253e2`。
- 服务状态：`success`，`completed=true`。
- 输出：`Pixoma/tea_h3_mountain_00001_.mp4`。
- 实际媒体：608 × 352，24 FPS，H.264 视频与 AAC 音频，5.167 秒。
- 服务器执行耗时：约 59.4 秒。
- 已使用 ffprobe 读取媒体信息，并使用 ffmpeg 完整解码，无错误。

实际视频保存在项目忽略目录 `.local-cache/video-generation/h3/`。任务与视频信息可以通过 ComfyUI `/history/{prompt_id}` 和 `/view` 接口查询。

## Studio 正式执行验证

- 工作流：`c432b87e-9e9e-48a5-8f9d-099b982f3872`，版本 `4b89f13f-b85b-4daa-b166-6a8f2de4c181`。
- 执行：`0922f047-23c7-43ec-8071-7c3dad01edc6`，状态 `succeeded`。
- 执行经过 Studio Store、独立 edge-agent、远程 ComfyUI、结果上传与资产登记。
- 视频：`1b52b76f-60fe-4477-993c-a57825ade700`，版本 `1c7267db-33ea-4446-9190-9e521116f49c`。
- 视频包含 608 × 352、24 FPS 的真实 H.264 画面和 AAC 音频，时长 5.167 秒；完整解码通过。
- 首尾帧已登记为项目图片资产，执行输出字段为 `first_frame`、`last_frame`；视频版本 metadata 保存两组资产引用。
- 尾帧使用内置 ffmpeg 对最后一秒解码并保留最终图片；与同一程序完整解码的最终图片 SHA-256 相同。

验收清单位于忽略目录 `.local-cache/real-video-20261031/h3-manifest.json`。`real_h3_acceptance` 示例负责注册与正式执行，`collect_video_boundaries` 示例负责已完成视频的首尾帧登记。

## 参考

- [ComfyUI H3 官方教程](https://docs.comfy.org/tutorials/video/minimax/minimax-h3)
- 服务器现有 Turbo 节点的 README 与示例工作流。
