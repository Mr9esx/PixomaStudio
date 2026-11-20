import { describe, expect, it } from 'vitest'
import { emptyVideoProject, teaVideoShots } from './video-editor-model'
import { editVideoNumber, renameVideoShot, setVideoAudioLoop } from './video-property-edit'

function project() {
  return { ...emptyVideoProject(), shots: teaVideoShots().slice(0, 2), audio: [{ id: 'music', assetId: 'audio', versionId: 'audio-v1', url: '/music.wav', start: 1, duration: 4, sourceIn: 0, sourceDuration: 8, volume: 1, loop: false }] }
}

describe('视频属性数值编辑', () => {
  it('名称输入通过校验后提交，空白名称不改变作品', () => {
    const source = project()
    const before = structuredClone(source)
    const shot = source.shots[0]
    const next = renameVideoShot(source, shot.id, { text: '  清晨茶园  ', baseline: shot.name })
    expect(next.shots[0]).toEqual({ ...shot, name: '清晨茶园' })
    expect(source).toEqual(before)
    expect(() => renameVideoShot(source, shot.id, { text: '  ', baseline: shot.name })).toThrow('填写分镜名称')
    expect(renameVideoShot(source, shot.id, { text: shot.name, baseline: shot.name })).toBe(source)
  })

  it('名称修改检查目标与编辑基线，保留其它入口的新内容', () => {
    const source = project()
    const shot = source.shots[0]
    const updated = { ...source, shots: source.shots.map(item => item.id === shot.id ? { ...item, name: '雨后茶园', storyboard: '水珠沿着茶叶滑动' } : item) }
    expect(() => renameVideoShot(updated, shot.id, { text: '清晨茶园', baseline: shot.name })).toThrow('分镜名称已更新')
    expect(renameVideoShot(updated, shot.id, { text: '清晨茶园', baseline: '雨后茶园' }).shots[0]).toMatchObject({ name: '清晨茶园', storyboard: '水珠沿着茶叶滑动' })
    expect(() => renameVideoShot(updated, 'removed', { text: '清晨茶园', baseline: shot.name })).toThrow('分镜已被删除')
  })

  it('完整输入通过校验后形成一个作品修改，原始作品保持不变', () => {
    const source = project()
    const before = structuredClone(source)
    const next = editVideoNumber(source, { kind: 'shot', id: source.shots[0].id, field: 'duration' }, { text: '4.02', baseline: 3 })
    expect(next.shots[0].duration).toBe(121 / 30)
    expect(next.shots[1]).toEqual(source.shots[1])
    expect(source).toEqual(before)
    expect(editVideoNumber(source, { kind: 'shot', id: source.shots[0].id, field: 'volume' }, { text: '0', baseline: 1 }).shots[0].volume).toBe(0)
  })

  it('空白、非法数字、负值、零时长和超出音量范围均不能写入作品', () => {
    const source = project()
    const duration = { kind: 'shot' as const, id: source.shots[0].id, field: 'duration' as const }
    for (const text of ['', ' ', '-', '1e', 'Infinity', '-0.001', '0', '0.001']) expect(() => editVideoNumber(source, duration, { text, baseline: 3 })).toThrow()
    expect(() => editVideoNumber(source, { ...duration, field: 'volume' }, { text: '2.1', baseline: 1 })).toThrow()
    expect(() => editVideoNumber(source, duration, { text: '3600', baseline: 3 })).toThrow()
  })

  it('拒绝越过源视频和成片边界，音频长度不会随输入静默缩短', () => {
    const source = project()
    source.shots[0].media = { assetId: 'video', versionId: 'video-v1', url: '/video.mp4', duration: 4 }
    expect(() => editVideoNumber(source, { kind: 'shot', id: source.shots[0].id, field: 'sourceIn' }, { text: '2', baseline: 0 })).toThrow('剪辑范围超出源视频')
    const audio = { kind: 'audio' as const, id: 'music', field: 'start' as const }
    expect(() => editVideoNumber(source, audio, { text: '3', baseline: 1 })).toThrow('音频超出成片范围')
    expect(() => editVideoNumber(source, { ...audio, field: 'sourceIn' }, { text: '5', baseline: 0 })).toThrow('音频剪辑范围超出素材')
    const looped = { ...source, audio: source.audio.map(item => ({ ...item, loop: true })) }
    expect(editVideoNumber(looped, { ...audio, field: 'sourceIn' }, { text: '5', baseline: 0 }).audio?.[0].duration).toBe(4)
    expect(source.audio[0].duration).toBe(4)
  })

  it('同一字段并发更新时保留输入，明确采用后使用最新作品的其它字段', () => {
    const source = project()
    const target = { kind: 'shot' as const, id: source.shots[0].id, field: 'duration' as const }
    const updated = { ...source, shots: source.shots.map((shot, index) => index === 0 ? { ...shot, duration: 4, storyboard: '雨后的茶园' } : shot) }
    expect(() => editVideoNumber(updated, target, { text: '5', baseline: 3 })).toThrow('该数值已更新')
    const accepted = editVideoNumber(updated, target, { text: '5', baseline: 4 })
    expect(accepted.shots[0]).toMatchObject({ duration: 5, storyboard: '雨后的茶园' })
    expect(editVideoNumber(updated, target, { text: '4', baseline: 3 })).toBe(updated)
    expect(() => editVideoNumber(updated, { ...target, id: 'removed' }, { text: '5', baseline: 3 })).toThrow('分镜已被删除')
  })

  it('缩短分镜提供包含音频影响的修改，供现有确认流程处理', () => {
    const source = project()
    const next = editVideoNumber(source, { kind: 'shot', id: source.shots[0].id, field: 'duration' }, { text: '1', baseline: 3 })
    expect(next.shots[0].duration).toBe(1)
    expect(next.audio?.[0].duration).toBe(3)
    expect(source.audio[0].duration).toBe(4)
  })

  it('开启循环只修改播放方式，素材范围足够时关闭循环保留时长', () => {
    const source = project()
    const looped = setVideoAudioLoop(source, 'music', true)
    expect(looped.audio?.[0]).toEqual({ ...source.audio[0], loop: true })
    expect(source.audio[0].loop).toBe(false)
    expect(setVideoAudioLoop(looped, 'music', false).audio?.[0]).toEqual(source.audio[0])
    expect(setVideoAudioLoop(source, 'music', false)).toBe(source)
  })

  it('关闭循环按素材剩余范围和整数帧形成待确认修改，保留原作品', () => {
    const source = { ...project(), audio: project().audio.map(audio => ({ ...audio, loop: true, sourceIn: 2 })) }
    const next = setVideoAudioLoop(source, 'music', false, 3.02)
    expect(next.audio?.[0]).toMatchObject({ loop: false, sourceIn: 2, sourceDuration: 3.02, duration: 1 })
    expect(next.audio?.[0].start).toBe(source.audio[0].start)
    expect(source.audio[0]).toMatchObject({ loop: true, duration: 4, sourceDuration: 8 })
    expect(next.shots).toEqual(source.shots)
  })

  it('缺失素材时长、剩余不足一帧或音频已删除时拒绝修改', () => {
    const source = { ...project(), audio: project().audio.map(audio => ({ ...audio, loop: true, sourceIn: 2, sourceDuration: undefined })) }
    expect(() => setVideoAudioLoop(source, 'music', false)).toThrow('音频时长尚未就绪')
    expect(() => setVideoAudioLoop(source, 'music', false, 2.01)).toThrow('至少保留一帧')
    expect(() => setVideoAudioLoop(source, 'music', false, NaN)).toThrow('音频时长无效')
    expect(() => setVideoAudioLoop(source, 'removed', false, 3)).toThrow('音频已被删除')
  })
})
