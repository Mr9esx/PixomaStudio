import { AbsoluteFill, Html5Audio, Html5Video, Sequence, interpolate, useCurrentFrame } from 'remotion'
import { layoutShots } from './model.mjs'
import { shotLifecycle } from './lifecycle.mjs'

export function SceneArt({ scene = 'mountain', frame = 0 }: { scene?: string; frame?: number }) {
  const drift = Math.sin(frame / 70) * 8
  return <svg viewBox='0 0 960 540' preserveAspectRatio='xMidYMid slice' aria-label={scene === 'mountain' ? '山间晨雾示意图' : scene === 'tea' ? '新茶叶片示意图' : '茶盏示意图'}>
    <rect width='960' height='540' fill='var(--secondary)' />
    <circle cx='730' cy='115' r='48' fill='var(--card)' opacity='.8' />
    <path d='M0 290L130 180 230 235 370 110 520 238 660 160 800 240 960 150V540H0Z' fill='var(--muted-foreground)' opacity='.12' />
    <path d='M0 360L150 270 270 320 460 215 610 340 780 260 960 350V540H0Z' fill='var(--muted-foreground)' opacity='.2' />
    <path d='M0 440Q180 300 360 410T700 405T1000 385V540H0Z' fill='var(--foreground)' opacity='.12' />
    <g transform={`translate(${drift} 0)`} fill='none' stroke='var(--card)' strokeWidth='18' opacity='.45'>
      <path d='M-60 300Q180 270 420 300T1030 285' /><path d='M-30 390Q200 360 460 385T1040 360' />
    </g>
    {scene === 'tea' ? <g transform='translate(450 320) rotate(-22)'>
      <path d='M-230 120Q-150-210 150-180 190 100-230 120Z' fill='var(--muted-foreground)' opacity='.8' />
      <path d='M-230 120L150-180M-110 20L-130-95M-25-42L-40-160M50-100L150-45M-130 45L-10 80' fill='none' stroke='var(--card)' strokeWidth='3' opacity='.55' />
      <ellipse cx='30' cy='-80' rx='13' ry='17' fill='var(--card)' opacity='.8' />
    </g> : scene === 'cup' ? <g>
      <ellipse cx='500' cy='452' rx='190' ry='27' fill='var(--foreground)' opacity='.12' />
      <path d='M345 315Q350 465 500 465T655 315Z' fill='var(--card)' />
      <ellipse cx='500' cy='315' rx='155' ry='34' fill='var(--border)' /><ellipse cx='500' cy='315' rx='137' ry='24' fill='var(--muted-foreground)' opacity='.5' />
      <path d='M650 340C755 300 755 420 630 410' fill='none' stroke='var(--card)' strokeWidth='20' />
      <path d={`M475 260Q${430 + drift} 220 475 180T475 90M535 260Q${575 - drift} 215 535 170`} fill='none' stroke='var(--card)' strokeWidth='9' opacity='.6' />
    </g> : <g fill='var(--foreground)' opacity='.32'><path d='M525 390L600 315 680 390Z' /><rect x='546' y='390' width='115' height='58' /><rect x='588' y='405' width='27' height='43' fill='var(--secondary)' /></g>}
  </svg>
}

function ShotVisual({ shot, subtitles }: { shot: any; subtitles: boolean }) {
  const frame = useCurrentFrame()
  const scale = interpolate(frame, [0, shot.duration], [1, 1.045])
  const state = shotLifecycle(shot)
  if (!state.canPreview) return <AbsoluteFill className='composition-empty'><span>{state.label}</span><small>{shot.name}</small></AbsoluteFill>
  return <AbsoluteFill>
    {shot.media ? <Html5Video src={shot.media.src} trimBefore={shot.sourceIn} playbackRate={shot.speed} muted={shot.muted !== false} style={{ width: '100%', height: '100%', objectFit: 'cover' }} /> : <AbsoluteFill style={{ transform: `scale(${scale})` }}><SceneArt scene={shot.scene} frame={frame} /></AbsoluteFill>}
    {subtitles && shot.subtitle ? <div className='composition-subtitle'>{shot.subtitle}</div> : null}
  </AbsoluteFill>
}

export function Composition({ project }: { project: any }) {
  if (!project.shots.length) return <AbsoluteFill className='composition-empty'>暂无分镜</AbsoluteFill>
  return <AbsoluteFill style={{ background: 'var(--background)' }}>
    {layoutShots(project.shots).map((shot: any) => <Sequence key={shot.id} from={shot.from} durationInFrames={shot.duration}><ShotVisual shot={shot} subtitles={project.subtitles} /></Sequence>)}
    {project.audio?.src ? <Html5Audio src={project.audio.src} volume={0.45} /> : null}
  </AbsoluteFill>
}
