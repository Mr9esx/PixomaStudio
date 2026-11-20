import { z } from 'zod'
import type { WorkflowDefinition, WorkflowVersion } from '@/lib/api/workflows'
import { validateVideoProject, type VideoProject, type VideoShot } from './video-editor-model'
import { videoGenerationInputIssues } from './video-generation-inputs'
import { videoDescriptionSource } from './video-input-sources'

export const videoPromptDraftSchema = z.object({
  shotId: z.string().min(1), workflowId: z.string().min(1), versionId: z.string().min(1),
  inputKey: z.string(), text: z.string(), sourceScript: z.string(), baseline: z.unknown().optional(),
  sourceField: z.enum(['script', 'storyboard']).optional(),
  sourceRevision: z.number().int().positive().optional(),
})
export type VideoPromptDraft = z.infer<typeof videoPromptDraftSchema>

export function videoPromptDraftKey(shot: VideoShot) {
  return JSON.stringify([shot.id, shot.workflowId, shot.versionId])
}

export function videoPromptFields(definition: Pick<WorkflowDefinition, 'inputs' | 'input_schema'>) {
  const properties = definition.input_schema.properties as Record<string, { enum?: unknown[] }> | undefined
  return definition.inputs.filter(field => field.type === 'string' && !properties?.[field.key]?.enum)
}

export function createVideoPromptDraft(shot: VideoShot, workflow: WorkflowVersion): VideoPromptDraft {
  if (shot.workflowId !== workflow.workflow_id || shot.versionId !== workflow.id) throw new Error('工作流版本已改变')
  const { field: sourceField, text: content, revision: sourceRevision } = videoDescriptionSource(shot)
  if (!content.trim()) throw new Error('填写分镜画面描述后继续')
  const fields = videoPromptFields(workflow.definition)
  if (!fields.length) throw new Error('当前工作流没有可填写的文字参数')
  const inputKey = fields.some(field => field.key === 'prompt') ? 'prompt' : ''
  return { shotId: shot.id, workflowId: workflow.workflow_id, versionId: workflow.id, inputKey, text: content, sourceScript: content, sourceField, sourceRevision, baseline: structuredClone(shot.inputs[inputKey]) }
}

export function applyVideoPromptDraft(project: VideoProject, draft: VideoPromptDraft, workflow: WorkflowVersion): VideoProject {
  const shot = project.shots.find(shot => shot.id === draft.shotId)
  if (!shot) throw new Error('分镜已被删除')
  if (shot.workflowId !== draft.workflowId || shot.versionId !== draft.versionId || workflow.workflow_id !== draft.workflowId || workflow.id !== draft.versionId) throw new Error('工作流版本已改变，请重新选择目标')
  if (shot.generationRequest) throw new Error('恢复当前生成请求后再采用提示词')
  if (!videoPromptFields(workflow.definition).some(field => field.key === draft.inputKey)) throw new Error('选择文字参数')
  if (!draft.text.trim()) throw new Error('填写待采用提示词')
  const source = videoDescriptionSource(shot)
  if (source.field !== (draft.sourceField ?? 'script') || source.text !== draft.sourceScript || (draft.sourceRevision !== undefined && source.revision !== draft.sourceRevision)) throw new Error('分镜内容已更新，重新读取内容或确认保留输入')
  if (JSON.stringify(shot.inputs[draft.inputKey]) !== JSON.stringify(draft.baseline)) throw new Error('生成参数已更新，请确认保留输入后再采用')
  const inputs = { ...shot.inputs, [draft.inputKey]: draft.text }
  const inputSources = { ...shot.inputSources, [draft.inputKey]: { field: draft.sourceField ?? 'script', revision: source.revision, sourceText: draft.sourceScript, value: draft.text, edited: draft.text !== draft.sourceScript } }
  const issues = videoGenerationInputIssues(workflow.definition.inputs, inputs, undefined, workflow.definition.input_schema).filter(issue => issue.key === draft.inputKey)
  if (issues.length) throw new Error(issues.map(issue => issue.message).join('；'))
  const next = { ...project, shots: project.shots.map(item => item.id === shot.id ? { ...item, inputs, inputSources } : item) }
  validateVideoProject(next)
  return next
}
