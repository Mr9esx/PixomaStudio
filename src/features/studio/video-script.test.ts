import { expect, it } from 'vitest'
import { emptyVideoProject, newVideoShot, normalizeVideoProject } from './video-editor-model'
import { addVideoScriptBlock, updateVideoScriptBlock, removeVideoScriptBlock, setVideoScriptSources, videoScriptSourceState, splitVideoScriptBlock, mergeVideoScriptBlocks, videoScriptSuccessors, validateVideoScript, importVideoScript } from './video-script'

it('文档采用进入完整脚本，明确追加或替换并保护现有分镜和并发编辑', () => {
  const original = normalizeVideoProject({ ...emptyVideoProject(), shots: [newVideoShot('开场', '晨光')] })
  const baseline = original.script!
  const appended = importVideoScript(original, baseline, 'imported', '茶园\n\n晨光照进窗户', 'append')
  expect(appended.script!.blocks.map(block => block.text)).toEqual(['晨光', '茶园\n\n晨光照进窗户'])
  expect(appended.shots).toEqual(original.shots)
  const replaced = importVideoScript(original, baseline, 'replacement', '新脚本', 'replace')
  expect(replaced.script!.blocks.map(block => block.text)).toEqual(['新脚本'])
  expect(replaced.script!.revision).toBe(baseline.revision + 1)
  expect(replaced.shots).toEqual(original.shots)
  expect(videoScriptSourceState(replaced.shots[0], replaced.script!)).toBe('missing')
  expect(() => importVideoScript(appended, baseline, 'another', '新内容', 'replace')).toThrow('脚本已修改')
  expect(() => importVideoScript(original, baseline, 'empty', '  ', 'append')).toThrow('没有可用')
})

it('旧项目建立稳定的完整脚本与来源标识，重复读取保持关联', () => {
  const original = { ...emptyVideoProject(), shots: [newVideoShot('开场', '清晨薄雾散开'), newVideoShot('结尾', '小林捧起茶杯')] }
  const normalized = normalizeVideoProject(original)
  expect(normalized.script?.blocks.map(block => block.text)).toEqual(['清晨薄雾散开', '小林捧起茶杯'])
  expect(normalized.shots[0].scriptSources?.[0].blockId).toBe(normalized.script?.blocks[0].id)
  expect(normalizeVideoProject(original)).toEqual(normalized)
})

it('文本分开与合并保留来源关系和分镜快照，用户明确关联新原文', () => {
  const original = normalizeVideoProject({ ...emptyVideoProject(), shots: [newVideoShot('开场', '晨光照进茶园')] })
  const id = original.script!.blocks[0].id
  const divided = splitVideoScriptBlock(original, id, 4, ['morning', 'garden'])
  expect(divided.script!.blocks.map(block => block.text)).toEqual(['晨光照进', '茶园'])
  expect(divided.shots).toEqual(original.shots)
  expect(videoScriptSuccessors(divided.script!, id).map(block => block.id)).toEqual(['morning', 'garden'])
  expect(videoScriptSourceState(divided.shots[0], divided.script!)).toBe('restructured')
  const combined = mergeVideoScriptBlocks(divided, ['morning', 'garden'], 'combined')
  expect(combined.script!.blocks[0]).toMatchObject({ text: '晨光照进\n\n茶园', ancestorIds: [id, 'morning', 'garden'] })
  expect(videoScriptSuccessors(combined.script!, id).map(block => block.id)).toEqual(['combined'])
  expect(videoScriptSuccessors(combined.script!, 'garden').map(block => block.id)).toEqual(['combined'])
  expect(validateVideoScript(combined)).toBeUndefined()
  const linked = setVideoScriptSources(combined.shots[0], combined.script!, ['combined'])
  expect(videoScriptSourceState(linked, combined.script!)).toBe('current')
  expect(() => addVideoScriptBlock(combined, id, '重复')).toThrow('重复')
  expect(() => splitVideoScriptBlock(original, id, 0, ['a', 'b'])).toThrow()
  expect(() => mergeVideoScriptBlocks(divided, ['morning', 'missing'], 'joined')).toThrow()
  expect(() => validateVideoScript({ ...combined, script: { ...combined.script!, blocks: [{ ...combined.script!.blocks[0], ancestorIds: ['combined'] }] } })).toThrow()
})

it('局部脚本修改只影响关联分镜，保留素材与采用的视频', () => {
  let project = normalizeVideoProject({ ...emptyVideoProject(), shots: [newVideoShot('开场', '薄雾'), newVideoShot('结尾', '茶杯')] })
  const first = project.script!.blocks[0].id
  const original = structuredClone(project)
  project = updateVideoScriptBlock(project, first, '晨光穿过薄雾')
  expect(videoScriptSourceState(project.shots[0], project.script!)).toBe('changed')
  expect(videoScriptSourceState(project.shots[1], project.script!)).toBe('current')
  expect(project.shots).toEqual(original.shots)
  expect(project.script!.blocks[0].revision).toBe(2)
  const restored = updateVideoScriptBlock(project, first, '薄雾')
  expect(videoScriptSourceState(restored.shots[0], restored.script!)).toBe('current')
})

it('原文删除保留分镜及当时原文，可关联到多个新的文本块', () => {
  let project = normalizeVideoProject({ ...emptyVideoProject(), shots: [newVideoShot('开场', '薄雾')] })
  const source = project.script!.blocks[0]
  project = removeVideoScriptBlock(project, source.id)
  expect(project.shots[0].scriptSources?.[0].text).toBe('薄雾')
  expect(videoScriptSourceState(project.shots[0], project.script!)).toBe('missing')
  project = addVideoScriptBlock(project, 'morning', '清晨')
  project = addVideoScriptBlock(project, 'tea', '茶园')
  const shot = setVideoScriptSources(project.shots[0], project.script!, ['morning', 'tea'])
  expect(shot.scriptSources?.map(source => source.text)).toEqual(['清晨', '茶园'])
  expect(videoScriptSourceState(shot, project.script!)).toBe('current')
  expect(() => setVideoScriptSources(shot, project.script!, ['missing'])).toThrow('不存在')
  expect(() => addVideoScriptBlock(project, 'tea', '重复')).toThrow('重复')
})
