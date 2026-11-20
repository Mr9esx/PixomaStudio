import { z } from 'zod'
import { compileSchema, draft04, draft06, draft07, draft2019, draft2020, type JsonError, type SchemaNode } from 'json-schema-library'
import { addFormats } from 'json-schema-library/formats'
import type { StudioAsset } from '@/lib/api/studio'
import type { WorkflowInput } from '@/lib/api/workflows'
import { validateVideoProject, type VideoProject, type VideoShot } from './video-editor-model'

const assetInput = z.object({ asset_id: z.string().min(1), asset_version_id: z.string().min(1) })
const mediaLabels: Record<string, string> = { image: '图片', video: '视频', audio: '音频', file: '文件' }
const drafts = [draft04, draft06, draft07, draft2019, draft2020]
addFormats(drafts)
const validators = new WeakMap<Record<string, unknown>, SchemaNode>()
export type VideoGenerationInputIssue = { key?: string; message: string }
export type VideoInputUploadTarget = { shotId: string; workflowId: string; versionId: string; field: WorkflowInput; baseline: unknown }

export function applyUploadedVideoMedia(project: VideoProject, target: Pick<VideoShot, 'id' | 'media' | 'sourceIn' | 'duration'>, asset: StudioAsset): VideoProject {
  const shot = project.shots.find(shot => shot.id === target.id)
  if (!shot) throw new Error('资产已导入，目标分镜已删除，可从项目资产重新选择')
  const version = asset.versions.find(version => version.version === asset.current_version)
  if (!version) throw new Error('上传资产的版本不存在')
  if (asset.kind !== 'video' || !version.mime_type.startsWith('video/')) throw new Error('资产已导入，选择视频素材后采用')
  const duration = version.metadata?.duration
  if (typeof duration !== 'number' || !Number.isFinite(duration) || duration <= 0) throw new Error('资产已导入，视频时长不可用')
  if ((shot.media?.assetId === asset.id && shot.media.versionId === version.id) || shot.versions?.some(item => item.media.assetId === asset.id && item.media.versionId === version.id)) return project
  const media = { assetId: asset.id, versionId: version.id, url: version.content_url, duration }
  const next = { ...project, shots: project.shots.map(item => item.id === target.id ? { ...item, versions: [...(item.versions ?? []), { id: version.id, media, status: 'candidate' as const }] } : item) }
  validateVideoProject(next)
  return next
}

export function applyUploadedVideoInput(project: VideoProject, target: VideoInputUploadTarget, asset: StudioAsset): VideoProject {
  const shot = project.shots.find(shot => shot.id === target.shotId)
  if (!shot) throw new Error('资产已导入，目标分镜已删除，可从项目资产重新选择')
  if (shot.workflowId !== target.workflowId || shot.versionId !== target.versionId) throw new Error('资产已导入，目标工作流已改变，可从项目资产重新选择')
  if (!mediaLabels[target.field.type]) throw new Error('上传目标需要是素材字段')
  const version = asset.versions.find(version => version.version === asset.current_version)
  if (!version) throw new Error('上传资产的版本不存在')
  const value = { asset_id: asset.id, asset_version_id: version.id }
  const issues = videoGenerationInputIssues([target.field], { [target.field.key]: value }, [asset])
  if (issues.length) throw new Error(`资产已导入，${issues[0].message}`)
  const parsedPrevious = assetInput.safeParse(shot.inputs[target.field.key])
  const parsedBaseline = assetInput.safeParse(target.baseline)
  const previous = JSON.stringify(parsedPrevious.success ? parsedPrevious.data : shot.inputs[target.field.key])
  if (previous === JSON.stringify(value)) return project
  if (previous !== JSON.stringify(parsedBaseline.success ? parsedBaseline.data : target.baseline)) throw new Error('资产已导入，目标参数已改变，可从项目资产重新选择')
  const next = { ...project, shots: project.shots.map(shot => shot.id === target.shotId ? { ...shot, inputs: { ...shot.inputs, [target.field.key]: value } } : shot) }
  validateVideoProject(next)
  return next
}

