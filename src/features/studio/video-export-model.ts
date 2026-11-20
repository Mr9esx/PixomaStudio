import { z } from 'zod'
import { serializeVideoContent, updateVideoProjectSettings, validateVideoProject, videoProjectSettingsSchema, videoProjectTiming, type VideoProject } from './video-editor-model'

export const videoExportSettingsSchema = videoProjectSettingsSchema.extend({
  name: videoProjectSettingsSchema.shape.name.refine(value => !/[<>:"/\\|?*\u0000-\u001f]/.test(value) && Boolean(value.replace(/\.mp4$/i, '').trim()), '填写有效文件名称，不包含路径或特殊字符'),
})
export type VideoExportIssue = { kind: 'project' | 'shot' | 'audio'; id: string; message: string; track?: 'video' | 'subtitle' }
export type VideoExportSnapshot = { id: string; project: VideoProject; sourceVersion?: number }

export function videoExportIssues(project: VideoProject): VideoExportIssue[] {
  const issues: VideoExportIssue[] = []
  const timing = videoProjectTiming(project)
  if (!project.shots.length) issues.push({ kind: 'project', id: '', message: '添加分镜后导出' })
  for (const shot of project.shots) {
    if (!shot.media) issues.push({ kind: 'shot', id: shot.id, message: '尚未采用视频' })
    else if (shot.media.duration !== undefined && shot.sourceIn + shot.duration > shot.media.duration + 1 / project.fps) issues.push({ kind: 'shot', id: shot.id, message: '剪辑范围超出源视频' })
    if (new TextEncoder().encode(shot.subtitle).length > 4096) issues.push({ kind: 'shot', id: shot.id, track: 'subtitle', message: '字幕内容超过 4096 字节' })
  }
  for (const audio of project.audio ?? []) {
    if (!timing.unknownCount && audio.start + audio.duration > timing.knownDuration + 1e-6) issues.push({ kind: 'audio', id: audio.id, message: '音频范围超出成片' })
    else if (audio.sourceDuration !== undefined && (audio.sourceIn >= audio.sourceDuration || (!audio.loop && audio.sourceIn + audio.duration > audio.sourceDuration + .05))) issues.push({ kind: 'audio', id: audio.id, message: '音频剪辑范围超出素材' })
  }
  return issues
}

export function createVideoExportRequest(sessionId: string, requestId: string, project: VideoProject, settings: Pick<VideoProject, 'name' | 'width' | 'height' | 'fps'>) {
  const issues = videoExportIssues(project)
  if (issues.length) throw new Error(issues[0].message)
  const parsed = videoExportSettingsSchema.parse(settings)
  const output = updateVideoProjectSettings(project, parsed)
  const outputIssues = videoExportIssues(output)
  if (outputIssues.length) throw new Error(outputIssues[0].message)
  return {
    session_id: sessionId, request_id: requestId, name: `${parsed.name.replace(/\.mp4$/i, '').trim()}.mp4`,
    width: output.width, height: output.height, fps: output.fps,
    shots: output.shots.map(shot => ({ asset_id: shot.media!.assetId, asset_version_id: shot.media!.versionId, source_in: shot.sourceIn, duration: shot.duration, subtitle: shot.subtitle, volume: shot.volume ?? 1 })),
    audio: output.audio?.map(audio => ({ asset_id: audio.assetId, asset_version_id: audio.versionId, start: audio.start, duration: audio.duration, source_in: audio.sourceIn, volume: audio.volume, loop: audio.loop })) ?? [],
  }
}
export type VideoExportRequest = ReturnType<typeof createVideoExportRequest>
export type VideoExportSubmission = { request: VideoExportRequest; project: VideoProject }

const exportTaskSchema = z.object({
  version: z.literal(1),
  request: z.object({ session_id: z.string().min(1), request_id: z.string().min(1) }).passthrough(),
  project: z.unknown().transform(value => { validateVideoProject(value as VideoProject); return value as VideoProject }),
  sourceVersion: z.number().int().nonnegative().optional(),
  status: z.enum(['checking', 'saving', 'exporting', 'failed', 'interrupted', 'completed']),
  submitted: z.boolean(),
  result: z.object({ assetId: z.string().min(1), versionId: z.string().min(1) }).optional(),
  error: z.string().optional(),
  problems: z.array(z.object({ kind: z.enum(['project', 'shot', 'audio']), id: z.string(), message: z.string(), track: z.enum(['video', 'subtitle']).optional() })),
})
export type VideoExportTask = Omit<z.infer<typeof exportTaskSchema>, 'request'> & { request: VideoExportRequest }

export const videoExportStatusSchema = z.discriminatedUnion('status', [
  z.object({ status: z.literal('exporting') }),
  z.object({ status: z.literal('interrupted') }),
  z.object({ status: z.literal('completed'), result: z.object({ assetId: z.string().min(1), versionId: z.string().min(1) }) }),
])

export function applyVideoExportStatus(task: VideoExportTask, requestId: string, status: z.infer<typeof videoExportStatusSchema>): VideoExportTask {
  if (task.request.request_id !== requestId || !task.submitted || task.status === 'completed') return task
  if (status.status === 'interrupted' && task.status === 'failed') return task
  if (status.status === task.status) return task
  return status.status === 'completed'
    ? { ...task, status: 'completed', result: status.result, error: undefined, problems: [] }
    : { ...task, status: status.status, error: undefined, problems: [] }
}

export const videoExportPhase = {
  checking: '正在检查素材', saving: '正在保存作品', exporting: '正在生成成片',
  failed: '导出失败', interrupted: '导出结果待确认', completed: '成片已生成',
} satisfies Record<VideoExportTask['status'], string>

export function videoExportPending(task?: VideoExportTask | null) {
  return Boolean(task && ['checking', 'saving', 'exporting'].includes(task.status))
}

export function createVideoExportTask(submission: VideoExportSubmission, sourceVersion?: number): VideoExportTask {
  const task = restoreVideoExportTask(submission.request.session_id, {
    version: 1, ...structuredClone(submission), sourceVersion, status: 'checking', submitted: false, problems: [],
  })
  return { ...task, status: 'checking' }
}

export function parseVideoExportTask(sessionId: string | undefined, saved: unknown): VideoExportTask {
  const task = exportTaskSchema.parse(saved)
  if (sessionId !== undefined && task.request.session_id !== sessionId) throw new Error('导出记录不属于当前会话')
  const request = createVideoExportRequest(task.request.session_id, task.request.request_id, task.project, task.project)
  if (serializeVideoContent(task.request) !== serializeVideoContent(request)) throw new Error('导出参数与保存的作品内容不一致')
  if (task.status === 'completed' && (!task.result || !task.submitted)) throw new Error('成片结果不完整')
  return { ...task, request }
}

export function restoreVideoExportTask(sessionId: string | undefined, saved: unknown): VideoExportTask {
  const task = parseVideoExportTask(sessionId, saved)
  return { ...task, status: ['checking', 'saving', 'exporting'].includes(task.status) ? 'interrupted' : task.status }
}
