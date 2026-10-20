import { useRef, useState } from 'react'
import { useMutation } from '@tanstack/react-query'
import { Check, RotateCcw, Upload } from 'lucide-react'
import { resolveStudioExpertCard, type StudioAsset, type StudioExpertCardRecord, type StudioExpertTask } from '@/lib/api/studio'
import { Badge } from '@/components/ui/badge'
import { Button } from '@/components/ui/button'
import { Checkbox } from '@/components/ui/checkbox'
import { Field, FieldLabel } from '@/components/ui/field'
import { Input } from '@/components/ui/input'
import { Textarea } from '@/components/ui/textarea'
import { Select, SelectContent, SelectItem, SelectTrigger, SelectValue } from '@/components/ui/select'
import { StatusDot } from '@/components/status-dot'
import { Dialog, DialogContent, DialogHeader, DialogTitle } from '@/components/ui/dialog'
import { AssetKindIcon, AssetPreview } from './studio-assets'

const statusLabels: Record<string, string> = { pending: '待开始', running: '进行中', waiting_user: '等待确认', waiting_tool: '等待生成', completed: '已完成', skipped: '已跳过', failed: '执行失败', paused: '已暂停', cancelled: '已取消', active: '进行中' }

export function ExpertTaskProgress({ tasks, onReturn }: { tasks: StudioExpertTask[]; onReturn?: (stepId: string, title: string) => void }) {
  const task = tasks.at(-1)
  if (!task) return null
  return <section className='flex flex-col gap-3 rounded-lg border bg-muted p-3' aria-label='专家任务进度'>
    <div className='flex items-center justify-between gap-3 text-sm'><span>{task.goal}</span><Badge variant='secondary'>{statusLabels[task.status]}</Badge></div>
    <ol className='flex flex-wrap gap-3'>{task.steps.map((step, index) => <li key={step.step_id} className='flex items-center gap-2'><StatusDot state={step.status === 'completed' ? 'ok' : 'warn'} label={statusLabels[step.status]} /><Button size='sm' variant='ghost' disabled={!onReturn} onClick={() => onReturn?.(step.step_id, step.title)} aria-label={`返回${step.title}`}>{index + 1}. {step.title} · {statusLabels[step.status]}{step.step_id === task.current_step_id ? ' · 当前' : ''}</Button></li>)}</ol>
  </section>
}

type Action = 'start' | 'defer' | 'skip' | 'accept' | 'revise'
const actionLabels: Record<Action, string> = { start: '开始步骤', defer: '继续补充', skip: '跳过步骤', accept: '确认结果', revise: '修改结果' }

