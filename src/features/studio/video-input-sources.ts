import type { WorkflowVersion } from '@/lib/api/workflows'
import { workflowDefaultValues } from '@/features/cases/workflow-parameter-fields'
import type { VideoShot, VideoInputSources } from './video-editor-model'

export function videoDescriptionSource(shot: VideoShot) {
  const field = shot.storyboard?.trim() ? 'storyboard' : 'script'
  return { field, text: shot[field] ?? '', revision: shot.contentRevisions?.[field] ?? 1 } as const
}

export function videoDescriptionInputState(shot: VideoShot, key: string) {
  const source = shot.inputSources?.[key]
  if (!source) return undefined
  const current = videoDescriptionSource(shot)
  return { changed: source.field !== current.field || source.sourceText !== current.text || (source.revision !== undefined && source.revision !== current.revision), edited: source.edited || shot.inputs[key] !== source.value }
}

export function editVideoShotInput(shot: VideoShot, key: string, value: unknown): VideoShot {
  const inputs = { ...shot.inputs }
  if (value === undefined) delete inputs[key]
  else inputs[key] = value
  return { ...shot, inputs, ...(shot.inputSources?.[key] ? { inputSources: { ...shot.inputSources, [key]: { ...shot.inputSources[key], edited: true } } } : {}) }
}

export function chooseVideoShotWorkflow(shot: VideoShot, workflow: WorkflowVersion): VideoShot {
  if (shot.workflowId === workflow.workflow_id && shot.versionId === workflow.id) return shot
  if (shot.generationRequest) throw new Error('先恢复当前生成请求，再更换工作流')
  const key = `${workflow.workflow_id}:${workflow.id}`
  const draft = shot.workflowDrafts?.[key]
  const defaults = { ...workflowDefaultValues(workflow.definition), ...(workflow.definition.type === 'online' ? workflow.definition.execution.defaults : {}) }
  const initialInputs = Object.fromEntries(workflow.definition.inputs.flatMap(field => defaults[field.key] === undefined ? [] : [[field.key, defaults[field.key]]]))
  const inputs: Record<string, unknown> = structuredClone(draft?.inputs ?? initialInputs)
  const inputSources: VideoInputSources = structuredClone(draft?.inputSources ?? {})
  const source = videoDescriptionSource(shot)
  const properties = workflow.definition.input_schema.properties as Record<string, { enum?: unknown[] }> | undefined
  if (!draft && source.text.trim() && workflow.definition.inputs.some(field => field.key === 'prompt' && field.type === 'string') && !properties?.prompt?.enum) {
    inputs.prompt = source.text
    inputSources.prompt = { field: source.field, revision: source.revision, sourceText: source.text, value: source.text, edited: false }
  }
  const referenceInputs = Object.fromEntries((shot.references ?? []).flatMap(reference => reference.inputKey ? [[reference.id, reference.inputKey]] : []))
  return {
    ...shot, workflowId: workflow.workflow_id, versionId: workflow.id, inputs, inputSources,
    references: (shot.references ?? []).map(reference => ({ ...reference, inputKey: draft?.referenceInputs?.[reference.id] })),
    workflowDrafts: { ...shot.workflowDrafts, ...(shot.workflowId && shot.versionId ? {
      [`${shot.workflowId}:${shot.versionId}`]: structuredClone({ inputs: shot.inputs, inputSources: shot.inputSources, referenceInputs }),
    } : {}) },
  }
}
