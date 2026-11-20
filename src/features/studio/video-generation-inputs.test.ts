import { describe, expect, it } from 'vitest'
import type { WorkflowInput } from '@/lib/api/workflows'
import type { StudioAsset } from '@/lib/api/studio'
import { applyUploadedVideoInput, applyUploadedVideoMedia, videoGenerationInputIssues, type VideoInputUploadTarget } from './video-generation-inputs'
import { emptyVideoProject, newVideoShot } from './video-editor-model'

const fields: WorkflowInput[] = [
  { key: 'prompt', type: 'string', required: true },
  { key: 'seed', type: 'number', required: true },
  { key: 'sound', type: 'boolean', required: true },
  { key: 'image', type: 'image', required: false },
]
const image: StudioAsset = {
  id: 'image', name: '茶园', kind: 'image', origin: 'user', current_version: 2,
  created_at: '', updated_at: '', versions: [
    { id: 'v1', version: 1, mime_type: 'image/png', size_bytes: 1024, content_url: '/image.png', created_at: '' },
    { id: 'v2', version: 2, mime_type: 'image/png', size_bytes: 2048, content_url: '/image-v2.png', created_at: '' },
  ],
}
const values = { prompt: '清晨茶园', seed: 0, sound: false }

describe('分镜视频导入', () => {
  const shot = { ...newVideoShot('开场'), duration: 3, media: { assetId: 'previous', versionId: 'previous-version', url: '/previous.mp4', duration: 6 } }
  const project = { ...emptyVideoProject(), shots: [shot, newVideoShot('结尾')] }
  const video: StudioAsset = { ...image, id: 'video', name: '导入视频', kind: 'video', current_version: 1, versions: [{ ...image.versions[0], id: 'video-version', mime_type: 'video/mp4', content_url: '/video.mp4', metadata: { duration: 2 } }] }

  it('上传视频进入原分镜候选并保留当前视频、剪辑和期间修改', () => {
    const candidate = { id: 'candidate', media: { assetId: 'candidate', versionId: 'candidate-version', url: '/candidate.mp4', duration: 3 }, status: 'candidate' as const }
    const current = { ...project, shots: [{ ...shot, name: '更新名称', script: '更新剧本', volume: .4, executionId: 'job', inputs: { prompt: '茶园' }, versions: [candidate] }, project.shots[1]] }
    const next = applyUploadedVideoMedia(current, shot, video)
    expect(next.shots[0]).toMatchObject({ name: '更新名称', script: '更新剧本', volume: .4, executionId: 'job', inputs: { prompt: '茶园' }, duration: shot.duration, sourceIn: shot.sourceIn, media: shot.media })
    expect(next.shots[0].versions).toContainEqual({ id: 'video-version', media: { assetId: 'video', versionId: 'video-version', url: '/video.mp4', duration: 2 }, status: 'candidate' })
    expect(next.shots[0].versions).toContainEqual(candidate)
    expect(next.shots[1]).toEqual(project.shots[1])
    expect(current.shots[0].media).toBe(shot.media)
  })

  it('目标删除时保留资产，导入期间剪辑或采用版本变化不影响候选归属', () => {
    expect(() => applyUploadedVideoMedia({ ...project, shots: project.shots.slice(1) }, shot, video)).toThrow('已删除')
    for (const changes of [{ duration: 2 }, { sourceIn: 1 }, { media: undefined }, { media: { ...shot.media, versionId: 'other-version' } }]) {
      const current = { ...project, shots: [{ ...shot, ...changes }, project.shots[1]] }
      const before = structuredClone(current)
      const collected = applyUploadedVideoMedia(current, shot, video)
      expect(collected.shots[0].media).toEqual(current.shots[0].media)
      expect(collected.shots[0].duration).toBe(current.shots[0].duration)
      expect(collected.shots[0].versions?.at(-1)?.media.versionId).toBe('video-version')
      expect(current).toEqual(before)
    }
  })

  it('拒绝非视频、缺失版本和不可用时长，重复采用同一版本保留剪辑范围', () => {
    expect(() => applyUploadedVideoMedia(project, shot, image)).toThrow('视频')
    expect(() => applyUploadedVideoMedia(project, shot, { ...video, current_version: 2 })).toThrow('版本')
    expect(() => applyUploadedVideoMedia(project, shot, { ...video, versions: [{ ...video.versions[0], metadata: {} }] })).toThrow('时长')
    const adopted = applyUploadedVideoMedia(project, shot, video)
    const trimmed = { ...adopted, shots: [{ ...adopted.shots[0], sourceIn: 1, duration: 1 }, adopted.shots[1]] }
    expect(applyUploadedVideoMedia(trimmed, shot, video)).toBe(trimmed)
  })
})

