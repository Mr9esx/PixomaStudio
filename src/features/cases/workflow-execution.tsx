import { useState } from 'react'
import { useMutation, useQuery, useQueryClient } from '@tanstack/react-query'
import { ArrowLeft, Play, Square, TriangleAlert } from 'lucide-react'
import { createWorkflowExecution, cancelWorkflowExecution, getWorkflowExecution, stopWaitingForWorkflowExecution, type Workflow, type WorkflowDefinition, type WorkflowExecution, type WorkflowExecutionOutput } from '@/lib/api/workflows'
import { getStudioAsset, uploadStudioAsset } from '@/lib/api/studio'
import { Alert, AlertTitle } from '@/components/ui/alert'
import { Button } from '@/components/ui/button'
import { Spinner } from '@/components/ui/spinner'
import { StatusDot } from '@/components/status-dot'
import { formatDateTime } from '@/lib/format'
import { CodeEditor } from '@/components/code-editor'
import { useMediaObjectUrl } from './lib/use-media-object-url'
import { WorkflowParameterFields, workflowDefaultValues } from './workflow-parameter-fields'
import type { StudioAsset } from '@/lib/api/studio'

export const executionStatusLabels: Record<WorkflowExecution['status'], string> = {
  result_unknown: '结果无法确认',
  pending: '等待节点', dispatching: '正在提交', running: '正在执行', collecting: '正在保存结果',
  succeeded: '执行成功', failed: '执行失败', cancelled: '已取消', submission_unknown: '正在确认提交结果',
}
export const executionFinished = (status: WorkflowExecution['status']) => ['succeeded', 'failed', 'cancelled', 'result_unknown'].includes(status)

function ExecutionOutput({ output }: { output: WorkflowExecutionOutput }) {
  const asset = useQuery({ queryKey: ['studio', 'asset', output.asset_id], queryFn: () => getStudioAsset(output.asset_id) })
  const version = asset.data?.versions.find((item) => item.id === output.asset_version_id)
  const media = useMediaObjectUrl(version?.content_url)
  const text = useQuery({ queryKey: ['studio', 'output-text', output.asset_version_id], queryFn: async () => {
    const response = await fetch(media!)
    if (!response.ok) throw new Error('读取文本结果失败')
    return response.text()
  }, enabled: Boolean(media) && output.media_type.startsWith('text/') })
  if (asset.error) return <p role='alert' className='text-sm text-destructive'>{asset.error.message}</p>
  if (asset.data && !version) throw new Error('执行结果的资产版本缺失')
  if (asset.isPending || !media) return <Spinner />
  return <div className='flex flex-col gap-3 rounded-xl bg-muted p-4'>
    <span className='text-sm'>{output.field_key}</span>
    {output.media_type.startsWith('image/') ? <img src={media} alt={output.field_key} className='max-h-96 w-full object-contain' />
      : output.media_type.startsWith('video/') ? <video src={media} controls playsInline aria-label={output.field_key} className='max-h-96 w-full' />
        : output.media_type.startsWith('audio/') ? <audio src={media} controls aria-label={output.field_key} /> : output.media_type.startsWith('text/') ? text.error ? <p role='alert' className='text-sm text-destructive'>{text.error.message}</p> : <pre className='max-h-96 overflow-auto whitespace-pre-wrap'>{text.data ?? ''}</pre> : null}
    <Button asChild size='sm' variant='outline' className='self-start'><a href={media} download={asset.data?.name ?? output.field_key}>下载结果</a></Button>
  </div>
}

