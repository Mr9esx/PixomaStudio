import type { TimelineEditor } from '@xzdarcy/react-timeline-editor'
import { fitVideoProjectTiming, quantizeVideoDuration, validateVideoProject, videoTimelineFrames, videoProjectTiming, type VideoProject } from './video-editor-model'

export const shotTracks = [
  { id: 'shot', label: '分镜' },
  { id: 'reference', label: '参考素材' }, { id: 'video', label: '视频' },
] as const
type TimelineRow = TimelineEditor['editorData'][number]
export type TimelineAction = TimelineRow['actions'][number]
export type VideoTimelineAction = TimelineAction & { shotId?: string; audioId?: string }
export type VideoTimelineRow = Omit<TimelineRow, 'actions'> & { actions: VideoTimelineAction[] }
export type TimelineEdit = { kind: 'move' | 'left' | 'right'; shotId?: string; audioId?: string; start: number; end: number }
export type VideoTrackDisplay = { trackHeights: Record<string, number>; collapsedTracks: string[] }

export function timelineRowAt(rows: VideoTimelineRow[], offset: number) {
  if (!Number.isFinite(offset)) throw new Error('轨道位置无效')
  let top = 0
  for (const row of rows) {
    const height = row.rowHeight ?? 42
    if (offset >= top && offset < top + height) return { row, top, height }
    top += height
  }
}

export function fitTimelineZoom(width: number, duration: number, unitWidth = 48): number {
  if (!Number.isFinite(width) || width <= 0 || !Number.isFinite(duration) || duration < 0) throw new Error('时间轴尺寸无效')
  return duration === 0 ? 1 : Math.min(4, Math.max(1, width - 120) / (unitWidth * duration))
}

export function timelineScale(zoom: number): number {
  if (!Number.isFinite(zoom) || zoom <= 0) throw new Error('时间轴缩放无效')
  return [1, 2, 5, 10, 15, 30, 60, 120, 300, 600, 900, 1800, 3600].find(value => value * zoom >= 1) ?? 3600
}

export function videoAssetDropTarget(project: VideoProject, kind: string, track: string, seconds: number) {
  if (!Number.isFinite(seconds) || seconds < 0) throw new Error('拖入位置无效')
  const frame = Math.round(seconds * project.fps)
  const time = frame / project.fps
  const frames = videoTimelineFrames(project.shots, project.fps)
  const timing = videoProjectTiming(project)
  const shot = timing.unknownCount ? project.shots[Math.floor(seconds)] : project.shots.find((_, index) => frames[index] <= frame && frame < frames[index + 1])
  const target = { time, shotId: shot?.id }
  if (track.startsWith('audio')) return { ...target, allowed: !timing.unknownCount && kind === 'audio' && frame < frames.at(-1)!, label: timing.unknownCount ? '分镜时长确定后调整音频位置' : kind !== 'audio' ? '成片音频轨道仅接受音频' : frame >= frames.at(-1)! ? '将音频放在成片范围内' : '添加成片音频' }
  if (track === 'video') return { ...target, allowed: kind === 'video', label: kind !== 'video' ? '视频轨道仅接受视频' : shot ? `替换「${shot.name}」的视频` : '创建视频分镜' }
  if (!shot) return { ...target, allowed: false, label: '将资产放入一个分镜内' }
  if (track === 'reference') {
    const allowed = ['image', 'audio', 'video', 'document'].includes(kind)
    return { ...target, allowed, label: allowed ? `添加「${shot.name}」的参考` : '此资产类型不能作为参考' }
  }
  return { ...target, allowed: false, label: '此轨道不接受资产' }
}

export function createTimelineRows(project: VideoProject, selectedId: string, selectedTrack?: string, _selectedAudioId?: string, display?: VideoTrackDisplay): VideoTimelineRow[] {
  const frames = videoTimelineFrames(project.shots, project.fps)
  const sequence = videoProjectTiming(project).unknownCount > 0
  const rows: VideoTimelineRow[] = shotTracks.map(track => ({ id: track.id, actions: project.shots.map((shot, index) => ({
    id: `${track.id}:${shot.id}`, shotId: shot.id, effectId: track.id,
    start: sequence ? index : frames[index] / project.fps, end: sequence ? index + 1 : frames[index + 1] / project.fps,
    selected: shot.id === selectedId && (!selectedTrack || track.id === selectedTrack), flexible: !sequence && track.id === 'video' && Boolean(shot.media), movable: track.id === 'video' || track.id === 'shot',
    minStart: 0,
  })) }))
  for (const row of rows) row.rowHeight = display?.collapsedTracks.includes(row.id) ? 28 : display?.trackHeights[row.id] ?? (row.id === 'shot' ? 128 : 42)
  return rows
}

export function editTimelineProject(project: VideoProject, edit: TimelineEdit): VideoProject {
  if (![edit.start, edit.end].every(Number.isFinite)) throw new Error('时间轴位置无效')
  const quantize = (value: number) => quantizeVideoDuration(value, project.fps)
  const start = quantize(edit.start), end = quantize(edit.end)
  if (start < 0 || end - start < 1 / project.fps - 1e-6) throw new Error('剪辑至少保留一帧')
  let next: VideoProject
  if (edit.shotId) {
    const index = project.shots.findIndex(shot => shot.id === edit.shotId)
    if (index < 0) throw new Error('分镜已被删除')
    const shot = project.shots[index]
    if (videoProjectTiming(project).unknownCount) {
      if (edit.kind !== 'move') throw new Error('分镜时长确定后调整时间轴范围')
      const shots = project.shots.filter(item => item.id !== shot.id)
      shots.splice(Math.max(0, Math.min(shots.length, Math.round(edit.start))), 0, shot)
      const next = { ...project, shots }
      validateVideoProject(next)
      return next
    }
    if (edit.kind === 'move') {
      const shots = project.shots.filter(item => item.id !== shot.id)
      const frames = videoTimelineFrames(shots, project.fps)
      const target = shots.filter((_, i) => (frames[i] + frames[i + 1]) / (2 * project.fps) < start).length
      shots.splice(target, 0, shot)
      next = { ...project, shots }
    } else {
      if (!shot.media) throw new Error('采用视频后调整使用范围')
      const originalStart = videoTimelineFrames(project.shots, project.fps)[index] / project.fps
      const sourceIn = quantize(shot.sourceIn + (edit.kind === 'left' ? start - originalStart : 0))
      next = { ...project, shots: project.shots.map(item => item.id === shot.id ? { ...item, duration: quantize(end - start), sourceIn } : item) }
    }
  } else if (edit.audioId) {
    if (videoProjectTiming(project).unknownCount) throw new Error('分镜时长确定后调整音频位置')
    const audio = project.audio?.find(item => item.id === edit.audioId)
    if (!audio) throw new Error('音频已被删除')
    const sourceIn = quantize(audio.sourceIn + (edit.kind === 'left' ? start - audio.start : 0))
    const duration = edit.kind === 'move' ? audio.duration : quantize(end - start)
    const total = videoTimelineFrames(project.shots, project.fps).at(-1)! / project.fps
    if (start + duration > total + 1e-6) throw new Error('音频超出成片范围')
    next = { ...project, audio: project.audio!.map(item => item.id === audio.id ? { ...audio, start, sourceIn, duration } : item) }
  } else throw new Error('时间轴对象缺失')
  next = fitVideoProjectTiming(next)
  validateVideoProject(next)
  return next
}
