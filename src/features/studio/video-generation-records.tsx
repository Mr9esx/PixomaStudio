import { useMutation, useQuery, useQueryClient } from '@tanstack/react-query'
import { useState } from 'react'
import { Accordion, AccordionContent, AccordionItem, AccordionTrigger } from '@/components/ui/accordion'
import { Alert, AlertTitle } from '@/components/ui/alert'
import { Button } from '@/components/ui/button'
import { Dialog, DialogContent, DialogHeader, DialogTitle } from '@/components/ui/dialog'
import { Empty, EmptyHeader, EmptyTitle } from '@/components/ui/empty'
import { Skeleton } from '@/components/ui/skeleton'
import { cancelWorkflowExecution, getWorkflowVersion, stopWaitingForWorkflowExecution, type WorkflowInput } from '@/lib/api/workflows'
import { getStudioAsset } from '@/lib/api/studio'
import { cancelVideoDependencyWait, chooseVideoDependencyOutput, listVideoGenerationBatches, retryVideoGeneration, videoGenerationPending, type VideoGeneration } from '@/lib/api/video-projects'
import type { VideoShot, VideoInputSources } from './video-editor-model'

export const videoGenerationLabels: Record<VideoGeneration['status'], string> = {
  submitting: '正在提交', submission_failed: '提交失败', pending: '等待执行', dispatching: '正在提交', running: '生成中', collecting: '正在保存视频',
  succeeded: '视频已生成', failed: '生成失败', cancelled: '已取消', result_unknown: '结果无法确认', submission_unknown: '提交结果未知',
  waiting_dependencies: '等待上游视频', waiting_output: '等待选择上游输出', blocked_dependencies: '上游未完成，尚未提交',
}

export function GenerationInputValue({ value, onPreview }: { value: unknown; onPreview?: (assetId: string, versionId: string) => void }) {
  const assetId = value && typeof value === 'object' && 'asset_id' in value && typeof value.asset_id === 'string' ? value.asset_id : undefined
  const versionId = value && typeof value === 'object' && 'asset_version_id' in value && typeof value.asset_version_id === 'string' ? value.asset_version_id : undefined
  const asset = useQuery({ queryKey: ['studio', 'asset', assetId], enabled: Boolean(assetId && versionId), queryFn: () => getStudioAsset(assetId!) })
  if (Array.isArray(value)) return <div className='flex flex-col gap-1'>{value.map((item, index) => <GenerationInputValue key={index} value={item} onPreview={onPreview} />)}</div>
  if (assetId && versionId) {
    if (asset.isPending) return <Skeleton className='h-8 w-32' />
    if (asset.error) return <span role='alert'>{asset.error.message}</span>
    const version = asset.data?.versions.find(version => version.id === versionId)
    return version ? onPreview ? <Button size='sm' variant='outline' onClick={() => onPreview(assetId, versionId)}>{asset.data?.name} · 版本 {version.version}</Button> : <span>{asset.data?.name} · 版本 {version.version}</span> : <span role='alert'>素材版本不可用</span>
  }
  return <>{value === undefined || value === null ? '未设置' : typeof value === 'string' ? value : typeof value === 'boolean' ? value ? '开启' : '关闭' : JSON.stringify(value)}</>
}

