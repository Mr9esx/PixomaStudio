import { useState } from 'react'
import { useMediaResource } from '@/features/cases/lib/use-media-object-url'
import type { StudioAsset } from '@/lib/api/studio'
import { AssetKindIcon } from './studio-assets'

export function VideoAssetThumbnail({ asset, label }: { asset: StudioAsset; label: string }) {
  const version = asset.versions.find(item => item.version === asset.current_version)
  const image = asset.kind === 'image' ? version?.content_url : undefined
  const media = useMediaResource(image)
  const src = media.src ?? version?.poster_url
  const [failedSource, setFailedSource] = useState<string>()
  const failed = Boolean(media.error || (src && failedSource === src))
  return <span className='sve-thumbnail'>
    {failed ? <span className='text-xs text-muted-foreground' title={media.error?.message || '缩略图无法显示'}>读取失败</span> : src ? <img src={src} alt={label} draggable={false} onError={() => setFailedSource(src)} /> : <AssetKindIcon kind={asset.kind} aria-hidden='true' />}
  </span>
}
