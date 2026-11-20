import type { WorkflowVersion } from '@/lib/api/workflows'
import type { StudioAsset } from '@/lib/api/studio'
import { serializeVideoContent, type VideoProject, type VideoShot } from './video-editor-model'
import { chooseVideoShotWorkflow } from './video-input-sources'
import { videoGenerationInputIssues } from './video-generation-inputs'
import { applyVideoWorkflowBatch } from './video-workflow-batch'

export type VideoWorkflowVersionReview = { baseline: VideoShot; shot: VideoShot; restoredDraft: boolean; changes: Array<{ key: string; message: string }> }

export function prepareVideoWorkflowVersion(source: VideoShot, from: WorkflowVersion, to: WorkflowVersion, assets: StudioAsset[]): VideoWorkflowVersionReview {
  if (source.workflowId !== from.workflow_id || source.versionId !== from.id) throw new Error('分镜工作流版本已改变')
  if (to.workflow_id !== from.workflow_id) throw new Error('选择同一工作流的发布版本')
  if (to.id === from.id) throw new Error('选择其他发布版本')
  if (!to.definition.outputs.some(output => output.type === 'video')) throw new Error('目标版本没有视频输出')
  const restoredDraft = Boolean(source.workflowDrafts?.[`${to.workflow_id}:${to.id}`])
  const shot = chooseVideoShotWorkflow(source, to)
  const changes: VideoWorkflowVersionReview['changes'] = []
  const transferred = new Set<string>()
  for (const [key, value] of Object.entries(source.inputs)) {
    const previous = from.definition.inputs.find(field => field.key === key)
    const field = to.definition.inputs.find(field => field.key === key)
    if (!field) { changes.push({ key, message: '目标版本没有此参数' }); continue }
    if (restoredDraft) continue
    if (!previous || previous.type !== field.type) { changes.push({ key, message: '参数类型已改变' }); continue }
    const issues = videoGenerationInputIssues(to.definition.inputs, { ...shot.inputs, [key]: value }, assets, to.definition.input_schema).filter(issue => issue.key === key)
    if (issues.length) { changes.push({ key, message: issues.map(issue => issue.message).join('；') }); continue }
    shot.inputs[key] = structuredClone(value)
    if (source.inputSources?.[key]) shot.inputSources = { ...shot.inputSources, [key]: structuredClone(source.inputSources[key]) }
    else if (shot.inputSources?.[key]) delete shot.inputSources[key]
    transferred.add(key)
  }
  if (!restoredDraft) shot.references = shot.references?.map(reference => {
    const previous = source.references?.find(item => item.id === reference.id)
    return previous?.inputKey && transferred.has(previous.inputKey) ? { ...reference, inputKey: previous.inputKey } : reference
  })
  for (const field of to.definition.inputs) if (!changes.some(change => change.key === field.key) && serializeVideoContent(source.inputs[field.key]) !== serializeVideoContent(shot.inputs[field.key])) changes.push({ key: field.key, message: restoredDraft ? '使用此版本保存的参数草稿' : '使用目标版本的参数' })
  return { baseline: structuredClone(source), shot, changes, restoredDraft }
}

export function applyVideoWorkflowVersion(project: VideoProject, review: VideoWorkflowVersionReview) {
  return applyVideoWorkflowBatch(project, [review])
}