export function StudioExpertCard({ record, task, assets, endpointBase, onResponded, onUploadAsset }: { record: StudioExpertCardRecord; task?: StudioExpertTask; assets: StudioAsset[]; endpointBase?: string; onResponded: () => void; onUploadAsset?: (file: File) => Promise<StudioAsset> }) {
  const card = record.expert_card
  const [values, setValues] = useState<Record<string, unknown>>(record.response_json?.inputs ?? card.suggested_inputs ?? {})
  const [comment, setComment] = useState(record.response_json?.comment ?? '')
  const [uploading, setUploading] = useState(false)
  const [uploadError, setUploadError] = useState('')
  const [uploaded, setUploaded] = useState<StudioAsset[]>([])
  const [previewAsset, setPreviewAsset] = useState<StudioAsset>()
  const requests = useRef(new Map<string, string>())
  const allAssets = [...assets, ...uploaded.filter(asset => !assets.some(existing => existing.id === asset.id))]
  const versions = allAssets.flatMap(asset => asset.versions.map(version => ({ asset, version })))
  const stale = record.status === 'pending' && task != null && record.expected_task_revision !== task.revision
  const respond = useMutation({ mutationFn: (action: Action) => {
    const key = JSON.stringify({ action, values, comment })
    const response_id = requests.current.get(key) ?? crypto.randomUUID()
    requests.current.set(key, response_id)
    return resolveStudioExpertCard(record.id, { response_id, action, inputs: values, comment }, endpointBase)
  }, onSuccess: onResponded })
  const disabled = respond.isPending || uploading || record.status !== 'pending' || stale
  const requiredMissing = (card.fields ?? []).some(field => {
    if (!field.required) return false
    const value = values[field.key]
    return value == null || (typeof value === 'string' && !value.trim()) || (Array.isArray(value) && value.length === 0)
  })
  const update = (key: string, value: unknown) => setValues(current => ({ ...current, [key]: value }))
  return <section className='flex flex-col gap-4 rounded-lg border bg-muted p-4' aria-label={card.mode === 'step_entry' ? '步骤输入确认' : '步骤结果确认'}>
    <div className='flex items-center justify-between gap-2'><span className='text-sm font-medium'>{task?.steps.find(step => step.step_id === record.step_id)?.title} · {card.mode === 'step_entry' ? '步骤输入' : '步骤结果'}</span>{record.response_action ? <Badge variant='secondary'>{actionLabels[record.response_action as Action]}</Badge> : null}</div>
    <p className='text-sm whitespace-pre-wrap'>{card.reason}</p>
    {card.results?.summary ? <p className='rounded-md border bg-background p-3 text-sm whitespace-pre-wrap'>{card.results.summary}</p> : null}
    {card.results?.assets?.map(ref => { const asset = allAssets.find(item => item.id === ref.asset_id); const version = asset?.versions.find(item => item.id === ref.asset_version_id); return asset && version ? <Button key={version.id} variant='outline' className='w-fit' onClick={() => setPreviewAsset({ ...asset, current_version: version.version, versions: [version] })}><AssetKindIcon kind={asset.kind} className='size-4' />{asset.name} · v{version.version}</Button> : null })}
    {(card.fields ?? []).map(field => <Field key={field.key}><FieldLabel htmlFor={`${record.id}-${field.key}`}>{field.label}{field.required ? ' *' : ''}</FieldLabel>
      {field.type === 'textarea' ? <Textarea id={`${record.id}-${field.key}`} value={String(values[field.key] ?? '')} disabled={disabled} onChange={event => update(field.key, event.target.value)} />
        : field.type === 'boolean' ? <Checkbox id={`${record.id}-${field.key}`} checked={values[field.key] === true} disabled={disabled} onCheckedChange={value => update(field.key, value === true)} />
          : field.type === 'select' ? <Select value={typeof values[field.key] === 'string' ? values[field.key] as string : ''} disabled={disabled} onValueChange={value => update(field.key, value)}><SelectTrigger id={`${record.id}-${field.key}`} className='bg-background'><SelectValue placeholder='请选择' /></SelectTrigger><SelectContent>{field.options?.map(value => <SelectItem key={value} value={value}>{value}</SelectItem>)}</SelectContent></Select>
            : field.type === 'asset' ? <Select value={(values[field.key] as { asset_version_id?: string } | undefined)?.asset_version_id ?? ''} disabled={disabled} onValueChange={id => { const selected = versions.find(item => item.version.id === id); if (!selected) throw new Error('资产版本不存在'); update(field.key, { asset_id: selected.asset.id, asset_version_id: id }) }}><SelectTrigger id={`${record.id}-${field.key}`} className='bg-background'><SelectValue placeholder='选择会话资产' /></SelectTrigger><SelectContent>{versions.map(item => <SelectItem key={item.version.id} value={item.version.id}>{item.asset.name} · v{item.version.version}</SelectItem>)}</SelectContent></Select>
              : field.type === 'assets' ? <div className='flex max-h-48 flex-col gap-2 overflow-y-auto rounded-md border bg-background p-3'>{versions.map(item => { const refs = (values[field.key] ?? []) as { asset_id: string; asset_version_id: string }[]; return <label key={item.version.id} className='flex items-center gap-2 text-sm'><Checkbox checked={refs.some(ref => ref.asset_version_id === item.version.id)} disabled={disabled} onCheckedChange={checked => update(field.key, checked ? [...refs, { asset_id: item.asset.id, asset_version_id: item.version.id }] : refs.filter(ref => ref.asset_version_id !== item.version.id))} />{item.asset.name} · v{item.version.version}</label> })}</div>
                : <Input id={`${record.id}-${field.key}`} type={field.type === 'number' ? 'number' : 'text'} value={String(values[field.key] ?? '')} disabled={disabled} onChange={event => update(field.key, field.type === 'number' ? event.target.value === '' ? undefined : Number(event.target.value) : event.target.value)} />}
      {(field.type === 'asset' || field.type === 'assets') && onUploadAsset ? <label className='flex w-fit cursor-pointer items-center gap-2 text-sm'><Upload className='size-4' />上传资产<input type='file' disabled={disabled} className='sr-only' onChange={event => { const file = event.target.files?.[0]; event.target.value = ''; if (!file) return; setUploading(true); setUploadError(''); void onUploadAsset(file).then(asset => { setUploaded(current => [...current, asset]); const version = asset.versions.find(item => item.version === asset.current_version); if (!version) throw new Error('上传资产版本不存在'); const ref = { asset_id: asset.id, asset_version_id: version.id }; update(field.key, field.type === 'asset' ? ref : [...((values[field.key] ?? []) as unknown[]), ref]) }).catch(error => setUploadError(error instanceof Error ? error.message : '上传失败')).finally(() => setUploading(false)) }} /></label> : null}
    </Field>)}
    {record.status === 'pending' ? <><Textarea aria-label='补充说明' placeholder='补充说明或修改要求' value={comment} disabled={disabled} onChange={event => setComment(event.target.value)} /><div className='flex flex-wrap gap-2'>{(card.mode === 'step_entry' ? ['start', 'defer', 'skip'] as Action[] : ['accept', 'revise'] as Action[]).map(action => <Button key={action} size='sm' variant={action === 'start' || action === 'accept' ? 'default' : 'outline'} disabled={disabled || (action === 'start' && requiredMissing)} onClick={() => respond.mutate(action)}>{action === 'accept' ? <Check /> : action === 'revise' ? <RotateCcw /> : null}{actionLabels[action]}</Button>)}</div></> : null}
    {record.status !== 'pending' && record.response_json?.comment ? <p className='text-sm whitespace-pre-wrap'>{record.response_json.comment}</p> : null}
    {stale ? <p className='text-sm text-warning'>任务资料已更新，请在对话中重新确认。</p> : null}
    {respond.error || uploadError ? <p role='alert' className='text-sm text-destructive'>{respond.error?.message ?? uploadError}</p> : null}
    <Dialog open={Boolean(previewAsset)} onOpenChange={open => { if (!open) setPreviewAsset(undefined) }}><DialogContent className='max-w-3xl' aria-describedby={undefined}><DialogHeader><DialogTitle>{previewAsset?.name} · v{previewAsset?.current_version}</DialogTitle></DialogHeader><div className='h-[60vh] overflow-hidden rounded-md border bg-background'>{previewAsset ? <AssetPreview asset={previewAsset} preview expanded /> : null}</div></DialogContent></Dialog>
  </section>
}
