import { apiFetch } from './client'
import type { VideoProject } from '@/features/studio/video-editor-model'
import type { VideoShot, VideoReference } from '@/features/studio/video-editor-model'
import type { WorkflowExecution, WorkflowInput } from './workflows'
import type { StudioAsset } from './studio'

export function createStudioVideoFrame(assetId: string, versionId: string, input: { session_id: string; frame: number; fps: number }) {
  return apiFetch<StudioAsset>(`/api/v1/studio/assets/${encodeURIComponent(assetId)}/versions/${encodeURIComponent(versionId)}/frame`, { method: 'POST', body: JSON.stringify(input) })
}

export type VideoGeneration = {
  id: string
  video_id: string
  shot_id: string
  session_id: string | null
  execution_id: string | null
  execution: WorkflowExecution | null
  snapshot: {
    legacy?: boolean
    project_version: number | null
    shot: Pick<VideoShot, 'id' | 'name'> & Partial<VideoShot>
    workflow_id: string
    workflow_name: string
    version_id: string
    version_name: string
    input_fields: WorkflowInput[]
    inputs: Record<string, unknown>
  }
  retry_of: string | null
  submission_error: string | null
  status: WorkflowExecution['status'] | 'submitting' | 'submission_failed' | 'waiting_dependencies' | 'waiting_output' | 'blocked_dependencies'
  dependency_state?: 'ready' | 'waiting' | 'waiting_output' | 'blocked' | 'cancelled'
  dependency_error?: string | null
  dependencies?: Array<{ source_generation_id: string; binding: VideoGenerationDependency }>
  created_at: string
}

const generationsPath = (id: string) => `/api/v1/studio/video-projects/${encodeURIComponent(id)}/generations`
export type VideoGenerationDependency = { shot_id: string; source_shot_id: string; input_key: string; position: 'first' | 'last' | 'fixed'; fps: number; frame?: number | null; output?: { field: string; index: number } | null }
export type VideoGenerationBatchRequest = { request_id: string; session_id: string; expected_version: number; shot_ids: string[]; dependencies?: VideoGenerationDependency[] }
export type VideoGenerationBatch = { id: string; video_id: string; session_id: string | null; request: VideoGenerationBatchRequest; created_at: string; generations: VideoGeneration[] }

export function createVideoGenerationBatch(videoId: string, input: VideoGenerationBatchRequest) {
  return apiFetch<VideoGenerationBatch>(`/api/v1/studio/video-projects/${encodeURIComponent(videoId)}/generation-batches`, { method: 'POST', body: JSON.stringify(input) })
}

export function listVideoGenerationBatches(videoId: string) {
  return apiFetch<VideoGenerationBatch[]>(`/api/v1/studio/video-projects/${encodeURIComponent(videoId)}/generation-batches`)
}

export function getVideoGenerationBatch(videoId: string, id: string) {
  return apiFetch<VideoGenerationBatch>(`/api/v1/studio/video-projects/${encodeURIComponent(videoId)}/generation-batches/${encodeURIComponent(id)}`)
}

export function listVideoGenerations(videoId: string) {
  return apiFetch<VideoGeneration[]>(generationsPath(videoId))
}

export function createVideoGeneration(videoId: string, input: { request_id: string; session_id: string; shot_id: string; expected_version: number }) {
  return apiFetch<VideoGeneration>(generationsPath(videoId), { method: 'POST', body: JSON.stringify(input) })
}

export function retryVideoGeneration(videoId: string, id: string, input: { request_id: string; session_id: string }) {
  return apiFetch<VideoGeneration>(`${generationsPath(videoId)}/${encodeURIComponent(id)}/retry`, { method: 'POST', body: JSON.stringify(input) })
}

export function videoGenerationPending(record: VideoGeneration) {
  return ['submitting', 'waiting_dependencies', 'waiting_output'].includes(record.status) || Boolean(record.execution?.waiting && !['succeeded', 'failed', 'cancelled', 'result_unknown'].includes(record.status))
}

export function cancelVideoDependencyWait(videoId: string, id: string, sessionId: string) {
  return apiFetch<VideoGeneration>(`${generationsPath(videoId)}/${encodeURIComponent(id)}/cancel-waiting`, { method: 'POST', body: JSON.stringify({ session_id: sessionId }) })
}

