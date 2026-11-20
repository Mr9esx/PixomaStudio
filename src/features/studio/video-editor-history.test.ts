import { expect, it } from 'vitest'
import { emptyVideoProject, newVideoShot, type VideoProject } from './video-editor-model'
import { restoreVideoEditorHistory, updateVideoGenerationState, videoHistoryProblem } from './video-editor-history'

it('撤销内容编辑时保留当前生成请求、任务关联和输入基线', () => {
  const shot = newVideoShot('茶园', '晨光')
  const previous = { ...emptyVideoProject(), shots: [shot] }
  const request = { id: 'request', workflowId: 'workflow', versionId: 'workflow-version', inputs: { prompt: '生成时的描述' } }
  const current = { ...previous, shots: [{ ...shot, script: '薄雾', inputs: { prompt: '新的描述' }, executionId: 'execution', executionInputs: '{"prompt":"生成时的描述"}', generationRequest: request }] }
  const restored = restoreVideoEditorHistory(previous, current, [shot.id])
  expect(restored.shots[0]).toMatchObject({ script: '晨光', inputs: {}, executionId: 'execution', executionInputs: current.shots[0].executionInputs, generationRequest: request })
  expect(current.shots[0].script).toBe('薄雾')
  expect(previous.shots[0].executionId).toBeUndefined()
})

it('撤销采用视频时保留已采用版本和已收集的候选，避免重复记录', () => {
  const original = { assetId: 'original', versionId: 'original-version', url: '/original.mp4', duration: 3 }
  const generated = { assetId: 'generated', versionId: 'generated-version', url: '/generated.mp4', duration: 3 }
  const candidate = { id: 'candidate-version', media: { assetId: 'candidate', versionId: 'candidate-version', url: '/candidate.mp4', duration: 3 }, status: 'candidate' as const, executionId: 'candidate-execution' }
  const shot = { ...newVideoShot(), duration: 3, media: original }
  const previous = { ...emptyVideoProject(), shots: [shot] }
  const current: VideoProject = { ...previous, shots: [{ ...shot, media: generated, sourceIn: 0.5, duration: 2, versions: [{ id: original.versionId, media: original, status: 'history' }, candidate], executionId: 'new-execution' }] }
  const restored = restoreVideoEditorHistory(previous, current, [])
  expect(restored.shots[0].media).toEqual(original)
  expect(restored.shots[0].versions).toEqual([candidate, { id: generated.versionId, media: generated, status: 'history', sourceIn: 0.5, duration: 2 }])
  expect(restored.shots[0].executionId).toBe('new-execution')
  expect(restoreVideoEditorHistory(restored, current, []).shots[0].versions).toEqual(restored.shots[0].versions)
  expect(current.shots[0].media).toEqual(generated)
})

it('阻止历史操作移除未结束任务的目标，允许编辑其它分镜', () => {
  const shot = newVideoShot('正在生成的分镜')
  const current = { ...emptyVideoProject(), shots: [shot, newVideoShot('其它分镜')] }
  const removed = { ...current, shots: current.shots.slice(1) }
  expect(videoHistoryProblem(removed, current, [shot.id])).toContain('正在生成的分镜')
  expect(() => restoreVideoEditorHistory(removed, current, [shot.id])).toThrow('生成任务')
  expect(videoHistoryProblem({ ...current, shots: [shot] }, current, [shot.id])).toBeUndefined()
  expect(restoreVideoEditorHistory(removed, current, []).shots).toEqual(removed.shots)
})

it('任务状态更新同步到相关历史快照，恢复已删除分镜时不会复用过期请求', () => {
  const shot = { ...newVideoShot(), generationRequest: { id: 'request', workflowId: 'workflow', versionId: 'version', inputs: {} } }
  const first = { ...emptyVideoProject(), shots: [shot] }
  const second = { ...first, shots: [{ ...shot, script: '已编辑' }, newVideoShot('其它分镜')] }
  const history = [first, second, emptyVideoProject()].map(project => updateVideoGenerationState(project, shot.id, { executionId: 'execution', executionInputs: '{}', generationRequest: undefined }))
  expect(history[0].shots[0]).toMatchObject({ script: '', executionId: 'execution', generationRequest: undefined })
  expect(history[1].shots[0]).toMatchObject({ script: '已编辑', executionId: 'execution', generationRequest: undefined })
  expect(history[1].shots[1]).toEqual(second.shots[1])
  expect(restoreVideoEditorHistory(history[0], emptyVideoProject(), []).shots[0].generationRequest).toBeUndefined()
  expect(first.shots[0].generationRequest).toBeDefined()
  expect(restoreVideoEditorHistory(first, emptyVideoProject(), []).shots[0].generationRequest).toBeUndefined()
  expect(updateVideoGenerationState(first, 'missing', { executionId: 'other' })).toBe(first)
})
