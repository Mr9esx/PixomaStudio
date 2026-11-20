import type { WorkflowDefinition } from '@/lib/api/workflows'
import type { StudioAsset } from '@/lib/api/studio'
import { Checkbox } from '@/components/ui/checkbox'
import { Field, FieldError, FieldGroup, FieldLabel } from '@/components/ui/field'
import { Input } from '@/components/ui/input'
import { Select, SelectContent, SelectGroup, SelectItem, SelectTrigger, SelectValue } from '@/components/ui/select'
import { Textarea } from '@/components/ui/textarea'
import { Button } from '@/components/ui/button'

type Property = { type?: string; title?: string; default?: unknown; enum?: unknown[]; minimum?: number; maximum?: number; multipleOf?: number; maxLength?: number; minItems?: number; maxItems?: number }

export function workflowDefaultValues(definition: Pick<WorkflowDefinition, 'inputs' | 'input_schema'>): Record<string, unknown> {
  const properties = definition.input_schema.properties as Record<string, Property> | undefined
  return Object.fromEntries(definition.inputs.flatMap(field => properties?.[field.key]?.default === undefined ? [] : [[field.key, properties[field.key].default]]))
}

export function WorkflowParameterFields({ definition, values, onChange, disabled, onUploadAsset, assets, errors, prefix = 'workflow-parameter' }: {
  definition: Pick<WorkflowDefinition, 'inputs' | 'input_schema'>
  values: Record<string, unknown>
  onChange: (key: string, value: unknown) => void
  disabled?: boolean
  onUploadAsset?: (key: string, file: File) => void
  assets?: StudioAsset[]
  errors?: Record<string, string>
  prefix?: string
}) {
  const properties = definition.input_schema.properties as Record<string, Property> | undefined
  return <FieldGroup>{definition.inputs.map(field => {
    const property = properties?.[field.key]
    const id = `${prefix}-${field.key}`
    const value = values[field.key]
    const media = ['image', 'video', 'audio', 'file'].includes(field.type)
    const multiple = media && property?.type === 'array'
    const references = (multiple && Array.isArray(value) ? value : []) as Array<{ asset_id: string; asset_version_id: string }>
    const full = multiple && property?.maxItems !== undefined && references.length >= property.maxItems
    const error = errors?.[field.key]
    const accessibility = { 'aria-invalid': Boolean(error), 'aria-describedby': error ? `${id}-error` : undefined }
    return <Field key={field.key} data-invalid={Boolean(error)} aria-describedby={error ? `${id}-error` : undefined}>
      <FieldLabel htmlFor={id}>{property?.title || field.description || field.key}{field.required ? ' *' : ''}</FieldLabel>
      {media ? <div className='flex flex-col gap-2'>
        {assets ? <Select value={multiple ? 'unselected' : (value as { asset_version_id?: string } | undefined)?.asset_version_id ?? 'unselected'} disabled={disabled || full} onValueChange={selected => {
          if (selected === 'unselected') { onChange(field.key, undefined); return }
          const asset = assets.find(item => item.versions.some(version => version.id === selected))
          if (!asset) throw new Error('资产版本不存在')
          const reference = { asset_id: asset.id, asset_version_id: selected }
          onChange(field.key, multiple ? [...references.filter(item => item.asset_version_id !== selected), reference] : reference)
        }}>
          <SelectTrigger id={id} {...accessibility}><SelectValue placeholder='选择项目资产' /></SelectTrigger>
          <SelectContent><SelectGroup><SelectItem value='unselected'>暂不选择</SelectItem>{assets.filter(asset => field.type === 'file' || asset.kind === field.type).flatMap(asset => asset.versions.map(version => <SelectItem key={version.id} value={version.id}>{asset.name} · v{version.version}</SelectItem>))}</SelectGroup></SelectContent>
        </Select> : null}
        <Input id={assets ? `${id}-upload` : id} {...accessibility} aria-label={`${property?.title || field.description || field.key}上传文件`} type='file' accept={field.type === 'file' ? undefined : `${field.type}/*`} required={field.required && (multiple ? !references.length : !value)} disabled={disabled || !onUploadAsset || full} onChange={event => { const file = event.currentTarget.files?.[0]; event.currentTarget.value = ''; if (file) onUploadAsset?.(field.key, file) }} />
        {multiple ? references.map((reference, index) => <div key={reference.asset_version_id} className='flex items-center justify-between gap-2 rounded-lg bg-muted px-3 py-2 text-sm'><span className='truncate'>{assets?.find(asset => asset.id === reference.asset_id)?.name ?? `素材 ${index + 1}`}</span><Button type='button' size='sm' variant='ghost' disabled={disabled} aria-label={`移除素材 ${index + 1}`} onClick={() => onChange(field.key, references.filter((_, current) => current !== index))}>移除</Button></div>) : null}
      </div> : field.type === 'boolean' ? <Checkbox id={id} {...accessibility} checked={value === true} disabled={disabled} onCheckedChange={checked => onChange(field.key, checked === true)} />
        : property?.enum ? <Select value={value === undefined ? '' : JSON.stringify(value)} required={field.required} disabled={disabled} onValueChange={selected => onChange(field.key, property.enum?.find(option => JSON.stringify(option) === selected))}>
          <SelectTrigger id={id} {...accessibility}><SelectValue /></SelectTrigger>
          <SelectContent><SelectGroup>{property.enum.map(option => <SelectItem key={JSON.stringify(option)} value={JSON.stringify(option)}>{option === '' ? '空字符串' : String(option)}</SelectItem>)}</SelectGroup></SelectContent>
        </Select> : field.type === 'number' ? <Input id={id} {...accessibility} type='number' min={property?.minimum} max={property?.maximum} step={property?.multipleOf ?? (property?.type === 'integer' ? 1 : 'any')} value={typeof value === 'number' && Number.isFinite(value) ? value : ''} required={field.required} disabled={disabled} onChange={event => onChange(field.key, event.target.value === '' ? undefined : event.target.valueAsNumber)} />
          : <Textarea id={id} {...accessibility} value={typeof value === 'string' ? value : ''} required={field.required} maxLength={property?.maxLength} disabled={disabled} onChange={event => onChange(field.key, event.target.value)} />}
      {error ? <FieldError id={`${id}-error`}>{error}</FieldError> : null}
    </Field>
  })}</FieldGroup>
}
