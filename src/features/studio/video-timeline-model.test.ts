import { describe, expect, it } from 'vitest'
import { emptyVideoProject, teaVideoShots, validateVideoProject } from './video-editor-model'
import { createTimelineRows, editTimelineProject, fitTimelineZoom, timelineRowAt, timelineScale, videoAssetDropTarget } from './video-timeline-model'

const project = () => ({ ...emptyVideoProject(), shots: teaVideoShots().map((shot, index) => ({ ...shot, id: `shot-${index}`, media: { assetId: 'media', versionId: 'version', url: '/video.mp4', duration: 5 } })) })

describe('开源时间轴与视频项目', () => {
  it('轨道高度与收起状态只改变显示，拖入位置按实际轨道边界确定', () => {
    const source = project()
    const before = structuredClone(source)
    const display = { trackHeights: { shot: 160, video: 64 }, collapsedTracks: ['shot'] }
    const rows = createTimelineRows(source, 'shot-1', 'shot', undefined, display)
    expect(rows.map(row => row.rowHeight)).toEqual([28, 42, 64])
    expect(timelineRowAt(rows, 27)?.row.id).toBe('shot')
    expect(timelineRowAt(rows, 28)).toMatchObject({ row: { id: 'reference' }, top: 28, height: 42 })
    expect(timelineRowAt(rows, 70)).toMatchObject({ row: { id: 'video' }, top: 70, height: 64 })
    expect(timelineRowAt(rows, 133)?.row.id).toBe('video')
    expect(timelineRowAt(rows, 134)).toBeUndefined()
    expect(timelineRowAt(rows, -1)).toBeUndefined()
    expect(() => timelineRowAt(rows, NaN)).toThrow()
    const expanded = createTimelineRows(source, '', undefined, undefined, { ...display, collapsedTracks: [] })
    expect(expanded[0].rowHeight).toBe(160)
    expect(expanded[0].actions).toEqual(createTimelineRows(source, '')[0].actions)
    expect(source).toEqual(before)
  })

  it('适应全部包含一小时作品，刻度随缩放保持可读间隔', () => {
    const zoom = fitTimelineZoom(360, 3600)
    expect(3600 * 48 * zoom).toBeLessThanOrEqual(360 - 120)
    expect(zoom).toBeGreaterThan(0)
    const scale = timelineScale(zoom)
    expect(scale * 48 * zoom).toBeGreaterThanOrEqual(48)
    expect(fitTimelineZoom(900, 0)).toBe(1)
    expect(fitTimelineZoom(900, 1)).toBeLessThanOrEqual(4)
    expect(() => fitTimelineZoom(0, 10)).toThrow('时间轴尺寸无效')
  })

  it('资产拖入按轨道类型和整数帧定位，片尾只允许新建视频分镜', () => {
    const source = project()
    expect(videoAssetDropTarget(source, 'image', 'reference', 3).shotId).toBe('shot-1')
    expect(videoAssetDropTarget(source, 'image', 'shot', 2.999).shotId).toBe('shot-1')
    expect(videoAssetDropTarget(source, 'image', 'shot', 1).allowed).toBe(false)
    expect(videoAssetDropTarget(source, 'document', 'script', 1).allowed).toBe(false)
    expect(videoAssetDropTarget(source, 'image', 'video', 1).allowed).toBe(false)
    expect(videoAssetDropTarget(source, 'video', 'video', 15)).toMatchObject({ allowed: true, shotId: undefined })
    expect(videoAssetDropTarget(source, 'audio', 'audio:music', 15).allowed).toBe(false)
    expect(videoAssetDropTarget(source, 'audio', 'audio:music', 14).allowed).toBe(true)
    expect(videoAssetDropTarget(source, 'image', 'reference', 15).allowed).toBe(false)
    expect(videoAssetDropTarget(source, 'document', 'subtitle', 1).allowed).toBe(false)
    expect(videoAssetDropTarget(emptyVideoProject(), 'video', 'video', 0).allowed).toBe(true)
  })

  it('属性选择标记相应轨道，已有音频保持项目数据', () => {
    const source = { ...project(), audio: [{ id: 'music', assetId: 'a', versionId: 'v', url: '/a.wav', start: 0, duration: 1, sourceIn: 0, volume: 1, loop: false }] }
    const script = createTimelineRows(source, 'shot-1', 'shot')
    expect(script.flatMap(row => row.actions).filter(action => action.selected).map(action => action.id)).toEqual(['shot:shot-1'])
    const audio = createTimelineRows(source, 'shot-1', 'audio', 'music')
    expect(audio.flatMap(row => row.actions).filter(action => action.selected)).toEqual([])
    expect(source.audio[0].id).toBe('music')
  })

  it('三条轨道共享整数帧边界，分镜提供更高的内容区域', () => {
    const source = project()
    const rows = createTimelineRows(source, 'shot-1')
    expect(rows.map(row => row.id)).toEqual(['shot', 'reference', 'video'])
    expect(rows[0].rowHeight).toBe(128)
    for (const row of rows) {
      expect(row.actions.map(action => [action.start, action.end])).toEqual([[0, 3], [3, 6], [6, 9], [9, 12], [12, 15]])
      expect(row.actions[1].selected).toBe(true)
    }
    rows[0].actions[0].end = 99
    expect(source.shots[0].duration).toBe(3)
    expect(rows[1].actions[0].end).toBe(3)
  })
  it('移动分镜保持三条轨道顺序和连续边界，保留所有业务字段', () => {
    const source = project()
    const next = editTimelineProject(source, { kind: 'move', shotId: 'shot-0', start: 7, end: 10 })
    expect(next.shots.map(shot => shot.id)).toEqual(['shot-1', 'shot-2', 'shot-0', 'shot-3', 'shot-4'])
    expect(next.shots[2]).toEqual(source.shots[0])
    expect(createTimelineRows(next, '')[2].actions.map(action => action.start)).toEqual([0, 3, 6, 9, 12])
    expect(source.shots[0].id).toBe('shot-0')
  })
  it('左右裁剪按帧取整并限制媒体范围，前后分镜共同移动', () => {
    const source = project()
    const right = editTimelineProject(source, { kind: 'right', shotId: 'shot-1', start: 3, end: 7.01 })
    expect(right.shots[1].duration).toBe(4)
    expect(createTimelineRows(right, '')[0].actions[2].start).toBe(7)
    const left = editTimelineProject(source, { kind: 'left', shotId: 'shot-1', start: 4, end: 6 })
    expect(left.shots[1].sourceIn).toBe(1)
    expect(left.shots[1].duration).toBe(2)
    expect(createTimelineRows(left, '')[2].actions[2].start).toBe(5)
    expect(() => validateVideoProject(left)).not.toThrow()
    expect(() => editTimelineProject(source, { kind: 'right', shotId: 'shot-1', start: 3, end: 9 })).toThrow()
    expect(() => editTimelineProject(source, { kind: 'left', shotId: 'shot-1', start: 2, end: 6 })).toThrow()
    expect(() => editTimelineProject(source, { kind: 'right', shotId: 'shot-1', start: 3, end: 3 })).toThrow()
  })
  it('不同时长的分镜可以移动到任意顺序位置', () => {
    const source = project()
    source.shots = source.shots.slice(0, 2).map((shot, index) => ({ ...shot, duration: index === 0 ? 1 : 3 }))
    const before = editTimelineProject(source, { kind: 'move', shotId: 'shot-1', start: 0, end: 3 })
    expect(before.shots.map(shot => shot.id)).toEqual(['shot-1', 'shot-0'])
    const after = editTimelineProject(before, { kind: 'move', shotId: 'shot-1', start: 1, end: 4 })
    expect(after.shots.map(shot => shot.id)).toEqual(['shot-0', 'shot-1'])
  })
  it('音频独立移动、左右裁剪，允许循环并限制片尾与素材边界', () => {
    const source = { ...project(), audio: [{ id: 'music', assetId: 'audio', versionId: 'v', url: '/audio.mp3', start: 1, duration: 3, sourceIn: 1, sourceDuration: 6, volume: .7, loop: false }] }
    const moved = editTimelineProject(source, { kind: 'move', audioId: 'music', start: 4.01, end: 7.01 })
    expect(moved.audio?.[0]).toEqual({ ...source.audio[0], start: 4 })
    const left = editTimelineProject(source, { kind: 'left', audioId: 'music', start: 2, end: 4 })
    expect(left.audio?.[0]).toEqual({ ...source.audio[0], start: 2, sourceIn: 2, duration: 2 })
    expect(() => editTimelineProject(source, { kind: 'right', audioId: 'music', start: 1, end: 8 })).toThrow()
    expect(() => editTimelineProject(source, { kind: 'move', audioId: 'music', start: 14, end: 17 })).toThrow()
    expect(createTimelineRows(source, '').map(row => row.id)).toEqual(['shot', 'reference', 'video'])
  })
  it('拒绝未知对象和非有限时间，空项目保留固定轨道', () => {
    expect(createTimelineRows(emptyVideoProject(), '')).toHaveLength(3)
    expect(() => editTimelineProject(project(), { kind: 'move', shotId: 'missing', start: 0, end: 3 })).toThrow()
    expect(() => editTimelineProject(project(), { kind: 'move', shotId: 'shot-0', start: NaN, end: 3 })).toThrow()
  })
})
