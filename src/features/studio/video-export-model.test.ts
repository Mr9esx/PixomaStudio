import { expect, it } from 'vitest'
import { emptyVideoProject, teaVideoShots, updateVideoProjectSettings } from './video-editor-model'
import { applyVideoExportStatus, createVideoExportRequest, createVideoExportTask, parseVideoExportTask, restoreVideoExportTask, videoExportIssues, videoExportStatusSchema } from './video-export-model'

function recoveryTask() {
  const project = { ...emptyVideoProject(), shots: [{ ...teaVideoShots()[0], media: { assetId: 'source', versionId: 'fixed-source', url: '/source.mp4', duration: 3 } }] }
  return { ...createVideoExportTask({ project, request: createVideoExportRequest('session', 'recovery-request', project, project) }, 7), status: 'interrupted' as const, submitted: true }
}

it('查询状态恢复原请求的执行阶段和固定成片，保留作品快照', () => {
  const task = recoveryTask()
  const running = applyVideoExportStatus(task, 'recovery-request', { status: 'exporting' })
  expect(running.status).toBe('exporting')
  expect(running.request).toBe(task.request)
  expect(running.project).toBe(task.project)
  const completed = applyVideoExportStatus({ ...running, status: 'failed', error: '连接中断' }, 'recovery-request', { status: 'completed', result: { assetId: 'rendered', versionId: 'fixed-result' } })
  expect(completed).toMatchObject({ status: 'completed', submitted: true, sourceVersion: 7, result: { assetId: 'rendered', versionId: 'fixed-result' }, problems: [] })
  expect(completed.error).toBeUndefined()
  expect(task.status).toBe('interrupted')
})

it('旧请求响应和未提交请求不更新记录，完成结果保持固定', () => {
  const task = recoveryTask()
  expect(applyVideoExportStatus(task, 'different-request', { status: 'exporting' })).toBe(task)
  const unsubmitted = { ...task, submitted: false }
  expect(applyVideoExportStatus(unsubmitted, 'recovery-request', { status: 'exporting' })).toBe(unsubmitted)
  const completed = { ...task, status: 'completed' as const, result: { assetId: 'output', versionId: 'fixed' } }
  expect(applyVideoExportStatus(completed, 'recovery-request', { status: 'interrupted' })).toBe(completed)
  expect(applyVideoExportStatus(task, 'recovery-request', { status: 'interrupted' })).toBe(task)
})

it('无法确认结果时保留失败原因，拒绝缺失结果与未知服务端状态', () => {
  const failed = { ...recoveryTask(), status: 'failed' as const, error: '素材读取失败' }
  expect(applyVideoExportStatus(failed, 'recovery-request', { status: 'interrupted' })).toBe(failed)
  expect(() => videoExportStatusSchema.parse({ status: 'completed' })).toThrow()
  expect(() => videoExportStatusSchema.parse({ status: 'completed', result: { assetId: '', versionId: 'fixed' } })).toThrow()
  expect(() => videoExportStatusSchema.parse({ status: 'unknown' })).toThrow()
})

it('使用独立输出规格和固定素材版本生成请求，保持作品不变', () => {
  const project = { ...emptyVideoProject(), shots: [{ ...teaVideoShots()[0], duration: 1.1, media: { assetId: 'video', versionId: 'version-one', url: '/clip.mp4', duration: 5 } }] }
  const request = createVideoExportRequest('session', 'request', project, { name: '茶园.MP4', width: 1920, height: 1080, fps: 24 })
  expect(request).toMatchObject({ session_id: 'session', request_id: 'request', name: '茶园.mp4', width: 1920, height: 1080, fps: 24 })
  expect(request.shots[0]).toMatchObject({ asset_id: 'video', asset_version_id: 'version-one', duration: 26 / 24 })
  expect(project.fps).toBe(30)
  expect(project.shots[0].duration).toBe(1.1)
  expect(() => createVideoExportRequest('session', 'request', project, { ...project, name: '../茶园' })).toThrow()
})

it('导出问题保留分镜与音频对象标识，检查缺少素材和源范围', () => {
  const first = teaVideoShots()[0]
  const second = { ...teaVideoShots()[1], media: { assetId: 'video', versionId: 'version-one', url: '/clip.mp4', duration: 2 } }
  const project = { ...emptyVideoProject(), shots: [first, second], audio: [{ id: 'music', assetId: 'audio', versionId: 'audio-one', url: '/audio.wav', start: 0, duration: 10, sourceIn: 0, volume: 1, loop: true }] }
  expect(videoExportIssues(project)).toEqual([
    { kind: 'shot', id: first.id, message: '尚未采用视频' },
    { kind: 'shot', id: second.id, message: '剪辑范围超出源视频' },
    { kind: 'audio', id: 'music', message: '音频范围超出成片' },
  ])
  expect(() => createVideoExportRequest('session', 'request', project, project)).toThrow('尚未采用视频')
})

