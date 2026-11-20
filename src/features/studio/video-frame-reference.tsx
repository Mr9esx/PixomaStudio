import { useState } from 'react'
import { useQuery } from '@tanstack/react-query'
import { Button } from '@/components/ui/button'
import { Dialog, DialogContent, DialogFooter, DialogHeader, DialogTitle } from '@/components/ui/dialog'
import { Field, FieldError, FieldGroup, FieldLabel } from '@/components/ui/field'
import { Input } from '@/components/ui/input'
import { Select, SelectContent, SelectGroup, SelectItem, SelectTrigger, SelectValue } from '@/components/ui/select'
import { getWorkflowVersion } from '@/lib/api/workflows'
import type { VideoProject, VideoReference } from './video-editor-model'

export type VideoFrameSelection = { sourceId: string; targetId: string; sourcePosition: 'first' | 'last' | 'fixed'; sourceFrame?: number; sourceFps?: number; role: VideoReference['role']; inputKey?: string }

export function VideoFrameReference({ project, targetId, pending, error, onApply, onClose }: {
  project: VideoProject; targetId: string; pending: boolean; error?: string; onApply: (selection: VideoFrameSelection) => void; onClose: () => void
}) {
  const sources = project.shots.filter(shot => shot.id !== targetId && shot.media)
  const [sourceId, setSourceId] = useState(() => sources[0]?.id ?? '')
  const [position, setPosition] = useState<VideoFrameSelection['sourcePosition']>('last')
  const [seconds, setSeconds] = useState('0')
  const [role, setRole] = useState<VideoReference['role']>('first_frame')
  const [inputKey, setInputKey] = useState('none')
  const target = project.shots.find(shot => shot.id === targetId)
  const source = sources.find(shot => shot.id === sourceId)
  const workflow = useQuery({ queryKey: ['workflow-version', target?.workflowId, target?.versionId], queryFn: () => getWorkflowVersion(target!.workflowId!, target!.versionId!), enabled: Boolean(target?.workflowId && target?.versionId) })
  const fields = workflow.data?.definition.inputs.filter(field => field.type === 'image') ?? []
  const frame = Math.round(Number(seconds) * project.fps)
  const positionValid = position !== 'fixed' || Boolean(seconds.trim() && Number.isFinite(Number(seconds)) && frame >= 0 && source?.media?.duration && frame / project.fps < source.media.duration)
  const canApply = target && source && positionValid && (inputKey === 'none' || fields.some(field => field.key === inputKey))
  return <Dialog open onOpenChange={value => { if (!value && !pending) onClose() }}><DialogContent aria-describedby={undefined} className='max-h-[85vh] overflow-y-auto shadow-lg'>
    <DialogHeader><DialogTitle>引用分镜画面 · {target?.name ?? '分镜已删除'}</DialogTitle></DialogHeader>
    <FieldGroup>
      <Field><FieldLabel htmlFor='frame-source'>来源分镜</FieldLabel><Select value={sourceId} disabled={pending} onValueChange={setSourceId}><SelectTrigger id='frame-source'><SelectValue placeholder='选择已采用视频的分镜' /></SelectTrigger><SelectContent><SelectGroup>{sources.map(shot => <SelectItem key={shot.id} value={shot.id}>{shot.name}</SelectItem>)}</SelectGroup></SelectContent></Select></Field>
      <Field><FieldLabel htmlFor='frame-position'>来源画面</FieldLabel><Select value={position} disabled={pending} onValueChange={value => setPosition(value as VideoFrameSelection['sourcePosition'])}><SelectTrigger id='frame-position'><SelectValue /></SelectTrigger><SelectContent><SelectGroup><SelectItem value='first'>剪辑首帧</SelectItem><SelectItem value='last'>剪辑尾帧</SelectItem><SelectItem value='fixed'>指定源视频位置</SelectItem></SelectGroup></SelectContent></Select></Field>
      {position === 'fixed' ? <Field><FieldLabel htmlFor='frame-seconds'>源视频位置（秒）</FieldLabel><Input id='frame-seconds' type='number' value={seconds} min={0} max={source?.media?.duration} step={1 / project.fps} disabled={pending} onChange={event => setSeconds(event.target.value)} aria-invalid={!positionValid} />{!positionValid ? <FieldError>位置需要在源视频范围内</FieldError> : null}</Field> : null}
      <Field><FieldLabel htmlFor='frame-role'>参考用途</FieldLabel><Select value={role} disabled={pending} onValueChange={value => setRole(value as VideoReference['role'])}><SelectTrigger id='frame-role'><SelectValue /></SelectTrigger><SelectContent><SelectGroup><SelectItem value='first_frame'>首帧</SelectItem><SelectItem value='last_frame'>尾帧</SelectItem><SelectItem value='reference'>创作参考</SelectItem></SelectGroup></SelectContent></Select></Field>
      <Field><FieldLabel htmlFor='frame-input'>生成输入</FieldLabel><Select value={inputKey} disabled={pending || workflow.isFetching} onValueChange={setInputKey}><SelectTrigger id='frame-input'><SelectValue /></SelectTrigger><SelectContent><SelectGroup><SelectItem value='none'>仅作为创作参考</SelectItem>{fields.map(field => <SelectItem key={field.key} value={field.key}>{field.description || field.key}</SelectItem>)}</SelectGroup></SelectContent></Select></Field>
      {workflow.error || error ? <FieldError>{error || workflow.error?.message}</FieldError> : null}
    </FieldGroup>
    <DialogFooter><Button variant='outline' disabled={pending} onClick={onClose}>取消</Button><Button disabled={pending || !canApply} onClick={() => onApply({ sourceId, targetId, sourcePosition: position, ...(position === 'fixed' ? { sourceFrame: frame, sourceFps: project.fps } : {}), role, inputKey: inputKey === 'none' ? undefined : inputKey })}>{pending ? '正在提取画面' : '提取并采用'}</Button></DialogFooter>
  </DialogContent></Dialog>
}
