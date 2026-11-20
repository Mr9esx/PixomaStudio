import { useState } from 'react'
import type { WorkflowVersion } from '@/lib/api/workflows'
import { Button } from '@/components/ui/button'
import { Dialog, DialogContent, DialogFooter, DialogHeader, DialogTitle } from '@/components/ui/dialog'
import { Field, FieldError, FieldGroup, FieldLabel } from '@/components/ui/field'
import { Select, SelectContent, SelectGroup, SelectItem, SelectTrigger, SelectValue } from '@/components/ui/select'
import { Textarea } from '@/components/ui/textarea'
import type { VideoShot } from './video-editor-model'
import { videoPromptFields, type VideoPromptDraft } from './video-prompt-draft'
import { videoDescriptionSource } from './video-input-sources'

export function VideoPromptDialog({ draft, shot, workflow, loading, loadError, disabled, onChange, onApply, onClose, onDiscard, onRetry }: {
  draft: VideoPromptDraft
  shot?: VideoShot
  workflow?: WorkflowVersion
  loading: boolean
  loadError?: string
  disabled: boolean
  onChange: (draft: VideoPromptDraft) => void
  onApply: () => void
  onClose: () => void
  onDiscard: () => void
  onRetry: () => void
}) {
  const [error, setError] = useState('')
  const fields = workflow ? videoPromptFields(workflow.definition) : []
  const properties = workflow?.definition.input_schema.properties as Record<string, { title?: string }> | undefined
  const versionChanged = shot?.workflowId !== draft.workflowId || shot?.versionId !== draft.versionId
  const currentSource = shot ? videoDescriptionSource(shot) : undefined
  const content = currentSource?.text ?? ''
  const conflict = Boolean(shot && (currentSource?.field !== (draft.sourceField ?? 'script') || content !== draft.sourceScript || (draft.sourceRevision !== undefined && currentSource?.revision !== draft.sourceRevision) || JSON.stringify(shot.inputs[draft.inputKey]) !== JSON.stringify(draft.baseline)))
  const issue = !shot ? '分镜已被删除' : versionChanged ? '工作流版本已改变，切换回原版本后继续' : loadError || error || (conflict ? '分镜内容或生成参数已更新，重新读取内容或确认保留输入' : '')
  const unavailable = disabled || !shot || versionChanged || !workflow || loading || Boolean(loadError)
  const value = shot?.inputs[draft.inputKey]
  const prefix = `video-prompt-${draft.shotId}`
  function change(next: VideoPromptDraft) { setError(''); onChange(next) }
  return <Dialog open onOpenChange={open => { if (!open) onClose() }}><DialogContent aria-describedby={undefined} className='max-h-[85vh] overflow-y-auto shadow-lg'>
    <DialogHeader><DialogTitle>待采用提示词{shot ? ` · ${shot.name}` : ''}</DialogTitle></DialogHeader>
    <FieldGroup>
      <Field><FieldLabel htmlFor={`${prefix}-target`}>生成参数</FieldLabel><Select value={draft.inputKey} disabled={unavailable} onValueChange={inputKey => change({ ...draft, inputKey, baseline: structuredClone(shot?.inputs[inputKey]) })}>
        <SelectTrigger id={`${prefix}-target`}><SelectValue placeholder={loading ? '正在读取工作流' : '选择文字参数'} /></SelectTrigger>
        <SelectContent><SelectGroup>{fields.map(field => <SelectItem key={field.key} value={field.key}>{properties?.[field.key]?.title || field.description || field.key}</SelectItem>)}</SelectGroup></SelectContent>
      </Select></Field>
      <Field><FieldLabel htmlFor={`${prefix}-current`}>当前内容</FieldLabel><Textarea id={`${prefix}-current`} readOnly value={typeof value === 'string' ? value : JSON.stringify(value) ?? ''} placeholder='尚未填写' className='max-h-40' /></Field>
      <Field data-invalid={Boolean(issue)}><FieldLabel htmlFor={`${prefix}-draft`}>待采用提示词</FieldLabel><Textarea id={`${prefix}-draft`} value={draft.text} disabled={unavailable} onChange={event => change({ ...draft, text: event.target.value })} className='min-h-40' aria-invalid={Boolean(issue)} aria-describedby={issue ? `${prefix}-error` : undefined} />
        {issue ? <FieldError id={`${prefix}-error`}>{issue}</FieldError> : null}
      </Field>
      <div className='flex flex-wrap gap-2'>
        <Button size='sm' variant='outline' disabled={unavailable || !content.trim()} onClick={() => { if (shot && currentSource) change({ ...draft, text: content, sourceField: currentSource.field, sourceRevision: currentSource.revision, sourceScript: content, baseline: structuredClone(shot.inputs[draft.inputKey]) }) }}>重新读取分镜内容</Button>
        {conflict ? <Button size='sm' variant='outline' disabled={unavailable} onClick={() => { if (shot && currentSource) change({ ...draft, sourceField: currentSource.field, sourceRevision: currentSource.revision, sourceScript: content, baseline: structuredClone(shot.inputs[draft.inputKey]) }) }}>保留输入</Button> : null}
        {loadError ? <Button size='sm' variant='outline' disabled={loading} onClick={onRetry}>重新读取工作流</Button> : null}
      </div>
    </FieldGroup>
    <DialogFooter><Button variant='ghost' onClick={onDiscard}>放弃待采用内容</Button><Button variant='outline' onClick={onClose}>关闭</Button><Button disabled={unavailable || conflict || !draft.inputKey || !draft.text.trim()} onClick={() => { try { onApply() } catch (cause) { setError(cause instanceof Error ? cause.message : String(cause)) } }}>采用提示词</Button></DialogFooter>
  </DialogContent></Dialog>
}
