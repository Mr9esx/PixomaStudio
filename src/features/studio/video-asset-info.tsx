import type { StudioAsset } from '@/lib/api/studio'
import { formatDateTime } from '@/lib/format'
import { Button } from '@/components/ui/button'
import { Field, FieldGroup, FieldLabel } from '@/components/ui/field'
import { Popover, PopoverContent, PopoverTrigger } from '@/components/ui/popover'
import { Select, SelectContent, SelectGroup, SelectItem, SelectTrigger, SelectValue } from '@/components/ui/select'
import { AssetDetailsInfo, AssetInfoRow, AssetSizeRow } from './studio-assets'
import { selectedVideoAsset } from './video-editor-view'

export function VideoAssetInfo({ asset, versionId, onVersionChange }: { asset: StudioAsset; versionId?: string; onVersionChange: (id: string) => void }) {
  const selection = selectedVideoAsset(asset, versionId)
  const version = selection?.versions.find(item => item.version === selection.current_version)
  return <Popover><PopoverTrigger asChild><Button size='sm' variant='ghost' aria-label='素材版本与信息'>{version ? `版本 ${version.version}` : '版本不可用'}</Button></PopoverTrigger>
    <PopoverContent className='max-h-[70vh] w-[min(320px,calc(100vw-24px))] overflow-y-auto shadow-lg'>
      <FieldGroup><Field><FieldLabel>预览版本</FieldLabel><Select value={version?.id ?? ''} onValueChange={onVersionChange}><SelectTrigger aria-label='选择素材版本'><SelectValue placeholder='选择可用版本' /></SelectTrigger><SelectContent><SelectGroup>{[...asset.versions].sort((a, b) => b.version - a.version).map(item => <SelectItem key={item.id} value={item.id}>版本 {item.version}{item.version === asset.current_version ? ' · 当前版本' : ''}</SelectItem>)}</SelectGroup></SelectContent></Select></Field>
        {version ? <AssetDetailsInfo>
          <AssetInfoRow label='名称' value={asset.name} />
          <AssetInfoRow label='格式' value={version.format || version.mime_type} />
          {version.width_px && version.height_px ? <AssetInfoRow label='尺寸' value={`${version.width_px}×${version.height_px}`} /> : null}
          {typeof version.metadata?.duration === 'number' ? <AssetInfoRow label='时长' value={`${version.metadata.duration.toFixed(3)} 秒`} /> : null}
          {typeof version.metadata?.fps === 'number' ? <AssetInfoRow label='帧率' value={`${version.metadata.fps.toLocaleString(undefined, { maximumFractionDigits: 3 })} fps`} /> : null}
          <AssetSizeRow bytes={version.size_bytes} />
          <AssetInfoRow label='创建时间' value={formatDateTime(version.created_at)} />
        </AssetDetailsInfo> : <p role='alert' className='text-sm'>资产版本已不可用，请重新选择。</p>}
      </FieldGroup>
    </PopoverContent>
  </Popover>
}
