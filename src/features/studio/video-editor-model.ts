import { z } from 'zod'
import { normalizeProjectScript, validateVideoScript, type VideoScript, type VideoScriptSource } from './video-script'
import { validateVideoCreativeProposals, type VideoCreativeProposal } from './video-creative-proposal'

const dimensionMessage = '尺寸需要是 16–4096 的偶数整数'
const dimensionSchema = z.number({ error: dimensionMessage }).refine(value => Number.isInteger(value) && value >= 16 && value <= 4096 && value % 2 === 0, dimensionMessage)
export const videoProjectSettingsSchema = z.object({
  name: z.string({ error: '填写视频名称' }).trim().min(1, '填写视频名称'),
  width: dimensionSchema,
  height: dimensionSchema,
  fps: z.number({ error: '帧率需要是 1–60 的整数' }).refine(value => Number.isInteger(value) && value >= 1 && value <= 60, '帧率需要是 1–60 的整数'),
})

export type VideoAssetReference = { assetId: string; versionId: string; url: string }
export const videoInputSourcesSchema = z.record(z.string().min(1), z.object({
  field: z.enum(['script', 'storyboard']), revision: z.number().int().positive().optional(), sourceText: z.string(), value: z.string(), edited: z.boolean(),
}))
export type VideoInputSources = z.infer<typeof videoInputSourcesSchema>
const videoWorkflowDraftSchema = z.record(z.string().min(1), z.object({
  inputs: z.record(z.string(), z.unknown()), inputSources: videoInputSourcesSchema.optional(),
  referenceInputs: z.record(z.string().min(1), z.string().min(1)).optional(),
}))
export type VideoGenerationSnapshot = { executionId: string; workflowId: string; versionId: string; inputs: Record<string, unknown> }
export type VideoMedia = VideoAssetReference & { duration?: number; firstFrame?: VideoAssetReference; lastFrame?: VideoAssetReference; generation?: VideoGenerationSnapshot }
export const videoReferenceRoles = ['storyboard', 'character', 'scene', 'style', 'first_frame', 'last_frame', 'voice', 'rhythm', 'reference'] as const
export const videoReferenceRoleLabels = { storyboard: '分镜画面', character: '角色', scene: '场景', style: '风格', first_frame: '首帧', last_frame: '尾帧', voice: '声音参考', rhythm: '节奏参考', reference: '创作参考' }
export type VideoReference = VideoAssetReference & {
  id: string
  kind: 'image' | 'audio' | 'video' | 'document'
  role: typeof videoReferenceRoles[number]
  inputKey?: string
  sourceShotId?: string
  sourcePosition?: 'first' | 'last' | 'fixed'
  sourceMediaVersionId?: string
  confirmedSourceVersionId?: string
  sourceFrame?: number
  sourceFps?: number
  confirmedSourceFrame?: number
  confirmedSourceFps?: number
}
export type VideoShot = {
  id: string
  name: string
  script: string
  scriptSources?: VideoScriptSource[]
  storyboard?: string
  contentRevisions?: { script: number; storyboard: number }
  references?: VideoReference[]
  subtitle: string
  duration: number
  plannedDuration?: number
  sourceIn: number
  workflowId?: string
  versionId?: string
  inputs: Record<string, unknown>
  inputSources?: VideoInputSources
  executionId?: string
  media?: VideoMedia
  volume?: number
  versions?: Array<{ id: string; media: VideoMedia; status: 'candidate' | 'history'; executionId?: string; sourceIn?: number; duration?: number }>
  workflowDrafts?: z.infer<typeof videoWorkflowDraftSchema>
  executionInputs?: string
  generationRequest?: { id: string; workflowId: string; versionId: string; inputs: Record<string, unknown> }
}

export type VideoAudio = VideoAssetReference & { id: string; start: number; duration: number; sourceIn: number; volume: number; loop: boolean; sourceDuration?: number; playbackOffset?: number }
export type VideoProject = { version: 1 | 2; name: string; width: number; height: number; fps: number; shots: VideoShot[]; audio?: VideoAudio[]; script?: VideoScript; proposals?: VideoCreativeProposal[] }

