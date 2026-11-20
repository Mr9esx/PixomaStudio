import { describe, expect, it } from 'vitest'
import type { WorkflowVersion } from '@/lib/api/workflows'
import { emptyVideoProject, reviseVideoShotContent, teaVideoShots, validateVideoProject } from './video-editor-model'
import { chooseVideoShotWorkflow, editVideoShotInput, videoDescriptionInputState } from './video-input-sources'

const workflow: WorkflowVersion = {
  id: 'version', workflow_id: 'workflow', version: '1.0.0', content_digest: '', notes: '', created_at: '',
  definition: { type: 'comfyui', inputs: [{ key: 'prompt', type: 'string', required: true }, { key: 'image', type: 'image', required: false }], input_schema: { type: 'object', properties: { prompt: { type: 'string' }, image: { type: 'object' } } }, outputs: [{ key: 'video', type: 'video' }], execution: { node_id: 'node', workflow: {}, input_bindings: [], output_bindings: [] } },
}

describe('分镜输入来源', () => {
  it('记录采用时的描述修订，内容恢复相同文字后仍识别待同步', () => {
    const first = reviseVideoShotContent(undefined, { ...teaVideoShots()[0], storyboard: '人物进入茶园' })
    const adopted = chooseVideoShotWorkflow(first, workflow)
    expect(adopted.inputSources?.prompt.revision).toBe(1)
    const second = reviseVideoShotContent(adopted, { ...adopted, storyboard: '人物离开茶园' })
    const third = reviseVideoShotContent(second, { ...second, storyboard: first.storyboard })
    expect(third.contentRevisions).toEqual({ script: 1, storyboard: 3 })
    expect(videoDescriptionInputState(third, 'prompt')).toEqual({ changed: true, edited: false })
    expect(reviseVideoShotContent(third, { ...third, name: '新名称' }).contentRevisions).toEqual(third.contentRevisions)
    expect(third.inputs.prompt).toBe(first.storyboard)
  })
  it('使用当前画面描述建立来源快照，描述变化保留已采用输入', () => {
    const shot = chooseVideoShotWorkflow({ ...teaVideoShots()[0], storyboard: '人物走入茶园' }, workflow)
    expect(shot.inputs.prompt).toBe('人物走入茶园')
    expect(videoDescriptionInputState(shot, 'prompt')).toEqual({ changed: false, edited: false })
    const changed = { ...shot, storyboard: '人物离开茶园' }
    expect(videoDescriptionInputState(changed, 'prompt')).toEqual({ changed: true, edited: false })
    expect(changed.inputs.prompt).toBe('人物走入茶园')
    const edited = editVideoShotInput(changed, 'prompt', '镜头缓慢前进')
    expect(videoDescriptionInputState(edited, 'prompt')).toEqual({ changed: true, edited: true })
    expect(shot.inputs.prompt).toBe('人物走入茶园')
  })

  it('切换工作流保存参数及来源草稿，返回时保留手工输入和素材映射', () => {
    const initial = chooseVideoShotWorkflow({ ...teaVideoShots()[0], storyboard: '茶园' }, workflow)
    const edited = { ...editVideoShotInput(initial, 'prompt', '人物近景'), references: [{ id: 'character', kind: 'image' as const, role: 'character' as const, assetId: 'asset', versionId: 'asset-version', url: '/asset', inputKey: 'image' }] }
    const second = chooseVideoShotWorkflow(edited, { ...workflow, id: 'second' })
    expect(second.inputs.prompt).toBe('茶园')
    expect(second.references[0].inputKey).toBeUndefined()
    const restored = chooseVideoShotWorkflow({ ...second, storyboard: '新描述' }, workflow)
    expect(restored.inputs.prompt).toBe('人物近景')
    expect(restored.inputSources).toEqual(edited.inputSources)
    expect(restored.references[0].inputKey).toBe('image')
    expect(videoDescriptionInputState(restored, 'prompt')).toEqual({ changed: true, edited: true })
    expect(chooseVideoShotWorkflow(restored, workflow)).toBe(restored)
  })

  it('历史文字来源固定，清空手工参数保留来源，并识别外部输入更新', () => {
    const shot = chooseVideoShotWorkflow({ ...teaVideoShots()[0], storyboard: '' }, workflow)
    expect(shot.inputSources?.prompt.field).toBe('script')
    expect(videoDescriptionInputState({ ...shot, storyboard: '新画面描述' }, 'prompt')?.changed).toBe(true)
    expect(editVideoShotInput(shot, 'prompt', undefined).inputs.prompt).toBeUndefined()
    expect(videoDescriptionInputState({ ...shot, inputs: { prompt: 'Chat 更新' } }, 'prompt')?.edited).toBe(true)
    expect(() => chooseVideoShotWorkflow({ ...shot, generationRequest: { id: 'request', workflowId: 'workflow', versionId: 'version', inputs: {} } }, { ...workflow, id: 'other' })).toThrow('生成请求')
  })

  it('保存时拒绝无效来源及草稿格式', () => {
    const shot = chooseVideoShotWorkflow(teaVideoShots()[0], workflow)
    expect(() => validateVideoProject({ ...emptyVideoProject(), shots: [{ ...shot, inputSources: { prompt: { field: 'storyboard', sourceText: '茶园', value: '茶园', edited: 'invalid' } } } as never] })).toThrow('输入来源格式无效')
    expect(() => validateVideoProject({ ...emptyVideoProject(), shots: [{ ...shot, workflowDrafts: { invalid: { inputs: [] } } } as never] })).toThrow('工作流草稿格式无效')
  })
})
