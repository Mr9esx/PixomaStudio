import { describe, expect, it } from 'vitest'
import type { WorkflowVersion } from '@/lib/api/workflows'
import { emptyVideoProject, newVideoShot } from './video-editor-model'
import { prepareVideoWorkflowVersion, applyVideoWorkflowVersion } from './video-workflow-version'

const original: WorkflowVersion = { id: 'old', workflow_id: 'workflow', version: '1.0.0', content_digest: '', notes: '', created_at: '', definition: { type: 'comfyui', inputs: [{ key: 'prompt', type: 'string', required: true }, { key: 'seed', type: 'number', required: true }], input_schema: { type: 'object', properties: { prompt: { type: 'string' }, seed: { type: 'number', minimum: 0, default: 3 } } }, outputs: [{ key: 'video', type: 'video' }], execution: { node_id: 'node', workflow: {}, input_bindings: [], output_bindings: [] } } }
const published: WorkflowVersion = { ...original, id: 'new', version: '2.0.0', definition: { ...original.definition, input_schema: { type: 'object', properties: { prompt: { type: 'string' }, seed: { type: 'number', minimum: 10, default: 12 } } } } }
const shot = () => ({ ...newVideoShot('茶园'), workflowId: 'workflow', versionId: 'old', inputs: { prompt: '手工近景', seed: 7 }, inputSources: { prompt: { field: 'storyboard' as const, sourceText: '人物进入茶园', value: '手工近景', edited: true } } })

describe('发布版本更新预览', () => {
  it('沿用兼容输入和来源，显示新限制影响，保留旧版本草稿及采用视频', () => {
    const source = { ...shot(), media: { assetId: 'video', versionId: 'fixed', url: '/video', duration: 2 }, duration: 1, sourceIn: 0.5 }
    const review = prepareVideoWorkflowVersion(source, original, published, [])
    expect(review.shot.inputs).toEqual({ prompt: '手工近景', seed: 12 })
    expect(review.shot.inputSources?.prompt).toEqual(source.inputSources.prompt)
    expect(review.changes.some(change => change.key === 'seed' && change.message.includes('10'))).toBe(true)
    expect(review.shot.workflowDrafts?.['workflow:old'].inputs).toEqual(source.inputs)
    expect(review.shot.media).toEqual(source.media)
    expect(source.versionId).toBe('old')
  })
  it('已有目标版本草稿时恢复该版本的手工输入', () => {
    const source = { ...shot(), workflowDrafts: { 'workflow:new': { inputs: { prompt: '新版草稿', seed: 15 } } } }
    const review = prepareVideoWorkflowVersion(source, original, published, [])
    expect(review.shot.inputs).toEqual({ prompt: '新版草稿', seed: 15 })
    expect(review.restoredDraft).toBe(true)
  })
  it('拒绝错误版本及并发修改，仅采用明确的生成配置', () => {
    const source = shot(), project = { ...emptyVideoProject(), shots: [source, { ...shot(), id: 'other' }] }
    const review = prepareVideoWorkflowVersion(source, original, published, [])
    expect(() => prepareVideoWorkflowVersion(source, original, { ...published, workflow_id: 'other' }, [])).toThrow('同一工作流')
    expect(() => applyVideoWorkflowVersion({ ...project, shots: [{ ...source, storyboard: '更新' }] }, review)).toThrow('已修改')
    expect(applyVideoWorkflowVersion(project, review).shots[1]).toEqual(project.shots[1])
    expect(applyVideoWorkflowVersion(project, review).shots[0].versionId).toBe('new')
  })
})