export function reviseVideoShotContent(previous: VideoShot | undefined, shot: VideoShot): VideoShot {
  const revision = (field: 'script' | 'storyboard') => previous
    ? (previous.contentRevisions?.[field] ?? 1) + ((previous[field] ?? '') === (shot[field] ?? '') ? 0 : 1)
    : shot.contentRevisions?.[field] ?? 1
  return { ...shot, contentRevisions: { script: revision('script'), storyboard: revision('storyboard') } }
}

export function normalizeVideoProject(project: Omit<VideoProject, 'version'> & { version: number }): VideoProject {
  if (project.version !== 1 && project.version !== 2) throw new Error('视频项目格式无效')
  validateVideoProject(project as VideoProject)
  const next: VideoProject = normalizeProjectScript({ ...structuredClone(project), version: 2, shots: project.shots.map(shot => reviseVideoShotContent(undefined, { ...structuredClone(shot), ...(!shot.media && shot.duration > 0 ? { plannedDuration: shot.plannedDuration ?? shot.duration, duration: 0 } : {}), storyboard: shot.storyboard ?? '', references: structuredClone(shot.references ?? []), volume: shot.volume ?? 1, versions: structuredClone(shot.versions ?? []) })), audio: structuredClone(project.audio ?? []) })
  validateVideoProject(next)
  return next
}

function validateReference(reference: VideoReference): void {
  if (!reference.id || !reference.assetId || !reference.versionId || !reference.url || !['image', 'audio', 'video', 'document'].includes(reference.kind) || !videoReferenceRoles.includes(reference.role)) throw new Error('参考资产格式无效')
  if (['storyboard', 'character', 'scene', 'style', 'first_frame', 'last_frame'].includes(reference.role) && reference.kind !== 'image') throw new Error('参考用途与资产类型不匹配')
  if (['voice', 'rhythm'].includes(reference.role) && reference.kind !== 'audio') throw new Error('参考用途与资产类型不匹配')
  if (Boolean(reference.sourceShotId) !== Boolean(reference.sourceMediaVersionId)) throw new Error('参考来源信息不完整')
  if (reference.sourcePosition && (!reference.sourceShotId || !['first', 'last', 'fixed'].includes(reference.sourcePosition))) throw new Error('参考画面类型无效')
  if (reference.sourcePosition === 'fixed' && reference.sourceFrame === undefined) throw new Error('指定画面位置缺失')
  for (const [frame, fps] of [[reference.sourceFrame, reference.sourceFps], [reference.confirmedSourceFrame, reference.confirmedSourceFps]]) {
    if (frame === undefined && fps === undefined) continue
    if (!reference.sourceShotId || !Number.isSafeInteger(frame) || frame! < 0 || !Number.isInteger(fps) || fps! < 1 || fps! > 60) throw new Error('参考画面位置无效')
  }
}

export function setVideoReference(shot: VideoShot, reference: VideoReference): VideoShot {
  validateReference(reference)
  const references = shot.references ?? []
  if (references.some(item => item.id !== reference.id && item.assetId === reference.assetId && item.versionId === reference.versionId && item.role === reference.role && item.inputKey === reference.inputKey)) return shot
  let next = shot
  for (const item of references) {
    if (item.id === reference.id || (['storyboard', 'first_frame', 'last_frame'].includes(reference.role) && item.role === reference.role)) next = removeVideoReference(next, item.id)
  }
  return { ...next, references: [...(next.references ?? []).map(item => reference.inputKey && item.inputKey === reference.inputKey ? { ...item, inputKey: undefined } : item), reference], inputs: reference.inputKey ? { ...next.inputs, [reference.inputKey]: { asset_id: reference.assetId, asset_version_id: reference.versionId } } : next.inputs }
}