it('按输出帧率检查素材尾部，并定位超过字节限制的字幕', () => {
  const shot = { ...teaVideoShots()[0], duration: 1, sourceIn: .03, media: { assetId: 'video', versionId: 'version', url: '/video.mp4', duration: 1 } }
  expect(videoExportIssues({ ...emptyVideoProject(), fps: 60, shots: [shot] })).toEqual([{ kind: 'shot', id: shot.id, message: '剪辑范围超出源视频' }])
  expect(videoExportIssues({ ...emptyVideoProject(), shots: [{ ...shot, sourceIn: 0, subtitle: '字幕'.repeat(700) }] })).toEqual([{ kind: 'shot', id: shot.id, track: 'subtitle', message: '字幕内容超过 4096 字节' }])
})

it('保存独立导出请求，重新读取后保留原参数并将未完成状态标记为待恢复', () => {
  const project = { ...emptyVideoProject(), shots: [{ ...teaVideoShots()[0], duration: 1, media: { assetId: 'video', versionId: 'original', url: '/video.mp4', duration: 2 } }] }
  const request = createVideoExportRequest('session', 'request', project, project)
  const task = createVideoExportTask({ request, project }, 7)
  project.shots[0].subtitle = '后续编辑'
  request.shots[0].asset_version_id = 'changed'
  expect(task.project.shots[0].subtitle).toBe('')
  expect(task.request.shots[0].asset_version_id).toBe('original')
  const restored = restoreVideoExportTask('session', JSON.parse(JSON.stringify({ ...task, status: 'exporting', submitted: true })))
  expect(restored).toMatchObject({ status: 'interrupted', sourceVersion: 7, submitted: true })
  expect(restored.request).toEqual(task.request)
  expect(restored.project).toEqual(task.project)
})

it('恢复完成结果的固定版本，拒绝错误会话、缺失结果和改变的导出参数', () => {
  const project = { ...emptyVideoProject(), shots: [{ ...teaVideoShots()[0], duration: 1, media: { assetId: 'video', versionId: 'original', url: '/video.mp4', duration: 2 } }] }
  const task = createVideoExportTask({ request: createVideoExportRequest('session', 'request', project, project), project })
  const completed = { ...task, status: 'completed', submitted: true, result: { assetId: 'output', versionId: 'fixed' } }
  expect(restoreVideoExportTask('session', JSON.parse(JSON.stringify(completed)))).toEqual(completed)
  expect(() => restoreVideoExportTask('other-session', completed)).toThrow('会话')
  expect(() => restoreVideoExportTask('session', { ...completed, result: undefined })).toThrow('成片')
  expect(() => restoreVideoExportTask('session', { ...task, request: { ...task.request, fps: 24 } })).toThrow('导出参数')
  expect(() => restoreVideoExportTask('session', { ...task, project: { ...task.project, shots: [] } })).toThrow()
})

it('作品历史可以读取原对话的记录，并保留服务报告的运行状态', () => {
  const project = { ...emptyVideoProject(), shots: [{ ...teaVideoShots()[0], duration: 1, media: { assetId: 'video', versionId: 'original', url: '/video.mp4', duration: 2 } }] }
  const task = createVideoExportTask({ request: createVideoExportRequest('old-session', 'request', project, project), project })
  const running = { ...task, status: 'exporting', submitted: true, request: Object.fromEntries(Object.entries(task.request).reverse()) }
  expect(parseVideoExportTask(undefined, running).status).toBe('exporting')
  expect(restoreVideoExportTask(undefined, running).status).toBe('interrupted')
  expect(() => parseVideoExportTask('new-session', running)).toThrow('会话')
})

it('保存不同输出帧率的完整请求，并保留失败原因和对象定位', () => {
  const project = { ...emptyVideoProject(), shots: [{ ...teaVideoShots()[0], duration: 1.1, media: { assetId: 'video', versionId: 'original', url: '/video.mp4', duration: 2 } }] }
  for (const fps of [1, 24, 25, 30, 60]) {
    const settings = { name: '茶园.MP4', width: 1920, height: 1080, fps }
    const task = createVideoExportTask({ request: createVideoExportRequest('session', `request-${fps}`, project, settings), project: updateVideoProjectSettings(project, settings) })
    const failed = { ...task, status: 'failed', error: '素材版本已不存在', problems: [{ kind: 'shot', id: project.shots[0].id, message: '素材版本已不存在' }] }
    const restored = restoreVideoExportTask('session', JSON.parse(JSON.stringify(failed)))
    expect(restored).toEqual(failed)
    expect(JSON.stringify(restored.request)).toBe(JSON.stringify(task.request))
  }
})
