import { useState } from 'react'
import { useQuery } from '@tanstack/react-query'
import { Button } from '@/components/ui/button'
import { Checkbox } from '@/components/ui/checkbox'
import { Dialog, DialogContent, DialogFooter, DialogHeader, DialogTitle } from '@/components/ui/dialog'
import { Field, FieldError, FieldLabel } from '@/components/ui/field'
import { Select, SelectContent, SelectGroup, SelectItem, SelectTrigger, SelectValue } from '@/components/ui/select'
import { getStudioAsset } from '@/lib/api/studio'
import { videoReferenceRoleLabels, type VideoAssetReference, type VideoProject, type VideoReference } from './video-editor-model'
import { videoReferenceUpdateTargets, type VideoReferenceUpdateTarget } from './video-reference-update'

export function VideoReferenceVersions({ project, reference, disabled, onApply, onClose }: {
  project: VideoProject; reference: VideoReference; disabled: boolean
  onApply: (targets: VideoReferenceUpdateTarget[], replacement: VideoAssetReference & { kind: VideoReference['kind'] }) => void; onClose: () => void
}) {
  const asset = useQuery({ queryKey: ['studio', 'asset', reference.assetId], queryFn: () => getStudioAsset(reference.assetId) })
  const [targets, setTargets] = useState(() => videoReferenceUpdateTargets(project, reference.assetId, reference.versionId))
  const [versionId, setVersionId] = useState<string>()
  const [error, setError] = useState('')
  const versions = asset.data?.versions ?? []
  const chosen = versions.find(version => version.id === versionId) ?? (versionId ? undefined : versions.find(version => version.version === asset.data?.current_version))
  const selected = targets.filter(target => target.selected)
  return <Dialog open onOpenChange={value => { if (!value) onClose() }}><DialogContent aria-describedby={undefined} className='max-h-[85vh] overflow-y-auto shadow-lg'>
    <DialogHeader><DialogTitle>采用素材版本 · {asset.data?.name ?? '读取素材'}</DialogTitle></DialogHeader>
    <Field><FieldLabel htmlFor='reference-version'>素材版本</FieldLabel><Select value={chosen?.id ?? ''} disabled={disabled || asset.isFetching} onValueChange={value => { setVersionId(value); setError('') }}><SelectTrigger id='reference-version'><SelectValue placeholder='选择版本' /></SelectTrigger><SelectContent><SelectGroup>{[...versions].sort((a, b) => b.version - a.version).map(version => <SelectItem key={version.id} value={version.id}>版本 {version.version}</SelectItem>)}</SelectGroup></SelectContent></Select></Field>
    <div className='flex max-h-72 flex-col gap-3 overflow-y-auto' aria-label='素材版本应用范围'>{targets.map((target, index) => <label key={`${target.shotId}:${target.reference.id}`} className='flex items-start gap-2 rounded-md border bg-muted p-3'>
      <Checkbox checked={target.selected} disabled={disabled} aria-label={`更新引用 ${target.shotName} ${videoReferenceRoleLabels[target.reference.role]}`} onCheckedChange={checked => { setTargets(current => current.map((item, at) => at === index ? { ...item, selected: checked === true } : item)); setError('') }} />
      <span className='flex flex-col gap-1 text-sm'><span>{target.shotName} · {videoReferenceRoleLabels[target.reference.role]}</span><span className='text-xs text-muted-foreground'>版本 {versions.find(version => version.id === target.reference.versionId)?.version ?? '不可用'}{target.mapped ? ' · 同时更新生成输入' : target.reference.inputKey ? ' · 保留手工选择的生成输入' : ''}</span></span>
    </label>)}</div>
    {asset.error || error ? <FieldError>{error || asset.error?.message}</FieldError> : null}
    <DialogFooter><Button variant='outline' onClick={onClose}>取消</Button><Button disabled={disabled || !chosen || !selected.length || selected.every(target => target.reference.versionId === chosen.id)} onClick={() => {
      if (!chosen || !asset.data) return
      try {
        onApply(selected, { assetId: asset.data.id, versionId: chosen.id, url: chosen.content_url, kind: reference.kind })
        onClose()
      } catch (cause) { setError(cause instanceof Error ? cause.message : String(cause)) }
    }}>采用到所选引用</Button></DialogFooter>
  </DialogContent></Dialog>
}