export function removeVideoReference(shot: VideoShot, referenceId: string): VideoShot {
  const reference = shot.references?.find(item => item.id === referenceId)
  if (!reference) throw new Error('参考资产已被移除')
  const references = shot.references!.filter(item => item.id !== referenceId)
  const inputs = { ...shot.inputs }
  if (reference.inputKey && !references.some(item => item.inputKey === reference.inputKey)) {
    const value = inputs[reference.inputKey]
    if (value && typeof value === 'object' && 'asset_id' in value && 'asset_version_id' in value && value.asset_id === reference.assetId && value.asset_version_id === reference.versionId) delete inputs[reference.inputKey]
  }
  return { ...shot, references, inputs }
}

export function videoShotLastFrame(shot: VideoShot, fps: number): number {
  if (!shot.media || shot.duration <= 0 || !Number.isInteger(fps) || fps < 1 || fps > 60) throw new Error('来源分镜没有有效的剪辑范围')
  return Math.round((shot.sourceIn + shot.duration) * fps) - 1
}

export function videoReferenceSourceChanged(reference: VideoReference, source: VideoShot | undefined, fps: number): boolean {
  if (!reference.sourceShotId) return false
  if (!source?.media || source.media.versionId !== reference.confirmedSourceVersionId) return true
  const position = videoReferencePosition(reference, source, fps)
  return reference.confirmedSourceFrame !== position.frame || reference.confirmedSourceFps !== position.fps
}

export function videoReferencePosition(reference: Pick<VideoReference, 'sourcePosition' | 'sourceFrame' | 'sourceFps'>, source: VideoShot, fps: number): { frame: number; fps: number } {
  if (reference.sourcePosition === 'fixed') {
    if (!Number.isSafeInteger(reference.sourceFrame) || reference.sourceFrame! < 0 || !Number.isInteger(reference.sourceFps) || reference.sourceFps! < 1 || reference.sourceFps! > 60) throw new Error('指定画面位置无效')
    return { frame: reference.sourceFrame!, fps: reference.sourceFps! }
  }
  const last = videoShotLastFrame(source, fps)
  return { frame: reference.sourcePosition === 'first' ? Math.round(source.sourceIn * fps) : last, fps }
}

export function resolveVideoReferenceSource(shot: VideoShot, referenceId: string, source: VideoShot | undefined, adopt: boolean, fps: number, frameAsset?: VideoAssetReference): VideoShot {
  const reference = shot.references?.find(item => item.id === referenceId)
  if (!reference?.sourceShotId) throw new Error('参考来源已不存在')
  if (source && source.id !== reference.sourceShotId) throw new Error('参考来源分镜不匹配')
  if (adopt) {
    if (!source?.media || !frameAsset) throw new Error('来源分镜没有可用的剪辑画面')
    const position = videoReferencePosition(reference, source, fps)
    return setVideoReference(shot, { ...reference, ...frameAsset, sourceMediaVersionId: source.media.versionId, confirmedSourceVersionId: source.media.versionId, sourceFrame: position.frame, sourceFps: position.fps, confirmedSourceFrame: position.frame, confirmedSourceFps: position.fps })
  }
  const position = source?.media ? videoReferencePosition(reference, source, fps) : undefined
  const confirmed = { ...reference, confirmedSourceVersionId: source?.media?.versionId, confirmedSourceFrame: position?.frame, confirmedSourceFps: position?.fps }
  if (!source?.media) {
    delete confirmed.sourceShotId
    delete confirmed.sourcePosition
    delete confirmed.sourceMediaVersionId
    delete confirmed.confirmedSourceVersionId
    delete confirmed.sourceFrame
    delete confirmed.sourceFps
    delete confirmed.confirmedSourceFrame
    delete confirmed.confirmedSourceFps
  }
  return { ...shot, references: shot.references!.map(item => item.id === referenceId ? confirmed : item) }
}

