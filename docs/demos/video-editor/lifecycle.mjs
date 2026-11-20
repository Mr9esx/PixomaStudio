export const shotPreviewStates = [
  ['empty', '尚未填写'], ['prepared', '资料已填写'], ['queued', '排队中'],
  ['running', '生成中'], ['collecting', '保存结果中'], ['cancelling', '取消中'],
  ['submission_unknown', '提交结果未知'], ['candidate', '新版本待采用'],
  ['ready', '视频已就绪'], ['stale', '生成资料已更新'], ['failed', '生成失败'],
  ['cancelled', '已取消'], ['frames_processing', '首尾帧提取中'],
  ['frames_failed', '首尾帧提取失败'], ['media_missing', '素材不可用'],
]

const activeStatuses = new Set(['queued', 'running', 'collecting', 'cancelling', 'submission_unknown'])

export function shotLifecycle(shot) {
  const hasVideo = Boolean(shot.ready || shot.media)
  const jobStatus = shot.generation?.status ?? 'idle'
  const frames = shot.frames?.status ?? (shot.media?.firstFrame && shot.media?.lastFrame ? 'ready' : 'unavailable')
  const result = {
    status: 'ready', label: '视频已就绪', primary: { label: '重新生成', action: 'generate', disabled: false },
    hasVideo, canPreview: hasVideo && !shot.mediaMissing, canGenerate: !activeStatuses.has(jobStatus), canExport: hasVideo && !shot.mediaMissing,
    canCancel: ['queued', 'running'].includes(jobStatus), canUseTailFrame: hasVideo && !shot.mediaMissing && frames === 'ready',
    frameAction: frames === 'failed' ? '重试提取' : null, frameStatus: frames,
    tone: 'ok', detail: '',
  }
  const active = {
    queued: ['排队中', '等待计算资源', '排队中'],
    running: ['生成中', hasVideo ? '当前采用的视频仍可预览和导出。' : '收到视频后可以预览结果。', '生成中'],
    collecting: ['保存结果中', '视频正在保存，暂时无法采用新结果。', '保存结果中'],
    cancelling: ['取消中', '等待服务确认取消结果。', '取消中'],
    submission_unknown: ['提交结果未知', '查询原任务后才能再次提交。', '查询状态'],
  }
  if (active[jobStatus]) {
    const [label, detail, actionLabel] = active[jobStatus]
    return { ...result, status: jobStatus, label, detail, tone: 'warn', primary: { label: actionLabel, action: jobStatus === 'submission_unknown' ? 'query' : 'wait', disabled: jobStatus !== 'submission_unknown' } }
  }
  if (shot.mediaMissing) return { ...result, status: 'media_missing', label: '素材不可用', tone: 'warn', detail: '重新导入视频后继续编辑。', primary: { label: '重新导入', action: 'import', disabled: false } }
  if (shot.versions?.some(version => version.status === 'candidate')) return { ...result, status: 'candidate', label: '新版本待采用', tone: 'warn', detail: hasVideo ? '当前视频保持不变，选择新版本后替换。' : '选择一个视频版本加入成片。', primary: { label: '查看新版本', action: 'versions', disabled: false } }
  if (jobStatus === 'failed') return { ...result, status: 'failed', label: hasVideo ? '重新生成失败' : '生成失败', detail: shot.generation.error || '生成服务返回错误，原视频与资料已保留。', tone: 'warn', primary: { label: '重试生成', action: 'generate', disabled: false } }
  if (jobStatus === 'cancelled') return { ...result, status: 'cancelled', label: '已取消', detail: hasVideo ? '已保留当前视频。' : '资料已保留，可以重新提交。', tone: 'warn', primary: { label: hasVideo ? '重新生成' : '继续生成', action: 'generate', disabled: false } }
  if (!hasVideo) {
    const prepared = Boolean(shot.script.trim() || shot.storyboard.trim())
    return { ...result, status: prepared ? 'prepared' : 'empty', label: prepared ? '待生成' : '待填写', tone: 'warn', primary: { label: prepared ? '生成视频' : '完善分镜', action: prepared ? 'generate' : 'edit', disabled: false } }
  }
  if (shot.inputsChanged) return { ...result, status: 'stale', label: '资料已更新', tone: 'warn', detail: '当前视频来自上次资料，可以继续使用或重新生成。' }
  if (frames === 'processing') return { ...result, status: 'frames_processing', label: '首尾帧提取中', tone: 'warn', detail: '视频可以播放，首尾帧就绪后可用于下一分镜。' }
  if (frames === 'failed') return { ...result, status: 'frames_failed', label: '首尾帧提取失败', tone: 'warn', detail: '视频可以继续使用，只需重试帧提取。' }
  return result
}

export function previewShot(shot, status, hasVideo) {
  if (status === 'actual') return shot
  if (!shotPreviewStates.some(item => item[0] === status)) throw new Error('未知分镜状态')
  const base = hasVideo === undefined ? shot : { ...shot, ready: hasVideo, media: hasVideo ? shot.media : undefined }
  const next = { ...base, generation: { status: 'idle' }, frames: { status: 'ready' }, versions: [], inputsChanged: false, mediaMissing: false }
  if (status === 'empty' || status === 'prepared') return { ...next, ready: false, media: undefined, script: status === 'empty' ? '' : shot.script || '设计山间晨雾的远景镜头。', storyboard: '' }
  if (activeStatuses.has(status) || ['failed', 'cancelled'].includes(status)) return { ...next, generation: { status, error: status === 'failed' ? '生成服务响应超时。' : '' } }
  if (status === 'candidate') return { ...next, versions: [{ id: 'preview-candidate', status: 'candidate', name: '新版本', isPreview: true }] }
  if (status === 'stale') return { ...next, ready: true, inputsChanged: true }
  if (status === 'frames_processing' || status === 'frames_failed') return { ...next, ready: true, frames: { status: status === 'frames_failed' ? 'failed' : 'processing' } }
  if (status === 'media_missing') return { ...next, ready: false, mediaMissing: true }
  return { ...next, ready: true }
}

export function nextPreviewStatus(status, hasVideo = true) {
  if (status === 'collecting' && !hasVideo) return 'frames_processing'
  const transitions = { empty: 'prepared', prepared: 'queued', queued: 'running', running: 'collecting', collecting: 'candidate', candidate: 'ready', cancelling: 'cancelled', submission_unknown: 'running', frames_processing: 'ready', frames_failed: 'frames_processing', failed: 'queued', cancelled: 'queued', stale: 'queued' }
  if (!transitions[status]) throw new Error('当前状态没有自动后续状态')
  return transitions[status]
}

export function projectLifecycle(project, saveStatus) {
  const states = project.shots.map(shotLifecycle)
  const missingShotIds = project.shots.filter((_, index) => !states[index].canExport).map(shot => shot.id)
  const pendingCount = states.filter(state => ['queued', 'running', 'collecting', 'cancelling', 'submission_unknown'].includes(state.status)).length
  const candidateCount = states.filter(state => state.status === 'candidate').length
  const readyCount = states.filter(state => state.canExport).length
  const status = !states.length ? 'empty' : saveStatus !== '已保存' ? 'unsaved' : missingShotIds.length ? 'incomplete' : 'ready'
  return { status, tone: status !== 'ready' || states.some(state => state.tone === 'warn') ? 'warn' : 'ok', canExport: status === 'ready', readyCount, pendingCount, candidateCount, missingShotIds, total: states.length, label: status === 'empty' ? '暂无分镜' : status === 'unsaved' ? saveStatus : `${readyCount}/${states.length} 分镜可用` }
}
