import { expect, it } from 'vitest'
import { emptyVideoProject, newVideoShot, normalizeVideoProject, adoptVideoMedia, fitVideoProjectTiming, validateVideoProject, videoProjectTiming, videoShotDuration, videoTimelineFrames, previewVideoShot } from './video-editor-model'
import { editVideoNumber } from './video-property-edit'
import { createTimelineRows, editTimelineProject } from './video-timeline-model'
import { videoExportIssues } from './video-export-model'

it('新分镜时长未定，未知分镜之后不提供绝对开始时间', () => {
  const unknown = newVideoShot('开场')
  expect(unknown.duration).toBe(0)
  expect(videoShotDuration(unknown)).toBeUndefined()
  const project = { ...emptyVideoProject(), shots: [{ ...newVideoShot('前景'), plannedDuration: 2 }, unknown, { ...newVideoShot('结尾'), plannedDuration: 4 }] }
  expect(videoProjectTiming(project)).toMatchObject({ knownDuration: 6, unknownCount: 1, starts: [0, 2, undefined] })
  expect(() => validateVideoProject(project)).not.toThrow()
  expect(videoTimelineFrames([unknown], 30)).toEqual([0, 0])
  const rows = createTimelineRows(project, unknown.id)
  expect(rows[0].actions.map(item => [item.start, item.end])).toEqual([[0, 1], [1, 2], [2, 3]])
  expect(rows[2].actions.every(item => item.flexible === false)).toBe(true)
  const reordered = editTimelineProject(project, { kind: 'move', shotId: project.shots[2].id, start: 0, end: 1 })
  expect(reordered.shots[0].name).toBe('结尾')
  expect(videoProjectTiming(reordered).starts).toEqual([0, 4, 6])
})

it('计划时长、工作流输入与视频使用范围独立，清除计划恢复未定', () => {
  let project = { ...emptyVideoProject(), shots: [{ ...newVideoShot(), inputs: { duration: 8 } }] }
  const target = { kind: 'shot' as const, id: project.shots[0].id, field: 'plannedDuration' as const }
  project = editVideoNumber(project, target, { text: '3', baseline: 0 }) as typeof project
  expect(project.shots[0]).toMatchObject({ plannedDuration: 3, duration: 0, inputs: { duration: 8 } })
  const adopted = adoptVideoMedia(project.shots[0], { assetId: 'media', versionId: 'version', url: '/media.mp4', duration: 5 }, { sourceIn: 1, duration: 4 })
  expect(videoShotDuration(adopted)).toBe(4)
  expect(adopted.plannedDuration).toBe(3)
  expect(adopted.inputs.duration).toBe(8)
  const changed = editVideoNumber({ ...project, shots: [adopted] }, target, { text: '7', baseline: 3 })
  expect(changed.shots[0].duration).toBe(4)
  expect(videoShotDuration(changed.shots[0])).toBe(4)
  const cleared = editVideoNumber(project, target, { text: '', baseline: 3 })
  expect(videoShotDuration(cleared.shots[0])).toBeUndefined()
})

it('旧分镜的计划迁移保持时间，未定项目保留已有音频', () => {
  const legacy = { ...emptyVideoProject(), shots: [{ ...newVideoShot(), duration: 3 }] }
  const project = normalizeVideoProject(legacy)
  expect(project.shots[0]).toMatchObject({ plannedDuration: 3, duration: 0 })
  expect(videoShotDuration(project.shots[0])).toBe(3)
  expect(normalizeVideoProject(project)).toEqual(project)
  const audio = { id: 'audio', assetId: 'a', versionId: 'v', url: '/audio.wav', start: 2, duration: 8, sourceIn: 0, volume: 1, loop: false }
  const incomplete = { ...project, shots: [newVideoShot(), ...project.shots], audio: [audio] }
  expect(fitVideoProjectTiming(incomplete).audio).toEqual([audio])
  expect(() => validateVideoProject(incomplete)).not.toThrow()
  expect(previewVideoShot(incomplete, project.shots[0].id).audio).toEqual([])
  expect(videoExportIssues(incomplete)).toEqual(incomplete.shots.map(shot => ({ kind: 'shot', id: shot.id, message: '尚未采用视频' })))
})

it('尚未采用视频的分镜通过计划字段安排时间，时间轴不修改视频范围', () => {
  const project = { ...emptyVideoProject(), shots: [{ ...newVideoShot(), plannedDuration: 3 }] }
  expect(() => editTimelineProject(project, { kind: 'right', shotId: project.shots[0].id, start: 0, end: 4 })).toThrow('采用视频后调整使用范围')
  expect(project.shots[0]).toMatchObject({ plannedDuration: 3, duration: 0 })
})