export function previewVideoShot(project: VideoProject, shotId: string): VideoProject {
  const index = project.shots.findIndex(shot => shot.id === shotId)
  if (index < 0) throw new Error('分镜已被删除')
  const frames = videoTimelineFrames(project.shots, project.fps)
  const start = frames[index] / project.fps, end = frames[index + 1] / project.fps
  const startKnown = videoProjectTiming(project).starts[index] !== undefined
  return { ...project, shots: [project.shots[index]], audio: startKnown ? project.audio?.flatMap(audio => {
    const from = Math.max(start, audio.start), until = Math.min(end, audio.start + audio.duration)
    return until > from ? [{ ...audio, start: from - start, duration: until - from, playbackOffset: from - audio.start }] : []
  }) : [] }
}

export function videoShotDuration(shot: VideoShot): number | undefined {
  return shot.media ? shot.duration : shot.plannedDuration ?? (shot.duration > 0 ? shot.duration : undefined)
}

export function videoProjectTiming(project: Pick<VideoProject, 'shots'>) {
  let knownDuration = 0, unknownCount = 0
  const starts = project.shots.map(shot => {
    const start = unknownCount ? undefined : knownDuration
    const duration = videoShotDuration(shot)
    if (duration === undefined) unknownCount += 1
    else knownDuration += duration
    return start
  })
  return { knownDuration, unknownCount, starts }
}

export function quantizeVideoDuration(seconds: number, fps: number): number {
  return Math.round(seconds * fps) / fps
}

export function fitVideoProjectTiming(project: VideoProject): VideoProject {
  const shots = project.shots.map(shot => ({ ...shot, duration: quantizeVideoDuration(shot.duration, project.fps), ...(shot.plannedDuration !== undefined ? { plannedDuration: quantizeVideoDuration(shot.plannedDuration, project.fps) } : {}) }))
  const timing = videoProjectTiming({ shots })
  return { ...project, shots, audio: timing.unknownCount ? project.audio : project.audio?.filter(audio => audio.start < timing.knownDuration).map(audio => ({ ...audio, duration: Math.min(audio.duration, timing.knownDuration - audio.start) })) }
}

export function updateVideoProjectSettings(project: VideoProject, settings: Pick<VideoProject, 'name' | 'width' | 'height' | 'fps'>): VideoProject {
  const parsed = videoProjectSettingsSchema.parse(settings)
  const next = parsed.fps === project.fps ? { ...project, ...parsed } : fitVideoProjectTiming({ ...project, ...parsed, shots: project.shots.map(shot => ({ ...shot, duration: shot.duration > 0 ? Math.max(1 / parsed.fps, quantizeVideoDuration(shot.duration, parsed.fps)) : 0, ...(shot.plannedDuration !== undefined ? { plannedDuration: Math.max(1 / parsed.fps, quantizeVideoDuration(shot.plannedDuration, parsed.fps)) } : {}) })) })
  validateVideoProject(next)
  return next
}

export function videoTimelineFrames(shots: VideoShot[], fps: number): number[] {
  const boundaries = [0]
  for (const shot of shots) boundaries.push(boundaries.at(-1)! + Math.round((videoShotDuration(shot) ?? 0) * fps))
  return boundaries
}

export function adoptVideoMedia(shot: VideoShot, media: VideoMedia, range?: { sourceIn: number; duration: number }): VideoShot {
  if (media.duration === undefined || !Number.isFinite(media.duration) || media.duration <= 0) throw new Error('视频时长尚未就绪')
  const selection = range ?? { sourceIn: 0, duration: shot.media ? shot.duration : media.duration }
  if (!Number.isFinite(selection.sourceIn) || !Number.isFinite(selection.duration) || selection.sourceIn < 0 || selection.duration <= 0 || selection.sourceIn + selection.duration > media.duration + 1e-6) throw new Error('选择有效的视频使用范围')
  return { ...shot, ...(!shot.media && shot.duration > 0 && shot.plannedDuration === undefined ? { plannedDuration: shot.duration } : {}), media, ...selection,
    versions: [...(shot.versions ?? []).filter(version => version.media.versionId !== media.versionId && version.media.versionId !== shot.media?.versionId), ...(shot.media && shot.media.versionId !== media.versionId ? [{ id: shot.media.versionId, media: shot.media, status: 'history' as const, sourceIn: shot.sourceIn, duration: shot.duration }] : [])] }
}

