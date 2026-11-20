---
name: pixoma-studio-video-creator
description: 在 PixomaStudio 中通过对话创作完整脚本、生成分镜提案、准备角色等参考素材、配置工作流并生成视频。用户要求创作视频、修改脚本、生成或调整分镜、更新角色参考、重新生成分镜视频时使用。根据当前请求选择对应操作，保存可供编辑器查看与采用的内容。
---

# PixomaStudioVideoCreator

## 读取作品与本次目标

视频创作请求调用 `studio_read_video_project`，读取完整脚本、分镜、参考版本、工作流配置、候选视频和最新 `asset_version`。素材创作请求根据消息的素材对象上下文调用 `studio_read_video_material`，读取固定素材及创作要求。以消息中的作品、素材或分镜标识选择目标。对话中的历史要求用于理解内容，执行范围以当前请求为准。

信息足够时直接创作。缺少会改变角色、剧情或目标作品的必要信息时，提出具体问题。使用现有角色名称与设定，参考素材需要读取后才能据此创作。每次调用写入工具前读取最新版本；出现版本冲突时重新读取，保留生成内容并检查目标变化。

## 创作完整脚本

`scope=script` 或明确的脚本创作请求使用 `studio_propose_video_script`：

- `expected_version` 使用读取结果中的 `asset_version`。
- `blocks` 是有序文本块，每项包含 `id`、`text`。保留原有文本块标识，新增内容提供新的唯一标识。
- 分开或合并原文时，为新文本块填写 `sourceBlockIds`，引用当前脚本中的来源标识，并从完整 `blocks` 列表移除被替换的原文本块。服务保存来源关系，分镜仍保留采用时的原文，供用户重新关联。
- `text` 描述具体人物行为、情节、环境变化及必要对白。尊重用户指定的受众、风格和表达限制。
- 返回的是待采用提案。工具成功后说明提案可在编辑器查看；只有用户采用后，脚本正文才发生变化。
- 脚本请求的执行范围到提案保存为止。

## 根据脚本生成分镜

`scope=storyboard` 或明确的分镜请求使用 `studio_propose_video_storyboard`：

- 按完整脚本安排画面与动作衔接。每个分镜描述主要人物、场景、动作及必要镜头变化。
- `changes` 每项包含 `shotId`、`action`、`name`、`storyboard`、`sourceBlockIds`。
- 修改已有分镜使用 `action=update` 和原 `shotId`。新增分镜使用 `action=add` 和新标识，列表顺序确定新增顺序。
- 删除建议使用 `action=remove` 和原 `shotId`，填写原名称、空描述与空 `sourceBlockIds`。服务保存原分镜快照，用户勾选后才移除；媒体和生成记录保留。
- 调整顺序时提供 `order`，列出建议修改后的全部分镜标识，包含新增并排除删除。仅调整顺序时 `changes` 为空；编辑器单独展示顺序建议，用户明确采用。指定局部范围时，保留范围之外的分镜顺序与位置。
- `sourceBlockIds` 指向本次请求的完整脚本文本块。只使用存在的标识；同一分镜可以关联多个文本块。
- 消息指定分镜范围时只提交这些分镜的修改。
- 系统保存请求时的原文和分镜内容。用户可以逐项采用；期间内容变化会在采用时提示冲突。
- 时长只依据用户计划或媒体数据表达。内容修改仅提交名称、画面描述与原文关联。
- 新分镜尚未采用视频时，`duration` 为 `0`。用户指定的计划使用 `plannedDuration`，清除计划使用 `null`。采用视频的 `duration` 与工作流 `inputs` 中的生成时长分别保存；调整其中一项保留其他时间设置。

## 准备参考素材

读取现有资产及版本，区分角色、场景、风格、声音、首帧和尾帧等用途。引用素材时使用明确的资产标识与版本标识。用户可在编辑器的“素材创作对话”入口继续或新建素材对话。

