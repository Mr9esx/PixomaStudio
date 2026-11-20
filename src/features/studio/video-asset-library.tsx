import { useLayoutEffect, useRef } from 'react'
import { LayoutGrid, List, MoreHorizontal, MessageCircle, Play, Plus, Upload } from 'lucide-react'
import { Button } from '@/components/ui/button'
import { Input } from '@/components/ui/input'
import { Empty, EmptyHeader, EmptyTitle } from '@/components/ui/empty'
import { Select, SelectContent, SelectGroup, SelectItem, SelectTrigger, SelectValue } from '@/components/ui/select'
import { DropdownMenu, DropdownMenuContent, DropdownMenuGroup, DropdownMenuItem, DropdownMenuRadioGroup, DropdownMenuRadioItem, DropdownMenuSub, DropdownMenuSubContent, DropdownMenuSubTrigger, DropdownMenuTrigger } from '@/components/ui/dropdown-menu'
import type { StudioAsset } from '@/lib/api/studio'
import { VideoAssetThumbnail } from './video-asset-thumbnail'
import { selectedVideoAsset, type VideoAssetBrowserState } from './video-editor-view'

export const videoAssetDragType = 'application/x-pixoma-asset'
export const videoAssetKindDragTypes = { image: 'application/x-pixoma-image', video: 'application/x-pixoma-video', audio: 'application/x-pixoma-audio', document: 'application/x-pixoma-document' }
export type VideoAssetImport = { id: string; file: File; status: 'queued' | 'uploading' | 'succeeded' | 'existing' | 'failed'; error?: string; asset?: StudioAsset }

