import { useState } from 'react'
import { useQueries, useQuery } from '@tanstack/react-query'
import { Button } from '@/components/ui/button'
import { Checkbox } from '@/components/ui/checkbox'
import { Dialog, DialogContent, DialogFooter, DialogHeader, DialogTitle } from '@/components/ui/dialog'
import { FieldError } from '@/components/ui/field'
import { Field, FieldLabel } from '@/components/ui/field'
import { Input } from '@/components/ui/input'
import { Select, SelectContent, SelectGroup, SelectItem, SelectTrigger, SelectValue } from '@/components/ui/select'
import { getWorkflowVersion } from '@/lib/api/workflows'
import { getComputeNode } from '@/lib/api/compute-nodes'
import { listGenerationConnections } from '@/lib/api/generation'
import type { StudioAsset } from '@/lib/api/studio'
import { videoGenerationPending, type VideoGeneration, type VideoGenerationDependency } from '@/lib/api/video-projects'
import type { VideoProject } from './video-editor-model'
import { reviewVideoGenerationShot, videoGenerationDependencyPlan, videoGenerationConnectionIssue } from './video-generation-batch-review'
import { GenerationInputValue } from './video-generation-records'

export function VideoGenerationBatchDialog({ project, initialIds, assets, enabledWorkflowIds, records, recordsReady, disabled, pending, error, onConfirm, onClose }: {
  project: VideoProject; initialIds: string[]; assets: StudioAsset[]; enabledWorkflowIds: string[]; records: VideoGeneration[]; recordsReady: boolean
  disabled: boolean; pending: boolean; error?: string; onConfirm: (baseline: VideoProject, ids: string[], dependencies: VideoGenerationDependency[]) => void; onClose: () => void
}) {
  const [baseline] = useState(() => structuredClone(project))
  const [ids, setIds] = useState(initialIds)
  const [dependencies, setDependencies] = useState<VideoGenerationDependency[]>([])
  const activeDependencies = dependencies.filter(dependency => ids.includes(dependency.shot_id))
  const workflows = useQueries({ queries: baseline.shots.map(shot => ({ queryKey: ['studio', 'video-workflow-version', shot.workflowId, shot.versionId], enabled: Boolean(shot.workflowId && shot.versionId), queryFn: () => getWorkflowVersion(shot.workflowId!, shot.versionId!) })) })
  const connections = useQuery({ queryKey: ['studio', 'generation-connections'], queryFn: listGenerationConnections, enabled: workflows.some(workflow => workflow.data?.definition.type === 'online'), refetchInterval: 5000 })
  const nodes = useQueries({ queries: workflows.map(workflow => {
    const definition = workflow.data?.definition
    const nodeId = definition?.type === 'comfyui' ? definition.execution.node_id : undefined
    return { queryKey: ['studio', 'compute-node', nodeId], enabled: Boolean(nodeId), queryFn: () => getComputeNode(nodeId!), refetchInterval: 5000 }
  }) })
  const entries = baseline.shots.map((shot, index) => {
    const workflow = workflows[index], node = nodes[index]
    const review = workflow.data ? reviewVideoGenerationShot(baseline, shot, workflow.data, assets, dependencies) : { inputs: shot.inputs, issues: [] }
    const issues = [...review.issues]
    if (!shot.workflowId || !shot.versionId) issues.push('选择已发布的视频工作流')
    else if (workflow.error) issues.push(workflow.error.message)
    else if (!workflow.data) issues.push('正在读取工作流')
    if (shot.workflowId && !enabledWorkflowIds.includes(shot.workflowId)) issues.push('工作流已停用')
    if (!recordsReady) issues.push('正在读取生成记录')
    if (records.some(record => record.shot_id === shot.id && videoGenerationPending(record))) issues.push('已有生成任务')
    if (workflow.data?.definition.type === 'comfyui' && node.data?.healthy !== true) issues.push(node.error?.message || '等待计算节点就绪')
    const connectionIssue = videoGenerationConnectionIssue(workflow.data?.definition, connections.isSuccess ? connections.data : undefined)
    if (connectionIssue) issues.push(connections.error?.message || connectionIssue)
    return { shot, workflow: workflow.data, inputs: review.inputs, issues }
  })
  let eligibleIds = entries.filter(entry => !entry.issues.length).map(entry => entry.shot.id)
  while (true) {
    const next = eligibleIds.filter(id => !dependencies.some(dependency => dependency.shot_id === id && !eligibleIds.includes(dependency.source_shot_id)))
    if (next.length === eligibleIds.length) break
    eligibleIds = next
  }
  const validIds = videoGenerationDependencyPlan(baseline, eligibleIds, dependencies.filter(dependency => eligibleIds.includes(dependency.shot_id))).order
  const plan = videoGenerationDependencyPlan(baseline, ids, activeDependencies)
  const selected = entries.filter(entry => ids.includes(entry.shot.id))
  function changeDependency(shotId: string, inputKey: string, value?: VideoGenerationDependency) {
    setDependencies(current => [...current.filter(dependency => dependency.shot_id !== shotId || dependency.input_key !== inputKey), ...(value ? [value] : [])])
  }
  return <Dialog open onOpenChange={open => { if (!open && !pending) onClose() }}><DialogContent aria-describedby={undefined} className='max-h-[85vh] overflow-y-auto shadow-lg sm:max-w-2xl'>
    <DialogHeader><DialogTitle>批量生成确认</DialogTitle></DialogHeader>
    {error ? <FieldError>{error}</FieldError> : null}
    <div className='flex flex-wrap items-center gap-2'><span className='text-sm'>已选择 {selected.length} 个分镜</span><Button size='sm' variant='outline' disabled={pending} onClick={() => setIds(baseline.shots.map(shot => shot.id))}>选择全部</Button><Button size='sm' variant='outline' disabled={pending} onClick={() => setIds(validIds)}>仅选择可生成分镜</Button><Button size='sm' variant='ghost' disabled={pending} onClick={() => setIds([])}>清除选择</Button></div>
    {plan.issues.map((issue, index) => <FieldError key={index}>{issue}</FieldError>)}
    {activeDependencies.length && !plan.issues.length ? <div className='rounded-lg border p-3' aria-label='生成依赖顺序'><p className='text-sm font-medium'>生成依赖顺序</p><ol className='mt-2 flex list-inside list-decimal flex-col gap-1 text-sm'>{plan.order.map(id => <li key={id}>{baseline.shots.find(shot => shot.id === id)?.name}{activeDependencies.filter(dependency => dependency.shot_id === id).map(dependency => <span key={dependency.input_key} className='text-muted-foreground'> · 等待「{baseline.shots.find(shot => shot.id === dependency.source_shot_id)?.name}」的{dependency.position === 'first' ? '首帧' : dependency.position === 'last' ? '尾帧' : `画面 ${dependency.frame ?? ''}`}</span>)}</li>)}</ol></div> : null}
    {entries.map(entry => <div key={entry.shot.id} className='flex flex-col gap-2 rounded-lg border p-3'>
      <label className='flex items-center gap-2 text-sm'><Checkbox aria-label={`批量生成：${entry.shot.name}`} checked={ids.includes(entry.shot.id)} disabled={pending} onCheckedChange={checked => setIds(current => checked ? [...current, entry.shot.id] : current.filter(id => id !== entry.shot.id))} />{entry.shot.name}</label>
      {entry.workflow ? <p className='text-xs text-muted-foreground'>固定版本 {entry.workflow.version}</p> : null}
      {entry.issues.map((issue, index) => <FieldError key={index}>{issue}</FieldError>)}
      {entry.workflow?.definition.inputs.filter(field => ['image', 'file'].includes(field.type) && (entry.workflow!.definition.input_schema.properties as Record<string, { type?: string }> | undefined)?.[field.key]?.type !== 'array').map(field => {
        const dependency = dependencies.find(dependency => dependency.shot_id === entry.shot.id && dependency.input_key === field.key)
        const prefix = `video-dependency-${entry.shot.id}-${field.key}`
        return <div key={field.key} className='flex flex-col gap-2 rounded-lg bg-muted p-3'><Field>
          <FieldLabel htmlFor={`${prefix}-source`}>{field.description || field.key}来源</FieldLabel>
          <Select value={dependency?.source_shot_id ?? 'current'} disabled={pending} onValueChange={id => changeDependency(entry.shot.id, field.key, id === 'current' ? undefined : { shot_id: entry.shot.id, source_shot_id: id, input_key: field.key, position: dependency?.position ?? 'last', fps: baseline.fps, ...(dependency?.position === 'fixed' ? { frame: dependency.frame } : {}) })}>
            <SelectTrigger id={`${prefix}-source`} aria-label={`${entry.shot.name} ${field.key} 来源`}><SelectValue /></SelectTrigger><SelectContent><SelectGroup><SelectItem value='current'>使用当前输入</SelectItem>{entries.filter(source => source.shot.id !== entry.shot.id && source.workflow?.definition.outputs.some(output => output.type === 'video')).map(source => <SelectItem key={source.shot.id} value={source.shot.id}>本次生成 · {source.shot.name}</SelectItem>)}</SelectGroup></SelectContent>
          </Select>
        </Field>{dependency ? <>
          <Field><FieldLabel htmlFor={`${prefix}-position`}>引用画面</FieldLabel><Select value={dependency.position} disabled={pending} onValueChange={position => changeDependency(entry.shot.id, field.key, { ...dependency, position: position as VideoGenerationDependency['position'], frame: position === 'fixed' ? 0 : undefined })}><SelectTrigger id={`${prefix}-position`} aria-label={`${entry.shot.name} ${field.key} 引用画面`}><SelectValue /></SelectTrigger><SelectContent><SelectGroup><SelectItem value='first'>视频首帧</SelectItem><SelectItem value='last'>视频尾帧</SelectItem><SelectItem value='fixed'>指定画面</SelectItem></SelectGroup></SelectContent></Select></Field>
          {dependency.position === 'fixed' ? <Field><FieldLabel htmlFor={`${prefix}-frame`}>画面帧位置（{dependency.fps} FPS）</FieldLabel><Input id={`${prefix}-frame`} aria-label={`${entry.shot.name} ${field.key} 画面帧位置`} type='number' min={0} step={1} disabled={pending} value={dependency.frame ?? ''} onChange={event => changeDependency(entry.shot.id, field.key, { ...dependency, frame: Number.isFinite(event.target.valueAsNumber) ? event.target.valueAsNumber : undefined })} /></Field> : null}
        </> : null}</div>
      })}
      {entry.workflow ? <details><summary className='cursor-pointer text-sm'>本次生成输入 · {entry.shot.name}</summary><dl className='mt-2 flex flex-col gap-2 text-sm'>{Object.entries(entry.inputs).map(([key, value]) => <div key={key}><dt className='text-muted-foreground'>{entry.workflow!.definition.inputs.find(field => field.key === key)?.description || key}</dt><dd className='whitespace-pre-wrap break-words'><GenerationInputValue value={value} /></dd></div>)}</dl></details> : null}
    </div>)}
    <DialogFooter><Button variant='outline' disabled={pending} onClick={onClose}>关闭</Button><Button disabled={disabled || pending || !selected.length || plan.issues.length > 0 || selected.some(entry => entry.issues.length)} onClick={() => onConfirm(baseline, ids, activeDependencies)}>{pending ? '正在提交批次' : `生成选定的 ${selected.length} 个分镜`}</Button></DialogFooter>
  </DialogContent></Dialog>
}
