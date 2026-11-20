import { useId, useMemo, useState } from 'react'
import { Settings2 } from 'lucide-react'
import { Button } from '@/components/ui/button'
import { Dialog, DialogContent, DialogFooter, DialogHeader, DialogTitle, DialogTrigger } from '@/components/ui/dialog'
import { Field, FieldError, FieldGroup, FieldLabel } from '@/components/ui/field'
import { Input } from '@/components/ui/input'
import type { StudioAsset } from '@/lib/api/studio'
import { updateVideoProjectSettings, videoProjectSettingsSchema, videoProjectTiming, type VideoProject } from './video-editor-model'

const fields = { name: '视频名称', width: '宽度（像素）', height: '高度（像素）', fps: '帧率' } as const
export type VideoSettingKey = keyof typeof fields

export function VideoSettingsFields({ values, errors, disabled, nameLabel = '视频名称', onChange }: {
  values: Record<VideoSettingKey, string | number>
  errors?: { path: PropertyKey[]; message: string }[]
  disabled?: boolean
  nameLabel?: string
  onChange: (key: VideoSettingKey, value: string) => void
}) {
  const prefix = useId()
  return <FieldGroup>{(Object.keys(fields) as VideoSettingKey[]).map(key => {
    const messages = errors?.filter(issue => issue.path[0] === key)
    const id = `${prefix}-${key}`
    return <Field key={key} data-invalid={Boolean(messages?.length)}><FieldLabel htmlFor={id}>{key === 'name' ? nameLabel : fields[key]}</FieldLabel>
      <Input id={id} disabled={disabled} type={key === 'name' ? 'text' : 'number'} value={values[key]} min={key === 'fps' ? 1 : 16} max={key === 'fps' ? 60 : 4096} step={key === 'fps' ? 1 : 2} aria-invalid={Boolean(messages?.length)} aria-describedby={messages?.length ? `${id}-error` : undefined} onChange={event => onChange(key, event.target.value)} />
      {messages?.length ? <FieldError id={`${id}-error`} errors={messages} /> : null}
    </Field>
  })}</FieldGroup>
}

export function VideoProjectSettings({ project, assets, disabled, onApply }: { project: VideoProject; assets: StudioAsset[]; disabled: boolean; onApply: (project: VideoProject) => void }) {
  const [open, setOpen] = useState(false)
  const [draft, setDraft] = useState<Partial<Record<VideoSettingKey, string>>>({})
  const validation = useMemo(() => videoProjectSettingsSchema.safeParse({ name: draft.name ?? project.name, width: Number(draft.width ?? project.width), height: Number(draft.height ?? project.height), fps: Number(draft.fps ?? project.fps) }), [draft, project])
  const preview = useMemo(() => {
    if (!validation.success) return undefined
    try { return { project: updateVideoProjectSettings(project, validation.data), error: undefined } }
    catch (cause) { return { project: undefined, error: cause instanceof Error ? cause.message : String(cause) } }
  }, [project, validation])
  const next = preview?.project
  const changed = next && JSON.stringify(next) !== JSON.stringify(project)
  const changedShots = next?.shots.filter((shot, index) => Math.abs(shot.duration - project.shots[index].duration) > 1e-6) ?? []
  const changedPlans = next?.shots.filter((shot, index) => shot.plannedDuration !== project.shots[index].plannedDuration) ?? []
  const timing = videoProjectTiming(project)
  const nextTiming = next ? videoProjectTiming(next) : undefined
  const changedAudio = project.audio?.filter(audio => {
    const updated = next?.audio?.find(item => item.id === audio.id)
    return next && (!updated || Math.abs(updated.duration - audio.duration) > 1e-6)
  }) ?? []
  return <Dialog open={open} onOpenChange={value => { setOpen(value); if (value) setDraft({}) }}>
    <DialogTrigger asChild><Button size='sm' variant='ghost' disabled={disabled} className='min-w-0' aria-label='编辑作品设置'><span className='max-w-48 truncate'>{project.name}</span><Settings2 data-icon='inline-end' /></Button></DialogTrigger>
    <DialogContent aria-describedby={undefined} className='max-h-[85vh] overflow-y-auto shadow-lg'>
      <DialogHeader><DialogTitle>作品设置</DialogTitle></DialogHeader>
      <VideoSettingsFields values={{ ...project, ...draft }} errors={validation.error?.issues} onChange={(key, value) => setDraft(current => ({ ...current, [key]: value }))} />
      {next && next.fps !== project.fps ? <div className='flex flex-col gap-2 text-sm' aria-label='帧率修改影响'>
        <p>已确定时长：{timing.knownDuration.toFixed(3)} 秒 → {nextTiming!.knownDuration.toFixed(3)} 秒{timing.unknownCount ? ` · ${timing.unknownCount} 个分镜时长未定` : ''}</p>
        <ul className='flex max-h-48 flex-col gap-1 overflow-y-auto'>
          {changedShots.map(shot => <li key={shot.id}>{shot.name}：{project.shots.find(item => item.id === shot.id)!.duration.toFixed(3)} 秒 → {shot.duration.toFixed(3)} 秒</li>)}
          {changedPlans.map(shot => <li key={`plan-${shot.id}`}>{shot.name}计划时长：{project.shots.find(item => item.id === shot.id)!.plannedDuration?.toFixed(3)} 秒 → {shot.plannedDuration?.toFixed(3)} 秒</li>)}
          {changedAudio.map(audio => <li key={audio.id}>{assets.find(asset => asset.id === audio.assetId)?.name ?? '音频'}：{audio.duration.toFixed(3)} 秒 → {next.audio?.find(item => item.id === audio.id)?.duration.toFixed(3) ?? '移除'}</li>)}
        </ul>
      </div> : null}
      {preview?.error ? <p role='alert' className='text-sm'>{preview.error}</p> : null}
      <DialogFooter><Button variant='outline' onClick={() => setOpen(false)}>取消</Button><Button disabled={disabled || !next || !changed} onClick={() => { if (next) { onApply(next); setOpen(false) } }}>应用修改</Button></DialogFooter>
    </DialogContent>
  </Dialog>
}
