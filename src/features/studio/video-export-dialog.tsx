import { useMemo, useState } from 'react'
import { Button } from '@/components/ui/button'
import { Checkbox } from '@/components/ui/checkbox'
import { Dialog, DialogContent, DialogFooter, DialogHeader, DialogTitle } from '@/components/ui/dialog'
import type { StudioAsset } from '@/lib/api/studio'
import { updateVideoProjectSettings, videoGenerationChanged } from './video-editor-model'
import { createVideoExportRequest, videoExportIssues, videoExportPhase, videoExportSettingsSchema, type VideoExportIssue, type VideoExportSubmission, type VideoExportSnapshot, type VideoExportTask } from './video-export-model'
import { VideoSettingsFields, type VideoSettingKey } from './video-project-settings'

export function VideoExportDialog({ sessionId, snapshot, assets, open, pending, refreshing, previewPending, task, error, onOpenChange, onEdit, onRefresh, onLocate, onExport, onResume, onPreview, onOpenSource }: {
  sessionId: string
  snapshot: VideoExportSnapshot
  assets: StudioAsset[]
  open: boolean
  pending: boolean
  refreshing: boolean
  previewPending: boolean
  task?: VideoExportTask
  error?: string
  onOpenChange: (open: boolean) => void
  onEdit: () => void
  onRefresh: () => void
  onLocate: (issue: VideoExportIssue) => void
  onExport: (submission: VideoExportSubmission) => void
  onResume: () => void
  onPreview: () => void
  onOpenSource: () => void
}) {
  const [draft, setDraft] = useState<Partial<Record<VideoSettingKey, string>>>({})
  const [requestId, setRequestId] = useState(snapshot.id)
  const [useAdopted, setUseAdopted] = useState(false)
  const changedInputs = snapshot.project.shots.filter(videoGenerationChanged)
  const subtitles = snapshot.project.shots.filter(shot => shot.subtitle.trim())
  const audio = snapshot.project.audio ?? []
  const values = { ...snapshot.project, ...draft }
  const validation = videoExportSettingsSchema.safeParse({ name: values.name, width: Number(values.width), height: Number(values.height), fps: Number(values.fps) })
  const preview = useMemo(() => {
    const settings = videoExportSettingsSchema.safeParse({ name: draft.name ?? snapshot.project.name, width: Number(draft.width ?? snapshot.project.width), height: Number(draft.height ?? snapshot.project.height), fps: Number(draft.fps ?? snapshot.project.fps) })
    if (!settings.success) return undefined
    try { return { project: updateVideoProjectSettings(snapshot.project, settings.data), error: undefined } }
    catch (cause) { return { project: undefined, error: cause instanceof Error ? cause.message : String(cause) } }
  }, [snapshot.project, draft])
  const output = preview?.project
  const projectIssues = videoExportIssues(snapshot.project)
  for (const issue of output ? videoExportIssues(output) : []) {
    if (!projectIssues.some(current => current.id === issue.id && current.kind === issue.kind && current.message === issue.message)) projectIssues.push(issue)
  }
  const issues = [...projectIssues, ...(task?.problems ?? [])]
  return <Dialog open={open} onOpenChange={onOpenChange}><DialogContent aria-describedby={undefined} className='max-h-[85vh] overflow-y-auto shadow-lg'>
    <DialogHeader><DialogTitle>导出成片</DialogTitle></DialogHeader>
    <VideoSettingsFields values={values} errors={validation.error?.issues} nameLabel='文件名称' disabled={pending || Boolean(task)} onChange={(key, value) => { setDraft(current => ({ ...current, [key]: value })); setRequestId(crypto.randomUUID()); onEdit() }} />
    {output ? <div className='flex flex-col gap-1 text-sm' aria-label='导出规格'>
      <span>MP4 · {output.width}×{output.height} · {output.fps} fps</span>
      <span>输出时长：{output.shots.reduce((sum, shot) => sum + shot.duration, 0).toFixed(3)} 秒</span>
      {snapshot.sourceVersion !== undefined ? <Button variant='link' size='sm' className='h-auto justify-start self-start p-0' onClick={onOpenSource}>查看来源作品 · 修订 {snapshot.sourceVersion}</Button> : null}
      {output.fps !== snapshot.project.fps ? <span>作品时长：{snapshot.project.shots.reduce((sum, shot) => sum + shot.duration, 0).toFixed(3)} 秒</span> : null}
    </div> : null}
    {changedInputs.length ? <div className='flex flex-col gap-2 rounded-md border bg-muted p-3' aria-label='生成输入修改检查'>
      <strong className='text-sm'>生成输入已修改</strong>
      <ul className='list-inside list-disc text-sm'>{changedInputs.map(shot => <li key={shot.id}>{shot.name}</li>)}</ul>
      <label className='flex items-center gap-2 text-sm'><Checkbox checked={Boolean(task) || useAdopted} disabled={Boolean(task) || pending} onCheckedChange={checked => setUseAdopted(checked === true)} />使用当前已采用版本</label>
    </div> : null}
    {subtitles.length || audio.length ? <div className='flex flex-col gap-2 rounded-md border bg-muted p-3' aria-label='导出包含的已有内容'>
      <strong className='text-sm'>已有内容</strong>
      <ul className='flex flex-col gap-1 text-sm'>{subtitles.map(shot => <li key={shot.id}>{shot.name} · 字幕：{shot.subtitle}</li>)}{audio.map(clip => <li key={clip.id}>{assets.find(asset => asset.id === clip.assetId)?.name ?? '音频'} · {clip.start.toFixed(3)}–{(clip.start + clip.duration).toFixed(3)} 秒{clip.loop ? ' · 循环' : ''}{clip.volume === 0 ? ' · 静音' : ''}</li>)}</ul>
    </div> : null}
    {issues.length ? <ul className='flex max-h-48 flex-col gap-2 overflow-y-auto' aria-label='导出检查结果'>{issues.map((issue, index) => {
      const label = issue.kind === 'shot' ? snapshot.project.shots.find(shot => shot.id === issue.id)?.name : issue.kind === 'audio' ? assets.find(asset => asset.id === snapshot.project.audio?.find(audio => audio.id === issue.id)?.assetId)?.name ?? '音频' : '作品'
      return <li key={`${issue.kind}:${issue.id}:${index}`}><Button className='h-auto w-full justify-start whitespace-normal text-left' variant='outline' size='sm' disabled={pending || issue.kind === 'project'} onClick={() => onLocate(issue)}>{label}：{issue.message}</Button></li>
    })}</ul> : null}
    {preview?.error || error || task?.error ? <p role='alert' className='text-sm'>{preview?.error || error || task?.error}</p> : null}
    {task ? <p role='status' className='text-sm'>{videoExportPhase[task.status]}</p> : null}
    <DialogFooter><Button variant='ghost' disabled={pending || previewPending || refreshing} onClick={onRefresh}>{refreshing ? '正在保存作品' : '使用当前作品'}</Button><Button variant='outline' onClick={() => onOpenChange(false)}>{pending ? '继续编辑' : '关闭'}</Button><Button disabled={pending || previewPending || refreshing || (!task && (!output || !validation.success || projectIssues.length > 0 || (changedInputs.length > 0 && !useAdopted)))} onClick={() => {
      if (task?.status === 'completed') onPreview()
      else if (task) onResume()
      else if (output && validation.success) onExport({ project: output, request: createVideoExportRequest(sessionId, requestId, snapshot.project, validation.data) })
    }}>{pending ? '导出中' : previewPending ? '正在读取成片' : task?.status === 'completed' ? '预览成片' : task?.status === 'interrupted' ? '恢复导出' : task || error ? '重试导出' : '开始导出'}</Button></DialogFooter>
  </DialogContent></Dialog>
}
