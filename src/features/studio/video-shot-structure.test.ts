import { describe, expect, it } from 'vitest'
import { emptyVideoProject, newVideoShot, type VideoProject, type VideoShot } from './video-editor-model'
import { applyVideoShotStructure, mergeVideoShots, splitVideoShot } from './video-shot-structure'
import { restoreVideoEditorHistory } from './video-editor-history'
import { videoDescriptionInputState } from './video-input-sources'

const original = (): VideoShot => ({ ...newVideoShot('茶园'), id: 'original', storyboard: '小林走入茶园并摘下茶叶', script: '人物进入茶园', subtitle: '清晨的茶园', plannedDuration: 6,
  workflowId: 'workflow', versionId: 'published', inputs: { prompt: '小林走入茶园', image: { asset_id: 'person', asset_version_id: 'person-v1' } },
  references: [{ id: 'person', assetId: 'person', versionId: 'person-v1', url: '/person.png', kind: 'image', role: 'character', inputKey: 'image' }],
  scriptSources: [{ blockId: 'opening', revision: 1, text: '小林走入茶园' }],
  media: { assetId: 'video', versionId: 'video-v1', url: '/video.mp4', duration: 8 }, duration: 4, sourceIn: 1,
  executionId: 'execution', executionInputs: '{}', generationRequest: { id: 'pending', workflowId: 'workflow', versionId: 'published', inputs: {} }, versions: [],
})
const project = (shots: VideoShot[]): VideoProject => ({ ...emptyVideoProject(), shots })

