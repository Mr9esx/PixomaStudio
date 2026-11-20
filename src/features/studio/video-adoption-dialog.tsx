import { useState, type ReactNode } from 'react'
import { Button } from '@/components/ui/button'
import { Dialog, DialogContent, DialogFooter, DialogHeader, DialogTitle } from '@/components/ui/dialog'
import { Field, FieldError, FieldGroup, FieldLabel } from '@/components/ui/field'
import { Input } from '@/components/ui/input'
import { Select, SelectContent, SelectGroup, SelectItem, SelectTrigger, SelectValue } from '@/components/ui/select'
import { videoProjectTiming, videoShotDuration, type VideoProject } from './video-editor-model'
import { previewVideoAdoption, type VideoAdoption, type VideoAdoptionRange } from './video-adoption'

export function VideoAdoptionDialog({ project, request, preview, disabled, onApply, onClose }: {
  project: VideoProject; request: VideoAdoption; preview: (onReady: (ready: boolean) => void) => ReactNode; disabled: boolean
  onApply: (range: VideoAdoptionRange) => void; onClose: () => void
}) {
  const fullDuration = Math.floor((request.media.duration ?? 0) * project.fps) / project.fps
  const history = request.shot.versions?.find(item => item.media.versionId === request.media.versionId && item.status === 'history')
  const [mode, setMode] = useState(request.shot.media ? 'preserve' : 'full')
  const [sourceIn, setSourceIn] = useState('0')
  const [duration, setDuration] = useState(String(request.shot.media ? request.shot.duration : fullDuration))
  const [applyError, setApplyError] = useState('')
  const [mediaReady, setMediaReady] = useState(false)
  const range = {
    sourceIn: sourceIn.trim() ? Math.round(Number(sourceIn) * project.fps) / project.fps : NaN,
    duration: duration.trim() ? Math.round(Number(duration) * project.fps) / project.fps : NaN,
  }
  let next: VideoProject | undefined
  let error = ''
  try { next = previewVideoAdoption(project, request, range) }
  catch (cause) { error = cause instanceof Error ? cause.message : String(cause) }
  const timing = videoProjectTiming(project)
  const nextTiming = next ? videoProjectTiming(next) : undefined
  const total = timing.knownDuration
  const nextTotal = nextTiming?.knownDuration
  const delta = nextTotal === undefined ? 0 : nextTotal - total
  const targetIndex = project.shots.findIndex(shot => shot.id === request.shot.id)
  const affectedAudio = project.audio?.filter(item => next && next.audio?.find(audio => audio.id === item.id)?.duration !== item.duration) ?? []
  function choose(value: string) {
    setMode(value); setApplyError('')
    setSourceIn(String(value === 'history' ? history?.sourceIn ?? 0 : 0))
    setDuration(String(value === 'history' ? history?.duration ?? fullDuration : value === 'plan' ? request.shot.plannedDuration : value === 'full' ? fullDuration : request.shot.duration))
  }
  return <Dialog open onOpenChange={open => { if (!open) onClose() }}><DialogContent aria-describedby={undefined} className='max-h-[85vh] overflow-y-auto shadow-lg'>
    <DialogHeader><DialogTitle>采用视频 · {request.shot.name}</DialogTitle></DialogHeader>
    {preview(setMediaReady)}
    <FieldGroup>
      <Field><FieldLabel htmlFor='video-adoption-mode'>使用范围</FieldLabel><Select value={mode} onValueChange={choose}>
        <SelectTrigger id='video-adoption-mode'><SelectValue /></SelectTrigger><SelectContent><SelectGroup>
          {request.shot.media ? <SelectItem value='preserve'>保留当前使用时长</SelectItem> : null}
          {request.shot.plannedDuration !== undefined ? <SelectItem value='plan'>按计划时长选取</SelectItem> : null}
          <SelectItem value='full'>使用完整视频</SelectItem>
          {history?.sourceIn !== undefined && history.duration !== undefined ? <SelectItem value='history'>恢复历史剪辑范围</SelectItem> : null}
          <SelectItem value='custom'>选择使用范围</SelectItem>
        </SelectGroup></SelectContent>
      </Select></Field>
      <Field><FieldLabel htmlFor='video-adoption-start'>源视频开始（秒）</FieldLabel><Input id='video-adoption-start' type='number' min={0} step='any' value={sourceIn} onChange={event => { setSourceIn(event.target.value); setMode('custom'); setApplyError('') }} /></Field>
      <Field data-invalid={Boolean(error)}><FieldLabel htmlFor='video-adoption-duration'>使用时长（秒）</FieldLabel><Input id='video-adoption-duration' type='number' min={0} step='any' value={duration} aria-invalid={Boolean(error)} onChange={event => { setDuration(event.target.value); setMode('custom'); setApplyError('') }} />{error ? <FieldError>{error}</FieldError> : null}</Field>
    </FieldGroup>
    {next && nextTotal !== undefined ? <div className='flex flex-col gap-2 rounded border bg-muted p-3 text-sm tabular-nums' aria-label='采用视频的时间变化'>
      <span>分镜时长：{request.newShot ? '未添加' : videoShotDuration(request.shot) === undefined ? '时长未定' : `${videoShotDuration(request.shot)!.toFixed(3)} 秒`} → {range.duration.toFixed(3)} 秒</span>
      <span>成片总时长：{total.toFixed(3)} 秒{timing.unknownCount ? ` + ${timing.unknownCount} 个未定分镜` : ''} → {nextTotal.toFixed(3)} 秒{nextTiming?.unknownCount ? ` + ${nextTiming.unknownCount} 个未定分镜` : ''}</span>
      {!request.newShot && !timing.unknownCount && delta !== 0 && project.shots.length > targetIndex + 1 ? <span>后续 {project.shots.length - targetIndex - 1} 个分镜{delta > 0 ? '延后' : '提前'} {Math.abs(delta).toFixed(3)} 秒</span> : null}
      {affectedAudio.map(audio => <span key={audio.id}>已有音频 {project.audio!.indexOf(audio) + 1}：{audio.duration.toFixed(3)} 秒 → {next.audio?.find(item => item.id === audio.id)?.duration.toFixed(3) ?? '移除'}</span>)}
    </div> : null}
    {applyError ? <FieldError>{applyError}</FieldError> : null}
    <DialogFooter><Button size='sm' variant='outline' onClick={onClose}>取消</Button><Button size='sm' disabled={disabled || !next || !mediaReady} onClick={() => { try { onApply(range) } catch (cause) { setApplyError(cause instanceof Error ? cause.message : String(cause)) } }}>采用此版本</Button></DialogFooter>
  </DialogContent></Dialog>
}