export function WorkflowExecutionResult({ executionId }: { executionId: string }) {
  const client = useQueryClient()
  const execution = useQuery({ queryKey: ['studio', 'execution', executionId], queryFn: () => getWorkflowExecution(executionId),
    refetchInterval: (query) => query.state.data && executionFinished(query.state.data.status) ? false : 1500,
  })
  const cancel = useMutation({ mutationFn: () => cancelWorkflowExecution(executionId),
    onSuccess: (next) => client.setQueryData(['studio', 'execution', executionId], next),
  })
  const stopWaiting = useMutation({ mutationFn: () => stopWaitingForWorkflowExecution(executionId), onSuccess: next => client.setQueryData(['studio', 'execution', executionId], next) })
  if (execution.isPending) return <Spinner />
  if (execution.error) return <Alert variant='destructive'><TriangleAlert /><AlertTitle>{execution.error.message}</AlertTitle></Alert>
  const record = execution.data
  const finished = executionFinished(record.status)
  return <section className='flex min-w-0 flex-col gap-3' aria-label='执行结果'>
    <div className='flex flex-wrap items-center gap-2'>
      <StatusDot label={executionStatusLabels[record.status]} problems={record.status === 'succeeded' ? 0 : 1} state={finished ? undefined : 'active'} />
      <span className='text-sm'>{executionStatusLabels[record.status]}</span>
      {record.cancel_requested && !finished ? <span className='text-sm text-muted-foreground'>正在取消</span> : null}
      {!finished && record.can_cancel !== false ? <Button type='button' size='sm' variant='outline' disabled={cancel.isPending || record.cancel_requested} onClick={() => cancel.mutate()}><Square />取消执行</Button> : null}
      {!finished && record.waiting ? <Button type='button' size='sm' variant='ghost' disabled={stopWaiting.isPending} onClick={() => stopWaiting.mutate()}>停止等待</Button> : null}
    </div>
    <dl className='grid grid-cols-[auto_minmax(0,1fr)] gap-x-4 gap-y-2 text-sm'><dt className='text-muted-foreground'>执行编号</dt><dd className='break-all font-mono text-xs'>{record.id}</dd><dt className='text-muted-foreground'>创建时间</dt><dd className='tabular-nums'>{formatDateTime(record.created_at)}</dd>{record.prompt_id ? <><dt className='text-muted-foreground'>任务编号</dt><dd className='break-all font-mono text-xs'>{record.prompt_id}</dd></> : null}</dl>
    {record.error || cancel.error || stopWaiting.error ? <Alert variant='destructive'><TriangleAlert /><AlertTitle>{record.error || cancel.error?.message || stopWaiting.error?.message}</AlertTitle></Alert> : null}
    {record.events?.length ? <details><summary className='cursor-pointer text-sm'>执行事件</summary><ol className='mt-3 flex flex-col gap-2'>{record.events.map(event => <li key={event.sequence} className='flex flex-wrap gap-3 text-xs'><span className='tabular-nums text-muted-foreground'>{formatDateTime(event.created_at)}</span><span>{({ accepted: '节点已接收', submission_started: '开始提交', submitted: '已提交 ComfyUI', progress: '执行进度', collecting: '正在收集结果', failed: '执行失败', cancelled: '已取消', submission_unknown: '提交结果待确认' } as Record<string, string>)[event.kind] ?? event.kind}</span>{event.kind === 'progress' ? <span>{JSON.stringify(event.payload)}</span> : null}</li>)}</ol></details> : null}
    {record.inputs ? <details><summary className='cursor-pointer text-sm'>输入参数</summary><CodeEditor value={JSON.stringify(record.inputs, null, 2)} readOnly minHeight={120} maxHeight={300} /></details> : null}
    {record.usage ? <details><summary className='cursor-pointer text-sm'>生成用量</summary><CodeEditor value={JSON.stringify(record.usage, null, 2)} readOnly minHeight={100} maxHeight={240} /></details> : null}
    {record.outputs.map((output) => <ExecutionOutput key={output.output_id} output={output} />)}
  </section>
}

