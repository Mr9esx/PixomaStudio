import { describe, expect, it } from 'vitest'
import type { WorkflowVersion } from '@/lib/api/workflows'
import { emptyVideoProject, newVideoShot } from './video-editor-model'
import { reviewVideoGenerationShot, validateVideoGenerationBatchSelection, videoGenerationDependencyPlan, videoGenerationBatchPendingSchema, videoGenerationConnectionIssue } from './video-generation-batch-review'

const workflow: WorkflowVersion = { id: 'version', workflow_id: 'workflow', version: '1.0.0', content_digest: '', notes: '', created_at: '', definition: { type: 'comfyui', inputs: [{ key: 'prompt', type: 'string', required: true }, { key: 'seed', type: 'number', required: true }], input_schema: { type: 'object', properties: { prompt: { type: 'string' }, seed: { type: 'number', default: 3 } }, required: ['prompt', 'seed'] }, outputs: [{ key: 'video', type: 'video' }], execution: { node_id: 'node', workflow: {}, input_bindings: [], output_bindings: [] } } }
const shot = () => ({ ...newVideoShot('晨光'), workflowId: 'workflow', versionId: 'version', inputs: { prompt: '手工近景' } })
describe('批量生成确认', () => {
  it('生成前检查服务连接、类型和认证设置，读取期间保持等待', () => {
    const definition = { ...workflow.definition, type: 'online' as const, execution: { provider: 'seedance' as const, operation: 'video' as const, connection_id: 'service', model: 'video', capability: 'text_to_video', defaults: {}, input_mapping: {}, output_mapping: {} } }
    const connection = { id: 'service', name: '视频服务', provider: 'seedance' as const, base_url: 'https://example.com', has_api_key: true, created_at: '' }
    expect(videoGenerationConnectionIssue(workflow.definition, undefined)).toBeUndefined()
    expect(videoGenerationConnectionIssue(definition, undefined)).toBe('正在读取生成服务连接')
    expect(videoGenerationConnectionIssue(definition, [])).toBe('配置生成服务连接')
    expect(videoGenerationConnectionIssue(definition, [{ ...connection, provider: 'minimax' }])).toBe('服务连接与工作流类型不一致')
    expect(videoGenerationConnectionIssue(definition, [{ ...connection, has_api_key: false }])).toBe('配置生成服务 API Key')
    expect(videoGenerationConnectionIssue(definition, [connection])).toBeUndefined()
    const custom = { ...definition, execution: { ...definition.execution, provider: 'http_json' as const, http: { submit: { method: 'POST' as const, path: '/tasks' }, query: { method: 'GET' as const, path: '/tasks/{task_id}' }, body: {}, task_id_pointer: '/id', status_pointer: '/status', error_pointer: '/error', status_mapping: {}, authentication: 'none' as const } } }
    expect(videoGenerationConnectionIssue(custom, [{ ...connection, provider: 'http_json', has_api_key: false }])).toBeUndefined()
    expect(videoGenerationConnectionIssue({ ...custom, execution: { ...custom.execution, http: { ...custom.execution.http, authentication: 'bearer' } } }, [{ ...connection, provider: 'http_json', has_api_key: false }])).toBe('配置生成服务 API Key')
  })
  it('只延后检查明确映射的画面参数，保留其他必填参数检查', () => {
    const source = shot(), project = { ...emptyVideoProject(), shots: [source] }
    const imageWorkflow: WorkflowVersion = { ...workflow, definition: { ...workflow.definition, inputs: [...workflow.definition.inputs, { key: 'first_frame', type: 'image', required: true }], input_schema: { ...workflow.definition.input_schema, required: ['prompt', 'seed', 'first_frame'], properties: { ...workflow.definition.input_schema.properties as object, first_frame: { type: 'object' } } } } }
    const dependency = { shot_id: source.id, source_shot_id: 'upstream', input_key: 'first_frame', position: 'last' as const, fps: 30 }
    expect(reviewVideoGenerationShot(project, source, imageWorkflow, [], [dependency]).issues).toEqual([])
    expect(reviewVideoGenerationShot(project, source, imageWorkflow, []).issues.length).toBeGreaterThan(0)
    expect(reviewVideoGenerationShot(project, { ...source, inputs: {} }, imageWorkflow, [], [dependency]).issues.some(issue => issue.includes('prompt'))).toBe(true)
    const request = { request_id: 'request', session_id: 'session', expected_version: 1, shot_ids: ['upstream', source.id], dependencies: [dependency] }
    expect(videoGenerationBatchPendingSchema.parse({ videoId: 'video', body: request }).body.dependencies).toEqual([dependency])
  })
  it('显示依赖执行顺序，并指出循环、重复映射和范围问题', () => {
    const project = { ...emptyVideoProject(), shots: ['opening', 'middle', 'ending'].map(id => ({ ...shot(), id, name: id === 'opening' ? '晨光' : id === 'middle' ? '采茶' : '茶馆' })) }
    const dependency = { shot_id: 'middle', source_shot_id: 'opening', input_key: 'first_frame', position: 'last' as const, fps: 30 }
    const second = { ...dependency, shot_id: 'ending', source_shot_id: 'middle' }
    expect(videoGenerationDependencyPlan(project, ['ending', 'middle', 'opening'], [dependency, second])).toEqual({ order: ['opening', 'middle', 'ending'], issues: [] })
    expect(videoGenerationDependencyPlan(project, ['middle'], [dependency]).issues.join('')).toContain('晨光')
    expect(videoGenerationDependencyPlan(project, ['opening', 'middle'], [dependency, { ...dependency, shot_id: 'opening', source_shot_id: 'middle' }]).issues.join('')).toContain('循环')
    expect(videoGenerationDependencyPlan(project, ['opening', 'middle'], [dependency, dependency]).issues.join('')).toContain('重复')
  })
  it('显示实际输入及默认参数，保留手工提示词', () => {
    const source = shot(), project = { ...emptyVideoProject(), shots: [source] }
    const result = reviewVideoGenerationShot(project, source, workflow, [])
    expect(result.inputs).toEqual({ prompt: '手工近景', seed: 3 })
    expect(result.issues).toEqual([])
    expect(source.inputs).toEqual({ prompt: '手工近景' })
    expect(reviewVideoGenerationShot(project, { ...source, inputs: {} }, workflow, []).issues.length).toBeGreaterThan(0)
  })
  it('检查固定版本、恢复请求和失效来源画面', () => {
    const source = shot(), project = { ...emptyVideoProject(), shots: [source] }
    expect(reviewVideoGenerationShot(project, source, { ...workflow, id: 'other' }, []).issues).toContain('工作流版本已改变')
    const pending = { ...source, generationRequest: { id: 'request', workflowId: 'workflow', versionId: 'version', inputs: {} } }
    expect(reviewVideoGenerationShot(project, pending, workflow, []).issues).toContain('恢复当前生成请求后继续')
    const referenced = { ...source, references: [{ id: 'frame', assetId: 'image', versionId: 'frame-v1', url: '/frame', role: 'first_frame' as const, kind: 'image' as const, sourceShotId: 'removed', sourceMediaVersionId: 'video-v1' }] }
    expect(reviewVideoGenerationShot(project, referenced, workflow, []).issues).toContain('确认来源画面更新')
  })
  it('提交前检查选定范围和分镜变化，允许其他分镜继续编辑', () => {
    const first = shot(), second = { ...shot(), id: 'second' }
    const source = { ...emptyVideoProject(), shots: [first, second] }
    expect(() => validateVideoGenerationBatchSelection(source, source, [])).toThrow('选择分镜')
    expect(() => validateVideoGenerationBatchSelection(source, source, [first.id, first.id])).toThrow('重复')
    expect(() => validateVideoGenerationBatchSelection(source, { ...source, shots: [second] }, [first.id])).toThrow('已删除')
    expect(() => validateVideoGenerationBatchSelection(source, { ...source, shots: [{ ...first, inputs: { prompt: '更新' } }, second] }, [first.id])).toThrow('已修改')
    expect(() => validateVideoGenerationBatchSelection(source, { ...source, shots: [first, { ...second, name: '更新名称' }] }, [first.id])).not.toThrow()
  })
  it('来源分镜在确认期间采用新视频时要求重新确认画面引用', () => {
    const source = { ...shot(), id: 'source', media: { assetId: 'video', versionId: 'video-v1', url: '/video', duration: 2 }, duration: 2 }
    const target = { ...shot(), references: [{ id: 'first', assetId: 'frame', versionId: 'frame-v1', url: '/frame', role: 'first_frame' as const, kind: 'image' as const, sourceShotId: source.id, sourceMediaVersionId: 'video-v1', confirmedSourceVersionId: 'video-v1', sourcePosition: 'first' as const, sourceFrame: 0, sourceFps: 30, confirmedSourceFrame: 0, confirmedSourceFps: 30 }] }
    const baseline = { ...emptyVideoProject(), shots: [source, target] }
    const changed = { ...baseline, shots: [{ ...source, media: { ...source.media, versionId: 'video-v2' } }, target] }
    expect(() => validateVideoGenerationBatchSelection(baseline, changed, [target.id])).toThrow('来源画面已更新')
  })
})
