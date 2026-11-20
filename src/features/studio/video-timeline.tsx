import { useLayoutEffect, useMemo, useRef, useState, type DragEvent } from 'react'
import { Timeline, type TimelineState } from '@xzdarcy/react-timeline-editor'
import { MoreHorizontal } from 'lucide-react'
import { Textarea } from '@/components/ui/textarea'
import { Button } from '@/components/ui/button'
import { Checkbox } from '@/components/ui/checkbox'
import { Dialog, DialogContent, DialogFooter, DialogHeader, DialogTitle } from '@/components/ui/dialog'
import { Alert, AlertTitle, AlertDescription } from '@/components/ui/alert'
import { Field, FieldGroup, FieldLabel } from '@/components/ui/field'
import { DropdownMenu, DropdownMenuContent, DropdownMenuGroup, DropdownMenuItem, DropdownMenuLabel, DropdownMenuRadioGroup, DropdownMenuRadioItem, DropdownMenuSeparator, DropdownMenuTrigger } from '@/components/ui/dropdown-menu'
import { ContextMenu, ContextMenuContent, ContextMenuGroup, ContextMenuItem, ContextMenuLabel, ContextMenuSeparator, ContextMenuTrigger } from '@/components/ui/context-menu'
import { videoTimelineFrames, videoProjectTiming, videoShotDuration, type VideoProject, type VideoShot } from './video-editor-model'
import { createTimelineRows, editTimelineProject, shotTracks, timelineRowAt, timelineScale, videoAssetDropTarget, type TimelineAction, type TimelineEdit, type VideoTimelineAction, type VideoTrackDisplay } from './video-timeline-model'
import { videoAssetDragType, videoAssetKindDragTypes } from './video-asset-library'
import type { VideoTimelineViewport } from './video-editor-view'
import { applyVideoTextDraft, recoverVideoTextDraft, type VideoTextDraft } from './video-text-draft'
import { videoScriptSourceState, videoScriptSourceLabels } from './video-script'
import '@xzdarcy/react-timeline-editor/dist/react-timeline-editor.css'

type Props = {
  batchSelection?: string[]
  onBatchSelectionChange?: (id: string, checked: boolean) => void
  project: VideoProject; selectedId: string; selectedTrack: string; selectedAudioId?: string; time: number; zoom: number; snap: boolean; fitRequest: number
  onCommit: (project: VideoProject) => void
  onSelect: (id: string, section?: string, openInspector?: boolean) => void
  onSeek: (seconds: number) => void
  onAudio: (id?: string, openInspector?: boolean) => void
  onError: (message: string) => void
  audioName: (id: string) => string
  onAssetDrop: (assetId: string, versionId: string, track: string, shotId: string | undefined, time: number) => void
  onScriptIdeas: (shotId: string) => void
  onDuplicateShot: (shotId: string) => void
  onSplitShot: (shotId: string) => void
  onStructure?: (mode: 'split' | 'merge', shotId: string) => void
  onRemoveShot: (shotId: string) => void
  canRemoveShot: (shotId: string) => boolean
  onRemoveAudio: (audioId: string) => void
  viewport: VideoTimelineViewport
  onViewportChange: (viewport: VideoTimelineViewport) => void
  display: VideoTrackDisplay
  onDisplayChange: (display: VideoTrackDisplay) => void
  textDraft?: VideoTextDraft
  onTextDraftChange: (draft: VideoTextDraft | undefined) => void
}
const effects = Object.fromEntries([...shotTracks.map(track => track.id), 'audio'].map(id => [id, { id, name: id }]))

