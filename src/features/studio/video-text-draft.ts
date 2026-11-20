import { z } from 'zod'
import { newVideoShot, validateVideoProject, type VideoProject } from './video-editor-model'

export const videoTextDraftSchema = z.object({
  id: z.string().min(1),
  name: z.string().min(1),
  key: z.enum(['script', 'subtitle']),
  value: z.string(),
  original: z.string(),
})
export type VideoTextDraft = z.infer<typeof videoTextDraftSchema>

export function applyVideoTextDraft(project: VideoProject, draft: VideoTextDraft, overwrite = false): VideoProject {
  const shot = project.shots.find(shot => shot.id === draft.id)
  if (!shot) throw new Error('目标分镜已删除，草稿已保留')
  if (shot[draft.key] === draft.value) return project
  if (!overwrite && shot[draft.key] !== draft.original) throw new Error('文字已被其他入口更新，草稿已保留')
  const next = { ...project, shots: project.shots.map(shot => shot.id === draft.id ? { ...shot, [draft.key]: draft.value } : shot) }
  validateVideoProject(next)
  return next
}

export function recoverVideoTextDraft(project: VideoProject, draft: VideoTextDraft) {
  const shot = { ...newVideoShot(draft.name), [draft.key]: draft.value }
  const next = { ...project, shots: [...project.shots, shot] }
  validateVideoProject(next)
  return { project: next, shotId: shot.id }
}