describe('生成参数素材上传', () => {
  const shot = { ...newVideoShot('开场'), workflowId: 'workflow', versionId: 'published', inputs: { prompt: '茶园' } }
  const project = { ...emptyVideoProject(), shots: [shot, newVideoShot('结尾')] }
  const target: VideoInputUploadTarget = { shotId: shot.id, workflowId: 'workflow', versionId: 'published', field: fields[3], baseline: undefined }

  it('将上传结果绑定到开始上传时的分镜和字段，保留其它内容及原始作品', () => {
    const next = applyUploadedVideoInput(project, target, image)
    expect(next.shots[0].inputs).toEqual({ prompt: '茶园', image: { asset_id: image.id, asset_version_id: 'v2' } })
    expect(next.shots[1]).toBe(project.shots[1])
    expect(project.shots[0].inputs).toEqual({ prompt: '茶园' })
    expect(applyUploadedVideoInput(next, target, image)).toBe(next)
    const historical = applyUploadedVideoInput(project, target, { ...image, current_version: 1 })
    expect(historical.shots[0].inputs.image).toEqual({ asset_id: image.id, asset_version_id: 'v1' })
    expect(image.current_version).toBe(2)
  })

  it('目标删除、更换工作流或发布版本后拒绝绑定，保留当前作品和上传资产', () => {
    expect(() => applyUploadedVideoInput({ ...project, shots: project.shots.slice(1) }, target, image)).toThrow('已导入')
    for (const changes of [{ workflowId: 'other' }, { versionId: 'new-version' }]) {
      const current = { ...project, shots: [{ ...shot, ...changes }, project.shots[1]] }
      expect(() => applyUploadedVideoInput(current, target, image)).toThrow('工作流')
      expect(current.shots[0].inputs).toEqual(shot.inputs)
    }
    expect(image.current_version).toBe(2)
  })

  it('上传期间重新选择或移除素材时保留用户当前选择', () => {
    const original = { asset_id: 'original', asset_version_id: 'original-version' }
    const captured = { ...target, baseline: original }
    const replacement = { asset_id: 'replacement', asset_version_id: 'replacement-version' }
    for (const value of [undefined, replacement]) {
      const current = { ...project, shots: [{ ...shot, inputs: { ...shot.inputs, image: value } }] }
      expect(() => applyUploadedVideoInput(current, captured, image)).toThrow('参数已改变')
      expect(current.shots[0].inputs.image).toBe(value)
    }
    const unchanged = { ...project, shots: [{ ...shot, inputs: { ...shot.inputs, image: { asset_version_id: original.asset_version_id, asset_id: original.asset_id } } }] }
    expect(applyUploadedVideoInput(unchanged, captured, image).shots[0].inputs.image).toEqual({ asset_id: image.id, asset_version_id: 'v2' })
  })

  it('上传期间其它字段发生变化时，只提交对应参数并保留当前任务关联', () => {
    const current = { ...project, shots: [{ ...shot, name: '新名称', script: '新增剧情', inputs: { prompt: '新的提示词', seed: 0 }, executionId: 'running-job' }, project.shots[1]] }
    const next = applyUploadedVideoInput(current, target, image)
    expect(next.shots[0]).toMatchObject({ name: '新名称', script: '新增剧情', executionId: 'running-job', inputs: { prompt: '新的提示词', seed: 0, image: { asset_id: image.id, asset_version_id: 'v2' } } })
    expect(current.shots[0].inputs).toEqual({ prompt: '新的提示词', seed: 0 })
  })

  it('拒绝缺失版本、不匹配的素材类型、超出大小限制及非素材字段', () => {
    expect(() => applyUploadedVideoInput(project, target, { ...image, current_version: 3 })).toThrow('版本')
    expect(() => applyUploadedVideoInput(project, target, { ...image, versions: image.versions.map(version => ({ ...version, mime_type: 'video/mp4' })) })).toThrow('图片')
    expect(() => applyUploadedVideoInput(project, target, { ...image, versions: image.versions.map(version => ({ ...version, size_bytes: 256 * 1024 * 1024 + 1 })) })).toThrow('256 MiB')
    expect(() => applyUploadedVideoInput(project, { ...target, field: fields[0] }, image)).toThrow('素材字段')
    expect(project.shots[0].inputs).toEqual({ prompt: '茶园' })
  })
})

