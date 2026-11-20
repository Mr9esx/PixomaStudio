import { useState } from 'react'
import { useQuery } from '@tanstack/react-query'
import { getWorkflowVersion } from '@/lib/api/workflows'
import type { StudioAsset } from '@/lib/api/studio'
import { Button } from '@/components/ui/button'
import { Checkbox } from '@/components/ui/checkbox'
import { Dialog, DialogContent, DialogFooter, DialogHeader, DialogTitle } from '@/components/ui/dialog'
import { Field, FieldError, FieldGroup, FieldLabel } from '@/components/ui/field'
import { Select, SelectContent, SelectGroup, SelectItem, SelectTrigger, SelectValue } from '@/components/ui/select'
import { Accordion, AccordionContent, AccordionItem, AccordionTrigger } from '@/components/ui/accordion'
import { WorkflowParameterFields } from '@/features/cases/workflow-parameter-fields'
import type { VideoProject } from './video-editor-model'
import { editVideoShotInput } from './video-input-sources'
import { videoGenerationInputIssues } from './video-generation-inputs'
import { prepareVideoWorkflowBatch, type VideoWorkflowBatchItem } from './video-workflow-batch'

export function VideoWorkflowBatchDialog({ project, initialIds, workflows, assets, disabled, onApply, onClose }: {
  project: VideoProject; initialIds: string[]; workflows: Array<{ id: string; name: string; default_version_id: string | null }>; assets: StudioAsset[]; disabled: boolean
  onApply: (batch: VideoWorkflowBatchItem[]) => void; onClose: () => void
}) {
  const [ids, setIds] = useState(initialIds)
  const [workflowId, setWorkflowId] = useState('')
  const [versionId, setVersionId] = useState('')
  const [batch, setBatch] = useState<VideoWorkflowBatchItem[]>()
  const [error, setError] = useState('')
  const option = workflows.find(item => item.id === workflowId)
  const workflow = useQuery({ queryKey: ['studio', 'video-workflow-version', workflowId, versionId], enabled: Boolean(workflowId && versionId), queryFn: () => getWorkflowVersion(workflowId, versionId) })
  return <Dialog open onOpenChange={open => { if (!open) onClose() }}><DialogContent aria-describedby={undefined} className='max-h-[85vh] overflow-y-auto shadow-lg'>
    <DialogHeader><DialogTitle>批量设置工作流</DialogTitle></DialogHeader>
    {error || workflow.error ? <FieldError>{error || workflow.error?.message}</FieldError> : null}
    {!batch ? <FieldGroup>
      <Field><FieldLabel>分镜范围</FieldLabel><div className='flex flex-wrap gap-2'><Button size='sm' variant='outline' onClick={() => setIds(project.shots.map(shot => shot.id))}>选择全部</Button><Button size='sm' variant='ghost' onClick={() => setIds([])}>清除选择</Button></div>
        {project.shots.map(shot => <label key={shot.id} className='flex items-center gap-2 text-sm'><Checkbox checked={ids.includes(shot.id)} aria-label={`批量设置：${shot.name}`} onCheckedChange={checked => setIds(current => checked ? [...current, shot.id] : current.filter(id => id !== shot.id))} />{shot.name}</label>)}
      </Field>
      <Field><FieldLabel>生成工作流</FieldLabel><Select value={workflowId} onValueChange={id => { setWorkflowId(id); setVersionId(workflows.find(item => item.id === id)?.default_version_id ?? '') }}><SelectTrigger><SelectValue placeholder='选择已发布的视频工作流' /></SelectTrigger><SelectContent><SelectGroup>{workflows.map(item => <SelectItem key={item.id} value={item.id}>{item.name}</SelectItem>)}</SelectGroup></SelectContent></Select></Field>
      <Button disabled={disabled || !ids.length || !workflow.data || workflow.isFetching} onClick={() => { try { setBatch(prepareVideoWorkflowBatch(project, ids, workflow.data!)); setError('') } catch (cause) { setError(String(cause)) } }}>预览 {ids.length} 个分镜设置</Button>
    </FieldGroup> : <>
      <p className='text-sm'>{option?.name} · {workflow.data?.version} · {batch.length} 个分镜</p>
      <Accordion type='multiple' defaultValue={batch.map(item => item.shot.id)}>{batch.map(item => {
        const issues = workflow.data ? videoGenerationInputIssues(workflow.data.definition.inputs, item.shot.inputs, assets, workflow.data.definition.input_schema) : []
        const errors = Object.fromEntries(issues.filter(issue => issue.key).map(issue => [issue.key!, issue.message]))
        return <AccordionItem key={item.shot.id} value={item.shot.id}><AccordionTrigger>{item.shot.name}{issues.length ? ` · ${issues.length} 项参数待处理` : ''}</AccordionTrigger><AccordionContent>
          {issues.filter(issue => !issue.key).map((issue, index) => <FieldError key={index}>{issue.message}</FieldError>)}
          {workflow.data ? <WorkflowParameterFields prefix={`video-batch-${item.shot.id}`} definition={workflow.data.definition} values={item.shot.inputs} assets={assets} errors={errors} disabled={disabled} onChange={(key, value) => setBatch(current => current!.map(entry => entry.shot.id === item.shot.id ? { ...entry, shot: editVideoShotInput(entry.shot, key, value) } : entry))} /> : null}
        </AccordionContent></AccordionItem>
      })}</Accordion>
    </>}
    <DialogFooter><Button variant='outline' onClick={onClose}>关闭</Button>{batch ? <><Button variant='ghost' disabled={disabled} onClick={() => { setBatch(undefined); setError('') }}>重新选择范围</Button><Button disabled={disabled} onClick={() => { try { onApply(batch) } catch (cause) { setError(String(cause)) } }}>采用工作流设置</Button></> : null}</DialogFooter>
  </DialogContent></Dialog>
}