describe('分镜结构操作', () => {
  it('合并后保留描述来源并使用新引用标识保存工作流草稿映射', () => {
    const source = original()
    source.inputSources = { prompt: { field: 'storyboard', sourceText: source.storyboard!, value: String(source.inputs.prompt), edited: true } }
    source.workflowDrafts = { 'workflow:other': { inputs: source.inputs, inputSources: source.inputSources, referenceInputs: { person: 'image' } } }
    const merged = mergeVideoShots([source, { ...original(), id: 'second', storyboard: '人物观察茶叶' }], 'merged', source.id)
    expect(videoDescriptionInputState(merged, 'prompt')).toEqual({ changed: true, edited: true })
    expect(merged.workflowDrafts?.['workflow:other'].inputSources).toEqual(source.inputSources)
    expect(merged.workflowDrafts?.['workflow:other'].referenceInputs).toEqual({ [merged.references![0].id]: 'image' })
    expect(source.workflowDrafts['workflow:other'].referenceInputs).toEqual({ person: 'image' })
  })
  it('分成多个分镜时保留创作内容，使用新的标识和独立视频选择', () => {
    const source = original()
    const parts = splitVideoShot(source, ['arrival', 'picking'])
    expect(parts.map(shot => shot.id)).toEqual(['arrival', 'picking'])
    expect(parts.map(shot => shot.subtitle)).toEqual(['清晨的茶园', ''])
    expect(parts.every(shot => !shot.media && !shot.executionId && !shot.generationRequest && !shot.plannedDuration && !shot.duration && !shot.sourceIn && shot.versions?.length === 0)).toBe(true)
    expect(parts[0].references).toEqual(source.references)
    expect(parts[1].inputs).toEqual(source.inputs)
    expect(parts[1].scriptSources).toEqual(source.scriptSources)
    expect(source.media?.versionId).toBe('video-v1')
    expect(() => splitVideoShot(source, ['arrival'])).toThrow('至少')
  })
  it('合并描述与引用，采用指定分镜的生成设置，保留手工输入', () => {
    const left = original()
    const right = { ...original(), id: 'second', name: '摘茶', storyboard: '小林摘下茶叶', inputs: { prompt: '手部特写', image: { asset_id: 'other', asset_version_id: 'other-v1' } }, references: [{ ...left.references![0], id: 'person' }, { ...left.references![0], id: 'scene', assetId: 'garden', versionId: 'garden-v1', role: 'scene' as const, inputKey: undefined }], scriptSources: [{ blockId: 'picking', revision: 1, text: '小林摘下茶叶' }] }
    const merged = mergeVideoShots([left, right], 'merged', right.id)
    expect(merged.inputs).toEqual(right.inputs)
    expect(merged.storyboard).toBe(`${left.storyboard}\n\n${right.storyboard}`)
    expect(merged.references).toHaveLength(2)
    expect(new Set(merged.references!.map(reference => reference.id)).size).toBe(2)
    expect(merged.references!.every(reference => !reference.inputKey)).toBe(true)
    expect(merged.scriptSources?.map(source => source.blockId)).toEqual(['opening', 'picking'])
    expect(merged.media).toBeUndefined()
    expect(merged.executionId).toBeUndefined()
    expect(merged.duration).toBe(0)
  })
  it('采用结构预览时保留其他分镜、音频与下游固定画面引用', () => {
    const source = original()
    const next = { ...newVideoShot('下游'), id: 'downstream', references: [{ id: 'frame', assetId: 'frame', versionId: 'frame-v1', url: '/frame.png', kind: 'image' as const, role: 'first_frame' as const, sourceShotId: source.id, sourceMediaVersionId: 'video-v1' }] }
    const current = { ...project([source, next]), audio: [{ id: 'music', assetId: 'music', versionId: 'music-v1', url: '/music.mp3', start: 0, duration: 2, sourceIn: 0, volume: 1, loop: false }] }
    const parts = splitVideoShot(source, ['arrival', 'picking'])
    const updated = applyVideoShotStructure(current, [source], parts)
    expect(updated.shots.map(shot => shot.id)).toEqual(['arrival', 'picking', 'downstream'])
    expect(updated.shots[2]).toEqual(next)
    expect(updated.audio).toEqual(current.audio)
    expect(current.shots[0]).toEqual(source)
  })
  it('拒绝源分镜变化、非连续合并、旧标识复用和超过分镜上限', () => {
    const source = original()
    const second = { ...newVideoShot('第二镜'), id: 'second' }
    const third = { ...newVideoShot('第三镜'), id: 'third' }
    const parts = splitVideoShot(source, ['arrival', 'picking'])
    expect(() => applyVideoShotStructure(project([{ ...source, storyboard: '新的描述' }]), [source], parts)).toThrow('已修改')
    expect(() => applyVideoShotStructure(project([source, second, third]), [source, third], [mergeVideoShots([source, third], 'merged', source.id)])).toThrow('连续')
    expect(() => applyVideoShotStructure(project([source]), [source], [{ ...parts[0], id: source.id }, parts[1]])).toThrow('新标识')
    expect(() => applyVideoShotStructure(project([source, ...Array.from({ length: 99 }, (_, index) => ({ ...newVideoShot(), id: `shot-${index}` }))]), [source], parts)).toThrow('100')
  })
  it('合并首尾画面需要明确选择，保留用户选择的原文版本', () => {
    const left = original()
    left.references = [{ ...left.references![0], role: 'first_frame' }]
    const right = { ...original(), id: 'second', references: [{ ...left.references[0], assetId: 'garden', versionId: 'garden-v1', inputKey: undefined }], scriptSources: [{ blockId: 'opening', revision: 2, text: '修改后的茶园原文' }] }
    const merged = mergeVideoShots([left, right], 'merged', left.id)
    expect(() => applyVideoShotStructure(project([left, right]), [left, right], [merged])).toThrow('只能选择一个')
    merged.references = merged.references!.slice(0, 1)
    merged.scriptSources = right.scriptSources
    const result = applyVideoShotStructure(project([left, right]), [left, right], [merged])
    expect(result.shots[0].scriptSources).toEqual(right.scriptSources)
    expect(result.shots[0].references![0].assetId).toBe('person')
  })
  it('撤销结构操作恢复原采用视频与剪辑，不重复提交原请求', () => {
    const source = original()
    const before = project([source])
    const after = applyVideoShotStructure(before, [source], splitVideoShot(source, ['arrival', 'picking']))
    const restored = restoreVideoEditorHistory(before, after, [])
    expect(restored.shots[0].id).toBe(source.id)
    expect(restored.shots[0].media).toEqual(source.media)
    expect([restored.shots[0].sourceIn, restored.shots[0].duration]).toEqual([1, 4])
    expect(restored.shots[0].generationRequest).toBeUndefined()
  })
  it('来源标识包含分隔字符时仍产生独立的引用标识', () => {
    const left = { ...original(), id: 'a:b' }
    left.references = [{ ...left.references![0], id: 'c' }]
    const right = { ...original(), id: 'a', references: [{ ...left.references[0], id: 'b:c', assetId: 'garden', versionId: 'garden-v1', inputKey: undefined }] }
    const merged = mergeVideoShots([left, right], 'merged', left.id)
    expect(new Set(merged.references!.map(reference => reference.id)).size).toBe(2)
    expect(() => applyVideoShotStructure(project([left, right]), [left, right], [merged])).not.toThrow()
  })
})
