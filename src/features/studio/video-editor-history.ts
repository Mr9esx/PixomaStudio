import { validateVideoProject, type VideoProject, type VideoShot } from './video-editor-model'

export type VideoGenerationState = Pick<VideoShot, 'generationRequest' | 'executionId' | 'executionInputs'>

export function updateVideoGenerationState(project: VideoProject, shotId: string, changes: Partial<VideoGenerationState>): VideoProject {
  const shot = project.shots.find(item => item.id === shotId)
  if (!shot || Object.entries(changes).every(([key, value]) => JSON.stringify(shot[key as keyof VideoGenerationState]) === JSON.stringify(value))) return project
  return { ...project, shots: project.shots.map(item => item.id === shotId ? { ...item, ...changes } : item) }
}

export function videoHistoryProblem(snapshot: VideoProject, current: VideoProject, protectedShotIds: string[]) {
  const targetIds = new Set(snapshot.shots.map(shot => shot.id))
  const protectedIds = new Set(protectedShotIds)
  const removed = current.shots.find(shot => protectedIds.has(shot.id) && !targetIds.has(shot.id))
  if (removed) return `「${removed.name}」的生成任务尚未结束或状态待确认，暂时无法移除该分镜`
}

export function restoreVideoEditorHistory(snapshot: VideoProject, current: VideoProject, protectedShotIds: string[]): VideoProject {
  const problem = videoHistoryProblem(snapshot, current, protectedShotIds)
  if (problem) throw new Error(problem)
  const shots = snapshot.shots.map(shot => {
    const latest = current.shots.find(item => item.id === shot.id)
    if (!latest) return { ...shot, generationRequest: undefined }
    const versions: NonNullable<VideoShot['versions']> = []
    const mediaIds = new Set(shot.media ? [shot.media.versionId] : [])
    for (const version of [...(shot.versions ?? []), ...(latest.versions ?? []), ...(latest.media ? [{ id: latest.media.versionId, media: latest.media, status: 'history' as const, sourceIn: latest.sourceIn, duration: latest.duration }] : [])]) {
      if (mediaIds.has(version.media.versionId)) continue
      mediaIds.add(version.media.versionId)
      versions.push(version)
    }
    return { ...shot, generationRequest: latest.generationRequest, executionId: latest.executionId, executionInputs: latest.executionInputs, versions }
  })
  const restored = { ...snapshot, shots }
  validateVideoProject(restored)
  return restored
}