export function WorkflowExecutionPanel({ workflow, definition = workflow.draft, versionId, onClose }: {
  workflow: Workflow; definition?: WorkflowDefinition; versionId?: string; onClose: () => void
}) {
  const client = useQueryClient()
  const [inputs, setInputs] = useState<Record<string, unknown>>(() => {
    const defaults = { ...workflowDefaultValues(definition), ...(definition.type === 'online' ? definition.execution.defaults : {}) }
    return Object.fromEntries(definition.inputs.flatMap(field => defaults[field.key] !== undefined ? [[field.key, defaults[field.key]]] : field.type === 'boolean' ? [[field.key, false]] : []))
  })
  const [executionId, setExecutionId] = useState<string>()
  const [requestId, setRequestId] = useState(() => crypto.randomUUID())
  const [uploadedAssets, setUploadedAssets] = useState<StudioAsset[]>([])
  const upload = useMutation({ mutationFn: async ({ key, file }: { key: string; file: File }) => {
    const asset = await uploadStudioAsset(file)
    const version = asset.versions.find((item) => item.version === asset.current_version)
    if (!version) throw new Error('上传文件的资产版本缺失')
    return { key, asset, value: { asset_id: asset.id, asset_version_id: version.id } }
  }, onSuccess: ({ key, value, asset }) => {
    setUploadedAssets(current => [...current, asset])
    const multiple = (definition.input_schema.properties as Record<string, { type?: string }>)[key]?.type === 'array'
    setInputs(current => ({ ...current, [key]: multiple ? [...(Array.isArray(current[key]) ? current[key] : []), value] : value }))
    setRequestId(crypto.randomUUID())
  } })
  const current = useQuery({ queryKey: ['studio', 'execution', executionId], queryFn: () => getWorkflowExecution(executionId!), enabled: Boolean(executionId) })
  const submit = useMutation({ mutationFn: () => createWorkflowExecution({ request_id: requestId, workflow_id: workflow.id, inputs,
    ...(versionId ? { version_id: versionId } : { draft_revision: workflow.revision }),
  }), onSuccess: (execution) => {
    setExecutionId(execution.id)
    client.setQueryData(['studio', 'execution', execution.id], execution)
    void client.invalidateQueries({ queryKey: ['studio', 'executions', workflow.id] })
    setRequestId(crypto.randomUUID())
  } })
  function update(key: string, value: unknown) {
    setInputs((current) => { const next = { ...current }; if (value === undefined) delete next[key]; else next[key] = value; return next })
    setRequestId(crypto.randomUUID())
  }
  const busy = upload.isPending || submit.isPending || Boolean(executionId && (!current.data || !executionFinished(current.data.status)))
  return <section aria-label='运行工作流' className='flex min-w-0 flex-col gap-6'>
    <header className='flex items-center gap-3'><Button size='icon-sm' variant='ghost' aria-label='返回工作流' onClick={onClose}><ArrowLeft /></Button><h2 className='min-w-0 truncate text-base font-semibold'>运行工作流 · {workflow.name}</h2></header>
    <div className='grid min-w-0 gap-8 min-[1200px]:grid-cols-[minmax(0,1fr)_16rem]'><div className='flex min-w-0 flex-col gap-6'>
    <form className='flex flex-col gap-5' onSubmit={(event) => { event.preventDefault(); submit.mutate() }}>
      <WorkflowParameterFields prefix='workflow-test' definition={definition} values={inputs} disabled={busy} assets={uploadedAssets} onUploadAsset={(key, file) => upload.mutate({ key, file })} onChange={update} />
      {upload.error || submit.error ? <Alert variant='destructive'><TriangleAlert /><AlertTitle>{(upload.error ?? submit.error)?.message}</AlertTitle></Alert> : null}
      <div className='flex gap-2'><Button type='submit' size='sm' disabled={busy || upload.isError}>{submit.isPending ? <Spinner /> : <Play />}{executionId ? '再次运行' : '运行工作流'}</Button><Button type='button' size='sm' variant='ghost' onClick={onClose}>返回工作流</Button></div>
    </form>
    {executionId ? <WorkflowExecutionResult executionId={executionId} /> : null}
    </div><aside className='flex flex-col gap-4 self-start rounded-xl bg-muted p-5 text-sm'><h3 className='font-medium'>本次运行</h3><dl className='grid grid-cols-[auto_minmax(0,1fr)] gap-x-4 gap-y-3'><dt className='text-muted-foreground'>工作流</dt><dd className='break-words'>{workflow.name}</dd><dt className='text-muted-foreground'>类型</dt><dd>{definition.type === 'comfyui' ? '自建 ComfyUI' : '在线模型'}</dd><dt className='text-muted-foreground'>版本</dt><dd>{versionId ? '保存版本' : '当前草稿'}</dd><dt className='text-muted-foreground'>输出</dt><dd>{definition.outputs.map(item => item.description || item.key).join('、')}</dd></dl></aside></div>
  </section>
}