function schemaMessage(error: JsonError): string {
  const data = error.data
  switch (error.code) {
    case 'required-property-error': return '填写此参数'
    case 'missing-dependency-error': return '补充关联参数'
    case 'minimum-error': return data.schema.exclusiveMinimum === true ? `必须大于 ${data.schema.minimum}` : data.schema.exclusiveMinimum === data.minimum ? `必须大于 ${data.minimum}` : `不能小于 ${data.minimum}`
    case 'maximum-error': return data.schema.exclusiveMaximum === true ? `必须小于 ${data.schema.maximum}` : data.schema.exclusiveMaximum === data.maximum ? `必须小于 ${data.maximum}` : `不能大于 ${data.maximum}`
    case 'exclusive-minimum-error': return `必须大于 ${data.minimum}`
    case 'exclusive-maximum-error': return `必须小于 ${data.maximum}`
    case 'multiple-of-error': return `填写 ${data.multipleOf} 的整数倍`
    case 'min-length-error': return `至少输入 ${data.minLength} 个字符`
    case 'min-length-one-error': return '填写此参数'
    case 'max-length-error': return `最多输入 ${data.maxLength} 个字符`
    case 'enum-error': return '选择一个有效选项'
    case 'const-error': return `必须为 ${JSON.stringify(data.expected)}`
    case 'pattern-error': return typeof data.schema.description === 'string' ? data.schema.description : '输入格式不符合要求'
    case 'type-error': return data.expected === 'integer' ? '填写整数' : '输入类型不符合要求'
    case 'multiple-one-of-error': return '当前参数同时满足多个互斥条件，请调整参数组合'
    case 'any-of-error': case 'one-of-error': case 'all-of-error': case 'not-error': return '参数组合不符合工作流要求'
    case 'no-additional-properties-error': return `工作流不接受参数 ${data.property}`
    case 'min-items-error': return `至少选择 ${data.minItems} 项`
    case 'max-items-error': return `最多选择 ${data.maximum} 项`
    case 'unique-items-error': return '移除重复项'
    default: return String(error.code).startsWith('format-') ? `填写有效的 ${data.schema.format}` : '输入不符合工作流要求'
  }
}

export function videoGenerationInputIssues(fields: WorkflowInput[], inputs: Record<string, unknown>, assets?: StudioAsset[], schema?: Record<string, unknown>) {
  const issues: VideoGenerationInputIssue[] = []
  for (const field of fields) {
    const value = inputs[field.key]
    const empty = value === undefined || value === null || (typeof value === 'string' && !value.trim())
    let message: string | undefined
    if (empty) {
      if (field.required) message = mediaLabels[field.type] ? '选择素材版本' : '填写此参数'
    } else if (field.type === 'number' && (typeof value !== 'number' || !Number.isFinite(value))) {
      message = '填写有效数字'
    } else if (mediaLabels[field.type]) {
      const parsed = assetInput.safeParse(value)
      if (!parsed.success) message = '选择素材版本'
      else if (assets) {
        const asset = assets.find(asset => asset.id === parsed.data.asset_id)
        const version = asset?.versions.find(version => version.id === parsed.data.asset_version_id)
        if (!version) message = '素材版本已不可用，请重新选择'
        else if (field.type !== 'file' && !version.mime_type.startsWith(`${field.type}/`)) message = `选择${mediaLabels[field.type]}素材`
        else if (version.size_bytes > 256 * 1024 * 1024) message = '素材文件不能超过 256 MiB'
      }
    }
    if (message) issues.push({ key: field.key, message })
  }
  if (schema) {
    let validator = validators.get(schema)
    if (!validator) {
      const draft = drafts.find(draft => new RegExp(draft.$schemaRegEx).test(String(schema.$schema ?? ''))) ?? draft2020
      validator = compileSchema(schema, { drafts, formatAssertion: [draft04, draft06, draft07].includes(draft), throwOnInvalidSchema: true, throwOnInvalidRef: true })
      validators.set(schema, validator)
    }
    const invalidKeys = new Set(issues.map(issue => issue.key))
    for (const error of validator.validate(inputs).errors) {
      const pointer = error.data.pointer
      const key = fields.find(field => {
        const path = `#/${field.key}`
        return pointer === path || pointer.startsWith(`${path}/`) || (pointer === '#' && (error.data.key === field.key || error.data.missingProperty === field.key))
      })?.key
      const message = schemaMessage(error)
      if (!invalidKeys.has(key) && !issues.some(issue => issue.key === key && issue.message === message)) issues.push({ key, message })
    }
  }
  return issues
}