素材生成请求需要查询当前工作流及其输入要求。长时间的角色创作保存在素材对话，作品引用具体采用版本。新素材生成成功后说明资产名称与版本，由用户选择应用分镜。一次更新多个分镜需要明确指定范围。

素材对话上下文包含 `material_id`、名称、类型、创作要求、本次固定资产版本和来源分镜。围绕该对象继续创作。调用 `studio_read_video_material` 查看已有版本和结果来源；引用版本使用当前消息中的标识。

用户明确采用当前素材对话的结果时，调用 `studio_adopt_video_material_result`，提供 `material_id`、结果的 `asset_id`、`asset_version_id` 和素材当前 `expected_version`；尚无资产时使用 `0`。服务登记新版本并保留旧版本。编辑器通过“用于当前分镜”或“选择素材版本与范围”应用到作品；采用素材结果后继续保留原有视频引用，直到用户指定应用范围。

## 配置工作流并生成视频

仅在请求明确包含视频生成时执行：

1. 调用 `studio_list_workflows` 与 `studio_get_workflow` 读取真实工作流版本、输入类型和必要参数。
2. 根据分镜画面描述与已采用参考准备输入。资产输入使用实际资产版本。保留用户已经手工修改的参数；修改时说明涉及字段。
3. 使用 `studio_update_video_shot` 保存明确指定分镜的工作流版本与输入。只提交需要修改的字段，完整 `inputs` 保留未修改参数。
4. 调用 `studio_generate_video_shot`，提供作品 `video_id`、明确的 `shot_id` 和保存后的 `expected_version`。服务保存本次输入后提交任务。使用 `studio_list_video_generations` 查询真实状态、错误与输出，保留生成记录标识和执行标识。
5. 输出视频作为候选版本。报告实际生成的资产与版本；生成成功与采用成功分别确认。

使用当前设置重新生成时，读取并保存当前分镜输入，调用 `studio_generate_video_shot`。沿用原输入重试时，查询原生成记录并调用 `studio_retry_video_generation`，提供 `video_id` 与 `generation_id`。分镜已经删除时，结果仍保存在生成记录和资产中，保留当前分镜列表。原任务结果未知时，明确说明再次生成会创建新任务，按用户指定动作执行。

一次生成多个分镜时，使用 `studio_generate_video_batch`，提供作品 `video_id`、最新 `expected_version` 与明确的 `shot_ids`。提交前逐个核实工作流版本和实际输入。批次固定全部选定输入，使用 `studio_list_video_generation_batches` 查看原批次进度；提交状态尚未确认时查询记录。

需要本次其他分镜的生成画面时，在批次 `dependencies` 中指定目标 `shot_id`、来源 `source_shot_id`、图片参数 `input_key`、画面 `position` 与提取 `fps`。`position` 使用 `first`、`last` 或 `fixed`；指定画面时提供 `frame`，帧序号从零开始。来源和目标同时选入批次。多个上游视频尚未明确选择时，读取真实输出，由用户选择后调用 `studio_choose_video_dependency_output`，提供下游 `generation_id`、`source_generation_id`、`input_key` 与输出的 `field`、`index`。用户取消等待时调用 `studio_cancel_video_dependency_wait`，保留已采用视频及历史记录。

使用已有分镜画面时，核实固定媒体版本和画面位置；来源修改后的参考更新通过编辑器明确采用。

## 采用与导出

优先通过编辑器采用窗口查看候选、使用范围和时间变化。用户明确要求通过对话采用时，使用 `studio_apply_video_result`，提供准确 `shot_id`、`execution_id`、`asset_version_id`、`source_in`、`duration` 和最新 `expected_version`。范围需要符合帧间隔和媒体实际时长。

采用后读取作品确认目标分镜的媒体版本和剪辑范围。导出仅在用户提出导出请求后调用 `studio_export_video`，检查实际返回的结果资产及 `asset_version_id`。使用 `studio_list_video_exports` 读取固定来源快照、修订和导出历史；导出期间的新编辑保留在当前作品中。工具失败时报告具体原因与可保留的创作内容；工具成功前不能宣称视频已生成、已采用或已导出。
