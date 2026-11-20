import { z } from 'zod'
import type { StudioAsset } from '@/lib/api/studio'
import { videoTimelineFrames, videoProjectTiming, videoShotDuration, type VideoProject } from './video-editor-model'
import { videoPromptDraftSchema } from './video-prompt-draft'
import { videoTextDraftSchema } from './video-text-draft'
import { videoStructureDraftSchema } from './video-structure-draft'

const viewSchema = z.object({
  version: z.literal(1).default(1),
  selectedId: z.string().default(''),
  batchSelection: z.array(z.string()).default([]),
  selectedAudioId: z.string().default(''),
  tab: z.enum(['shot', 'script', 'reference', 'video', 'subtitle', 'audio']).default('shot'),
  inspector: z.boolean().default(false),
  previewShotId: z.string().default(''),
  assetPreview: z.object({ assetId: z.string().min(1), versionId: z.string().min(1), time: z.number().nonnegative().default(0) }).optional(),
  loop: z.boolean().default(false),
  playheadFrame: z.number().int().nonnegative().default(0),
  playheadShotId: z.string().optional(),
  zoom: z.number().positive().max(4).default(1),
  snap: z.boolean().default(true),
  library: z.boolean().default(false),
  previewOpen: z.boolean().default(true),
  monitorVolume: z.number().min(0).max(1).default(1),
  assetVersions: z.record(z.string(), z.string()).default({}),
  search: z.string().default(''),
  assetKind: z.enum(['all', 'image', 'video', 'audio', 'document']).default('all'),
  assetView: z.enum(['grid', 'list']).default('grid'),
  assetScrollTop: z.number().nonnegative().default(0),
  scrollLeft: z.number().nonnegative().default(0),
  scrollTop: z.number().nonnegative().default(0),
  trackHeights: z.record(z.string(), z.number().int().min(42).max(160)).default({}),
  collapsedTracks: z.array(z.string()).default([]),
  numberDrafts: z.record(z.string(), z.object({ text: z.string(), baseline: z.number(), error: z.string().optional() })).default({}),
  nameDrafts: z.record(z.string(), z.object({ text: z.string(), baseline: z.string(), error: z.string().optional() })).default({}),
  promptDrafts: z.record(z.string(), videoPromptDraftSchema).default({}),
  structureDrafts: z.record(z.string(), videoStructureDraftSchema).default({}),
  scriptIdeas: z.string().default(''),
  textDraft: videoTextDraftSchema.optional(),
})
export type VideoEditorViewState = z.infer<typeof viewSchema>
export type VideoAssetPreviewState = NonNullable<VideoEditorViewState['assetPreview']>
export type VideoAssetBrowserState = Pick<VideoEditorViewState, 'search' | 'assetKind' | 'assetView' | 'assetScrollTop'>
export type VideoTimelineViewport = Pick<VideoEditorViewState, 'scrollLeft' | 'scrollTop'>

export function selectedVideoAsset(asset: StudioAsset, versionId?: string): StudioAsset | undefined {
  const version = asset.versions.find(item => versionId === undefined ? item.version === asset.current_version : item.id === versionId)
  return version ? { ...asset, current_version: version.version } : undefined
}

export function videoPreviewFrame(seconds: number, fps: number, totalFrames: number): number {
  if (!Number.isFinite(seconds) || !Number.isInteger(fps) || fps <= 0) throw new Error('播放时间无效')
  return Math.max(0, Math.min(Math.max(0, totalFrames - 1), Math.round(seconds * fps)))
}

export function videoAssetPreviewTime(seconds: number, duration: number): number {
  if (!Number.isFinite(seconds) || seconds < 0 || !Number.isFinite(duration) || duration < 0) throw new Error('素材预览时间无效')
  return Math.min(seconds, duration)
}

export function restoreVideoEditorView(project: VideoProject, saved: unknown) {
  const view = viewSchema.parse(saved)
  const boundaries = videoTimelineFrames(project.shots, project.fps)
  const sequence = videoProjectTiming(project).unknownCount > 0
  const selectedId = project.shots.some(shot => shot.id === view.selectedId) ? view.selectedId : project.shots[0]?.id ?? ''
  const selected = project.shots.find(shot => shot.id === selectedId)
  const previewIndex = project.shots.findIndex(shot => shot.id === view.previewShotId)
  const loop = view.loop && (sequence ? Boolean(selected) : previewIndex >= 0)
  const start = !sequence && loop ? boundaries[previewIndex] : 0
  const end = sequence ? Math.round((selected ? videoShotDuration(selected) ?? 0 : 0) * project.fps) : loop ? boundaries[previewIndex + 1] : boundaries.at(-1)!
  const savedIndex = project.shots.findIndex(shot => shot.id === view.playheadShotId)
  const position = sequence ? view.playheadShotId === selectedId ? view.playheadFrame : 0 : savedIndex >= 0 ? boundaries[savedIndex] + view.playheadFrame : view.playheadFrame
  const playheadFrame = Math.max(start, Math.min(Math.max(0, end - 1), position))
  const selectedAudioId = project.audio?.some(audio => audio.id === view.selectedAudioId) ? view.selectedAudioId : ''
  return {
    ...view,
    selectedId,
    batchSelection: [...new Set(view.batchSelection)].filter(id => project.shots.some(shot => shot.id === id)),
    selectedAudioId,
    tab: view.tab === 'audio' && !selectedAudioId ? 'shot' as const : view.tab,
    inspector: view.inspector && project.shots.length > 0,
    previewShotId: sequence ? selectedId : previewIndex >= 0 ? view.previewShotId : '',
    playheadShotId: sequence ? selectedId : undefined,
    loop,
    playheadFrame,
    frame: playheadFrame - start,
  }
}
