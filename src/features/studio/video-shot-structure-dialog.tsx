import { useState } from 'react'
import { Button } from '@/components/ui/button'
import { Checkbox } from '@/components/ui/checkbox'
import { Dialog, DialogContent, DialogFooter, DialogHeader, DialogTitle } from '@/components/ui/dialog'
import { Field, FieldError, FieldLabel } from '@/components/ui/field'
import { Input } from '@/components/ui/input'
import { Textarea } from '@/components/ui/textarea'
import { RadioGroup, RadioGroupItem } from '@/components/ui/radio-group'
import type { StudioAsset } from '@/lib/api/studio'
import { removeVideoReference, serializeVideoContent, setVideoReference, videoReferenceRoleLabels, type VideoProject, type VideoShot } from './video-editor-model'
import { applyVideoShotStructure, mergeVideoShots, splitVideoShot } from './video-shot-structure'
import type { VideoStructureDraft } from './video-structure-draft'

export type VideoStructureRequest = VideoStructureDraft['request']

export function VideoShotStructureDialog({ draft, currentProject, assets, disabled, canRemove, onChange, onApply, onClose, onDiscard, onRestart }: {
  draft: VideoStructureDraft; currentProject: VideoProject; assets: StudioAsset[]; disabled: boolean; canRemove: (id: string) => boolean
  onChange: (draft: VideoStructureDraft) => void; onApply: (sources: VideoShot[], replacements: VideoShot[]) => void; onClose: () => void; onDiscard: () => void; onRestart: () => void
}) {
  const { request, selectedIds, settingsId, drafts, referenceChoices } = draft
  const { project, mode, shotId } = request
  const current = project.shots.find(shot => shot.id === shotId)!
  const [error, setError] = useState('')
  const sources = project.shots.filter(shot => selectedIds.includes(shot.id))
  const sourceIndex = currentProject.shots.findIndex(shot => shot.id === sources[0]?.id)
  const sourceChanged = sources.some((source, index) => serializeVideoContent(currentProject.shots[sourceIndex + index]) !== serializeVideoContent(source))
  const affected = [...currentProject.shots.filter(shot => !selectedIds.includes(shot.id)), ...(drafts ?? [])].filter(shot => shot.references?.some(reference => reference.sourceShotId && selectedIds.includes(reference.sourceShotId)))
  const hasSubtitle = sources.some(shot => shot.subtitle.trim())
  const hasScript = sources.some(shot => shot.script.trim())
  let validation = ''
  if (drafts) {
    try { applyVideoShotStructure(currentProject, sources, drafts) } catch (cause) { validation = cause instanceof Error ? cause.message : String(cause) }
  }
  function change(changes: Partial<VideoStructureDraft>) { onChange({ ...draft, ...changes }); setError('') }
  function patch(id: string, changes: Partial<VideoShot>) { change({ drafts: drafts?.map(shot => shot.id === id ? { ...shot, ...changes } : shot) }) }
  function previewMerge() {
    try {
      const merged = mergeVideoShots(sources, crypto.randomUUID(), settingsId)
      const index = project.shots.findIndex(shot => shot.id === sources[0].id)
      if (sources.some((shot, at) => project.shots[index + at]?.id !== shot.id)) throw new Error('选择顺序连续的分镜')
      change({ drafts: [merged], referenceChoices: merged.references ?? [] })
    } catch (cause) { setError(cause instanceof Error ? cause.message : String(cause)) }
  }
  return <Dialog open onOpenChange={open => { if (!open) onClose() }}><DialogContent aria-describedby={undefined} className='max-h-[85vh] overflow-y-auto shadow-lg sm:max-w-2xl'>
    <DialogHeader><DialogTitle>{mode === 'split' ? '分成多个分镜' : '合并分镜'}</DialogTitle></DialogHeader>
    {!drafts ? <>
      <Field><FieldLabel>合并范围</FieldLabel><div className='flex max-h-56 flex-col gap-2 overflow-y-auto'>{project.shots.map(shot => <label key={shot.id} className='flex items-center gap-2 text-sm'><Checkbox aria-label={`合并 ${shot.name}`} checked={selectedIds.includes(shot.id)} disabled={disabled || !canRemove(shot.id)} onCheckedChange={checked => change({ selectedIds: checked ? [...selectedIds, shot.id] : selectedIds.filter(id => id !== shot.id) })} />{shot.name}</label>)}</div></Field>
      <Field><FieldLabel>沿用生成设置</FieldLabel><RadioGroup value={settingsId} onValueChange={settingsId => change({ settingsId })} disabled={disabled}>{sources.map(shot => <label key={shot.id} className='flex items-center gap-2 text-sm'><RadioGroupItem value={shot.id} />{shot.name}{!shot.workflowId ? ' · 未配置工作流' : ''}</label>)}</RadioGroup></Field>
    </> : <>
      <p className='text-sm'>{sources.map(shot => shot.name).join('、')} → {drafts.length} 个新分镜 · 时长未定</p>
      <p className='text-sm'>生成设置：沿用{sources.find(shot => shot.id === settingsId)?.name ?? current.name}</p>
      {sources.some(shot => shot.media || shot.versions?.length) ? <p className='text-sm'>原视频保留在资产中，新分镜分别选择使用的视频。</p> : null}
      {sources.some(shot => shot.executionId || shot.generationRequest) ? <p className='text-sm'>原生成任务保留原分镜标识，结果继续进入生成记录。</p> : null}
      {affected.length ? <p className='text-sm'>画面引用保持原版本：{affected.map(shot => shot.name).join('、')}。这些分镜将显示来源已改变。</p> : null}
      {currentProject.audio?.length ? <p className='text-sm'>{currentProject.audio.length} 条成片音频保持原起点与使用范围。</p> : null}
      {drafts.map((shot, index) => <section key={shot.id} className='flex flex-col gap-3 rounded-md border bg-muted p-3' aria-label={`新分镜 ${index + 1}`}>
        <div className='flex items-center justify-between gap-2'><strong className='text-sm'>分镜 {index + 1}</strong>{mode === 'split' ? <div className='flex gap-1'>
          <Button size='sm' variant='ghost' disabled={disabled || index === 0} onClick={() => { const next = [...drafts]; [next[index - 1], next[index]] = [next[index], next[index - 1]]; change({ drafts: next }) }}>上移</Button>
          <Button size='sm' variant='ghost' disabled={disabled || drafts.length <= 2} onClick={() => change({ drafts: drafts.filter(value => value.id !== shot.id) })}>移除</Button>
        </div> : null}</div>
        <Field><FieldLabel htmlFor={`structure-name-${shot.id}`}>名称</FieldLabel><Input id={`structure-name-${shot.id}`} aria-label={`新分镜名称 ${index + 1}`} value={shot.name} disabled={disabled} onChange={event => patch(shot.id, { name: event.target.value })} /></Field>
        <Field><FieldLabel htmlFor={`structure-description-${shot.id}`}>画面描述</FieldLabel><Textarea id={`structure-description-${shot.id}`} aria-label={`新分镜描述 ${index + 1}`} value={shot.storyboard ?? ''} disabled={disabled} onChange={event => patch(shot.id, { storyboard: event.target.value })} /></Field>
        {hasScript ? <Field><FieldLabel htmlFor={`structure-script-${shot.id}`}>原分镜文字</FieldLabel><Textarea id={`structure-script-${shot.id}`} value={shot.script} disabled={disabled} onChange={event => patch(shot.id, { script: event.target.value })} /></Field> : null}
        {hasSubtitle ? <Field><FieldLabel htmlFor={`structure-subtitle-${shot.id}`}>已有字幕</FieldLabel><Textarea id={`structure-subtitle-${shot.id}`} value={shot.subtitle} disabled={disabled} onChange={event => patch(shot.id, { subtitle: event.target.value })} /></Field> : null}
        {shot.scriptSources?.map(source => {
          const alternatives = sources.flatMap(original => (original.scriptSources ?? []).filter(item => item.blockId === source.blockId).map(item => ({ shotId: original.id, name: original.name, source: item }))).filter((item, at, all) => all.findIndex(other => serializeVideoContent(other.source) === serializeVideoContent(item.source)) === at)
          return <Field key={source.blockId}><FieldLabel>关联原文 · 修订 {source.revision}</FieldLabel>{alternatives.length > 1 ? <RadioGroup value={alternatives.find(item => serializeVideoContent(item.source) === serializeVideoContent(source))?.shotId} disabled={disabled} onValueChange={value => patch(shot.id, { scriptSources: shot.scriptSources!.map(item => item.blockId === source.blockId ? alternatives.find(option => option.shotId === value)!.source : item) })}>{alternatives.map(item => <label key={item.shotId} className='flex items-start gap-2 text-sm'><RadioGroupItem value={item.shotId} /><span className='whitespace-pre-wrap'>{item.name} · 修订 {item.source.revision}<br />{item.source.text}</span></label>)}</RadioGroup> : <p className='whitespace-pre-wrap text-sm'>{source.text}</p>}</Field>
        })}
        {referenceChoices.length ? <Field><FieldLabel>参考素材</FieldLabel>{referenceChoices.map(reference => {
          const asset = assets.find(asset => asset.id === reference.assetId)
          const version = asset?.versions.find(version => version.id === reference.versionId)
          return <label key={reference.id} className='flex items-start gap-2 text-sm'><Checkbox checked={Boolean(shot.references?.some(item => item.id === reference.id))} disabled={disabled} onCheckedChange={checked => {
          const next = checked ? setVideoReference(shot, reference) : removeVideoReference(shot, reference.id)
          patch(shot.id, { references: next.references, inputs: next.inputs })
        }} /><span>{videoReferenceRoleLabels[reference.role]} · {asset?.name ?? '参考素材'} · {version ? `版本 ${version.version}` : '固定版本'}{reference.inputKey ? ' · 同步生成输入' : ''}</span></label>
        })}</Field> : null}
      </section>)}
      {mode === 'split' ? <Button size='sm' variant='outline' disabled={disabled || project.shots.length - 1 + drafts.length >= 100} onClick={() => {
        const next = splitVideoShot(current, [...drafts.map(shot => shot.id), crypto.randomUUID()]).at(-1)!
        change({ drafts: [...drafts, next] })
      }}>增加分镜</Button> : null}
    </>}
    {error || validation ? <FieldError>{error || validation}</FieldError> : null}
    {sourceChanged ? <Button variant='outline' disabled={disabled || !canRemove(shotId) || !currentProject.shots.some(shot => shot.id === shotId)} onClick={() => { setError(''); onRestart() }}>放弃草稿并重新预览</Button> : null}
    <DialogFooter><Button variant='ghost' disabled={disabled} onClick={onDiscard}>放弃草稿</Button>{mode === 'merge' && drafts ? <Button variant='ghost' disabled={disabled} onClick={() => change({ drafts: undefined })}>重新选择分镜</Button> : null}<Button variant='outline' onClick={onClose}>关闭</Button>
      <Button disabled={disabled || sources.some(shot => !canRemove(shot.id)) || (drafts ? Boolean(validation) : sources.length < 2 || !selectedIds.includes(settingsId))} onClick={() => {
        if (!drafts) { previewMerge(); return }
        try { onApply(sources, drafts); onClose() } catch (cause) { setError(cause instanceof Error ? cause.message : String(cause)) }
      }}>{drafts ? '采用分镜结构' : '预览合并结果'}</Button>
    </DialogFooter>
  </DialogContent></Dialog>
}
