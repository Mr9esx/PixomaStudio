import { useState } from 'react'
import { useMutation, useQueryClient } from '@tanstack/react-query'
import { Button } from '@/components/ui/button'
import { Field, FieldError, FieldLabel } from '@/components/ui/field'
import { Popover, PopoverContent, PopoverTrigger } from '@/components/ui/popover'
import { Select, SelectContent, SelectGroup, SelectItem, SelectTrigger, SelectValue } from '@/components/ui/select'
import type { StudioAsset } from '@/lib/api/studio'
import { adoptVideoMaterialResult, videoMaterialKinds, type StudioVideoMaterial, type StudioVideoMaterialContext } from '@/lib/api/video-projects'

export function VideoMaterialContext({ material, context, sessionId, assets, error, loading, onVersion }: {
  material: StudioVideoMaterial; context?: StudioVideoMaterialContext; sessionId: string; assets: StudioAsset[]; error?: string; loading: boolean; onVersion: (id: string) => void
}) {
  const client = useQueryClient()
  const [selection, setSelection] = useState('')
  const candidates = assets.filter(asset => !material.asset || asset.kind === material.asset.kind).flatMap(asset => asset.versions.filter(version => asset.usages?.some(usage => usage.session_id === sessionId && usage.asset_version_id === version.id)).map(version => ({ asset, version })))
  const selected = candidates.find(item => item.version.id === selection)
  const adopt = useMutation({ mutationFn: (input: Parameters<typeof adoptVideoMaterialResult>[1]) => adoptVideoMaterialResult(material.id, input), onSuccess: async (result, input) => {
    const adopted = result.results.find(result => result.source_version_id === input.asset_version_id && result.source_session_id === sessionId)
    if (adopted) onVersion(adopted.asset_version_id)
    setSelection('')
    await Promise.all([
      client.invalidateQueries({ queryKey: ['studio', 'video-work'] }),
      client.invalidateQueries({ queryKey: ['studio', 'material-context'] }),
      client.invalidateQueries({ queryKey: ['studio', 'session'] }),
      client.invalidateQueries({ queryKey: ['studio', 'video-project-assets'] }),
    ])
  } })
  return <Popover><PopoverTrigger asChild><Button size='sm' variant='outline' aria-label='当前素材创作对象' className='max-w-56 min-w-0'><span className='truncate'>{videoMaterialKinds[material.kind]} · {material.name}{context?.asset_version ? ` · 版本 ${context.asset_version.version}` : ''}</span></Button></PopoverTrigger>
    <PopoverContent className='flex max-h-[75vh] w-80 flex-col gap-3 overflow-y-auto shadow-lg' aria-label='素材创作对象设置'>
      <strong className='text-sm'>{videoMaterialKinds[material.kind]} · {material.name}</strong>
      {material.brief ? <p className='whitespace-pre-wrap text-sm'>{material.brief}</p> : null}
      {material.asset ? <Field><FieldLabel htmlFor='material-context-version'>本次对话参考版本</FieldLabel><Select value={context?.asset_version?.id ?? ''} disabled={loading || adopt.isPending} onValueChange={onVersion}><SelectTrigger id='material-context-version'><SelectValue /></SelectTrigger><SelectContent><SelectGroup>{material.asset.versions.map(version => <SelectItem key={version.id} value={version.id}>版本 {version.version}</SelectItem>)}</SelectGroup></SelectContent></Select></Field> : null}
      <Field><FieldLabel htmlFor='material-result'>采用创作结果</FieldLabel><Select value={selection} disabled={adopt.isPending} onValueChange={value => { setSelection(value); adopt.reset() }}><SelectTrigger id='material-result'><SelectValue placeholder='选择当前对话的资产版本' /></SelectTrigger><SelectContent><SelectGroup>{candidates.map(({ asset, version }) => <SelectItem key={version.id} value={version.id}>{asset.name} · 版本 {version.version}</SelectItem>)}</SelectGroup></SelectContent></Select></Field>
      {error || adopt.error ? <FieldError>{error || adopt.error?.message}</FieldError> : null}
      <Button size='sm' disabled={!selected || adopt.isPending || loading} onClick={() => { if (selected) adopt.mutate(adopt.variables ?? { request_id: crypto.randomUUID(), session_id: sessionId, asset_id: selected.asset.id, asset_version_id: selected.version.id, expected_version: material.asset?.current_version ?? 0 }) }}>{adopt.isPending ? '正在登记结果' : '登记为素材版本'}</Button>
    </PopoverContent>
  </Popover>
}