function GenerationInputs({ record, shot, onPreview }: { record: VideoGeneration; shot?: VideoShot; onPreview: (assetId: string, versionId: string) => void }) {
  const [compare, setCompare] = useState(false)
  const workflow = useQuery({ queryKey: ['studio', 'video-workflow-version', shot?.workflowId, shot?.versionId], enabled: Boolean(compare && shot?.workflowId && shot.versionId), queryFn: () => getWorkflowVersion(shot!.workflowId!, shot!.versionId!) })
  const renderInputs = (inputs: Record<string, unknown>, fields: WorkflowInput[], sources?: VideoInputSources) => <dl className='flex flex-col gap-2'>{Object.entries(inputs).map(([key, value]) => <div key={key} className='flex flex-col gap-1'><dt className='text-muted-foreground'>{fields.find(field => field.key === key)?.description || key}</dt><dd className='whitespace-pre-wrap break-words'><GenerationInputValue value={value} onPreview={onPreview} />{sources?.[key] ? <details className='mt-2 text-xs text-muted-foreground'><summary>描述来源{sources[key].revision ? ` · 修订 ${sources[key].revision}` : ''}{sources[key].edited || value !== sources[key].value ? ' · 已手工修改' : ''}</summary><p className='mt-1 whitespace-pre-wrap'>{sources[key].sourceText}</p></details> : null}</dd></div>)}</dl>
  return <div className='flex flex-col gap-3'>
    {shot ? <Button size='sm' variant='ghost' className='self-start' onClick={() => setCompare(value => !value)}>{compare ? '收起当前设置' : '对照当前设置'}</Button> : null}
    <div className='flex flex-col gap-4 sm:flex-row'>
      <div className='flex min-w-0 flex-1 flex-col gap-2'><p>{record.dependency_state && record.dependency_state !== 'ready' ? '待提交输入' : '本次输入'}</p>{renderInputs(record.snapshot.inputs, record.snapshot.input_fields, record.snapshot.shot.workflowId === record.snapshot.workflow_id && record.snapshot.shot.versionId === record.snapshot.version_id ? record.snapshot.shot.inputSources : undefined)}</div>
      {compare && shot ? <div className='flex min-w-0 flex-1 flex-col gap-2'><p>当前编辑内容</p>
        {workflow.isFetching ? <Skeleton className='h-8 w-32' /> : workflow.error ? <Alert><AlertTitle>{workflow.error.message}</AlertTitle></Alert> : <>
          {shot.workflowId !== record.snapshot.workflow_id || shot.versionId !== record.snapshot.version_id ? <p>工作流版本已修改</p> : null}
          {renderInputs(shot.inputs, workflow.data?.definition.inputs ?? [], shot.inputSources)}
        </>}
      </div> : null}
    </div>
  </div>
}

