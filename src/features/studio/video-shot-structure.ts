import { duplicateVideoShot, serializeVideoContent, validateVideoProject, type VideoProject, type VideoShot } from './video-editor-model'

export function splitVideoShot(source: VideoShot, ids: string[]): VideoShot[] {
  if (ids.length < 2) throw new Error('至少创建两个新分镜')
  if (new Set(ids).size !== ids.length) throw new Error('新分镜标识重复')
  return ids.map((id, index) => ({ ...duplicateVideoShot(source, id), name: `${source.name} · ${index + 1}`, storyboard: index === 0 ? source.storyboard ?? '' : '', script: index === 0 ? source.script : '', subtitle: index === 0 ? source.subtitle : '', plannedDuration: undefined }))
}

export function mergeVideoShots(sources: VideoShot[], id: string, workflowShotId: string): VideoShot {
  if (sources.length < 2) throw new Error('至少选择两个分镜')
  const settings = sources.find(shot => shot.id === workflowShotId)
  if (!settings) throw new Error('选择生成设置的来源分镜')
  const references = new Map<string, NonNullable<VideoShot['references']>[number]>()
  for (const shot of [settings, ...sources.filter(shot => shot.id !== settings.id)]) {
    for (const original of shot.references ?? []) {
      const key = JSON.stringify([original.assetId, original.versionId, original.role])
      if (references.has(key)) continue
      const reference = { ...structuredClone(original), id: `${id}:reference:${references.size + 1}` }
      if (reference.inputKey && serializeVideoContent(settings.inputs[reference.inputKey]) !== serializeVideoContent({ asset_id: reference.assetId, asset_version_id: reference.versionId })) delete reference.inputKey
      references.set(key, reference)
    }
  }
  const sourcesByBlock = new Map<string, NonNullable<VideoShot['scriptSources']>[number]>()
  for (const shot of sources) for (const source of shot.scriptSources ?? []) if (!sourcesByBlock.has(source.blockId)) sourcesByBlock.set(source.blockId, structuredClone(source))
  const workflowDrafts = settings.workflowDrafts ? Object.fromEntries(Object.entries(settings.workflowDrafts).map(([key, draft]) => [key, {
    ...structuredClone(draft), ...(draft.referenceInputs ? { referenceInputs: Object.fromEntries(Object.entries(draft.referenceInputs).flatMap(([referenceId, inputKey]) => {
      const original = settings.references?.find(reference => reference.id === referenceId)
      const reference = original ? references.get(JSON.stringify([original.assetId, original.versionId, original.role])) : undefined
      return reference ? [[reference.id, inputKey]] : []
    })) } : {}),
  }])) : undefined
  return { ...duplicateVideoShot(settings, id), workflowDrafts, name: sources.map(shot => shot.name).join(' / '), storyboard: sources.map(shot => shot.storyboard?.trim()).filter(Boolean).join('\n\n'), script: sources.map(shot => shot.script.trim()).filter(Boolean).join('\n\n'), subtitle: sources.map(shot => shot.subtitle.trim()).filter(Boolean).join('\n'), plannedDuration: undefined, references: [...references.values()], scriptSources: [...sourcesByBlock.values()] }
}

export function applyVideoShotStructure(project: VideoProject, sources: VideoShot[], replacements: VideoShot[]): VideoProject {
  if (!((sources.length === 1 && replacements.length >= 2) || (sources.length >= 2 && replacements.length === 1))) throw new Error('分镜结构操作范围无效')
  const sourceIds = new Set(sources.map(shot => shot.id))
  if (sourceIds.size !== sources.length) throw new Error('来源分镜重复')
  const index = project.shots.findIndex(shot => shot.id === sources[0].id)
  if (index < 0 || sources.some(source => !project.shots.some(shot => shot.id === source.id))) throw new Error('来源分镜已删除')
  if (sources.some((source, at) => project.shots[index + at]?.id !== source.id)) throw new Error('选择顺序连续的分镜，当前顺序已改变')
  if (sources.some((source, at) => serializeVideoContent(project.shots[index + at]) !== serializeVideoContent(source))) throw new Error('来源分镜已修改，请重新预览')
  for (const shot of replacements) {
    if (!shot.id || project.shots.some(source => source.id === shot.id)) throw new Error('结构操作需要新标识')
    if (shot.media || shot.executionId || shot.generationRequest || shot.executionInputs || shot.versions?.length || shot.duration || shot.sourceIn) throw new Error('新分镜需要独立选择视频和生成任务')
    for (const role of ['storyboard', 'first_frame', 'last_frame']) if ((shot.references ?? []).filter(reference => reference.role === role).length > 1) throw new Error('每个新分镜只能选择一个分镜画面、首帧或尾帧参考')
  }
  const next = { ...project, shots: [...project.shots.slice(0, index), ...structuredClone(replacements), ...project.shots.slice(index + sources.length)] }
  validateVideoProject(next)
  return next
}
