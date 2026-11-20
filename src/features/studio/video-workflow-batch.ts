import type { WorkflowVersion } from '@/lib/api/workflows'
import { serializeVideoContent, validateVideoProject, type VideoProject, type VideoShot } from './video-editor-model'
import { chooseVideoShotWorkflow } from './video-input-sources'

export type VideoWorkflowBatchItem = { baseline: VideoShot; shot: VideoShot }

export function prepareVideoWorkflowBatch(project: VideoProject, ids: string[], workflow: WorkflowVersion): VideoWorkflowBatchItem[] {
  if (!ids.length) throw new Error('选择分镜')
  if (new Set(ids).size !== ids.length) throw new Error('分镜选择重复')
  if (ids.some(id => !project.shots.some(shot => shot.id === id))) throw new Error('选定分镜已删除')
  if (!workflow.definition.outputs.some(output => output.type === 'video')) throw new Error('选择能够生成视频的工作流')
  return project.shots.filter(shot => ids.includes(shot.id)).map(shot => {
    if (shot.generationRequest) throw new Error(`${shot.name}：恢复当前生成请求后继续`)
    return { baseline: structuredClone(shot), shot: structuredClone(chooseVideoShotWorkflow(shot, workflow)) }
  })
}

export function applyVideoWorkflowBatch(project: VideoProject, batch: VideoWorkflowBatchItem[]): VideoProject {
  if (!batch.length) throw new Error('选择分镜')
  if (new Set(batch.map(item => item.shot.id)).size !== batch.length) throw new Error('分镜选择重复')
  for (const item of batch) {
    if (item.shot.id !== item.baseline.id) throw new Error('分镜标识已改变')
    const current = project.shots.find(shot => shot.id === item.baseline.id)
    if (!current) throw new Error(`${item.baseline.name}已删除`)
    if (serializeVideoContent(current) !== serializeVideoContent(item.baseline)) throw new Error(`${current.name}已修改，请重新预览`)
  }
  const next = { ...project, shots: project.shots.map(shot => {
    const item = batch.find(item => item.shot.id === shot.id)
    return item ? { ...shot, workflowId: item.shot.workflowId, versionId: item.shot.versionId, inputs: structuredClone(item.shot.inputs), inputSources: structuredClone(item.shot.inputSources), workflowDrafts: structuredClone(item.shot.workflowDrafts), references: structuredClone(item.shot.references) } : shot
  }) }
  validateVideoProject(next)
  return next
}
