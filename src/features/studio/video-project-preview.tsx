import { useCallback, useEffect, useRef, useState } from 'react'
import { createPortal } from 'react-dom'
import { useQueryClient } from '@tanstack/react-query'
import { Player, type PlayerRef } from '@remotion/player'
import { AbsoluteFill, Html5Audio, Html5Video, Sequence, useBufferState, useVideoConfig } from 'remotion'
import { useMediaObjectUrl } from '@/features/cases/lib/use-media-object-url'
import { resolveMediaKey } from '@/lib/api/media'
import { videoTimelineFrames, type VideoAudio, type VideoProject, type VideoShot } from './video-editor-model'
import { VideoMediaError } from './video-media-error'
import { Button } from '@/components/ui/button'

function useBufferedMediaUrl(value?: string) {
  const src = useMediaObjectUrl(value)
  const buffer = useBufferState()
  useEffect(() => {
    if (value === undefined || src) return
    const handle = buffer.delayPlayback()
    return () => handle.unblock()
  }, [value, src, buffer])
  if (value !== undefined && !resolveMediaKey(value)) throw new Error('素材地址无效')
  return src
}

function Clip({ shot, fps }: { shot: VideoShot; fps: number }) {
  const src = useBufferedMediaUrl(shot.media?.url)
  const { height } = useVideoConfig()
  return <AbsoluteFill className='items-center justify-center' style={{ colorScheme: 'light', backgroundColor: 'CanvasText', color: 'Canvas' }}>
    {src ? <Html5Video src={src} volume={shot.volume ?? 1} pauseWhenBuffering trimBefore={Math.round(shot.sourceIn * fps)} style={{ width: '100%', height: '100%', objectFit: 'contain' }} /> : !shot.media ? <span>{shot.name} · 尚未采用视频</span> : null}
    {shot.subtitle ? <div style={{ position: 'absolute', left: '50%', bottom: Math.floor(height / 15), transform: 'translateX(-50%)', width: 'max-content', fontFamily: '"Heiti SC", sans-serif', fontSize: Math.floor(height / 25), lineHeight: 1, whiteSpace: 'pre', textAlign: 'left', color: 'Canvas', WebkitTextStroke: '2px color-mix(in srgb, CanvasText 65%, transparent)', paintOrder: 'stroke fill' }}>{shot.subtitle}</div> : null}
  </AbsoluteFill>
}

function AudioClip({ audio, fps }: { audio: VideoAudio; fps: number }) {
  const src = useBufferedMediaUrl(audio.url)
  return src ? <Sequence from={-Math.round((audio.playbackOffset ?? 0) * fps)} layout='none'><Html5Audio src={src} volume={audio.volume} loop={audio.loop} pauseWhenBuffering trimBefore={Math.round(audio.sourceIn * fps)} /></Sequence> : null
}

function Composition({ project }: { project: VideoProject }) {
  const frames = videoTimelineFrames(project.shots, project.fps)
  return <AbsoluteFill>{project.shots.map((shot, index) => frames[index + 1] > frames[index] ? <Sequence key={shot.id} from={frames[index]} durationInFrames={frames[index + 1] - frames[index]}><Clip shot={shot} fps={project.fps} /></Sequence> : null)}{project.audio?.map(audio => <Sequence key={audio.id} from={Math.round(audio.start * project.fps)} durationInFrames={Math.max(1, Math.round(audio.duration * project.fps))}><AudioClip audio={audio} fps={project.fps} /></Sequence>)}</AbsoluteFill>
}

function PreviewError({ container, message, reloading, fullscreen, onShown, onRetry, onExit }: {
  container: HTMLDivElement | null; message: string; reloading: boolean; fullscreen: boolean
  onShown: (shown: boolean) => void; onRetry: () => void; onExit: () => void
}) {
  useEffect(() => { onShown(true) }, [onShown])
  return container ? createPortal(<div className='absolute inset-0 z-10 flex items-center justify-center overflow-auto bg-background text-foreground' onPointerDown={event => event.stopPropagation()} onClick={event => event.stopPropagation()} onKeyDown={event => event.stopPropagation()}><div className='flex min-w-0 flex-col items-center gap-2 p-2'><VideoMediaError message={message} loading={reloading} onRetry={onRetry} />{fullscreen ? <Button size='sm' variant='outline' aria-label='退出全屏预览' onClick={onExit}>退出全屏</Button> : null}</div></div>, container) : null
}

export function VideoProjectPreview({ project, frame, loop, playerRef }: { project: VideoProject; frame: number; loop: boolean; playerRef: (player: PlayerRef | null) => void }) {
  const client = useQueryClient()
  const [epoch, setEpoch] = useState(0)
  const [reloading, setReloading] = useState(false)
  const [hasError, setHasError] = useState(false)
  const [container, setContainer] = useState<HTMLDivElement | null>(null)
  const [fullscreen, setFullscreen] = useState(false)
  const attachedPlayer = useRef<PlayerRef | null>(null)
  const updateFullscreen = useCallback((event: { detail: { isFullscreen: boolean } }) => setFullscreen(event.detail.isFullscreen), [])
  const attachPlayer = useCallback((player: PlayerRef | null) => {
    attachedPlayer.current?.removeEventListener('fullscreenchange', updateFullscreen)
    attachedPlayer.current = player
    player?.addEventListener('fullscreenchange', updateFullscreen)
    setFullscreen(player?.isFullscreen() ?? false)
    setContainer(player?.getContainerNode() ?? null)
    playerRef(player)
  }, [playerRef, updateFullscreen])
  const frames = videoTimelineFrames(project.shots, project.fps).at(-1)!
  async function reload() {
    if (reloading) return
    setReloading(true)
    try {
      const keys = [...new Set([...project.shots.map(shot => shot.media?.url), ...(project.audio ?? []).map(audio => audio.url)].map(resolveMediaKey).filter((key): key is string => Boolean(key)))]
      await Promise.all(keys.map(key => client.resetQueries({ queryKey: ['studio', 'media-preview', key], exact: true })))
      setHasError(false)
      setEpoch(value => value + 1)
    } finally { setReloading(false) }
  }
  if (!frames) return <span className='text-sm text-muted-foreground'>{project.shots[0]?.name} · 未采用视频</span>
  return <Player key={epoch} ref={attachPlayer} component={Composition} inputProps={{ project }} initialFrame={Math.max(0, Math.min(frame, frames - 1))}
    errorFallback={({ error }) => <PreviewError container={container} message={error.message} reloading={reloading} fullscreen={fullscreen} onShown={setHasError} onRetry={() => { void reload() }} onExit={() => attachedPlayer.current?.exitFullscreen()} />}
    showPosterWhenBuffering showPosterWhenBufferingAndPaused posterFillMode='player-size' renderPoster={({ isBuffering }) => isBuffering ? <div className='pointer-events-none absolute inset-0 flex items-center justify-center'><span role='status' className='rounded border bg-popover px-2 py-1 text-xs text-popover-foreground'>正在读取素材</span></div> : null}
    durationInFrames={Math.max(1, frames)} fps={project.fps} compositionWidth={project.width} compositionHeight={project.height} controls={fullscreen && !hasError} loop={loop}
    style={{ width: '100%', maxHeight: '100%', aspectRatio: `${project.width}/${project.height}` }} />
}
