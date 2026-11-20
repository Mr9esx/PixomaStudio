import { serializeVideoContent, validateVideoProject, type VideoAssetReference, type VideoProject, type VideoReference, type VideoShot } from './video-editor-model'

export type VideoReferenceUpdateTarget = {
  shotId: string; shotName: string; reference: VideoReference; selected: boolean
  workflowId?: string; versionId?: string; input?: unknown; mapped: boolean
}

export function videoReferenceUpdateTargets(project: VideoProject, assetId: string, versionId: string): VideoReferenceUpdateTarget[] {
  return project.shots.flatMap(shot => (shot.references ?? []).filter(reference => reference.assetId === assetId).map(reference => {
    const input = reference.inputKey ? shot.inputs[reference.inputKey] : undefined
    const mapped = Boolean(reference.inputKey && serializeVideoContent(input) === serializeVideoContent({ asset_id: reference.assetId, asset_version_id: reference.versionId }))
    return structuredClone({ shotId: shot.id, shotName: shot.name, reference, selected: reference.versionId === versionId, workflowId: shot.workflowId, versionId: shot.versionId, input, mapped })
  }))
}

export function applyVideoReferenceVersion(project: VideoProject, targets: VideoReferenceUpdateTarget[], replacement: VideoAssetReference & { kind: VideoReference['kind'] }): VideoProject {
  if (!targets.length) throw new Error('选择需要更新的引用')
  const seen = new Set<string>()
  for (const target of targets) {
    const key = JSON.stringify([target.shotId, target.reference.id])
    if (seen.has(key)) throw new Error('引用选择重复')
    seen.add(key)
    const shot = project.shots.find(shot => shot.id === target.shotId)
    const reference = shot?.references?.find(reference => reference.id === target.reference.id)
    if (!shot || !reference) throw new Error(`${target.shotName}的分镜或引用已删除`)
    if (reference.assetId !== replacement.assetId || reference.kind !== replacement.kind) throw new Error('替换资产与引用不匹配')
    if (serializeVideoContent(reference) !== serializeVideoContent(target.reference) || (reference.inputKey && (shot.workflowId !== target.workflowId || shot.versionId !== target.versionId || serializeVideoContent(shot.inputs[reference.inputKey]) !== serializeVideoContent(target.input)))) throw new Error(`${shot.name}的引用或生成输入已修改，请重新选择范围`)
  }
  const shots = project.shots.map(shot => {
    let next: VideoShot = shot
    for (const target of targets.filter(target => target.shotId === shot.id)) {
      if (target.reference.versionId === replacement.versionId) continue
      const reference: VideoReference = { ...target.reference, ...replacement }
      for (const key of ['sourceShotId', 'sourceMediaVersionId', 'confirmedSourceVersionId', 'sourcePosition', 'sourceFrame', 'sourceFps', 'confirmedSourceFrame', 'confirmedSourceFps'] as const) delete reference[key]
      if (!target.mapped) delete reference.inputKey
      next = { ...next, references: next.references!.map(item => item.id === reference.id ? reference : item), inputs: reference.inputKey ? { ...next.inputs, [reference.inputKey]: { asset_id: replacement.assetId, asset_version_id: replacement.versionId } } : next.inputs }
    }
    return next
  })
  const result = { ...project, shots }
  validateVideoProject(result)
  return result
}
