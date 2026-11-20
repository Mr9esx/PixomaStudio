import { z } from 'zod'
import { validateVideoProject, type VideoProject, type VideoReference, type VideoShot } from './video-editor-model'
import { splitVideoShot } from './video-shot-structure'

const shotSchema: z.ZodType<VideoShot> = z.object({
  id: z.string().min(1), name: z.string(), script: z.string(), subtitle: z.string(),
  duration: z.number().nonnegative(), sourceIn: z.number().nonnegative(), inputs: z.record(z.string(), z.unknown()),
}).passthrough()

export const videoStructureDraftSchema = z.object({
  request: z.object({ mode: z.enum(['split', 'merge']), shotId: z.string().min(1), project: z.custom<VideoProject>().transform(project => { validateVideoProject(project); return project }) }),
  selectedIds: z.array(z.string().min(1)), settingsId: z.string().min(1),
  drafts: z.array(shotSchema).optional(), referenceChoices: z.array(z.custom<VideoReference>()),
})
export type VideoStructureDraft = z.infer<typeof videoStructureDraftSchema>

export function videoStructureDraftKey(mode: VideoStructureDraft['request']['mode'], shotId: string) { return JSON.stringify([mode, shotId]) }

export function createVideoStructureDraft(project: VideoProject, mode: VideoStructureDraft['request']['mode'], shotId: string): VideoStructureDraft {
  validateVideoProject(project)
  const snapshot = structuredClone(project)
  const index = snapshot.shots.findIndex(shot => shot.id === shotId)
  if (index < 0) throw new Error('来源分镜已删除')
  const drafts = mode === 'split' ? splitVideoShot(snapshot.shots[index], [crypto.randomUUID(), crypto.randomUUID()]) : undefined
  const start = Math.max(0, Math.min(index, snapshot.shots.length - 2))
  return { request: { mode, project: snapshot, shotId }, selectedIds: mode === 'split' ? [shotId] : snapshot.shots.slice(start, start + 2).map(shot => shot.id), settingsId: shotId, drafts, referenceChoices: drafts?.[0].references ?? [] }
}
