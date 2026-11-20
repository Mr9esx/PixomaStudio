import { describe, expect, it } from 'vitest'
import { emptyVideoProject, newVideoShot, setVideoReference, type VideoProject } from './video-editor-model'
import { applyVideoReferenceVersion, videoReferenceUpdateTargets } from './video-reference-update'

const reference = { id: 'character', assetId: 'person', versionId: 'v1', url: '/v1.png', kind: 'image' as const, role: 'character' as const, inputKey: 'image' }
const replacement = { assetId: 'person', versionId: 'v3', url: '/v3.png', kind: 'image' as const }
const project = (): VideoProject => ({ ...emptyVideoProject(), shots: ['opening', 'middle', 'ending'].map((id, index) => ({ ...setVideoReference({ ...newVideoShot(), id }, { ...reference, versionId: index === 2 ? 'v2' : 'v1' }), media: { assetId: 'video', versionId: 'video-v1', url: '/video.mp4', duration: 4 }, duration: 2 })) })

describe('素材版本应用范围', () => {
  it('默认选择相同旧版本，其他版本保留选择', () => {
    const targets = videoReferenceUpdateTargets(project(), 'person', 'v1')
    expect(targets.map(target => [target.shotId, target.selected])).toEqual([['opening', true], ['middle', true], ['ending', false]])
  })
  it('只更新选定引用和对应输入，保留采用视频及任务内容', () => {
    const original = project()
    const targets = videoReferenceUpdateTargets(original, 'person', 'v1').filter(target => target.selected)
    const updated = applyVideoReferenceVersion(original, targets, replacement)
    expect(updated.shots.slice(0, 2).every(shot => shot.references![0].versionId === 'v3' && (shot.inputs.image as { asset_version_id: string }).asset_version_id === 'v3')).toBe(true)
    expect(updated.shots[2]).toEqual(original.shots[2])
    expect(updated.shots.map(shot => shot.media)).toEqual(original.shots.map(shot => shot.media))
    expect(original.shots[0].references![0].versionId).toBe('v1')
  })
  it('保留手工设置的输入，解除该引用的输入关联', () => {
    const original = project()
    original.shots[0].inputs.image = { asset_id: 'other', asset_version_id: 'other-v1' }
    const targets = videoReferenceUpdateTargets(original, 'person', 'v1').filter(target => target.shotId === 'opening')
    const updated = applyVideoReferenceVersion(original, targets, replacement)
    expect(updated.shots[0].inputs).toEqual(original.shots[0].inputs)
    expect(updated.shots[0].references![0].versionId).toBe('v3')
    expect(updated.shots[0].references![0].inputKey).toBeUndefined()
  })
  it('引用、输入或工作流并发修改时拒绝覆盖，独立描述修改继续保留', () => {
    const original = project()
    const targets = videoReferenceUpdateTargets(original, 'person', 'v1').filter(target => target.selected)
    const changed = structuredClone(original)
    changed.shots[0].storyboard = '新的画面描述'
    expect(applyVideoReferenceVersion(changed, targets, replacement).shots[0].storyboard).toBe('新的画面描述')
    changed.shots[1].inputs.image = { asset_id: 'other', asset_version_id: 'other-v1' }
    expect(() => applyVideoReferenceVersion(changed, targets, replacement)).toThrow('已修改')
    expect(() => applyVideoReferenceVersion({ ...original, shots: original.shots.slice(1) }, targets, replacement)).toThrow('已删除')
    expect(() => applyVideoReferenceVersion(original, targets, { ...replacement, assetId: 'another' })).toThrow('不匹配')
  })
})