export function collectVideoMedia(shot: VideoShot, target: string | { shotId: string; executionId: string }, media: VideoMedia): VideoShot {
  const executionId = typeof target === 'string' ? target : target.executionId
  if (typeof target === 'string') { if (shot.executionId !== executionId) return shot }
  else {
    if (shot.id !== target.shotId) throw new Error('生成结果的目标分镜不匹配')
    if (media.generation?.executionId !== executionId) throw new Error('生成结果与生成任务不匹配')
  }
  if (shot.media?.versionId === media.versionId || shot.versions?.some(version => version.media.versionId === media.versionId)) return shot
  return { ...shot, versions: [...(shot.versions ?? []), { id: media.versionId, media: structuredClone(media), executionId, status: 'candidate' }] }
}

export function serializeVideoContent(value: unknown) {
  return JSON.stringify(value, (_key, item) => item && typeof item === 'object' && !Array.isArray(item) ? Object.fromEntries(Object.keys(item).sort().map(key => [key, item[key]])) : item)
}

export function videoGenerationChanged(shot: VideoShot): boolean {
  const generation = shot.media?.generation
  if (!generation) return false
  return shot.workflowId !== generation.workflowId || shot.versionId !== generation.versionId || serializeVideoContent(shot.inputs) !== serializeVideoContent(generation.inputs)
}

export function emptyVideoProject(): VideoProject {
  return { version: 2, name: '未命名视频', width: 1280, height: 720, fps: 30, shots: [], audio: [] }
}

export function newVideoShot(name = '未命名分镜', script = ''): VideoShot {
  return { id: crypto.randomUUID(), name, script, storyboard: '', references: [], subtitle: '', duration: 0, sourceIn: 0, inputs: {}, volume: 1, versions: [] }
}

export function duplicateVideoShot(source: VideoShot, id: string, reuseMedia = false): VideoShot {
  if (!id.trim() || id === source.id) throw new Error('复制分镜需要新的标识')
  if (reuseMedia && !source.media) throw new Error('当前分镜尚未采用视频')
  const copy = structuredClone(source)
  return { ...copy, id, name: `${source.name} · 副本`, plannedDuration: source.plannedDuration ?? (!source.media && source.duration > 0 ? source.duration : undefined),
    media: reuseMedia ? copy.media : undefined, duration: reuseMedia ? source.duration : 0, sourceIn: reuseMedia ? source.sourceIn : 0,
    executionId: undefined, executionInputs: undefined, generationRequest: undefined, versions: [],
  }
}

export function teaVideoShots(): VideoShot[] {
  return [
    ['雾起山间', '清晨薄雾笼罩山间茶园，晨光穿过云层，远景，镜头缓慢推进，自然写实，安静的茶品牌广告，无文字。'],
    ['一叶新茶', '新鲜茶叶上晶莹的露珠，清晨自然光，微距特写，镜头轻微横移，浅景深，写实茶品牌广告，无文字。'],
    ['手采春意', '采茶人的手指轻轻摘下一片嫩绿茶叶，近景，自然阳光，动作缓慢，写实茶品牌广告，无文字。'],
    ['热水入盏', '热水缓缓注入白色陶瓷茶盏，金色茶汤泛起涟漪，近景，固定镜头，写实茶品牌广告，无文字。'],
    ['茶香入席', '木桌上一盏热茶升起轻柔蒸汽，背景是朦胧山间茶园，缓慢拉远，温暖晨光，写实茶品牌广告，无文字。'],
  ].map(([name, script]) => ({ ...newVideoShot(name, script), duration: 3 }))
}

