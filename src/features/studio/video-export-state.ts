import { queryOptions, skipToken, type QueryClient } from '@tanstack/react-query'
import { restoreVideoExportTask, type VideoExportTask } from './video-export-model'
import { exportVideoWork, listVideoWorkExports } from '@/lib/api/video-work-exports'

function readTask(sessionId: string, videoId?: string) {
  const saved = (videoId ? localStorage.getItem(`video-editor-export-work:${videoId}`) : null) ?? localStorage.getItem(`video-editor-export:${sessionId}`)
  const task = saved ? restoreVideoExportTask(videoId ? undefined : sessionId, JSON.parse(saved)) : null
  if (task && videoId) localStorage.setItem(`video-editor-export-work:${videoId}`, JSON.stringify(task))
  return task
}

export function videoExportTaskOptions(sessionId: string, videoId?: string) {
  return queryOptions<VideoExportTask | null>({
    queryKey: ['studio', 'video-export-task', videoId ? 'work' : 'session', videoId ?? sessionId],
    queryFn: videoId ? async ({ client }) => {
      const local = readTask(sessionId, videoId)
      const records = await listVideoWorkExports(videoId)
      if (local?.status === 'completed' && !records.some(record => record.request.request_id === local.request.request_id)) {
        records.unshift(await exportVideoWork(videoId, sessionId, local, true))
        void client.invalidateQueries({ queryKey: ['studio', 'video-export-history', videoId] })
      }
      const selected = local && local.status !== 'completed' ? records.find(record => record.request.request_id === local.request.request_id) ?? local : records[0] ?? local
      if (selected) localStorage.setItem(`video-editor-export-work:${videoId}`, JSON.stringify(selected))
      return selected
    } : skipToken,
    enabled: Boolean(videoId),
    gcTime: Infinity,
    staleTime: videoId ? 0 : Infinity,
    refetchOnWindowFocus: false,
    initialData: () => readTask(sessionId, videoId),
  })
}

export function storeVideoExportTask(client: QueryClient, task: VideoExportTask, videoId?: string) {
  const sessionId = task.request.session_id
  client.setQueryData(videoExportTaskOptions(sessionId, videoId).queryKey, task)
  localStorage.setItem(videoId ? `video-editor-export-work:${videoId}` : `video-editor-export:${sessionId}`, JSON.stringify(task))
}
