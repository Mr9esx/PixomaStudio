import { apiFetch } from './client'
import { parseVideoExportTask, type VideoExportTask } from '@/features/studio/video-export-model'

const path = (videoId: string) => `/api/v1/studio/video-projects/${encodeURIComponent(videoId)}/exports`

export async function listVideoWorkExports(videoId: string) {
  const records = await apiFetch<unknown[]>(path(videoId))
  return records.map(record => parseVideoExportTask(undefined, record))
}

export async function getVideoWorkExport(videoId: string, requestId: string) {
  return parseVideoExportTask(undefined, await apiFetch<unknown>(`${path(videoId)}/${encodeURIComponent(requestId)}`))
}

export async function exportVideoWork(videoId: string, sessionId: string, task: VideoExportTask, registerOnly = false) {
  return parseVideoExportTask(undefined, await apiFetch<unknown>(path(videoId), { method: 'POST', body: JSON.stringify({ session_id: sessionId, request: task.request, project: task.project, source_version: task.sourceVersion, register_only: registerOnly }) }))
}
