import { describe, expect, it } from 'vitest'
import type { WorkflowVersion } from '@/lib/api/workflows'
import { emptyVideoProject, teaVideoShots } from './video-editor-model'
import { applyVideoPromptDraft, createVideoPromptDraft, videoPromptDraftKey, videoPromptFields } from './video-prompt-draft'

const workflow: WorkflowVersion = {
  id: 'version', workflow_id: 'workflow', version: '1.0.0', content_digest: '', notes: '', created_at: '',
  definition: {
    type: 'comfyui', inputs: [{ key: 'prompt', type: 'string', required: true }, { key: 'style', type: 'string', required: false }, { key: 'image', type: 'image', required: true }],
    input_schema: { type: 'object', properties: { prompt: { type: 'string', maxLength: 80 }, style: { enum: ['natural', 'painted'] }, image: { type: 'object' } }, required: ['prompt', 'image'] },
    outputs: [{ key: 'video', type: 'video' }], execution: { node_id: 'node', workflow: {}, input_bindings: [], output_bindings: [] },
  },
}
function project() {
  return { ...emptyVideoProject(), shots: teaVideoShots().slice(0, 2).map(shot => ({ ...shot, workflowId: 'workflow', versionId: 'version', inputs: { prompt: '原提示词', seed: 7 } })) }
}

describe('剧本提示词待采用内容', () => {
  it('根据分镜画面描述准备提示词，并检查描述修改', () => {
    const source = project()
    source.shots[0].script = ''
    source.shots[0].storyboard = '阳光穿过薄雾，镜头缓慢前进'
    const draft = createVideoPromptDraft(source.shots[0], workflow)
    expect(draft.text).toBe(source.shots[0].storyboard)
    expect(draft.sourceField).toBe('storyboard')
    expect(applyVideoPromptDraft(source, draft, workflow).shots[0].inputs.prompt).toBe(source.shots[0].storyboard)
    const changed = { ...source, shots: [{ ...source.shots[0], storyboard: '茶杯近景' }] }
    expect(() => applyVideoPromptDraft(changed, draft, workflow)).toThrow('分镜内容已更新')
  })
  it('从剧本创建独立草稿，只在明确采用时修改目标参数', () => {
    const source = project(), before = structuredClone(source)
    const draft = createVideoPromptDraft(source.shots[0], workflow)
    expect(draft.text).toBe(source.shots[0].script)
    expect(draft.inputKey).toBe('prompt')
    expect(source).toEqual(before)
    const next = applyVideoPromptDraft(source, { ...draft, text: '晨光中，露珠从茶叶滑下。' }, workflow)
    expect(next.shots[0]).toEqual({ ...source.shots[0], inputs: { ...source.shots[0].inputs, prompt: '晨光中，露珠从茶叶滑下。' }, inputSources: { prompt: { field: 'script', revision: 1, sourceText: draft.sourceScript, value: '晨光中，露珠从茶叶滑下。', edited: true } } })
    expect(next.shots[1]).toEqual(source.shots[1])
    expect(source).toEqual(before)
  })

  it('采用前检查剧本和目标参数基线，保留其它字段的更新', () => {
    const source = project(), draft = createVideoPromptDraft(source.shots[0], workflow)
    const renamed = { ...source, shots: source.shots.map(shot => ({ ...shot, name: '新名称' })) }
    expect(applyVideoPromptDraft(renamed, draft, workflow).shots[0].name).toBe('新名称')
    const edited = { ...source, shots: [{ ...source.shots[0], inputs: { prompt: '手动修改', seed: 9 } }, source.shots[1]] }
    expect(() => applyVideoPromptDraft(edited, draft, workflow)).toThrow('生成参数已更新')
    expect(applyVideoPromptDraft(edited, { ...draft, baseline: '手动修改' }, workflow).shots[0].inputs.seed).toBe(9)
    expect(() => applyVideoPromptDraft({ ...source, shots: [{ ...source.shots[0], script: '新剧本' }] }, draft, workflow)).toThrow('分镜内容已更新')
  })

  it('目标、工作流版本和请求状态发生变化时保留待采用内容', () => {
    const source = project(), draft = createVideoPromptDraft(source.shots[0], workflow)
    expect(() => applyVideoPromptDraft({ ...source, shots: [] }, draft, workflow)).toThrow('分镜已被删除')
    expect(() => applyVideoPromptDraft(source, draft, { ...workflow, id: 'new-version' })).toThrow('工作流版本已改变')
    const pending = { ...source, shots: [{ ...source.shots[0], generationRequest: { id: 'request', workflowId: 'workflow', versionId: 'version', inputs: {} } }] }
    expect(() => applyVideoPromptDraft(pending, draft, workflow)).toThrow('恢复当前生成请求')
    expect(videoPromptDraftKey(source.shots[0])).not.toBe(videoPromptDraftKey({ ...source.shots[0], versionId: 'new-version' }))
  })

  it('只允许自由文字参数，校验空白和字段限制，不要求其它参数同时填写完成', () => {
    const source = project(), draft = createVideoPromptDraft(source.shots[0], workflow)
    expect(videoPromptFields(workflow.definition).map(field => field.key)).toEqual(['prompt'])
    expect(() => applyVideoPromptDraft(source, { ...draft, text: ' ' }, workflow)).toThrow('填写待采用提示词')
    expect(() => applyVideoPromptDraft(source, { ...draft, text: '茶'.repeat(81) }, workflow)).toThrow('最多输入 80 个字符')
    expect(() => applyVideoPromptDraft(source, { ...draft, inputKey: 'image' }, workflow)).toThrow('选择文字参数')
    expect(applyVideoPromptDraft(source, { ...draft, text: source.shots[0].inputs.prompt }, workflow).shots[0].inputSources?.prompt.sourceText).toBe(draft.sourceScript)
  })

  it('没有标准 prompt 参数时由用户选择目标，空剧本不能创建待采用内容', () => {
    const source = project()
    const custom = { ...workflow, definition: { ...workflow.definition, inputs: [{ key: 'positive', type: 'string' as const, required: false }, { key: 'negative', type: 'string' as const, required: false }], input_schema: { type: 'object', properties: { positive: { type: 'string' }, negative: { type: 'string' } } } } }
    const draft = createVideoPromptDraft(source.shots[0], custom)
    expect(draft.inputKey).toBe('')
    expect(() => applyVideoPromptDraft(source, draft, custom)).toThrow('选择文字参数')
    const next = applyVideoPromptDraft(source, { ...draft, inputKey: 'positive' }, custom)
    expect(next.shots[0].inputs.positive).toBe(source.shots[0].script)
    expect(next.shots[0].inputs.prompt).toBe('原提示词')
    expect(() => createVideoPromptDraft({ ...source.shots[0], script: ' ' }, custom)).toThrow('填写分镜画面描述')
  })
})
