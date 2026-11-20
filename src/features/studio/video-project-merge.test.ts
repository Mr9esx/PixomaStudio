import { expect, it } from 'vitest'
import { emptyVideoProject, newVideoShot, normalizeVideoProject, reviseVideoShotContent } from './video-editor-model'
import { updateVideoScriptBlock, splitVideoScriptBlock, validateVideoScript } from './video-script'
import { mergeVideoProjects } from './video-project-merge'

it('不同文字字段的并发修改保留各自修订号', () => {
  const base = normalizeVideoProject({ ...emptyVideoProject(), shots: [newVideoShot('开场')] })
  const local = { ...base, shots: [reviseVideoShotContent(base.shots[0], { ...base.shots[0], script: '人物进入' })] }
  const remote = { ...base, shots: [reviseVideoShotContent(base.shots[0], { ...base.shots[0], storyboard: '人物近景' })] }
  const result = mergeVideoProjects(base, local, remote)
  expect(result.conflicts).toEqual([])
  expect(result.project.shots[0]).toMatchObject({ script: '人物进入', storyboard: '人物近景', contentRevisions: { script: 2, storyboard: 2 } })
})

it('文本结构与其他原文独立合并，同一原文结构冲突保留双方完整内容', () => {
  const base = normalizeVideoProject({ ...emptyVideoProject(), shots: [newVideoShot('开场', '清晨薄雾'), newVideoShot('结尾', '傍晚')] })
  const local = splitVideoScriptBlock(base, base.script!.blocks[0].id, 2, ['morning', 'mist'])
  const unrelated = updateVideoScriptBlock(base, base.script!.blocks[1].id, '晚霞')
  const merged = mergeVideoProjects(base, local, unrelated)
  expect(merged.conflicts).toEqual([])
  expect(merged.project.script!.blocks.map(block => block.text)).toEqual(['清晨', '薄雾', '晚霞'])
  expect(validateVideoScript(merged.project)).toBeUndefined()
  const remote = updateVideoScriptBlock(base, base.script!.blocks[0].id, '阳光照进山间')
  const conflict = mergeVideoProjects(base, local, remote, 'remote')
  expect(conflict.project.script).toEqual(remote.script)
  expect(conflict.conflicts).toEqual([{ object: 'script', field: 'structure', local: local.script, remote: remote.script }])
  expect(validateVideoScript(conflict.project)).toBeUndefined()
})

it('合并不同脚本文本块与不同创作提案，保留局部采用结果', () => {
  const base = normalizeVideoProject({ ...emptyVideoProject(), shots: [newVideoShot('开场', '清晨'), newVideoShot('结尾', '傍晚')] })
  const local = updateVideoScriptBlock(base, base.script!.blocks[0].id, '晨光穿过窗户')
  const remote = updateVideoScriptBlock(base, base.script!.blocks[1].id, '晚霞映红天空')
  const result = mergeVideoProjects(base, local, remote)
  expect(result.conflicts).toEqual([])
  expect(result.project.script!.blocks.map(block => block.text)).toEqual(['晨光穿过窗户', '晚霞映红天空'])
  const proposal = { id: 'proposal', kind: 'script' as const, baseline: base.script!, script: base.script! }
  const proposals = mergeVideoProjects({ ...base, proposals: [proposal] }, { ...base, proposals: [] }, { ...base, proposals: [proposal, { ...proposal, id: 'second' }] })
  expect(proposals.conflicts).toEqual([])
  expect(proposals.project.proposals?.map(item => item.id)).toEqual(['second'])
})

it('合并不同字段与不同分镜修改，保留两侧新增对象', () => {
  const base = { ...emptyVideoProject(), shots: [newVideoShot('开场'), newVideoShot('结尾')] }
  const local = structuredClone(base), remote = structuredClone(base)
  local.shots[0].script = '人物进入'; remote.shots[0].subtitle = '清晨'
  local.shots.push(newVideoShot('片尾')); remote.shots[1].storyboard = '近景'
  const result = mergeVideoProjects(base, local, remote)
  expect(result.conflicts).toEqual([])
  expect(result.project.shots).toHaveLength(3)
  expect(result.project.shots[0]).toMatchObject({ script: '人物进入', subtitle: '清晨' })
  expect(result.project.shots[1].storyboard).toBe('近景')
})

it('同字段冲突支持分别采用两侧内容，其他修改保持合并', () => {
  const base = { ...emptyVideoProject(), shots: [newVideoShot('开场')] }
  const local = structuredClone(base), remote = structuredClone(base)
  local.shots[0].script = '本地剧情'; remote.shots[0].script = 'Chat 剧情'; remote.name = '作品新名称'
  const result = mergeVideoProjects(base, local, remote)
  expect(result.conflicts).toHaveLength(1)
  expect(result.conflicts[0]).toMatchObject({ field: 'script', local: '本地剧情', remote: 'Chat 剧情' })
  expect(result.project.name).toBe('作品新名称')
  expect(result.project.shots[0].script).toBe('本地剧情')
  expect(mergeVideoProjects(base, local, remote, 'remote').project.shots[0].script).toBe('Chat 剧情')
})

it('区分未修改对象的删除与删除期间的编辑', () => {
  const base = { ...emptyVideoProject(), shots: [newVideoShot('开场'), newVideoShot('结尾')] }
  const local = structuredClone(base), remote = structuredClone(base)
  remote.shots.shift()
  expect(mergeVideoProjects(base, local, remote).project.shots).toEqual(remote.shots)
  local.shots[0].script = '正在编辑'
  expect(mergeVideoProjects(base, local, remote).conflicts).toHaveLength(1)
  expect(mergeVideoProjects(base, local, remote).project.shots).toHaveLength(2)
  expect(mergeVideoProjects(base, local, remote, 'remote').project.shots).toHaveLength(1)
})

it('保留单侧排序，标记两侧不同排序', () => {
  const base = { ...emptyVideoProject(), shots: [newVideoShot('一'), newVideoShot('二'), newVideoShot('三')] }
  const local = structuredClone(base), remote = structuredClone(base)
  remote.shots.reverse()
  expect(mergeVideoProjects(base, local, remote).project.shots.map(shot => shot.id)).toEqual(remote.shots.map(shot => shot.id))
  local.shots.push(local.shots.shift()!)
  expect(mergeVideoProjects(base, local, remote).conflicts.some(conflict => conflict.field === 'order')).toBe(true)
})
