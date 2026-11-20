import type { StudioRun } from '@/lib/api/studio'

export type VideoScriptRequest = {
  id: string
  sessionId: string
  label: string
  status: 'queued' | 'submitting' | 'submitted' | 'failed' | 'cancelled'
  runId?: string
  messageId?: string
  clientMessageId?: string
  error?: string
}

export function videoScriptRequestStatus(request: VideoScriptRequest, run?: Pick<StudioRun, 'status'>) {
  if (run) return ({ queued: '等待生成', running: '正在生成', waiting_approval: '等待批准', waiting_clarification: '等待补充信息', waiting_workflow: '等待工作流', succeeded: '对话已完成', failed: '生成失败', cancelled: '已取消' })[run.status]
  return ({ queued: '排队中', submitting: '正在提交', submitted: '正在读取进度', failed: '提交失败', cancelled: '已取消' })[request.status]
}

export function updateVideoScriptRequests(requests: VideoScriptRequest[], update: VideoScriptRequest) {
  const existing = requests.find(request => request.id === update.id && request.sessionId === update.sessionId)
  if (!existing) return [...requests, update]
  return requests.map(request => request === existing ? { ...request, ...update } : request)
}
