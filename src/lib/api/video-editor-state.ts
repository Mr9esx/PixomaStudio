import { apiFetch } from './client'
import type { VideoEditorViewState } from '@/features/studio/video-editor-view'

export function getVideoEditorState(videoId: string) {
  return apiFetch<unknown>(`/api/v1/studio/video-projects/${encodeURIComponent(videoId)}/editor-state`)
}

export function saveVideoEditorState(input: { videoId: string; sessionId: string; state: VideoEditorViewState }) {
  return apiFetch<VideoEditorViewState>(`/api/v1/studio/video-projects/${encodeURIComponent(input.videoId)}/editor-state`, { method: 'PUT', body: JSON.stringify({ session_id: input.sessionId, state: input.state }) })
}