export function VideoGenerationRecords({ records, shots, sessionId, videoId, loading, error, onRefresh, onClose, onSelectShot, onPreview }: {
  records: VideoGeneration[]; shots: VideoShot[]; sessionId: string; videoId?: string; loading: boolean; error?: string
  onRefresh: () => void; onClose: () => void; onSelectShot: (id: string) => void
  onPreview: (assetId: string, versionId: string) => void
}) {
  const client = useQueryClient()
  const [batchId, setBatchId] = useState<string>()
  const batches = useQuery({ queryKey: ['studio', 'video-generation-batches', videoId], enabled: Boolean(videoId), queryFn: () => listVideoGenerationBatches(videoId!), refetchInterval: query => query.state.data?.some(batch => batch.generations.some(videoGenerationPending)) ? 2000 : false })
  const batch = batches.data?.find(batch => batch.id === batchId)
  const displayed = batch ? batch.generations : records
  const [retryRequests, setRetryRequests] = useState<Record<string, string>>({})
  const outputChoice = useMutation({ mutationFn: ({ record, inputKey, source, field, index }: { record: VideoGeneration; inputKey: string; source: string; field: string; index: number }) => chooseVideoDependencyOutput(record.video_id, record.id, { session_id: sessionId, input_key: inputKey, source_generation_id: source, output: { field, index } }), onSuccess: () => Promise.all([client.invalidateQueries({ queryKey: ['studio', 'video-generations'] }), client.invalidateQueries({ queryKey: ['studio', 'video-generation-batches'] })]) })
  const action = useMutation({ mutationFn: async ({ record, kind }: { record: VideoGeneration; kind: 'retry' | 'cancel' | 'stop' | 'cancel-waiting' }) => {
    if (kind === 'cancel-waiting') return cancelVideoDependencyWait(record.video_id, record.id, sessionId)
    if (kind === 'retry') {
      const request = retryRequests[record.id] ?? crypto.randomUUID()
      setRetryRequests(previous => ({ ...previous, [record.id]: request }))
      const result = await retryVideoGeneration(record.video_id, record.id, { request_id: request, session_id: sessionId })
      setRetryRequests(previous => { const next = { ...previous }; delete next[record.id]; return next })
      return result
    }
    if (!record.execution_id) throw new Error('任务尚未提交，读取任务状态后重试')
    return kind === 'cancel' ? cancelWorkflowExecution(record.execution_id) : stopWaitingForWorkflowExecution(record.execution_id)
  }, onSettled: async () => {
    await Promise.all([client.invalidateQueries({ queryKey: ['studio', 'video-generations'] }), client.invalidateQueries({ queryKey: ['studio', 'video-executions'] }), client.invalidateQueries({ queryKey: ['studio', 'video-generation-batches'] })])
  } })
  return <Dialog open onOpenChange={open => { if (!open) onClose() }}><DialogContent aria-describedby={undefined} className='shadow-lg sm:max-w-2xl'>
    <DialogHeader><DialogTitle>生成记录</DialogTitle></DialogHeader>
    {batches.error ? <Alert><AlertTitle>{batches.error.message}</AlertTitle><Button size='sm' variant='outline' onClick={() => void batches.refetch()}>重新读取批次</Button></Alert> : null}
    {batches.data?.length ? <div className='flex max-h-40 flex-col gap-2 overflow-y-auto' aria-label='生成批次'><Button size='sm' variant={batchId ? 'ghost' : 'secondary'} aria-pressed={!batchId} onClick={() => setBatchId(undefined)}>全部生成记录</Button>{batches.data.map(batch => {
      const ready = batch.generations.filter(generation => generation.status === 'succeeded').length
      const failed = batch.generations.filter(generation => ['failed', 'submission_failed', 'blocked_dependencies'].includes(generation.status)).length
      const pending = batch.generations.filter(videoGenerationPending).length
      const cancelled = batch.generations.filter(generation => generation.status === 'cancelled').length
      const uncertain = batch.generations.length - ready - failed - pending - cancelled
      return <Button key={batch.id} size='sm' variant={batchId === batch.id ? 'secondary' : 'outline'} aria-pressed={batchId === batch.id} className='h-auto flex-col items-start whitespace-normal text-left' onClick={() => setBatchId(batch.id)}><span>批次 · {batch.generations.length} 个分镜 · {new Date(batch.created_at).toLocaleString()}</span><span className='text-xs text-muted-foreground'>{ready} 个已生成 · {failed} 个失败 · {pending} 个处理中{cancelled ? ` · ${cancelled} 个已取消` : ''}{uncertain ? ` · ${uncertain} 个需要确认` : ''}</span></Button>
    })}</div> : null}
    {error || action.error ? <Alert><AlertTitle>{error || action.error?.message}</AlertTitle><Button size='sm' variant='outline' onClick={() => { action.reset(); onRefresh() }}>重新读取</Button></Alert> : null}
    {loading ? <Skeleton className='h-24 w-full' /> : !records.length ? <Empty><EmptyHeader><EmptyTitle>暂无生成记录</EmptyTitle></EmptyHeader></Empty> :
      <Accordion type='single' collapsible className='max-h-[65vh] overflow-y-auto'>{displayed.map(record => {
        const shot = shots.find(shot => shot.id === record.shot_id)
        const execution = record.execution
        const pending = videoGenerationPending(record)
        const blocked = records.some(other => other.shot_id === record.shot_id && videoGenerationPending(other))
        return <AccordionItem key={record.id} value={record.id}>
          <AccordionTrigger><span className='flex min-w-0 flex-col gap-1 text-left'><span>{record.snapshot.shot.name}{shot ? '' : ' · 分镜已删除'}</span><span className='text-xs text-muted-foreground tabular-nums'>{new Date(record.created_at).toLocaleString()} · {videoGenerationLabels[record.status]}</span></span></AccordionTrigger>
          <AccordionContent><div className='flex flex-col gap-3'>
            <p>{record.snapshot.workflow_name} · {record.snapshot.version_name}</p>
            {record.snapshot.shot.storyboard ? <p className='whitespace-pre-wrap'>{record.snapshot.shot.storyboard}</p> : null}
            <GenerationInputs record={record} shot={shot} onPreview={(assetId, versionId) => { onPreview(assetId, versionId); onClose() }} />
            {record.dependency_error ? <Alert><AlertTitle>{record.dependency_error}</AlertTitle></Alert> : null}
            {record.dependencies?.map(dependency => <p key={dependency.binding.input_key} className='text-sm text-muted-foreground'>{records.find(source => source.id === dependency.source_generation_id)?.snapshot.shot.name ?? dependency.binding.source_shot_id} · {dependency.binding.position === 'first' ? '首帧' : dependency.binding.position === 'last' ? '尾帧' : `指定画面 ${dependency.binding.frame}`} → {record.snapshot.input_fields.find(field => field.key === dependency.binding.input_key)?.description || dependency.binding.input_key}</p>)}
            {outputChoice.error ? <Alert><AlertTitle>{outputChoice.error.message}</AlertTitle></Alert> : null}
            {record.status === 'waiting_output' ? record.dependencies?.map(dependency => {
              const source = records.find(source => source.id === dependency.source_generation_id)
              return <div key={dependency.binding.input_key} className='flex flex-wrap gap-2'><span className='w-full text-sm'>{dependency.binding.input_key} · {source?.snapshot.shot.name}</span>{source?.execution?.outputs.filter(output => output.media_type.startsWith('video/')).map(output => <Button key={output.output_id} size='sm' variant='outline' disabled={outputChoice.isPending} onClick={() => outputChoice.mutate({ record, inputKey: dependency.binding.input_key, source: dependency.source_generation_id, field: output.field_key, index: output.index })}>采用输出 {output.field_key} · {output.index + 1}</Button>)}</div>
            }) : null}
            {record.submission_error || execution?.error ? <Alert><AlertTitle>{record.submission_error || execution?.error}</AlertTitle></Alert> : null}
            {record.status === 'result_unknown' || record.status === 'submission_unknown' ? <p>原任务结果仍未确认，再次生成会创建新的任务。</p> : null}
            <div className='flex flex-wrap gap-2'>
              {shot ? <Button size='sm' variant='outline' onClick={() => { onSelectShot(shot.id); onClose() }}>查看分镜</Button> : null}
              {execution?.outputs.filter(output => output.media_type.startsWith('video/')).map((output, index) => <Button key={output.output_id} size='sm' variant='outline' onClick={() => { onPreview(output.asset_id, output.asset_version_id); onClose() }}>预览结果 {index + 1}</Button>)}
              {!pending && (!record.dependency_state || record.dependency_state === 'ready') ? <Button size='sm' variant='outline' disabled={blocked || action.isPending} onClick={() => action.mutate({ record, kind: 'retry' })}>使用原输入再次生成</Button> : null}
              {record.status === 'waiting_dependencies' || record.status === 'waiting_output' ? <Button size='sm' variant='outline' disabled={action.isPending} onClick={() => action.mutate({ record, kind: 'cancel-waiting' })}>取消依赖等待</Button> : null}
              {pending && execution && execution.can_cancel !== false ? <Button size='sm' variant='outline' disabled={action.isPending || execution.cancel_requested} onClick={() => action.mutate({ record, kind: 'cancel' })}>{execution.cancel_requested ? '正在取消' : '取消生成'}</Button> : null}
              {execution?.waiting ? <Button size='sm' variant='ghost' disabled={action.isPending} onClick={() => action.mutate({ record, kind: 'stop' })}>停止等待</Button> : null}
            </div>
          </div></AccordionContent>
        </AccordionItem>
      })}</Accordion>}
  </DialogContent></Dialog>
}
