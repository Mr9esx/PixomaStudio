import { fitVideoProjectTiming, quantizeVideoDuration, validateVideoProject, type VideoProject } from './video-editor-model'

export type VideoNumberTarget = { kind: 'shot'; id: string; field: 'plannedDuration' | 'duration' | 'sourceIn' | 'volume' } | { kind: 'audio'; id: string; field: 'start' | 'duration' | 'sourceIn' | 'volume' }
export type VideoPropertyDraft<T extends string | number> = { text: string; baseline: T; error?: string }
export type VideoNumberDraft = VideoPropertyDraft<number>
export type VideoNameDraft = VideoPropertyDraft<string>

export function renameVideoShot(project: VideoProject, shotId: string, draft: VideoNameDraft): VideoProject {
  const shot = project.shots.find(shot => shot.id === shotId)
  if (!shot) throw new Error('分镜已被删除')
  const name = draft.text.trim()
  if (!name) throw new Error('填写分镜名称')
  if (name === shot.name) return project
  if (shot.name !== draft.baseline) throw new Error('分镜名称已更新，请选择使用最新内容或保留输入')
  const next = { ...project, shots: project.shots.map(item => item.id === shotId ? { ...item, name } : item) }
  validateVideoProject(next)
  return next
}

export function setVideoAudioLoop(project: VideoProject, audioId: string, loop: boolean, sourceDuration?: number): VideoProject {
  const audio = project.audio?.find(audio => audio.id === audioId)
  if (!audio) throw new Error('音频已被删除')
  if (audio.loop === loop) return project
  const source = sourceDuration ?? audio.sourceDuration
  if (source !== undefined && (!Number.isFinite(source) || source <= 0)) throw new Error('音频时长无效')
  if (!loop && source === undefined) throw new Error('音频时长尚未就绪')
  const available = source === undefined ? audio.duration : Math.floor((source - audio.sourceIn) * project.fps + 1e-7) / project.fps
  if (available < 1 / project.fps) throw new Error('音频可用范围需要至少保留一帧')
  const next = { ...project, audio: project.audio!.map(item => item.id === audioId ? { ...item, loop, sourceDuration: source, duration: loop ? item.duration : Math.min(item.duration, available) } : item) }
  validateVideoProject(next)
  return next
}

export function editVideoNumber(project: VideoProject, target: VideoNumberTarget, draft: VideoNumberDraft): VideoProject {
  let previous: number
  if (target.kind === 'shot') {
    const shot = project.shots.find(shot => shot.id === target.id)
    if (!shot) throw new Error('分镜已被删除')
    previous = shot[target.field] ?? (target.field === 'plannedDuration' ? 0 : 1)
  } else {
    const audio = project.audio?.find(audio => audio.id === target.id)
    if (!audio) throw new Error('音频已被删除')
    previous = audio[target.field]
  }
  const parsed = Number(draft.text)
  const clearPlan = target.field === 'plannedDuration' && !draft.text.trim()
  if ((!draft.text.trim() && !clearPlan) || !Number.isFinite(parsed)) throw new Error('填写有效数字')
  if (parsed < 0) throw new Error('数值不能小于零')
  const value = target.field === 'volume' ? parsed : quantizeVideoDuration(parsed, project.fps)
  if (target.field === 'duration' && value < 1 / project.fps) throw new Error('使用时长至少保留一帧')
  if (target.field === 'plannedDuration' && !clearPlan && value < 1 / project.fps) throw new Error('计划时长至少保留一帧')
  if (value === previous) return project
  if (previous !== draft.baseline) throw new Error('该数值已更新，请选择使用最新内容或保留输入')
  const next = target.kind === 'shot'
    ? fitVideoProjectTiming({ ...project, shots: project.shots.map(shot => shot.id === target.id ? { ...shot, [target.field]: clearPlan ? undefined : value, ...(target.field === 'plannedDuration' && !shot.media ? { duration: 0 } : {}) } : shot) })
    : { ...project, audio: project.audio!.map(audio => audio.id === target.id ? { ...audio, [target.field]: value } : audio) }
  validateVideoProject(next)
  return next
}