export function VideoAssetLibrary({ assets, assetVersions, browser, onBrowserChange, uploading, imports, selected, onVersionChange, onPreview, onReference, onAdd, onUpload, onRetry, onDismissImports }: {
  assets: StudioAsset[]
  assetVersions: Record<string, string>
  browser: VideoAssetBrowserState
  onBrowserChange: (state: VideoAssetBrowserState) => void
  onVersionChange: (assetId: string, versionId: string) => void
  uploading: boolean
  imports: VideoAssetImport[]
  selected: boolean
  onPreview: (asset: StudioAsset) => void
  onReference: (asset: StudioAsset) => void
  onAdd: (asset: StudioAsset, track: string) => void
  onUpload: (files: File[]) => void
  onRetry: (id: string) => void
  onDismissImports: () => void
}) {
  const { search, assetKind: kind, assetView: view } = browser
  const scrollArea = useRef<HTMLDivElement>(null)
  const visible = assets.filter(asset => asset.name.toLocaleLowerCase().includes(search.toLocaleLowerCase()) && (kind === 'all' || asset.kind === kind))
  useLayoutEffect(() => {
    if (scrollArea.current) scrollArea.current.scrollTop = browser.assetScrollTop
  }, [browser.assetScrollTop, visible.length, imports.length, view])
  return <div className='sve-library' aria-label='项目资产库'>
    <Input aria-label='搜索项目资产' placeholder='搜索资产' value={search} onChange={event => onBrowserChange({ ...browser, search: event.target.value, assetScrollTop: 0 })} />
    <div className='sve-library-tools'>
      <Select value={kind} onValueChange={value => onBrowserChange({ ...browser, assetKind: value as VideoAssetBrowserState['assetKind'], assetScrollTop: 0 })}><SelectTrigger aria-label='资产类型' size='sm'><SelectValue /></SelectTrigger><SelectContent><SelectGroup>{[['all', '全部'], ['image', '图片'], ['video', '视频'], ['audio', '音频'], ['document', '文档']].map(([value, label]) => <SelectItem key={value} value={value}>{label}</SelectItem>)}</SelectGroup></SelectContent></Select>
      <Button variant='ghost' size='icon-sm' aria-label={view === 'grid' ? '使用资产列表' : '使用资产网格'} onClick={() => onBrowserChange({ ...browser, assetView: view === 'grid' ? 'list' : 'grid', assetScrollTop: 0 })}>{view === 'grid' ? <List /> : <LayoutGrid />}</Button>
      <label className='sve-upload' aria-label='导入项目资产'><Upload className='size-4' /><input type='file' multiple aria-label='导入项目资产' disabled={uploading} onChange={event => { onUpload(Array.from(event.target.files ?? [])); event.target.value = '' }} /></label>
    </div>
    {imports.length ? <div className='flex max-h-48 shrink-0 flex-col gap-2 overflow-auto border-b pb-2' aria-label='资产导入结果' aria-live='polite'>
      {imports.map(item => <div key={item.id} className='flex flex-col gap-1 text-xs'>
        <div className='flex items-center gap-2'><span className='min-w-0 flex-1 truncate' title={item.file.name}>{item.file.name}</span><span className='shrink-0 text-muted-foreground'>{({ queued: '等待导入', uploading: '导入中', succeeded: '已导入', existing: '已存在', failed: '导入失败' })[item.status]}</span>{item.status === 'failed' ? <Button size='sm' variant='ghost' disabled={uploading} onClick={() => onRetry(item.id)}>重试</Button> : null}</div>
        {item.asset && (item.status === 'succeeded' || item.status === 'existing') ? <Button size='sm' variant='ghost' className='max-w-full self-start' aria-label={`查看导入资产：${item.asset.name}`} onClick={() => onPreview(item.asset!)}><span className='truncate'>查看{item.asset.name}</span></Button> : null}
        {item.error ? <span className='break-words text-muted-foreground'>{item.error}</span> : null}
      </div>)}
      <Button size='sm' variant='ghost' disabled={uploading} onClick={onDismissImports}>清除导入记录</Button>
    </div> : null}
    <div ref={scrollArea} className='sve-asset-scroll' role='region' aria-label='资产浏览结果' tabIndex={0} onScroll={event => {
      const top = event.currentTarget.scrollTop
      if (top !== browser.assetScrollTop) onBrowserChange({ ...browser, assetScrollTop: top })
    }}><div className='sve-asset-grid' data-view={view}>{visible.map(asset => {
      const selection = selectedVideoAsset(asset, assetVersions[asset.id])
      const version = selection?.versions.find(item => item.version === selection.current_version)
      return <article className='sve-asset-card' key={asset.id} draggable={Boolean(version)} onDragStart={event => {
      if (!version) { event.preventDefault(); return }
      event.dataTransfer.setData(videoAssetDragType, JSON.stringify({ assetId: asset.id, versionId: version.id, kind: asset.kind }))
      const kindType = videoAssetKindDragTypes[asset.kind as keyof typeof videoAssetKindDragTypes]
      if (kindType) event.dataTransfer.setData(kindType, '1')
      event.dataTransfer.effectAllowed = 'copy'
    }}>
      <Button className='sve-asset-open' variant='ghost' size='sm' disabled={!selection} aria-label={`预览资产：${asset.name}`} onClick={() => onPreview(selection!)}>{selection ? <VideoAssetThumbnail asset={selection} label={asset.name} /> : <span className='sve-muted'>版本不可用</span>}<span className='truncate'>{asset.name}</span></Button>
      {version ? <span className='truncate px-2 pb-1 text-xs text-muted-foreground'>版本 {version.version}{typeof version.metadata?.duration === 'number' ? ` · ${version.metadata.duration.toFixed(1)} 秒` : version.width_px && version.height_px ? ` · ${version.width_px}×${version.height_px}` : ''}</span> : null}
      <DropdownMenu><DropdownMenuTrigger asChild><Button className='sve-asset-menu' variant='ghost' size='icon-sm' aria-label={`资产操作：${asset.name}`}><MoreHorizontal /></Button></DropdownMenuTrigger><DropdownMenuContent align='end'><DropdownMenuGroup>
        <DropdownMenuItem disabled={!selection} onSelect={() => onPreview(selection!)}><Play />预览</DropdownMenuItem>
        {asset.versions.length > 1 || !selection ? <DropdownMenuSub><DropdownMenuSubTrigger>选择版本</DropdownMenuSubTrigger><DropdownMenuSubContent><DropdownMenuRadioGroup value={version?.id ?? ''} onValueChange={id => onVersionChange(asset.id, id)}>{[...asset.versions].sort((a, b) => b.version - a.version).map(item => <DropdownMenuRadioItem key={item.id} value={item.id}>版本 {item.version}{item.version === asset.current_version ? ' · 当前版本' : ''}</DropdownMenuRadioItem>)}</DropdownMenuRadioGroup></DropdownMenuSubContent></DropdownMenuSub> : null}
        <DropdownMenuItem disabled={!selection} onSelect={() => onReference(selection!)}><MessageCircle />添加到对话</DropdownMenuItem>
        {['image', 'video', 'audio', 'document'].includes(asset.kind) ? <DropdownMenuItem disabled={!selection || !selected} onSelect={() => onAdd(selection!, 'reference')}><Plus />作为分镜参考</DropdownMenuItem> : null}
        {asset.kind === 'image' ? <DropdownMenuItem disabled={!selection || !selected} onSelect={() => onAdd(selection!, 'shot')}><Plus />作为分镜画面</DropdownMenuItem> : null}
        {asset.kind === 'video' ? <DropdownMenuItem disabled={!selection} onSelect={() => onAdd(selection!, 'video')}><Plus />{selected ? '用于选中分镜' : '创建视频分镜'}</DropdownMenuItem> : null}
        {asset.kind === 'document' ? <DropdownMenuItem disabled={!selection} onSelect={() => onAdd(selection!, 'script')}><Plus />用于完整脚本</DropdownMenuItem> : null}
        {asset.kind === 'audio' ? <DropdownMenuItem disabled={!selection || !selected} onSelect={() => onAdd(selection!, 'audio')}><Plus />添加成片音频</DropdownMenuItem> : null}
      </DropdownMenuGroup></DropdownMenuContent></DropdownMenu>
    </article>})}</div>
    {!visible.length ? <Empty><EmptyHeader><EmptyTitle>{assets.length ? '没有匹配的资产' : '导入图片、视频或音频'}</EmptyTitle></EmptyHeader></Empty> : null}
    </div>
  </div>
}