export function VideoTimeline({ project, selectedId, selectedTrack, selectedAudioId, time, zoom, snap, fitRequest, viewport, onViewportChange, display, onDisplayChange, textDraft: inline, onTextDraftChange, onCommit, onSelect, onSeek, onAudio, onError, audioName, onAssetDrop, onScriptIdeas, onDuplicateShot, onSplitShot, onStructure, onRemoveShot, canRemoveShot, onRemoveAudio, batchSelection, onBatchSelectionChange }: Props) {
  const timeline = useRef<TimelineState>(null)
  const labels = useRef<HTMLDivElement>(null)
  const gesture = useRef<{ project: VideoProject; action: VideoTimelineAction } | undefined>(undefined)
  const scroll = useRef({ left: viewport.scrollLeft, top: viewport.scrollTop })
  const previousZoom = useRef(zoom)
  const [dropHint, setDropHint] = useState<{ top: number; height: number; allowed: boolean; label: string }>()
  const [epoch, setEpoch] = useState(0)
  const restoredEpoch = useRef(-1)
  const [draft, setDraft] = useState<VideoProject>()
  const [textConflict, setTextConflict] = useState(false)
  const [expandedShotId, setExpandedShotId] = useState<string>()
  const expandedShot = project.shots.find(shot => shot.id === expandedShotId)
  const cancelled = useRef(false)
  const latest = useRef(project); latest.current = project
  const latestInline = useRef(inline); latestInline.current = inline
  const textTarget = inline ? project.shots.find(shot => shot.id === inline.id) : undefined
  useLayoutEffect(() => {
    if (!inline) { setTextConflict(false); return }
    const shot = project.shots.find(shot => shot.id === inline.id)
    if (!shot || (shot[inline.key] !== inline.original && shot[inline.key] !== inline.value)) setTextConflict(true)
  }, [project, inline])
  function setInline(next: VideoTextDraft | undefined) { latestInline.current = next; onTextDraftChange(next) }
  const rows = useMemo(() => {
    const rows = createTimelineRows(draft ?? project, selectedId, selectedTrack, selectedAudioId, display)
    // 活动块由库控制位置，其余轨道使用同步后的业务边界。
    const moving = gesture.current?.action
    if (moving && draft) for (const row of rows) row.actions = row.actions.map(action => action.id === moving.id ? { ...moving } : action)
    return rows
  }, [project, draft, selectedId, selectedTrack, selectedAudioId, display, epoch])
  const sequence = videoProjectTiming(project).unknownCount > 0
  const previousSequence = useRef(sequence)
  const unitWidth = sequence ? 240 : 48
  const scale = sequence ? 1 : timelineScale(zoom)
  const duration = videoTimelineFrames(project.shots, project.fps).at(-1)! / project.fps
  const minScaleCount = sequence ? Math.max(1, project.shots.length + 1) : Math.max(1, Math.ceil((duration || 15) / scale) + 1)
  useLayoutEffect(() => { timeline.current?.setTime(time) }, [time, epoch])
  useLayoutEffect(() => {
    restoredEpoch.current = epoch
    scroll.current = { left: viewport.scrollLeft, top: viewport.scrollTop }
    timeline.current?.setScrollLeft(viewport.scrollLeft)
    timeline.current?.setScrollTop(viewport.scrollTop)
    if (labels.current) labels.current.scrollTop = viewport.scrollTop
  }, [viewport.scrollLeft, viewport.scrollTop, epoch])
  useLayoutEffect(() => {
    if (previousSequence.current === sequence) return
    previousSequence.current = sequence
    scroll.current.left = 0
    timeline.current?.setScrollLeft(0)
    onViewportChange({ scrollLeft: 0, scrollTop: scroll.current.top })
  }, [sequence, onViewportChange])
  useLayoutEffect(() => {
    scroll.current.left *= zoom / previousZoom.current
    previousZoom.current = zoom
    timeline.current?.setScrollLeft(scroll.current.left)
  }, [zoom])
  useLayoutEffect(() => { if (fitRequest === 0) return; scroll.current.left = 0; timeline.current?.setScrollLeft(0) }, [fitRequest])
  function begin(action: TimelineAction) {
    const item = action as VideoTimelineAction
    gesture.current = { project: latest.current, action: { ...item } }
    if (item.shotId) onSelect(item.shotId, item.effectId, false)
    else onAudio(item.audioId, false)
  }
  function edit(action: TimelineAction, start: number, end: number, kind: TimelineEdit['kind']): TimelineEdit {
    const item = action as VideoTimelineAction
    return { shotId: item.shotId, audioId: item.audioId, start, end, kind }
  }
  function preview(change: TimelineEdit) {
    const baseline = gesture.current?.project
    if (!baseline || latest.current !== baseline) return false
    try { setDraft(editTimelineProject(baseline, change)); return true } catch { return false }
  }
  function finish(change: TimelineEdit) {
    const baseline = gesture.current?.project
    let reset = true
    try {
      if (!baseline || latest.current !== baseline) throw new Error('项目已更新，请重新进行时间轴编辑')
      const next = editTimelineProject(baseline, change)
      if (JSON.stringify(next) !== JSON.stringify(baseline)) { onCommit(next); reset = false }
    } catch (cause) { onError(String(cause)) }
    gesture.current = undefined; setDraft(undefined); if (reset) setEpoch(value => value + 1)
  }
  function closeText() { cancelled.current = true; setInline(undefined); setTextConflict(false) }
  function textEdit(shot: VideoShot, key: 'script' | 'subtitle') {
    if (inline?.id === shot.id && inline.key === key) return
    if (textConflict) return
    let current = latest.current
    if (inline) {
      try { const next = applyVideoTextDraft(current, inline); if (next !== current) onCommit(next); current = next }
      catch (cause) { setTextConflict(true); onError(String(cause)); return }
    }
    const target = current.shots.find(value => value.id === shot.id)
    if (!target) return
    if (display.collapsedTracks.includes(key)) onDisplayChange({ ...display, collapsedTracks: display.collapsedTracks.filter(id => id !== key) })
    cancelled.current = false; setInline({ id: target.id, name: target.name, key, value: target[key], original: target[key] })
  }
  function openEdit(item: VideoTimelineAction, track: string) {
    const shot = latest.current.shots.find(value => value.id === item.shotId)
    if (!shot) { if (item.audioId) onAudio(item.audioId, true); return }
    onSelect(shot.id, track, false)
    if (track === 'shot') { setExpandedShotId(shot.id); return }
    if (track === 'script' && !shot.script.trim()) onScriptIdeas(shot.id)
    else if (track === 'script' || track === 'subtitle') textEdit(shot, track)
    else onSelect(shot.id, track, true)
  }
  function commitText() {
    if (!inline) return
    try { const next = applyVideoTextDraft(latest.current, inline, true); if (next !== latest.current) onCommit(next); closeText() }
    catch (cause) { onError(String(cause)) }
  }
  function saveText() {
    if (!inline || cancelled.current || latestInline.current !== inline || textConflict) return
    const shot = latest.current.shots.find(shot => shot.id === inline.id)
    if (!shot || (shot[inline.key] !== inline.original && shot[inline.key] !== inline.value)) { setTextConflict(true); return }
    try { const next = applyVideoTextDraft(latest.current, inline); if (next !== latest.current) onCommit(next); closeText() }
    catch (cause) { onError(String(cause)) }
  }
  function recoverText() {
    if (!inline) return
    try {
      const recovered = recoverVideoTextDraft(latest.current, inline)
      onCommit(recovered.project); onSelect(recovered.shotId, inline.key, true); closeText()
    } catch (cause) { onError(String(cause)) }
  }
  function dropPosition(event: DragEvent<HTMLDivElement>) {
    const rect = event.currentTarget.getBoundingClientRect()
    if (event.clientX < rect.left + 96 || event.clientY < rect.top + 32) return
    const position = timelineRowAt(rows, event.clientY - rect.top - 32 + scroll.current.top)
    if (!position) return
    return { ...position, top: position.top - scroll.current.top, seconds: Math.max(0, (event.clientX - rect.left - 96 - 12 + scroll.current.left) / (unitWidth * zoom)) }
  }
  return <div className='sve-timeline' aria-label='视频时间轴' data-timeline-library='react-timeline-editor' data-mode={sequence ? 'sequence' : 'time'} data-playhead={sequence ? undefined : time}
    onDragOver={event => {
      if (!event.dataTransfer.types.includes(videoAssetDragType)) return
      event.preventDefault()
      const position = dropPosition(event)
      if (!position) { event.dataTransfer.dropEffect = 'none'; setDropHint(undefined); return }
      const kind = Object.entries(videoAssetKindDragTypes).find(([, type]) => event.dataTransfer.types.includes(type))?.[0] ?? ''
      const target = videoAssetDropTarget(project, kind, position.row.id, position.seconds)
      event.dataTransfer.dropEffect = target.allowed ? 'copy' : 'none'
      setDropHint(previous => previous?.top === position.top && previous.height === position.height && previous.allowed === target.allowed && previous.label === target.label ? previous : { top: position.top, height: position.height, allowed: target.allowed, label: target.label })
    }}
    onDragLeave={event => { if (!event.currentTarget.contains(event.relatedTarget as Node | null)) setDropHint(undefined) }}
    onDrop={event => {
      if (!event.dataTransfer.types.includes(videoAssetDragType)) return
      event.preventDefault(); setDropHint(undefined)
      try {
        const payload = JSON.parse(event.dataTransfer.getData(videoAssetDragType)) as { assetId: string; versionId: string; kind: string }
        if (!payload || typeof payload.assetId !== 'string' || typeof payload.versionId !== 'string' || typeof payload.kind !== 'string') throw new Error('拖动的资产信息无效')
        const position = dropPosition(event)
        if (!position) throw new Error('将资产放入对应轨道')
        const target = videoAssetDropTarget(project, payload.kind, position.row.id, position.seconds)
        if (!target.allowed) throw new Error(target.label)
        onAssetDrop(payload.assetId, payload.versionId, position.row.id, target.shotId, target.time)
      } catch (cause) { onError(String(cause)) }
    }}>
    <div className='sve-timeline-labels'><div className='sve-timeline-label-header'>轨道</div><div className='sve-timeline-label-scroll' ref={labels}><div>{rows.map(row => {
      const name = shotTracks.find(track => track.id === row.id)?.label ?? (row.actions[0]?.audioId ? audioName(row.actions[0].audioId) : '成片音频')
      const collapsed = display.collapsedTracks.includes(row.id)
      return <div className='sve-timeline-label' key={row.id} style={{ height: row.rowHeight }}>
        <DropdownMenu><DropdownMenuTrigger asChild><Button variant='ghost' size='sm' className='sve-track-menu' disabled={Boolean(inline) || Boolean(draft)} aria-label={`${name}轨道显示设置${collapsed ? '，已收起' : ''}`}><span className='truncate'>{name}</span><MoreHorizontal className='size-3 shrink-0' /></Button></DropdownMenuTrigger>
          <DropdownMenuContent align='start'><DropdownMenuGroup><DropdownMenuLabel>{name}</DropdownMenuLabel>
            <DropdownMenuItem onSelect={() => onDisplayChange({ ...display, collapsedTracks: collapsed ? display.collapsedTracks.filter(id => id !== row.id) : [...display.collapsedTracks, row.id] })}>{collapsed ? '展开轨道' : '收起轨道'}</DropdownMenuItem>
            <DropdownMenuSeparator /><DropdownMenuLabel>展开高度</DropdownMenuLabel>
            <DropdownMenuRadioGroup value={String(display.trackHeights[row.id] ?? (row.id === 'shot' ? 128 : 42))} onValueChange={value => onDisplayChange({ trackHeights: { ...display.trackHeights, [row.id]: Number(value) }, collapsedTracks: display.collapsedTracks.filter(id => id !== row.id) })}>
              <DropdownMenuRadioItem value='42'>紧凑</DropdownMenuRadioItem><DropdownMenuRadioItem value='128'>展开内容</DropdownMenuRadioItem><DropdownMenuRadioItem value='160'>更多内容</DropdownMenuRadioItem>
            </DropdownMenuRadioGroup>
          </DropdownMenuGroup></DropdownMenuContent>
        </DropdownMenu>
      </div>
    })}</div></div></div>
    <Timeline key={epoch} ref={timeline} style={{ width: '100%', height: '100%' }} editorData={rows} effects={effects}
      scale={scale} scaleWidth={unitWidth * zoom * scale} scaleSplitCount={sequence ? 1 : scale === 1 ? project.fps : Math.min(10, scale)} hideCursor={sequence} startLeft={12} minScaleCount={minScaleCount} maxScaleCount={sequence ? Math.max(minScaleCount, 101) : Math.max(minScaleCount, Math.ceil(3600 / scale) + 1)}
      rowHeight={42} gridSnap={snap} dragLine={snap} autoScroll enableRowDrag={false} autoReRender={false} disableDrag={Boolean(inline)}
      getAssistDragLineActionIds={({ action, editorData }) => editorData.flatMap(row => row.actions.filter(item => item.id !== action.id && (!(action as VideoTimelineAction).shotId || (item as VideoTimelineAction).shotId !== (action as VideoTimelineAction).shotId)).map(item => item.id))}
      onScroll={({ scrollLeft, scrollTop }) => { if (restoredEpoch.current !== epoch) return; scroll.current = { left: scrollLeft, top: scrollTop }; if (labels.current) labels.current.scrollTop = scrollTop; if (scrollLeft !== viewport.scrollLeft || scrollTop !== viewport.scrollTop) onViewportChange({ scrollLeft, scrollTop }) }}
      onChange={() => false}
      onClickTimeArea={seconds => { if (sequence) { const shot = project.shots[Math.floor(seconds)]; if (shot) onSelect(shot.id, 'shot', false) } else onSeek(seconds); return false }} onCursorDrag={onSeek} onCursorDragEnd={seconds => {
        onSeek(seconds)
        const lastFrame = Math.max(0, videoTimelineFrames(project.shots, project.fps).at(-1)! - 1)
        const target = Math.max(0, Math.min(lastFrame, Math.round(seconds * project.fps))) / project.fps
        requestAnimationFrame(() => timeline.current?.setTime(target))
      }}
      onActionMoveStart={({ action }) => begin(action)} onActionResizeStart={({ action }) => begin(action)}
      onActionMoving={({ action, start, end }) => preview(edit(action, start, end, 'move'))}
      onActionResizing={({ action, start, end, dir }) => preview(edit(action, start, end, dir))}
      onActionMoveEnd={({ action, start, end }) => finish(edit(action, start, end, 'move'))}
      onActionResizeEnd={({ action, start, end, dir }) => finish(edit(action, start, end, dir))}
      onClickActionOnly={(_, { action, row }) => { const item = action as VideoTimelineAction; if (item.shotId) onSelect(item.shotId, row.id, false); else onAudio(item.audioId, false) }}
      onDoubleClickAction={(_, { action, row }) => openEdit(action as VideoTimelineAction, row.id)}
      onClickRow={(_, { row }) => { if (row.id.startsWith('audio')) onAudio(undefined, false) }}
      getScaleRender={seconds => <span>{sequence ? seconds < project.shots.length ? `分镜 ${Math.floor(seconds) + 1}` : '' : `${seconds}s`}</span>}
      getActionRender={(action, row) => {
        const item = action as VideoTimelineAction
        const shot = (draft ?? project).shots.find(shot => shot.id === item.shotId)
        const track = shotTracks.find(track => track.id === row.id)
        const collapsed = display.collapsedTracks.includes(row.id)
        if (!shot) return <ContextMenu onOpenChange={open => { if (open) onAudio(item.audioId, false) }}><ContextMenuTrigger asChild><button className={`sve-block${item.selected ? ' is-selected' : ''}`} data-collapsed={collapsed} aria-pressed={Boolean(item.selected)} aria-label={`选择音频：${audioName(item.audioId!)}`} onKeyDown={event => {
          if (event.key === 'Enter') { event.preventDefault(); openEdit(item, row.id); return }
          if (!['ArrowLeft', 'ArrowRight'].includes(event.key)) return
          event.preventDefault(); const delta = (event.key === 'ArrowLeft' ? -1 : 1) / project.fps
          begin(item); finish(edit(item, item.start + (event.shiftKey ? 0 : delta), item.end + delta, event.shiftKey ? 'right' : 'move'))
        }}>{audioName(item.audioId!)} · {(item.end - item.start).toFixed(1)}s</button></ContextMenuTrigger><ContextMenuContent onClick={event => event.stopPropagation()}><ContextMenuGroup><ContextMenuLabel>{audioName(item.audioId!)}</ContextMenuLabel><ContextMenuItem onSelect={() => openEdit(item, row.id)}>编辑音频</ContextMenuItem><ContextMenuItem onSelect={() => onSeek(item.start)}>定位到音频开头</ContextMenuItem><ContextMenuSeparator /><ContextMenuItem onSelect={() => onRemoveAudio(item.audioId!)}>移除音频</ContextMenuItem></ContextMenuGroup></ContextMenuContent></ContextMenu>
        return <ContextMenu onOpenChange={open => { if (open) onSelect(shot.id, row.id, false) }}><ContextMenuTrigger asChild><div className='sve-timeline-content' data-shot-id={shot.id} data-track={row.id} data-start={action.start} data-end={action.end}>
          {row.id === 'shot' && onBatchSelectionChange ? <div className='absolute right-2 top-2 z-10' onMouseDown={event => event.stopPropagation()} onClick={event => event.stopPropagation()}><Checkbox aria-label={`选择批量分镜：${shot.name}`} checked={batchSelection?.includes(shot.id) ?? false} onCheckedChange={checked => onBatchSelectionChange(shot.id, checked === true)} /></div> : null}
          <button className={`sve-block${item.selected ? ' is-selected' : ''}`} data-collapsed={collapsed} data-expanded={!collapsed && row.rowHeight! > 42} aria-label={`${track?.label}：${shot.name}`} aria-pressed={Boolean(item.selected)}
            onKeyDown={event => {
              if (event.key === 'Enter') { event.preventDefault(); openEdit(item, row.id); return }
              if (sequence && ['ArrowLeft', 'ArrowRight'].includes(event.key)) {
                event.preventDefault()
                const index = project.shots.findIndex(value => value.id === shot.id)
                const target = index + (event.key === 'ArrowLeft' ? -1 : 1)
                if (event.shiftKey && target >= 0 && target < project.shots.length) { begin(item); finish(edit(item, target, target + 1, 'move')) }
                else if (!event.shiftKey && project.shots[target]) onSelect(project.shots[target].id, row.id, false)
                return
              }
              if (row.id !== 'video' || !['ArrowLeft', 'ArrowRight'].includes(event.key) || (!shot.media && !event.shiftKey)) return
              event.preventDefault(); const delta = (event.key === 'ArrowLeft' ? -1 : 1) / project.fps
              begin(item)
              if (event.altKey) finish(edit(item, item.start + delta, item.end, 'left'))
              else if (event.shiftKey) { const index = project.shots.findIndex(value => value.id === shot.id); const target = index + (delta < 0 ? -1 : 1); if (target >= 0 && target < project.shots.length) { const start = videoTimelineFrames(project.shots.filter(value => value.id !== shot.id).slice(0, target), project.fps).at(-1)! / project.fps; finish(edit(item, start, start + videoShotDuration(shot)!, 'move')) } else { gesture.current = undefined } }
              else finish(edit(item, item.start, item.end + delta, 'right'))
            }}>
            {collapsed ? shot.name : row.id === 'shot' ? <span className='flex h-full min-w-0 flex-col gap-2 overflow-hidden py-2'><strong className='truncate'>{project.shots.findIndex(item => item.id === shot.id) + 1}. {shot.name}</strong><span className='line-clamp-4 whitespace-pre-wrap text-xs text-muted-foreground'>{shot.storyboard || '填写画面描述'}</span>{project.script && ['changed', 'missing', 'restructured'].includes(videoScriptSourceState(shot, project.script)) ? <span className='text-xs text-muted-foreground'>{videoScriptSourceLabels[videoScriptSourceState(shot, project.script)]}</span> : null}</span> : row.id === 'reference' ? `${shot.references?.length ?? 0} 个参考` : `${shot.media ? '已采用视频' : '未采用视频'} · ${videoShotDuration(shot) === undefined ? '时长未定' : `${shot.media ? '' : '计划 '}${videoShotDuration(shot)!.toFixed(1)} 秒`}`}
          </button>
          {inline?.id === shot.id && inline.key === row.id ? <Textarea autoFocus aria-label={`编辑${track?.label}`} className='sve-inline' value={inline.value} onMouseDown={event => event.stopPropagation()} onClick={event => event.stopPropagation()} onDoubleClick={event => event.stopPropagation()} onChange={event => setInline({ ...inline, value: event.target.value })} onBlur={saveText} onKeyDown={event => { event.stopPropagation(); if (event.key === 'Escape') closeText() }} /> : null}
        </div></ContextMenuTrigger><ContextMenuContent onClick={event => event.stopPropagation()}><ContextMenuGroup>
          <ContextMenuLabel>{shot.name} · {track?.label}</ContextMenuLabel>
          <ContextMenuItem onSelect={() => openEdit(item, row.id)}>{row.id === 'shot' ? '展开分镜' : `编辑${track?.label}`}</ContextMenuItem>
          <ContextMenuItem onSelect={() => sequence ? onSelect(shot.id, row.id, false) : onSeek(item.start)}>定位到分镜开头</ContextMenuItem>
          <ContextMenuSeparator />
          <ContextMenuItem onSelect={() => onDuplicateShot(shot.id)}>复制分镜</ContextMenuItem>
          {onStructure ? <><ContextMenuItem disabled={!canRemoveShot(shot.id)} onSelect={() => onStructure('split', shot.id)}>分成多个分镜</ContextMenuItem><ContextMenuItem disabled={!canRemoveShot(shot.id) || project.shots.length < 2} onSelect={() => onStructure('merge', shot.id)}>合并分镜</ContextMenuItem></> : null}
          <ContextMenuItem disabled={sequence || !canRemoveShot(shot.id) || time <= item.start || time >= item.end} onSelect={() => onSplitShot(shot.id)}>在播放位置分割分镜</ContextMenuItem>
          <ContextMenuItem disabled={!canRemoveShot(shot.id)} onSelect={() => onRemoveShot(shot.id)}>删除整个分镜</ContextMenuItem>
        </ContextMenuGroup></ContextMenuContent></ContextMenu>
      }} />
    <div className='sve-drop-layer'>
      {dropHint ? <div className='sve-drop-hint' style={{ top: dropHint.top, height: dropHint.height }} data-allowed={dropHint.allowed} role='status'>{dropHint.label}</div> : null}
    </div>
    <Dialog open={Boolean(expandedShot)} onOpenChange={open => { if (!open) setExpandedShotId(undefined) }}><DialogContent className='max-h-[85vh] overflow-y-auto shadow-lg sm:max-w-xl' aria-describedby={undefined}>
      <DialogHeader><DialogTitle>{expandedShot?.name}</DialogTitle></DialogHeader>
      <p className='whitespace-pre-wrap text-sm'>{expandedShot?.storyboard || '尚未填写画面描述'}</p>
      {expandedShot?.scriptSources?.length ? <div className='flex flex-col gap-2 rounded-md border bg-muted p-3'><strong className='text-sm'>脚本原文</strong>{expandedShot.scriptSources.map(source => <p key={source.blockId} className='whitespace-pre-wrap text-sm'>{source.text}</p>)}</div> : null}
      <DialogFooter><Button size='sm' variant='outline' onClick={() => setExpandedShotId(undefined)}>关闭</Button><Button size='sm' onClick={() => { if (expandedShot) onSelect(expandedShot.id, 'shot', true); setExpandedShotId(undefined) }}>编辑分镜</Button></DialogFooter>
    </DialogContent></Dialog>
    {inline ? <Alert className='sve-text-conflict' aria-label='待保存文字' onKeyDown={event => event.stopPropagation()}>
      <AlertTitle>{!textTarget ? `「${inline.name}」已删除` : textConflict ? `${inline.key === 'subtitle' ? '字幕' : '脚本'}已被其他入口更新` : '待保存文字'}</AlertTitle>
      <AlertDescription><FieldGroup>
        {textTarget ? <Field><FieldLabel htmlFor='timeline-current-text'>当前内容</FieldLabel><Textarea id='timeline-current-text' className='max-h-40 resize-y' readOnly value={textTarget[inline.key]} /></Field> : null}
        <Field><FieldLabel htmlFor='timeline-draft-text'>草稿</FieldLabel><Textarea id='timeline-draft-text' className='max-h-40 resize-y' value={inline.value} onChange={event => setInline({ ...inline, value: event.target.value })} /></Field>
        <div className='flex flex-wrap gap-2'><Button size='sm' variant='outline' onClick={closeText}>{textTarget ? '使用最新内容' : '放弃草稿'}</Button><Button size='sm' onClick={textTarget ? commitText : recoverText}>{textTarget ? '保留当前输入' : '保存为新分镜'}</Button></div>
      </FieldGroup></AlertDescription>
    </Alert> : null}
  </div>
}
