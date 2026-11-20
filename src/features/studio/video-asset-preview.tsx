import { useRef, useState, type SyntheticEvent } from 'react'
import { useMediaResource } from '@/features/cases/lib/use-media-object-url'
import type { StudioAsset } from '@/lib/api/studio'
import { AssetPreview as LibraryAssetPreview } from './studio-assets'
import { videoAssetPreviewTime } from './video-editor-view'
import { VideoMediaError } from './video-media-error'

export function VideoAssetPreview({ asset, time, onTimeChange }: { asset: StudioAsset; time: number; onTimeChange: (time: number) => void }) {
  const version = asset.versions.find(item => item.version === asset.current_version)
  const media = useMediaResource(['image', 'video', 'audio'].includes(asset.kind) ? version?.content_url : undefined)
  const src = media.src
  const ready = useRef(false)
  const [error, setError] = useState('')
  const restoreTime = (event: SyntheticEvent<HTMLMediaElement>) => {
    const media = event.currentTarget
    if (!Number.isFinite(media.duration) || media.duration < 0) { setError('无法读取素材时长'); return }
    media.currentTime = videoAssetPreviewTime(time, media.duration)
    ready.current = true
    onTimeChange(media.currentTime)
  }
  const updateTime = (event: SyntheticEvent<HTMLMediaElement>) => {
    if (ready.current) onTimeChange(event.currentTarget.currentTime)
  }
  const mediaEvents = { onLoadStart: () => { ready.current = false }, onLoadedMetadata: restoreTime, onTimeUpdate: updateTime, onSeeked: updateTime, onPause: updateTime, onError: () => { ready.current = false; setError('素材无法播放') } }
  const failure = media.error?.message || error
  if (failure) return <VideoMediaError message={failure} loading={media.isFetching} onRetry={() => { ready.current = false; void media.refetch().then(result => { if (result.isSuccess) setError('') }) }} />
  if (asset.kind === 'document' && version) return <div className='size-full overflow-auto bg-background text-foreground'><LibraryAssetPreview asset={{ ...asset, versions: [version] }} preview expanded /></div>
  if (!['image', 'video', 'audio', 'document'].includes(asset.kind)) return <span className='sve-muted'>该资产不支持内容预览</span>
  if (!src) return <span className='sve-muted'>正在读取资产</span>
  if (asset.kind === 'image') return <img className='h-full max-h-full max-w-full object-contain' src={src} alt={asset.name} onError={() => setError('图片无法显示')} />
  if (asset.kind === 'video') return <video className='h-full max-h-full max-w-full' src={src} controls aria-label={asset.name} {...mediaEvents} />
  if (asset.kind === 'audio') return <audio src={src} controls aria-label={asset.name} {...mediaEvents} />
  return <span className='sve-muted'>{asset.name}</span>
}