describe('视频生成参数检查', () => {
  it('必填文字不能只有空白，零值与布尔 false 均保留', () => {
    expect(videoGenerationInputIssues(fields, values, [image])).toEqual([])
    expect(videoGenerationInputIssues(fields, { ...values, prompt: ' \n ' }, [image])).toEqual([{ key: 'prompt', message: '填写此参数' }])
    expect(videoGenerationInputIssues(fields, {}, [image]).map(issue => issue.key)).toEqual(['prompt', 'seed', 'sound'])
  })

  it('拒绝无法提交的非有限数值，检查过程不改变输入', () => {
    const inputs = { ...values, seed: NaN }
    const before = structuredClone(inputs)
    expect(videoGenerationInputIssues(fields, inputs, [image])).toEqual([{ key: 'seed', message: '填写有效数字' }])
    expect(inputs).toEqual(before)
    expect(videoGenerationInputIssues(fields, { ...values, seed: Infinity }, [image])).toHaveLength(1)
  })

  it('使用固定素材版本，拒绝缺失版本、错误类型及超过大小限制的素材', () => {
    const inputs = { ...values, image: { asset_id: image.id, asset_version_id: 'v1' } }
    expect(videoGenerationInputIssues(fields, inputs, [image])).toEqual([])
    expect(videoGenerationInputIssues(fields, { ...inputs, image: { ...inputs.image, asset_version_id: 'removed' } }, [image])).toEqual([{ key: 'image', message: '素材版本已不可用，请重新选择' }])
    expect(videoGenerationInputIssues(fields, inputs, [{ ...image, versions: [{ ...image.versions[0], mime_type: 'video/mp4' }] }])).toEqual([{ key: 'image', message: '选择图片素材' }])
    expect(videoGenerationInputIssues(fields, inputs, [{ ...image, versions: [{ ...image.versions[0], size_bytes: 256 * 1024 * 1024 + 1 }] }])).toEqual([{ key: 'image', message: '素材文件不能超过 256 MiB' }])
  })

  it('资产目录未读取完成时不判定素材缺失，格式不完整的引用始终需要处理', () => {
    const inputs = { ...values, image: { asset_id: image.id, asset_version_id: 'v1' } }
    expect(videoGenerationInputIssues(fields, inputs)).toEqual([])
    expect(videoGenerationInputIssues(fields, inputs, [])).toHaveLength(1)
    expect(videoGenerationInputIssues(fields, { ...values, image: {} })).toEqual([{ key: 'image', message: '选择素材版本' }])
    expect(videoGenerationInputIssues(fields, { ...values, image: null }, [])).toEqual([])
  })

  it('检查整数、上下限、倍数、枚举和文字规则，保留原始数据与 Schema', () => {
    const schema = { type: 'object', properties: { seed: { type: 'integer', minimum: 0, maximum: 8, multipleOf: 2 }, prompt: { type: 'string', minLength: 2, maxLength: 8, pattern: '^茶' }, mode: { enum: ['fast', 'quality'], default: 'fast' } } }
    const inputs = { ...values, prompt: 'a', seed: 9.5, mode: 'unknown' }
    const definition = [...fields, { key: 'mode', type: 'enum' as const, required: false }]
    const before = structuredClone({ schema, inputs })
    const issues = videoGenerationInputIssues(definition, inputs, [], schema)
    expect(issues).toEqual(expect.arrayContaining([
      { key: 'seed', message: '不能大于 8' }, { key: 'seed', message: '填写整数' }, { key: 'seed', message: '填写 2 的整数倍' },
      { key: 'prompt', message: '至少输入 2 个字符' }, { key: 'mode', message: '选择一个有效选项' },
    ]))
    expect(videoGenerationInputIssues(definition, { ...values, prompt: '茶园', seed: 4 }, [], schema)).toEqual([])
    expect({ schema, inputs }).toEqual(before)
  })

  it('解析内部引用并处理条件必填和字段依赖', () => {
    const schema = { type: 'object', $defs: { seed: { type: 'integer', minimum: 1 } }, properties: { seed: { $ref: '#/$defs/seed' } }, if: { properties: { sound: { const: true } }, required: ['sound'] }, then: { required: ['image'] }, dependentRequired: { image: ['seed'] } }
    expect(videoGenerationInputIssues(fields, { ...values, seed: -1 }, [], schema)).toContainEqual({ key: 'seed', message: '不能小于 1' })
    expect(videoGenerationInputIssues(fields, { ...values, sound: true, seed: 2 }, [], schema)).toContainEqual({ key: 'image', message: '填写此参数' })
    expect(videoGenerationInputIssues(fields.map(field => ({ ...field, required: false })), { image: { asset_id: image.id, asset_version_id: 'v1' } }, [image], schema)).toContainEqual({ key: 'seed', message: '补充关联参数' })
  })

  it('字段组合错误显示在表单中，特殊字符字段可以定位', () => {
    const combined = { type: 'object', oneOf: [{ required: ['seed'] }, { required: ['sound'] }] }
    expect(videoGenerationInputIssues(fields, values, [], combined)).toContainEqual({ key: undefined, message: '当前参数同时满足多个互斥条件，请调整参数组合' })
    const escaped = [{ key: '画面/时长~秒', type: 'number' as const, required: false }]
    expect(videoGenerationInputIssues(escaped, { '画面/时长~秒': -1 }, [], { type: 'object', properties: { '画面/时长~秒': { minimum: 1 } } })).toEqual([{ key: escaped[0].key, message: '不能小于 1' }])
  })

  it('支持各版本 Schema 的边界和格式校验规则', () => {
    const inputs = [{ key: 'email', type: 'string' as const, required: true }]
    for (const draft of ['draft-04', 'draft-06', 'draft-07']) {
      const schema = { $schema: `http://json-schema.org/${draft}/schema#`, type: 'object', properties: { email: { type: 'string', format: 'email' } } }
      expect(videoGenerationInputIssues(inputs, { email: 'invalid' }, [], schema)).toHaveLength(1)
    }
    for (const draft of ['2019-09', '2020-12']) {
      const schema = { $schema: `https://json-schema.org/draft/${draft}/schema`, type: 'object', properties: { email: { type: 'string', format: 'email' } } }
      expect(videoGenerationInputIssues(inputs, { email: 'invalid' }, [], schema)).toEqual([])
    }
    const schema = { $schema: 'http://json-schema.org/draft-04/schema#', type: 'object', properties: { seed: { minimum: 0, exclusiveMinimum: true } } }
    expect(videoGenerationInputIssues(fields, values, [], schema)).toContainEqual({ key: 'seed', message: '必须大于 0' })
    expect(videoGenerationInputIssues(fields, { ...values, seed: .5 }, [], schema)).toEqual([])
  })

  it('无效 Schema 和无法解析的引用直接报告错误', () => {
    expect(() => videoGenerationInputIssues(fields, values, [], { type: 'object', properties: 42 })).toThrow()
    expect(() => videoGenerationInputIssues(fields, values, [], { type: 'object', properties: { seed: { $ref: '#/$defs/missing' } } })).toThrow()
  })
})