export function chooseVideoDependencyOutput(videoId: string, id: string, input: { session_id: string; input_key: string; source_generation_id: string; output: { field: string; index: number } }) {
  return apiFetch<VideoGeneration>(`${generationsPath(videoId)}/${encodeURIComponent(id)}/dependency-output`, { method: 'POST', body: JSON.stringify(input) })
}

export type StudioVideoWork = {
  id: string
  asset_id: string
  asset_version: number
  project: VideoProject
  main_session_id: string | null
  project_id: string | null
  materials: StudioVideoMaterial[]
  chats: Array<{ session_id: string; asset_id: string | null; title: string; material_id: string | null; selected_version_id: string | null }>
}

export type VideoReferenceLocation = { shot: string; reference: string }
export type AssetVideoUsage = { video_id: string; video_name: string; project_name: string; references: Array<{ shot_id: string; shot_name: string; reference: VideoReference; version: number }> }

export function getAssetVideoUsages(assetId: string) {
  return apiFetch<AssetVideoUsage[]>(`/api/v1/studio/assets/${encodeURIComponent(assetId)}/video-usages`)
}

export const videoMaterialKinds = { character: '人物', scene: '场景', style: '风格', voice: '声音', reference: '素材' } as const
export type StudioVideoMaterial = { id: string; name: string; kind: keyof typeof videoMaterialKinds; brief: string; asset_id: string | null; asset: StudioAsset | null; results: Array<{ source_session_id: string | null; source_session_title: string; source_asset_id: string; source_version_id: string; asset_version_id: string; created_at: string }> }
export type StudioVideoMaterialContext = { video_id: string; material_id: string; name: string; kind: keyof typeof videoMaterialKinds; brief: string; asset_id: string | null; asset_version: StudioAsset['versions'][number] | null; source: Record<string, unknown> }

export function getVideoMaterialContext(videoId: string, sessionId: string, versionId?: string) {
  return apiFetch<StudioVideoMaterialContext>(`/api/v1/studio/video-projects/${encodeURIComponent(videoId)}/chats/${encodeURIComponent(sessionId)}/material-context${versionId ? `?asset_version_id=${encodeURIComponent(versionId)}` : ''}`)
}

export function getVideoMaterialForAsset(assetId: string) {
  return apiFetch<StudioVideoMaterial | null>(`/api/v1/studio/video-materials?asset_id=${encodeURIComponent(assetId)}`)
}

export function adoptVideoMaterialResult(materialId: string, input: { request_id: string; session_id: string; asset_id: string; asset_version_id: string; expected_version: number }) {
  return apiFetch<StudioVideoMaterial>(`/api/v1/studio/video-materials/${encodeURIComponent(materialId)}/results`, { method: 'POST', body: JSON.stringify(input) })
}

export function getStudioVideoWork(id: string) {
  return apiFetch<StudioVideoWork>(`/api/v1/studio/video-projects/${encodeURIComponent(id)}`)
}

export function listStudioVideoWorks(projectId: string) {
  return apiFetch<Array<{ id: string; name: string; asset_version: number; main_session_id: string | null; shot_count: number }>>(`/api/v1/studio/video-projects?project_id=${encodeURIComponent(projectId)}`)
}

export function openStudioVideoWork(id: string) {
  return apiFetch<StudioVideoWork>(`/api/v1/studio/video-projects/${encodeURIComponent(id)}/open`, { method: 'POST' })
}

export function bindStudioVideoChat(videoId: string, sessionId: string, assetId?: string) {
  return apiFetch<StudioVideoWork>(`/api/v1/studio/video-projects/${encodeURIComponent(videoId)}/chats`, {
    method: 'POST', body: JSON.stringify({ session_id: sessionId, asset_id: assetId }),
  })
}

export function createStudioVideoMaterialChat(videoId: string, input: { request_id: string; title: string; asset_id?: string; asset_version_id?: string; material_id?: string; name?: string; kind?: keyof typeof videoMaterialKinds; brief?: string; shot_ids?: string[] }) {
  return apiFetch<{ session_id: string; material_id: string; work: StudioVideoWork }>(`/api/v1/studio/video-projects/${encodeURIComponent(videoId)}/material-chats`, {
    method: 'POST', body: JSON.stringify(input),
  })
}
