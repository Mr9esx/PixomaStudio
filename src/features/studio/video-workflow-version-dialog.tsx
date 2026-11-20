import { useState } from 'react'
import { useQuery } from '@tanstack/react-query'
import { getWorkflowVersion, type WorkflowVersion } from '@/lib/api/workflows'
import type { StudioAsset } from '@/lib/api/studio'
import { Button } from '@/components/ui/button'
import { Dialog, DialogContent, DialogFooter, DialogHeader, DialogTitle } from '@/components/ui/dialog'
import { FieldError } from '@/components/ui/field'
import { Skeleton } from '@/components/ui/skeleton'
import { WorkflowParameterFields } from '@/features/cases/workflow-parameter-fields'
import type { VideoShot } from './video-editor-model'
import { GenerationInputValue } from './video-generation-records'
import { editVideoShotInput } from './video-input-sources'
import { videoGenerationInputIssues } from './video-generation-inputs'
import { prepareVideoWorkflowVersion, type VideoWorkflowVersionReview } from './video-workflow-version'

export function VideoWorkflowVersionDialog({ shot, from, targetId, assets, disabled, onApply, onClose }: { shot: VideoShot; from: WorkflowVersion; targetId: string; assets: StudioAsset[]; disabled: boolean; onApply: (review: VideoWorkflowVersionReview) => void; onClose: () => void }) {
  const target = useQuery({ queryKey: ['studio', 'video-workflow-version', from.workflow_id, targetId], queryFn: () => getWorkflowVersion(from.workflow_id, targetId) })
  const [review, setReview] = useState<VideoWorkflowVersionReview>()
  const [error, setError] = useState('')
  const issues = review && target.data ? videoGenerationInputIssues(target.data.definition.inputs, review.shot.inputs, assets, target.data.definition.input_schema) : []
  return <Dialog open onOpenChange={open => { if (!open) onClose() }}><DialogContent aria-describedby={undefined} className='max-h-[85vh] overflow-y-auto shadow-lg'>
    <DialogHeader><DialogTitle>采用发布版本 · {shot.name}</DialogTitle></DialogHeader>
    {target.isPending ? <Skeleton className='h-20 w-full' /> : target.error ? <><FieldError>{target.error.message}</FieldError><Button variant='outline' onClick={() => void target.refetch()}>重新读取</Button></> : <>
      <p className='text-sm'>当前版本 {from.version} · 目标版本 {target.data.version}</p>
      {target.data.notes ? <p className='whitespace-pre-wrap text-sm text-muted-foreground'>{target.data.notes}</p> : null}
      {!review ? <Button disabled={disabled} onClick={() => { try { setReview(prepareVideoWorkflowVersion(shot, from, target.data, assets)); setError('') } catch (cause) { setError(cause instanceof Error ? cause.message : String(cause)) } }}>预览参数变化</Button> : <>
        {review.restoredDraft ? <p className='text-sm'>恢复目标版本的参数草稿</p> : null}
        {review.changes.map(change => <div key={change.key} className='flex flex-col gap-1 rounded-lg border p-3 text-sm'><strong>{target.data.definition.inputs.find(field => field.key === change.key)?.description || change.key}</strong><p className='text-muted-foreground'>{change.message}</p><div>当前：<GenerationInputValue value={shot.inputs[change.key]} /></div></div>)}
        <WorkflowParameterFields prefix={`video-version-${shot.id}`} definition={target.data.definition} values={review.shot.inputs} assets={assets} errors={Object.fromEntries(issues.filter(issue => issue.key).map(issue => [issue.key!, issue.message]))} disabled={disabled} onChange={(key, value) => setReview({ ...review, shot: editVideoShotInput(review.shot, key, value) })} />
        {issues.filter(issue => !issue.key).map((issue, index) => <FieldError key={index}>{issue.message}</FieldError>)}
      </>}
    </>}
    {error ? <FieldError>{error}</FieldError> : null}
    <DialogFooter><Button variant='outline' onClick={onClose}>关闭</Button><Button disabled={disabled || !review || issues.length > 0} onClick={() => { if (review) { try { onApply(review) } catch (cause) { setError(cause instanceof Error ? cause.message : String(cause)) } } }}>采用版本设置</Button></DialogFooter>
  </DialogContent></Dialog>
}
