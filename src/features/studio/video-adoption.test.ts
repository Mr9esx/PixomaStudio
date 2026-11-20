import { expect, it } from 'vitest'
import { emptyVideoProject, newVideoShot } from './video-editor-model'
import { createVideoAdoption, previewVideoAdoption } from './video-adoption'

it('采用预览同时计算成片与已有音频范围，保留期间的文字修改', () => {
  const shot = { ...newVideoShot('开场'), duration: 4, media: { assetId: 'old', versionId: 'old-v1', url: '/old.mp4', duration: 5 } }
  const project = { ...emptyVideoProject(), shots: [shot, { ...newVideoShot('结尾'), duration: 4 }], audio: [{ id: 'audio', assetId: 'audio', versionId: 'audio-v1', url: '/audio.wav', sourceIn: 0, start: 0, duration: 8, volume: 1, loop: false }] }
  const media = { assetId: 'new', versionId: 'new-v1', url: '/new.mp4', duration: 2 }
  const request = createVideoAdoption(project, shot, media)
  const current = { ...project, shots: [{ ...shot, name: '修改名称' }, project.shots[1]] }
  const next = previewVideoAdoption(current, request, { sourceIn: 0, duration: 2 })
  expect(next.shots[0]).toMatchObject({ name: '修改名称', duration: 2, media })
  expect(next.audio?.[0].duration).toBe(6)
  expect(project.audio[0].duration).toBe(8)
  expect(next.shots[1]).toEqual(current.shots[1])
})

it('采用窗口期间目标删除、采用版本、剪辑或帧率改变时拒绝旧请求', () => {
  const shot = newVideoShot('开场')
  const project = { ...emptyVideoProject(), shots: [shot] }
  const media = { assetId: 'video', versionId: 'v1', url: '/video.mp4', duration: 5 }
  const request = createVideoAdoption(project, shot, media)
  for (const current of [
    { ...project, shots: [] },
    { ...project, fps: 24 },
    { ...project, shots: [{ ...shot, duration: 4 }] },
    { ...project, shots: [{ ...shot, sourceIn: 1 }] },
    { ...project, shots: [{ ...shot, media }] },
  ]) expect(() => previewVideoAdoption(current, request, { sourceIn: 0, duration: 5 })).toThrow()
  expect(() => previewVideoAdoption(project, request, { sourceIn: 0, duration: 1.01 })).toThrow('帧')
})

it('从资产创建分镜时直到采用才加入作品，并固定来源对象', () => {
  const project = emptyVideoProject()
  const shot = newVideoShot('导入视频')
  const media = { assetId: 'video', versionId: 'v1', url: '/video.mp4', duration: 5 }
  const request = createVideoAdoption(project, shot, media, true)
  shot.name = '另一个名称'
  const next = previewVideoAdoption(project, request, { sourceIn: 0, duration: 5 })
  expect(next.shots[0]).toMatchObject({ name: '导入视频', media, duration: 5 })
  expect(project.shots).toEqual([])
  expect(() => previewVideoAdoption(next, request, { sourceIn: 0, duration: 5 })).toThrow('已存在')
})
