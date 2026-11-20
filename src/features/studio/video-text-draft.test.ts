import { expect, it } from 'vitest'
import { emptyVideoProject, newVideoShot } from './video-editor-model'
import { applyVideoTextDraft, recoverVideoTextDraft, type VideoTextDraft } from './video-text-draft'

it('文字提交保留其它字段和分镜，不修改原始作品', () => {
  const shot = { ...newVideoShot('开场', '原剧本'), subtitle: '原字幕' }
  const project = { ...emptyVideoProject(), shots: [shot, newVideoShot('结尾')] }
  const draft: VideoTextDraft = { id: shot.id, name: shot.name, key: 'script', value: '新的故事', original: shot.script }
  const next = applyVideoTextDraft(project, draft)
  expect(next.shots[0]).toEqual({ ...shot, script: '新的故事' })
  expect(next.shots[1]).toBe(project.shots[1])
  expect(project.shots[0].script).toBe('原剧本')
  expect(applyVideoTextDraft(next, draft)).toBe(next)
})

it('目标内容冲突需要明确采用，目标删除时保留原项目', () => {
  const shot = newVideoShot('开场', '远端内容')
  const project = { ...emptyVideoProject(), shots: [shot] }
  const draft: VideoTextDraft = { id: shot.id, name: shot.name, key: 'script', value: '输入内容', original: '编辑基线' }
  expect(() => applyVideoTextDraft(project, draft)).toThrow('更新')
  expect(applyVideoTextDraft(project, draft, true).shots[0].script).toBe('输入内容')
  expect(() => applyVideoTextDraft(emptyVideoProject(), draft, true)).toThrow('删除')
  expect(project.shots[0].script).toBe('远端内容')
})

it('剧本和字幕草稿分别恢复为新分镜，使用新标识且保留原项目内容', () => {
  const project = { ...emptyVideoProject(), shots: [newVideoShot('现有分镜', '现有内容')] }
  for (const key of ['script', 'subtitle'] as const) {
    const draft: VideoTextDraft = { id: 'removed', name: '原分镜', key, value: '保留的文字', original: '原始文字' }
    const recovered = recoverVideoTextDraft(project, draft)
    expect(recovered.project.shots).toHaveLength(2)
    expect(recovered.project.shots[0]).toBe(project.shots[0])
    const shot = recovered.project.shots[1]
    expect(shot.id).toBe(recovered.shotId)
    expect(shot.id).not.toBe('removed')
    expect(shot[key]).toBe('保留的文字')
    expect(shot[key === 'script' ? 'subtitle' : 'script']).toBe('')
    expect(shot.executionId).toBeUndefined()
    expect(project.shots).toHaveLength(1)
  }
})

it('分镜数量达到限制时拒绝恢复并保留草稿', () => {
  const project = { ...emptyVideoProject(), shots: Array.from({ length: 100 }, () => newVideoShot()) }
  const draft: VideoTextDraft = { id: 'removed', name: '原分镜', key: 'script', value: '保留的文字', original: '' }
  expect(() => recoverVideoTextDraft(project, draft)).toThrow('100')
  expect(project.shots).toHaveLength(100)
  expect(draft.value).toBe('保留的文字')
})
