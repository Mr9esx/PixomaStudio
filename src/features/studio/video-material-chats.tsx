import { useState } from 'react'
import { useMutation, useQuery, useQueryClient } from '@tanstack/react-query'
import { Button } from '@/components/ui/button'
import { Dialog, DialogContent, DialogFooter, DialogHeader, DialogTitle } from '@/components/ui/dialog'
import { Field, FieldError, FieldGroup, FieldLabel } from '@/components/ui/field'
import { Input } from '@/components/ui/input'
import { Textarea } from '@/components/ui/textarea'
import { Checkbox } from '@/components/ui/checkbox'
import { Select, SelectContent, SelectGroup, SelectItem, SelectTrigger, SelectValue } from '@/components/ui/select'
import type { StudioAsset } from '@/lib/api/studio'
import { createStudioVideoMaterialChat, getStudioVideoWork, getVideoMaterialForAsset, videoMaterialKinds, type StudioVideoWork, type StudioVideoMaterial } from '@/lib/api/video-projects'

export function VideoMaterialChats({ videoId, assets, shotId, onUse, onOpen, onClose }: {
  videoId: string; assets: StudioAsset[]; shotId?: string; onUse: (material: StudioVideoMaterial, asset: StudioAsset, versionId: string) => void; onOpen: (work: StudioVideoWork, sessionId: string) => Promise<void>; onClose: () => void
}) {
  const client = useQueryClient()
  const [title, setTitle] = useState('')
  const [assetId, setAssetId] = useState('new')
  const [name, setName] = useState('')
  const [kind, setKind] = useState<StudioVideoMaterial['kind']>('character')
  const [brief, setBrief] = useState('')
  const [versionId, setVersionId] = useState<string>()
  const [includeShot, setIncludeShot] = useState(Boolean(shotId))
  const [requestId, setRequestId] = useState(() => crypto.randomUUID())
  const [openError, setOpenError] = useState('')
  const work = useQuery({ queryKey: ['studio', 'video-work', videoId], queryFn: () => getStudioVideoWork(videoId) })
  const materialLookup = useQuery({ queryKey: ['studio', 'material-for-asset', assetId], queryFn: () => getVideoMaterialForAsset(assetId.slice(6)), enabled: assetId.startsWith('asset:'), retry: false })
  const material = work.data?.materials.find(item => `material:${item.id}` === assetId) ?? (assetId.startsWith('asset:') ? materialLookup.data : undefined)
  const selectedAsset = material?.asset ?? assets.find(item => `asset:${item.id}` === assetId)
  const selectedVersion = selectedAsset?.versions.find(item => versionId ? item.id === versionId : item.version === selectedAsset.current_version)
  const sourceShot = work.data?.project.shots.find(shot => shot.id === shotId)
  function changed() { setRequestId(crypto.randomUUID()); create.reset() }
  const create = useMutation({ mutationFn: () => createStudioVideoMaterialChat(videoId, { request_id: requestId, title: title.trim(), ...(material ? { material_id: material.id } : { asset_id: selectedAsset?.id, name: name.trim(), kind, brief }), asset_version_id: selectedVersion?.id, shot_ids: includeShot && shotId ? [shotId] : [] }), onSuccess: async result => {
    client.setQueryData(['studio', 'video-work', videoId], result.work)
    await client.invalidateQueries({ queryKey: ['studio', 'sessions'] })
    await onOpen(result.work, result.session_id)
    onClose()
  } })
  async function open(sessionId: string) {
    if (!work.data) return
    setOpenError('')
    try { await onOpen(work.data, sessionId); onClose() }
    catch (cause) { setOpenError(cause instanceof Error ? cause.message : String(cause)) }
  }
  return <Dialog open onOpenChange={value => { if (!value && !create.isPending) onClose() }}><DialogContent aria-describedby={undefined} className='max-h-[85vh] overflow-y-auto shadow-lg'>
    <DialogHeader><DialogTitle>素材创作对话</DialogTitle></DialogHeader>
    {work.isPending ? <span aria-busy='true'>正在读取对话</span> : work.isError ? <FieldError>{work.error.message}</FieldError> : <div className='flex flex-col gap-2'>
      {work.data.main_session_id ? <Button size='sm' variant='outline' onClick={() => void open(work.data.main_session_id!)}>打开视频主对话</Button> : null}
      {work.data.materials.map(material => <div key={material.id} className='flex flex-col gap-2 rounded-md border bg-muted p-3'><strong className='text-sm'>{videoMaterialKinds[material.kind]} · {material.name}</strong>
        {work.data.chats.filter(chat => chat.material_id === material.id).map(chat => <Button size='sm' variant='outline' key={chat.session_id} onClick={() => void open(chat.session_id)}>{chat.title}</Button>)}
        {material.asset && shotId ? <Button size='sm' variant='outline' onClick={() => {
          const asset = material.asset!
          const version = asset.versions.find(version => version.version === asset.current_version)
          if (!version) { setOpenError('素材版本缺失'); return }
          onUse(material, asset, version.id); onClose()
        }}>用于当前分镜 · 版本 {material.asset.current_version}</Button> : null}
      </div>)}
    </div>}
    <FieldGroup>
      <Field><FieldLabel htmlFor='video-material-asset'>创作素材</FieldLabel><Select value={assetId} disabled={create.isPending} onValueChange={value => { setAssetId(value); setVersionId(undefined); changed(); const material = work.data?.materials.find(item => `material:${item.id}` === value); const asset = assets.find(item => `asset:${item.id}` === value); setName(material?.name ?? asset?.name ?? ''); setKind(material?.kind ?? 'character'); setBrief(material?.brief ?? ''); setTitle(material || asset ? `${material?.name ?? asset?.name} · 创作` : '') }}>
        <SelectTrigger id='video-material-asset'><SelectValue /></SelectTrigger><SelectContent><SelectGroup><SelectItem value='new'>新素材</SelectItem>{work.data?.materials.map(material => <SelectItem key={material.id} value={`material:${material.id}`}>{videoMaterialKinds[material.kind]} · {material.name}</SelectItem>)}{assets.filter(asset => asset.id !== videoId && !work.data?.materials.some(material => material.asset_id === asset.id)).map(asset => <SelectItem key={asset.id} value={`asset:${asset.id}`}>{asset.name}</SelectItem>)}</SelectGroup></SelectContent>
      </Select></Field>
      {!material ? <>
        <Field><FieldLabel htmlFor='video-material-name'>素材名称</FieldLabel><Input id='video-material-name' value={name} disabled={create.isPending} placeholder='例如：小林' onChange={event => { setName(event.target.value); changed() }} /></Field>
        <Field><FieldLabel htmlFor='video-material-kind'>素材类型</FieldLabel><Select value={kind} disabled={create.isPending} onValueChange={value => { setKind(value as StudioVideoMaterial['kind']); changed() }}><SelectTrigger id='video-material-kind'><SelectValue /></SelectTrigger><SelectContent><SelectGroup>{Object.entries(videoMaterialKinds).map(([value, label]) => <SelectItem key={value} value={value}>{label}</SelectItem>)}</SelectGroup></SelectContent></Select></Field>
        <Field><FieldLabel htmlFor='video-material-brief'>创作要求</FieldLabel><Textarea id='video-material-brief' value={brief} disabled={create.isPending} onChange={event => { setBrief(event.target.value); changed() }} /></Field>
      </> : null}
      {material ? <p className='text-sm'>{videoMaterialKinds[material.kind]} · {material.name}</p> : null}
      {selectedAsset ? <Field><FieldLabel htmlFor='video-material-version'>参考版本</FieldLabel><Select value={selectedVersion?.id ?? ''} disabled={create.isPending} onValueChange={value => { setVersionId(value); changed() }}><SelectTrigger id='video-material-version'><SelectValue /></SelectTrigger><SelectContent><SelectGroup>{selectedAsset.versions.map(version => <SelectItem key={version.id} value={version.id}>版本 {version.version}</SelectItem>)}</SelectGroup></SelectContent></Select></Field> : null}
      {sourceShot ? <label className='flex items-center gap-2 text-sm'><Checkbox checked={includeShot} disabled={create.isPending} onCheckedChange={value => { setIncludeShot(value === true); changed() }} />参考分镜：{sourceShot.name}</label> : null}
      <Field><FieldLabel htmlFor='video-material-title'>对话名称</FieldLabel><Input id='video-material-title' value={title} disabled={create.isPending} placeholder='例如：小林 · 人物创作' onChange={event => { setTitle(event.target.value); changed() }} /></Field>
      {create.error || materialLookup.error || openError ? <FieldError>{create.error?.message || materialLookup.error?.message || openError}</FieldError> : null}
    </FieldGroup>
    <DialogFooter><Button size='sm' variant='outline' disabled={create.isPending} onClick={onClose}>关闭</Button><Button size='sm' disabled={!title.trim() || (!material && !name.trim()) || !work.data || create.isPending || (assetId.startsWith('asset:') && !materialLookup.isSuccess)} onClick={() => create.mutate()}>{create.isPending ? '正在创建' : '创建素材对话'}</Button></DialogFooter>
  </DialogContent></Dialog>
}