export function validateVideoProject(project: VideoProject): void {
  if (![1, 2].includes(project.version) || !Array.isArray(project.shots)) throw new Error('视频项目格式无效')
  validateVideoScript(project)
  validateVideoCreativeProposals(project)
  const settings = videoProjectSettingsSchema.safeParse(project)
  if (!settings.success) throw new Error(settings.error.issues[0].message)
  if (project.shots.length > 100) throw new Error('分镜最多 100 项')
  if (project.audio && project.audio.length > 100) throw new Error('音频最多 100 项')
  const timing = videoProjectTiming(project)
  const total = timing.knownDuration
  if (total > 3600) throw new Error('视频总时长不能超过一小时')
  if (new Set(project.shots.map(shot => shot.id)).size !== project.shots.length) throw new Error('分镜标识重复')
  for (const shot of project.shots) {
    if (!shot.id || !shot.name.trim() || !Number.isFinite(shot.duration) || shot.duration < 0 || (shot.media && shot.duration === 0) || !Number.isFinite(shot.sourceIn) || shot.sourceIn < 0) throw new Error('分镜名称或剪辑范围无效')
    if (shot.plannedDuration !== undefined && (!Number.isFinite(shot.plannedDuration) || shot.plannedDuration <= 0 || shot.plannedDuration > 3600 || Math.abs(shot.plannedDuration - quantizeVideoDuration(shot.plannedDuration, project.fps)) > 1e-6)) throw new Error('计划时长需要符合项目帧间隔且不超过一小时')
    if (Math.abs(shot.duration - quantizeVideoDuration(shot.duration, project.fps)) > 1e-6) throw new Error('分镜时长需要符合项目帧间隔')
    if (typeof shot.script !== 'string' || typeof shot.subtitle !== 'string' || !shot.inputs || typeof shot.inputs !== 'object' || Array.isArray(shot.inputs)) throw new Error('分镜内容格式无效')
    if (shot.inputSources !== undefined && !videoInputSourcesSchema.safeParse(shot.inputSources).success) throw new Error('输入来源格式无效')
    if (shot.contentRevisions !== undefined && !z.object({ script: z.number().int().positive(), storyboard: z.number().int().positive() }).safeParse(shot.contentRevisions).success) throw new Error('分镜文字修订格式无效')
    if (shot.workflowDrafts !== undefined && !videoWorkflowDraftSchema.safeParse(shot.workflowDrafts).success) throw new Error('工作流草稿格式无效')
    if (project.version === 2 && (typeof shot.storyboard !== 'string' || !Array.isArray(shot.references))) throw new Error('分镜画面描述或参考格式无效')
    if (shot.references) {
      if (!Array.isArray(shot.references) || new Set(shot.references.map(reference => reference.id)).size !== shot.references.length) throw new Error('参考资产标识重复')
      for (const reference of shot.references) validateReference(reference)
    }
    if (shot.volume !== undefined && (!Number.isFinite(shot.volume) || shot.volume < 0 || shot.volume > 2)) throw new Error('视频音量需要在零至二之间')
    if (shot.media?.duration && shot.sourceIn + shot.duration > shot.media.duration + .05) throw new Error('剪辑范围超出源视频')
  }
  if (project.audio && new Set(project.audio.map(audio => audio.id)).size !== project.audio.length) throw new Error('音频标识重复')
  for (const audio of project.audio ?? []) {
    if (!audio.id || !audio.assetId || !audio.versionId || !audio.url || !Number.isFinite(audio.start) || audio.start < 0 || (!timing.unknownCount && audio.start >= total) || !Number.isFinite(audio.duration) || audio.duration <= 0 || !Number.isFinite(audio.sourceIn) || audio.sourceIn < 0 || !Number.isFinite(audio.volume) || audio.volume < 0 || audio.volume > 2 || typeof audio.loop !== 'boolean') throw new Error('音频参数无效')
    if (!timing.unknownCount && audio.start + audio.duration > total + 1e-6) throw new Error('音频超出成片范围')
    if (audio.sourceDuration !== undefined && (!Number.isFinite(audio.sourceDuration) || audio.sourceDuration <= 0 || audio.sourceIn >= audio.sourceDuration || (!audio.loop && audio.sourceIn + audio.duration > audio.sourceDuration + .05))) throw new Error('音频剪辑范围超出素材')
  }
}
