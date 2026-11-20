import { describe, expect, it } from 'vitest'
import type { WorkflowVersion } from '@/lib/api/workflows'
import { emptyVideoProject, newVideoShot } from './video-editor-model'
import { prepareVideoWorkflowBatch, applyVideoWorkflowBatch } from './video-workflow-batch'

const workflow: WorkflowVersion = { id: 'published', workflow_id: 'workflow', version: '1.0.0', content_digest: '', notes: '', created_at: '', definition: { type: 'comfyui', inputs: [{ key: 'prompt', type: 'string', required: true }], input_schema: { type: 'object', properties: { prompt: { type: 'string' } } }, outputs: [{ key: 'video', type: 'video' }], execution: { node_id: 'node', workflow: {}, input_bindings: [], output_bindings: [] } } }
const project = () => ({ ...emptyVideoProject(), shots: ['晨光', '采茶', '茶馆'].map(name => ({ ...newVideoShot(name), id: name, storyboard: name })) })

describe('批量工作流设置', () => {
  it('按作品顺序准备明确范围，每个分镜使用自己的描述，采用仅修改选定范围', () => {
    const source = project()
    const batch = prepareVideoWorkflowBatch(source, ['采茶', '晨光'], workflow)
    expect(batch.map(item => item.shot.id)).toEqual(['晨光', '采茶'])
    expect(batch.map(item => item.shot.inputs.prompt)).toEqual(['晨光', '采茶'])
    const next = applyVideoWorkflowBatch(source, batch)
    expect(next.shots[2]).toEqual(source.shots[2])
    expect(source.shots[0].workflowId).toBeUndefined()
    expect(next.shots[0].inputSources?.prompt.sourceText).toBe('晨光')
  })
  it('保留每个分镜的参数草稿和手工值，允许预览中分别编辑', () => {
    const source = project()
    source.shots[0] = { ...source.shots[0], workflowDrafts: { 'workflow:published': { inputs: { prompt: '手工近景' } } } } as typeof source.shots[number]
    const batch = prepareVideoWorkflowBatch(source, ['晨光', '采茶'], workflow)
    expect(batch[0].shot.inputs.prompt).toBe('手工近景')
    batch[1].shot.inputs.prompt = '手部特写'
    expect(applyVideoWorkflowBatch(source, batch).shots[1].inputs.prompt).toBe('手部特写')
  })
  it('拒绝空范围、重复选择、删除目标和待恢复的请求', () => {
    const source = project()
    expect(() => prepareVideoWorkflowBatch(source, [], workflow)).toThrow('选择分镜')
    expect(() => prepareVideoWorkflowBatch(source, ['晨光', '晨光'], workflow)).toThrow('重复')
    expect(() => prepareVideoWorkflowBatch(source, ['不存在'], workflow)).toThrow('删除')
    const busy = { ...source, shots: [{ ...source.shots[0], generationRequest: { id: 'pending', workflowId: 'other', versionId: 'other', inputs: {} } }] }
    expect(() => prepareVideoWorkflowBatch(busy, ['晨光'], workflow)).toThrow('生成请求')
  })
  it('采用前校验范围与并发修改，其他分镜的修改保持', () => {
    const source = project(), batch = prepareVideoWorkflowBatch(source, ['晨光'], workflow)
    const changed = { ...source, shots: source.shots.map(shot => shot.id === '晨光' ? { ...shot, storyboard: '新描述' } : shot) }
    expect(() => applyVideoWorkflowBatch(changed, batch)).toThrow('已修改')
    expect(() => applyVideoWorkflowBatch(source, [batch[0], batch[0]])).toThrow('重复')
    const other = { ...source, shots: source.shots.map(shot => shot.id === '茶馆' ? { ...shot, name: '室内茶馆' } : shot) }
    expect(applyVideoWorkflowBatch(other, batch).shots[2].name).toBe('室内茶馆')
  })
})
