import { adoptVideoMedia, fitVideoProjectTiming, validateVideoProject, type VideoMedia, type VideoProject, type VideoShot } from './video-editor-model'

export type VideoAdoption = { shot: VideoShot; media: VideoMedia; fps: number; newShot: boolean }
export type VideoAdoptionRange = { sourceIn: number; duration: number }

export function createVideoAdoption(project: VideoProject, shot: VideoShot, media: VideoMedia, newShot = false): VideoAdoption {
  return structuredClone({ shot, media, fps: project.fps, newShot })
}

export function previewVideoAdoption(project: VideoProject, request: VideoAdoption, range: VideoAdoptionRange): VideoProject {
  if (project.fps !== request.fps) throw new Error('项目帧率已改变，重新选择视频版本')
  const current = project.shots.find(shot => shot.id === request.shot.id)
  if (request.newShot && current) throw new Error('分镜已存在，重新选择目标')
  if (!request.newShot && !current) throw new Error('目标分镜已删除')
  if (current && (current.media?.assetId !== request.shot.media?.assetId || current.media?.versionId !== request.shot.media?.versionId || current.sourceIn !== request.shot.sourceIn || current.duration !== request.shot.duration || current.plannedDuration !== request.shot.plannedDuration)) throw new Error('当前视频、计划或剪辑已改变，重新选择视频版本')
  if ([range.sourceIn, range.duration].some(value => Math.abs(value * project.fps - Math.round(value * project.fps)) > 1e-6)) throw new Error('使用范围需要符合项目帧间隔')
  const adopted = adoptVideoMedia(current ?? request.shot, request.media, range)
  const next = fitVideoProjectTiming({ ...project, shots: request.newShot ? [...project.shots, adopted] : project.shots.map(shot => shot.id === adopted.id ? adopted : shot) })
  validateVideoProject(next)
  return next
}
