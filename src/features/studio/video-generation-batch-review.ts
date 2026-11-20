import type { WorkflowDefinition, WorkflowVersion } from '@/lib/api/workflows'
import type { GenerationConnection } from '@/lib/api/generation'
import { z } from 'zod'
import type { StudioAsset } from '@/lib/api/studio'
import type { VideoGenerationDependency } from '@/lib/api/video-projects'
import { workflowDefaultValues } from '@/features/cases/workflow-parameter-fields'
import { serializeVideoContent, videoReferenceSourceChanged, type VideoProject, type VideoShot } from './video-editor-model'
import { videoGenerationInputIssues } from './video-generation-inputs'

export const videoGenerationBatchPendingSchema = z.object({ videoId: z.string().min(1), body: z.object({ request_id: z.string().min(1), session_id: z.string().min(1), expected_version: z.number().int().nonnegative(), shot_ids: z.array(z.string().min(1)).min(1).max(100), dependencies: z.array(z.object({ shot_id: z.string().min(1), source_shot_id: z.string().min(1), input_key: z.string().min(1), position: z.enum(['first', 'last', 'fixed']), fps: z.number().int().min(1).max(60), frame: z.number().int().nonnegative().nullish(), output: z.object({ field: z.string().min(1), index: z.number().int().nonnegative() }).nullish() })).optional() }) })
export type VideoGenerationBatchPending = z.infer<typeof videoGenerationBatchPendingSchema>

export function videoGenerationConnectionIssue(definition: WorkflowDefinition | undefined, connections: GenerationConnection[] | undefined) {
  if (definition?.type !== 'online') return
  if (!connections) return '正在读取生成服务连接'
  const connection = connections.find(item => item.id === definition.execution.connection_id)
  if (!connection) return '配置生成服务连接'
  if (connection.provider !== definition.execution.provider) return '服务连接与工作流类型不一致'
  if (!connection.has_api_key && (definition.execution.provider !== 'http_json' || definition.execution.http?.authentication !== 'none')) return '配置生成服务 API Key'
}

export function reviewVideoGenerationShot(project: VideoProject, shot: VideoShot, workflow: WorkflowVersion, assets: StudioAsset[], dependencies: VideoGenerationDependency[] = []) {
  const inputs = { ...workflowDefaultValues(workflow.definition), ...shot.inputs }
  const keys = dependencies.filter(dependency => dependency.shot_id === shot.id).map(dependency => dependency.input_key)
  const schema = structuredClone(workflow.definition.input_schema)
  if (Array.isArray(schema.required)) schema.required = schema.required.filter(key => !keys.includes(String(key)))
  for (const key of keys) delete inputs[key]
  const issues: string[] = []
  if (shot.workflowId !== workflow.workflow_id || shot.versionId !== workflow.id) issues.push('工作流版本已改变')
  if (shot.generationRequest) issues.push('恢复当前生成请求后继续')
  if (!workflow.definition.outputs.some(output => output.type === 'video')) issues.push('工作流没有视频输出')
  if (shot.references?.some(reference => !keys.includes(reference.inputKey ?? '') && videoReferenceSourceChanged(reference, project.shots.find(source => source.id === reference.sourceShotId), project.fps))) issues.push('确认来源画面更新')
  issues.push(...videoGenerationInputIssues(workflow.definition.inputs.filter(field => !keys.includes(field.key)), inputs, assets, schema).map(issue => `${issue.key ? `${workflow.definition.inputs.find(field => field.key === issue.key)?.description || issue.key}：` : ''}${issue.message}`))
  return { inputs, issues }
}

export function videoGenerationDependencyPlan(project: VideoProject, ids: string[], dependencies: VideoGenerationDependency[]) {
  const issues: string[] = []
  const name = (id: string) => project.shots.find(shot => shot.id === id)?.name ?? id
  const keys = new Set<string>()
  for (const dependency of dependencies) {
    const key = JSON.stringify([dependency.shot_id, dependency.input_key])
    if (keys.has(key)) issues.push(`${name(dependency.shot_id)}的参数依赖重复`)
    keys.add(key)
    if (!ids.includes(dependency.shot_id) || !ids.includes(dependency.source_shot_id)) issues.push(`将来源分镜「${name(dependency.source_shot_id)}」与目标分镜同时选入批次`)
    if (!Number.isInteger(dependency.fps) || dependency.fps < 1 || dependency.fps > 60 || !['first', 'last', 'fixed'].includes(dependency.position)) issues.push(`${name(dependency.shot_id)}的画面位置或帧率无效`)
    if (dependency.position === 'fixed' && (!Number.isSafeInteger(dependency.frame) || dependency.frame! < 0)) issues.push(`${name(dependency.shot_id)}需要指定有效的画面帧位置`)
  }
  let remaining = project.shots.filter(shot => ids.includes(shot.id)).map(shot => shot.id)
  const order: string[] = []
  while (remaining.length) {
    const ready = remaining.filter(id => !dependencies.some(dependency => dependency.shot_id === id && remaining.includes(dependency.source_shot_id)))
    if (!ready.length) { issues.push(`分镜依赖存在循环：${remaining.map(name).join('、')}`); break }
    order.push(...ready); remaining = remaining.filter(id => !ready.includes(id))
  }
  return { order, issues }
}

export function validateVideoGenerationBatchSelection(baseline: VideoProject, current: VideoProject, ids: string[], dependencies: VideoGenerationDependency[] = []) {
  if (!ids.length) throw new Error('选择分镜')
  if (new Set(ids).size !== ids.length) throw new Error('分镜选择重复')
  if (baseline.fps !== current.fps || baseline.width !== current.width || baseline.height !== current.height) throw new Error('项目规格已改变，请重新确认')
  const plan = videoGenerationDependencyPlan(baseline, ids, dependencies)
  if (plan.issues.length) throw new Error(plan.issues.join('；'))
  for (const id of ids) {
    const original = baseline.shots.find(shot => shot.id === id), shot = current.shots.find(shot => shot.id === id)
    if (!original || !shot) throw new Error('选定分镜已删除')
    if (serializeVideoContent(original) !== serializeVideoContent(shot)) throw new Error(`${shot.name}已修改，请重新确认`)
    if (shot.references?.some(reference => !dependencies.some(dependency => dependency.shot_id === shot.id && dependency.input_key === reference.inputKey) && videoReferenceSourceChanged(reference, current.shots.find(source => source.id === reference.sourceShotId), current.fps))) throw new Error(`${shot.name}的来源画面已更新，请重新确认`)
  }
}
