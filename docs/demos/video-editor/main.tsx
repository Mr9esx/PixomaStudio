import { useEffect, useMemo, useRef, useState, type CSSProperties, type ReactNode } from 'react'
import { createRoot } from 'react-dom/client'
import { Player, type PlayerRef } from '@remotion/player'
import { Square } from 'lucide-react'
import { ArrowDown, ArrowDownToLine, ArrowLeft, ArrowRight, ArrowUp, AudioLines, Bot, Check, ChevronDown, ChevronRight, Copy, Download, FileText, Film, Folder, FolderOpen, ImageIcon, Library, Maximize2, MessageCircle, MessageSquarePlus, Minimize2, Moon, MoreHorizontal, PanelLeftClose, PanelLeftOpen, Pause, Play, Plus, Redo2, RotateCcw, Scissors, Settings2, ShieldCheck, SkipBack, Sparkles, Sun, Trash2, Undo2, Upload, Video, Volume2, VolumeX, Workflow, X, ZoomIn, ZoomOut } from 'lucide-react'
import { Button } from '@/components/ui/button'
import { Badge } from '@/components/ui/badge'
import { Input } from '@/components/ui/input'
import { Textarea } from '@/components/ui/textarea'
import { Field, FieldGroup, FieldLabel } from '@/components/ui/field'
import { Tabs, TabsContent, TabsList, TabsTrigger } from '@/components/ui/tabs'
import { Dialog, DialogContent, DialogDescription, DialogFooter, DialogHeader, DialogTitle } from '@/components/ui/dialog'
import { DropdownMenu, DropdownMenuContent, DropdownMenuGroup, DropdownMenuItem, DropdownMenuTrigger } from '@/components/ui/dropdown-menu'
import { Select, SelectContent, SelectGroup, SelectItem, SelectTrigger, SelectValue } from '@/components/ui/select'
import { TooltipProvider } from '@/components/ui/tooltip'
import { IconButtonTooltip } from '@/components/ui/icon-button-tooltip'
import { Separator } from '@/components/ui/separator'
import { Empty, EmptyContent, EmptyHeader, EmptyTitle } from '@/components/ui/empty'
import { Switch } from '@/components/ui/switch'
import { Conversation, ConversationContent } from '@/components/ai-elements/conversation'
import { Message, MessageContent } from '@/components/ai-elements/message'
import { InputGroup, InputGroupAddon, InputGroupTextarea } from '@/components/ui/input-group'
import { StatusDot } from '@/components/status-dot'
import { Logo } from '@/assets/logo'
import { cn } from '@/lib/utils'
import logoSrc from '../../../public/images/logo.png?inline'
import { Composition, SceneArt } from './composition'
import { importMedia, extractVideoFrame, downloadJson, type ImportedMedia } from './media'
import { FPS, seedProject, migrateProject, layoutShots, editShot, moveShot, splitShot, removeShot, applyHistory, newShot, timecode } from './model.mjs'
import { shotLifecycle, projectLifecycle, previewShot, shotPreviewStates, nextPreviewStatus } from './lifecycle.mjs'
import './style.css'

type Shot = ReturnType<typeof seedProject>['shots'][number] & { media?: ImportedMedia; muted?: boolean; from?: number; end?: number }
type Project = Omit<ReturnType<typeof seedProject>, 'assets'> & { assets: any[]; audio?: ImportedMedia | false }
type DemoMessage = { id: string; role: 'user' | 'assistant'; text: string; shotIds?: string[]; pending?: boolean }
type RequestRecord = { id: string; shotIds: string[]; instruction: string; kind: 'chat' | 'video'; createdAt: string }
const chatStates = [['queued', '请求排队中'], ['running', '回复中'], ['waiting_approval', '等待授权'], ['failed', '回复失败'], ['cancelled', '回复已停止']]
const exportStates = [['checking', '检查素材中'], ['blocked', '素材不完整'], ['queued', '等待渲染'], ['rendering', '渲染中'], ['succeeded', '已导出'], ['failed', '导出失败'], ['cancelled', '导出已取消'], ['stale', '作品已有新修改']]

const database = new Promise<IDBDatabase>((resolve, reject) => {
  const request = indexedDB.open('pixoma-video-demo', 1)
  request.onupgradeneeded = () => request.result.createObjectStore('projects')
  request.onsuccess = () => resolve(request.result)
  request.onerror = () => reject(request.error)
})

async function loadProject() {
  const db = await database
  return new Promise<Project | undefined>((resolve, reject) => {
    const request = db.transaction('projects').objectStore('projects').get('current')
    request.onsuccess = () => resolve(request.result)
    request.onerror = () => reject(request.error)
  })
}

async function persistProject(project: Project) {
  const db = await database
  return new Promise<void>((resolve, reject) => {
    const transaction = db.transaction('projects', 'readwrite')
    transaction.objectStore('projects').put(project, 'current')
    transaction.oncomplete = () => resolve()
    transaction.onerror = () => reject(transaction.error)
  })
}

function IconAction({ label, children, ...props }: { label: string; children: ReactNode } & React.ComponentProps<typeof Button>) {
  return <IconButtonTooltip label={label}><Button type='button' variant='ghost' size='icon-sm' aria-label={label} {...props}>{children}</Button></IconButtonTooltip>
}

function MediaThumb({ item }: { item: any }) {
  return item.kind === 'audio' ? <div className='audio-thumb'><AudioLines /></div> : item.firstFrame || item.src ? <img src={item.firstFrame || item.src} alt={item.name} /> : <SceneArt scene={item.scene} />
}

function Demo() {
  const [history, setHistory] = useState<any>({ past: [], present: seedProject(), future: [] })
  const project: Project = history.present
  const [initialized, setInitialized] = useState(false)
  const [saved, setSaved] = useState('读取中')
  const [selectedId, setSelectedId] = useState('shot-2')
  const [scope, setScope] = useState<string[]>([])
  const [panel, setPanel] = useState('editor')
  const [propertyTab, setPropertyTab] = useState('shot')
  const [inspectorOpen, setInspectorOpen] = useState(false)
  const [inlineEdit, setInlineEdit] = useState<{ id: string; field: 'script' | 'subtitle' | 'storyboard'; value: string } | null>(null)
  const inlineCancelled = useRef(false)
  const [durationDraft, setDurationDraft] = useState('6')
  const [resizePreview, setResizePreview] = useState<{ id: string; duration: number } | null>(null)
  const clipResize = useRef<{ id: string; x: number; duration: number; width: number; total: number; next: number; max: number } | null>(null)
  const [previewMode, setPreviewMode] = useState('full')
  const [previewAsset, setPreviewAsset] = useState<any>(null)
  const [frame, setFrame] = useState(180)
  const [playing, setPlaying] = useState(false)
  const [muted, setMuted] = useState(true)
  const [zoom, setZoom] = useState(1)
  const [expanded, setExpanded] = useState(false)
  const [chatVisible, setChatVisible] = useState(true)
  const [sidebarVisible, setSidebarVisible] = useState(true)
  const [mobileView, setMobileView] = useState('editor')
  const [dark, setDark] = useState(false)
  const [modal, setModal] = useState<string | null>(null)
  const [draft, setDraft] = useState('')
  const [messages, setMessages] = useState<DemoMessage[]>([])
  const [requests, setRequests] = useState<RequestRecord[]>([])
  const [notice, setNotice] = useState('')
  const [error, setError] = useState('')
  const [importing, setImporting] = useState(false)
  const [assetFilter, setAssetFilter] = useState('all')
  const [assetSearch, setAssetSearch] = useState('')
  const [libraryPreview, setLibraryPreview] = useState<any>(null)
  const [assignmentAsset, setAssignmentAsset] = useState<any>(null)
  const [assignmentTarget, setAssignmentTarget] = useState('')
  const [shotScenario, setShotScenarioValue] = useState('actual')
  const [scenarioHasVideo, setScenarioHasVideo] = useState<boolean | null>(null)
  const [chatScenario, setChatScenario] = useState('actual')
  const [saveScenario, setSaveScenario] = useState('actual')
  const [exportScenario, setExportScenario] = useState('actual')
  const [exportRange, setExportRange] = useState('all')
  const [exportResolution, setExportResolution] = useState('1080p')
  const [generationPrompt, setGenerationPrompt] = useState('')
  const [projectName, setProjectName] = useState('')
  const [projectRatio, setProjectRatio] = useState('16:9')
  const [trimStart, setTrimStart] = useState('0')
  const [trimDuration, setTrimDuration] = useState('6')
  const [chatWidth, setChatWidth] = useState(368)
  const player = useRef<PlayerRef>(null)
  const lastFullFrame = useRef(180)
  const fileInput = useRef<HTMLInputElement>(null)
  const draftInput = useRef<HTMLTextAreaElement>(null)
  const importTarget = useRef<'library' | 'reference' | 'video' | 'audio'>('library')
  const dragShot = useRef<string | null>(null)
  const resizeOrigin = useRef<{ x: number; width: number } | null>(null)
  const persistenceGeneration = useRef(0)
  const shots: any[] = layoutShots(project.shots)
  const selected = shots.find(shot => shot.id === selectedId)
  const duration = shots.at(-1)?.end ?? 1
  const selectedIndex = shots.findIndex(shot => shot.id === selectedId)
  const scopedShots = scope.map(id => shots.find(shot => shot.id === id)).filter(Boolean)
  const scopeLabel = scopedShots.length ? scopedShots.map(shot => shot.name).join('、') : '整个视频'
  const presentationProject = useMemo(() => ({ ...project, shots: project.shots.map(shot => shot.id === selectedId ? previewShot(shot, shotScenario, scenarioHasVideo ?? undefined) : shot) }), [project, selectedId, shotScenario, scenarioHasVideo])
  const presentationShot = presentationProject.shots.find(shot => shot.id === selectedId)
  const displayShots = layoutShots(presentationProject.shots.map(shot => resizePreview?.id === shot.id ? { ...shot, duration: resizePreview.duration } : shot))
  const displayDuration = displayShots.at(-1)?.end ?? 1
  const selectedState = selected ? shotLifecycle(presentationShot) : null
  const effectiveSaved = saveScenario === 'actual' ? saved : saveScenario
  const projectState = projectLifecycle(presentationProject, effectiveSaved)
  const previewing = [shotScenario, chatScenario, saveScenario, exportScenario].some(value => value !== 'actual')
  const previewProject = useMemo(() => previewMode === 'shot' && presentationShot ? { ...presentationProject, shots: [presentationShot] } : presentationProject, [presentationProject, previewMode, selectedId])
  const previewDuration = previewMode === 'shot' && selected ? selected.duration : duration
  const playbackShot = previewMode === 'shot' ? presentationShot : layoutShots(presentationProject.shots).find(shot => frame >= shot.from && frame < shot.end)
  const playbackState = playbackShot ? shotLifecycle(playbackShot) : null
  const timelineWidth = Math.max(720, displayDuration / FPS * 43 * zoom)
  const assets = project.assets

  useEffect(() => { setDurationDraft(selected ? String(selected.duration / FPS) : '') }, [selectedId, selected?.duration])

  useEffect(() => {
    loadProject().then(stored => {
      if (stored) setHistory({ past: [], present: migrateProject(stored), future: [] })
      setInitialized(true)
    }).catch(cause => setError(`读取本地项目失败：${cause.message}`))
  }, [])

  useEffect(() => {
    if (!initialized) return
    const generation = ++persistenceGeneration.current
    setSaved('保存中')
    const timer = setTimeout(() => persistProject(project).then(() => {
      if (generation === persistenceGeneration.current) setSaved('已保存')
    }).catch(cause => { setSaved('保存失败'); setError(cause.message) }), 350)
    return () => clearTimeout(timer)
  }, [project, initialized])

  useEffect(() => { document.documentElement.classList.toggle('dark', dark) }, [dark])
  useEffect(() => {
    if (!notice) return
    const timer = setTimeout(() => setNotice(''), 3200)
    return () => clearTimeout(timer)
  }, [notice])

  useEffect(() => {
    const current = player.current
    if (!current) return
    const onFrame = (event: any) => {
      setFrame(event.detail.frame)
      if (previewMode === 'full') lastFullFrame.current = event.detail.frame
    }
    const onPlay = () => setPlaying(true)
    const onPause = () => setPlaying(false)
    current.addEventListener('frameupdate', onFrame)
    current.addEventListener('play', onPlay)
    current.addEventListener('pause', onPause)
    current.addEventListener('ended', onPause)
    current.seekTo(previewMode === 'shot' ? 0 : Math.min(lastFullFrame.current, previewDuration - 1))
    return () => {
      current.removeEventListener('frameupdate', onFrame)
      current.removeEventListener('play', onPlay)
      current.removeEventListener('pause', onPause)
      current.removeEventListener('ended', onPause)
    }
  }, [panel, previewMode, previewAsset, initialized])

  useEffect(() => {
    const onKey = (event: KeyboardEvent) => {
      if (modal || (event.target as HTMLElement).closest('input, textarea, [contenteditable=true], [role=slider]')) return
      if (event.key === 'Escape') { setInspectorOpen(false); setPreviewMode('full'); return }
      if ((event.metaKey || event.ctrlKey) && event.key.toLowerCase() === 'z') { event.preventDefault(); event.shiftKey ? redo() : undo(); return }
      if (event.code === 'Space' && panel === 'editor') { event.preventDefault(); player.current?.toggle() }
      if (event.key === 'ArrowRight') player.current?.seekTo(Math.min(frame + 1, previewDuration - 1))
      if (event.key === 'ArrowLeft') player.current?.seekTo(Math.max(frame - 1, 0))
    }
    window.addEventListener('keydown', onKey)
    return () => window.removeEventListener('keydown', onKey)
  }, [modal, history, panel, frame, previewDuration])

  function commit(next: Project, message?: string) {
    setHistory((current: any) => applyHistory(current, next))
    if (message) setNotice(message)
  }
  function setShotScenario(value: string) {
    if (value === 'actual') setScenarioHasVideo(null)
    else if (['empty', 'prepared'].includes(value)) setScenarioHasVideo(false)
    else if (['ready', 'stale', 'frames_processing', 'frames_failed'].includes(value)) setScenarioHasVideo(true)
    else if (shotScenario === 'actual') setScenarioHasVideo(Boolean(selected?.ready || selected?.media))
    setShotScenarioValue(value)
  }
  function patchShot(changes: any) {
    if (!selected) return
    commit(editShot(project, selected.id, changes))
  }
  function undo() {
    setHistory((current: any) => current.past.length ? { past: current.past.slice(0, -1), present: current.past.at(-1), future: [current.present, ...current.future] } : current)
  }
  function redo() {
    setHistory((current: any) => current.future.length ? { past: [...current.past, current.present], present: current.future[0], future: current.future.slice(1) } : current)
  }
  function selectShot(id: string, tab = 'shot') {
    setSelectedId(id)
    if (id !== selectedId) setShotScenario('actual')
    setPropertyTab(tab)
    setPreviewAsset(null)
    const shot = shots.find(item => item.id === id)
    if (shot) player.current?.seekTo(previewMode === 'shot' ? 0 : shot.from)
  }
  function openInspector(tab = propertyTab) {
    setPropertyTab(tab)
    setInspectorOpen(true)
  }
  function beginInlineEdit(shot: any, field: 'script' | 'subtitle' | 'storyboard') {
    if (previewing) return
    inlineCancelled.current = false
    setInlineEdit({ id: shot.id, field, value: shot[field] })
  }
  function finishInlineEdit() {
    if (!inlineEdit) return
    if (!inlineCancelled.current) commit(editShot(project, inlineEdit.id, { [inlineEdit.field]: inlineEdit.value }))
    setInlineEdit(null)
  }
  function applyDuration() {
    if (!selected || previewing) return
    const next = Math.round(Number(durationDraft) * FPS)
    if (!Number.isFinite(next) || next < 1 || selected.sourceIn + Math.ceil(next * selected.speed) > selected.sourceDuration) {
      setDurationDraft(String(selected.duration / FPS))
      setError('时长超出源素材范围')
      return
    }
    if (next !== selected.duration) patchShot({ duration: next })
  }
  function startClipResize(event: React.PointerEvent<HTMLSpanElement>, shot: any) {
    event.stopPropagation()
    event.preventDefault()
    selectShot(shot.id, 'video')
    const lane = event.currentTarget.closest('.track-lane')!.getBoundingClientRect()
    clipResize.current = { id: shot.id, x: event.clientX, duration: shot.duration, width: lane.width, total: duration, next: shot.duration, max: Math.floor((shot.sourceDuration - shot.sourceIn) / shot.speed) }
    event.currentTarget.setPointerCapture(event.pointerId)
  }
  function updateClipResize(event: React.PointerEvent<HTMLSpanElement>) {
    const resize = clipResize.current
    if (!resize) return
    resize.next = Math.max(1, Math.min(resize.max, resize.duration + Math.round((event.clientX - resize.x) / resize.width * resize.total)))
    setResizePreview({ id: resize.id, duration: resize.next })
  }
  function finishClipResize(cancelled = false) {
    const resize = clipResize.current
    if (!resize) return
    if (!cancelled && resize.next !== resize.duration) commit(editShot(project, resize.id, { duration: resize.next }))
    clipResize.current = null
    setResizePreview(null)
  }
  function seekTimeline(event: React.PointerEvent<HTMLDivElement>) {
    const rect = event.currentTarget.getBoundingClientRect()
    const target = Math.min(duration - 1, Math.max(0, Math.round((event.clientX - rect.left) / rect.width * duration)))
    lastFullFrame.current = target
    setPreviewMode('full')
    if (previewMode === 'full') player.current?.seekTo(target)
  }
  function addShot() {
    const shot = newShot(crypto.randomUUID())
    const next = [...project.shots]
    next.splice(selectedIndex >= 0 ? selectedIndex + 1 : next.length, 0, shot)
    commit({ ...project, shots: next }, '已新增分镜')
    setSelectedId(shot.id)
    setShotScenario('actual')
    setPropertyTab('shot')
    setPanel('editor')
    setInspectorOpen(false)
    setPreviewMode('full')
    const from = layoutShots(next).find(item => item.id === shot.id)!.from
    requestAnimationFrame(() => player.current?.seekTo(from))
  }
  function addToChat(id: string) {
    setScope(current => current.includes(id) ? current : [...current, id])
    setChatVisible(true)
    setMobileView('chat')
    setTimeout(() => draftInput.current?.focus(), 60)
  }
  function deleteSelected() {
    if (!selected) return
    if (selectedState && ['queued', 'running', 'collecting', 'cancelling', 'submission_unknown'].includes(selectedState.status)) { setModal('delete-active'); return }
    finishDelete()
  }
  function finishDelete() {
    if (!selected) return
    commit(removeShot(project, selected.id), '已删除分镜，可撤销')
    setScope(current => current.filter(id => id !== selected.id))
    setSelectedId(shots[Math.max(0, selectedIndex - 1)]?.id ?? '')
    setShotScenario('actual')
    setModal(null)
  }
  function copySelected() {
    if (!selected) return
    const copy = { ...structuredClone(selected), id: crypto.randomUUID(), name: `${selected.name} · 副本` }
    const next = [...project.shots]
    next.splice(selectedIndex + 1, 0, copy)
    commit({ ...project, shots: next }, '已复制分镜')
    setSelectedId(copy.id)
  }
  function splitSelected() {
    if (!selected) return
    const local = previewMode === 'shot' ? frame : frame - selected.from
    if (local <= 0 || local >= selected.duration) { setError('把播放游标移到当前分镜内部后分割'); return }
    commit(splitShot(project, selected.id, local, crypto.randomUUID()), '已分割分镜')
  }
  function openImport(target: 'library' | 'reference' | 'video' | 'audio') {
    importTarget.current = target
    fileInput.current?.click()
  }
  async function onImport(file: File) {
    setImporting(true)
    setError('')
    const item = await importMedia(file)
    if (importTarget.current === 'video' && item.kind !== 'video') throw new Error('选择视频文件')
    if (importTarget.current === 'audio' && item.kind !== 'audio') throw new Error('选择音频文件')
    const importedAssets: any[] = [item]
    if (item.kind === 'video') {
      importedAssets.push({ id: `${item.id}:first`, name: `${item.name} · 首帧`, kind: 'image', src: item.firstFrame })
      importedAssets.push({ id: `${item.id}:last`, name: `${item.name} · 尾帧`, kind: 'image', src: item.lastFrame })
    }
    let next: any = { ...project, assets: [...project.assets, ...importedAssets] }
    if (importTarget.current === 'audio') next.audio = item
    else if (selected && importTarget.current !== 'library') {
      if (importTarget.current === 'video') {
        const previous = selected.versions.map((version: any) => ({ ...version, status: 'history' }))
        if (selected.media && !previous.some((version: any) => version.id === selected.media.id)) previous.push({ id: selected.media.id, media: selected.media, status: 'history' })
        next = editShot(next, selected.id, { media: item, ready: true, sourceIn: 0, sourceDuration: item.duration, duration: Math.min(selected.duration, item.duration!), speed: 1, inputsChanged: false, generation: { status: 'idle' }, frames: { status: 'ready' }, versions: [...previous, { id: item.id, media: item, status: 'active' }] })
      }
      else next = editShot(next, selected.id, { references: [...selected.references, { ...item, role: '参考' }] })
    }
    commit(next, item.kind === 'video' ? '已导入视频并提取边界画面' : '已导入资产')
    setImporting(false)
  }
  function useAsset(item: any, targetId = selectedId) {
    const target = project.shots.find(shot => shot.id === targetId)
    if (!target) { setError('选择一个分镜后添加参考'); return }
    if (target.references.some((ref: any) => ref.id === item.id)) { setNotice('当前分镜已引用此资产'); return }
    commit(editShot(project, targetId, { references: [...target.references, { ...item, role: '参考' }] }))
    setNotice('已添加参考')
  }
  function openAssignment(item: any) {
    setAssignmentAsset(item)
    setAssignmentTarget(selectedId)
    setLibraryPreview(null)
    setModal('assign')
  }
  async function chainFrame() {
    if (!selected?.media?.lastFrame) return
    const nextShot = shots[selectedIndex + 1] ?? newShot(crypto.randomUUID())
    const sourceEnd = (selected.sourceIn + selected.duration * selected.speed) / FPS
    const src = await extractVideoFrame(selected.media.src, sourceEnd - 0.000001)
    const reference = { id: crypto.randomUUID(), name: `${selected.name} · 尾帧`, kind: 'image', src, role: '首帧', sourceShotId: selected.id }
    const nextShots = shots[selectedIndex + 1] ? project.shots : [...project.shots, nextShot]
    const next = editShot({ ...project, shots: nextShots, assets: [...project.assets, reference] }, nextShot.id, { references: [...nextShot.references.filter((ref: any) => ref.role !== '首帧'), reference] })
    commit(next, '已设置下一分镜首帧')
    selectShot(nextShot.id, 'reference')
  }
  function sendMessage() {
    if (!draft.trim()) return
    if (chatScenario !== 'actual') { setChatScenario(chatScenario === 'running' ? 'queued' : 'running'); setDraft(''); return }
    const shotIds = scope.filter(id => project.shots.some(shot => shot.id === id))
    const request = { id: crypto.randomUUID(), shotIds, instruction: draft.trim(), kind: 'chat' as const, createdAt: new Date().toISOString() }
    setMessages(current => [...current, { id: request.id, role: 'user', text: request.instruction, shotIds, pending: true }])
    setRequests(current => [...current, request])
    setDraft('')
    setNotice('已保存对话请求，尚未连接模型服务')
  }
  function saveGeneration() {
    if (!selected || !generationPrompt.trim()) return
    const request: RequestRecord = { id: crypto.randomUUID(), shotIds: [selected.id], instruction: generationPrompt.trim(), kind: 'video', createdAt: new Date().toISOString() }
    setRequests(current => [...current, request])
    downloadJson({ ...request, shot: selected, model: '由工作流配置决定', status: 'draft' }, `${selected.name}-生成请求.json`)
    setModal(null)
    setNotice('已保存生成请求')
  }
  function openGeneration() {
    if (!selected || !selectedState) return
    if (selectedState.primary.action === 'edit') {
      if (shotScenario !== 'actual') setShotScenario('prepared')
    else { openInspector('shot'); setTimeout(() => document.getElementById('shot-script')?.focus(), 50) }
      return
    }
    if (selectedState.primary.action === 'versions') { setModal('versions'); return }
    if (selectedState.primary.action === 'query') {
      if (shotScenario !== 'actual') setShotScenario('running')
      else setModal('connection')
      return
    }
    if (selectedState.primary.action === 'import') { setShotScenario('actual'); openImport('video'); return }
    setGenerationPrompt([presentationShot.script, presentationShot.storyboard].filter(Boolean).join('\n'))
    setModal('generate')
  }
  function resetStatePreview() {
    setShotScenario('actual')
    setChatScenario('actual')
    setSaveScenario('actual')
    setExportScenario('actual')
  }
  function adoptVersion(version: any) {
    if (!selected) return
    if (version.isPreview) { setShotScenario('ready'); setModal(null); return }
    const media = version.media
    if (!media) throw new Error('视频版本缺少素材')
    commit(editShot(project, selected.id, { media, ready: true, sourceIn: 0, sourceDuration: media.duration, duration: Math.min(selected.duration, media.duration), speed: 1, inputsChanged: false, generation: { status: 'idle' }, frames: { status: 'ready' }, versions: selected.versions.map((item: any) => ({ ...item, status: item.id === version.id ? 'active' : 'history' })) }), '已采用视频版本')
    setModal(null)
  }
  function moveSelected(direction: number) {
    const other = shots[selectedIndex + direction]
    if (!selected || !other) return
    const next = [...project.shots]
    ;[next[selectedIndex], next[selectedIndex + direction]] = [next[selectedIndex + direction], next[selectedIndex]]
    commit({ ...project, shots: next })
  }

  const previewWidth = project.ratio === '9:16' ? 608 : project.ratio === '1:1' ? 1080 : 1920
  const selectedFrames = presentationShot?.media && selectedState?.frameStatus === 'ready' && selectedState.canPreview ? [{ name: '首帧', src: presentationShot.media.firstFrame }, { name: '尾帧', src: presentationShot.media.lastFrame }] : []
  const filteredAssets = assets.filter(item => (assetFilter === 'all' || item.kind === assetFilter) && item.name.toLocaleLowerCase().includes(assetSearch.trim().toLocaleLowerCase()))
  const exportShots = exportRange === 'selected' ? shots.filter(shot => scope.length ? scope.includes(shot.id) : shot.id === selectedId) : shots
  const missingExport = exportShots.filter(shot => !shot.media)
  const exportCheck = projectLifecycle({ ...presentationProject, shots: exportShots.map(shot => shot.id === selectedId ? previewShot(shot, shotScenario, scenarioHasVideo ?? undefined) : shot) }, effectiveSaved)
  const canAdvancePreview = ['empty', 'prepared', 'queued', 'running', 'collecting', 'cancelling', 'submission_unknown', 'frames_processing', 'frames_failed', 'failed', 'cancelled', 'stale'].includes(shotScenario)
  const chatActive = ['queued', 'running'].includes(chatScenario)

  return <TooltipProvider delayDuration={200}><div className={cn('demo-shell', expanded && 'editor-expanded', !sidebarVisible && 'sidebar-hidden', !chatVisible && 'chat-hidden')} data-mobile-view={mobileView} style={{ '--chat-width': `${chatWidth}px` } as CSSProperties}>
    <aside className='studio-nav' aria-label='Studio 导航'>
      <div className='brand'><Logo src={logoSrc} /><span>Pixoma</span><IconAction label='收起导航' onClick={() => setSidebarVisible(false)}><PanelLeftClose /></IconAction></div>
      <div className='nav-items'>
        <Button variant='secondary' size='sm' onClick={() => { setChatVisible(true); setMobileView('chat') }}><MessageCircle />对话</Button>
        <Button variant='ghost' size='sm' onClick={() => setPanel('assets')}><Library />资产库</Button>
        <Button variant='ghost' size='sm' onClick={() => setModal('workflows')}><Workflow />工作流</Button>
        <Button variant='ghost' size='sm' onClick={() => setModal('skills')}><Sparkles />技能与专家</Button>
      </div>
      <Separator />
      <div className='nav-section-head'><span>项目</span><IconAction label='创建视频项目' onClick={() => { setProjectName(''); setModal('create') }}><Plus /></IconAction></div>
      <Button className='nav-project' variant='ghost' size='sm' onClick={() => { setPanel('editor'); setMobileView('editor') }}><FolderOpen />山间茶事<ChevronDown /></Button>
      <Button className='nav-session' variant='secondary' size='sm' onClick={() => { setPanel('editor'); setMobileView('editor') }}><MessageCircle />品牌短片创作</Button>
      <div className='nav-section-head'><span>最近会话</span><IconAction label='新建视频对话' onClick={() => setModal('create')}><MessageSquarePlus /></IconAction></div>
      <div className='session-list'><span>秋日新品视觉</span><span>角色与场景设定</span><span>片头字幕设计</span></div>
      <div className='nav-bottom'><span className='demo-label'>交互 Demo</span><IconAction label={dark ? '使用亮色主题' : '使用暗色主题'} onClick={() => setDark(value => !value)}>{dark ? <Sun /> : <Moon />}</IconAction><IconAction label='演示设置' onClick={() => setModal('settings')}><Settings2 /></IconAction></div>
    </aside>

    <main className='studio-main'>
      <header className='workspace-header'>
        <div className='header-leading'>{!sidebarVisible ? <IconAction label='展开导航' onClick={() => setSidebarVisible(true)}><PanelLeftOpen /></IconAction> : null}<span className='project-breadcrumb'>山间茶事</span><ChevronRight /><h1>品牌短片创作</h1><Badge variant='outline'>示例项目</Badge></div>
        <div className='header-actions'><span className='save-label'>{effectiveSaved === '已保存' ? <Check /> : <StatusDot state='warn' label={effectiveSaved} />}{effectiveSaved}</span>{effectiveSaved === '保存失败' ? <Button variant='ghost' size='sm' onClick={() => saveScenario !== 'actual' ? setSaveScenario('保存中') : persistProject(project).then(() => setSaved('已保存')).catch(cause => setError(cause.message))}>重试保存</Button> : null}<IconAction label='切换明暗主题' onClick={() => setDark(value => !value)}>{dark ? <Sun /> : <Moon />}</IconAction></div>
      </header>
      <div className='mobile-switch'><Button size='sm' variant={mobileView === 'chat' ? 'secondary' : 'ghost'} onClick={() => setMobileView('chat')}><MessageCircle />对话</Button><Button size='sm' variant={mobileView === 'editor' ? 'secondary' : 'ghost'} onClick={() => setMobileView('editor')}><Film />视频编辑器</Button></div>
      {previewing ? <div className='state-preview-banner' data-state-preview='true'><Badge variant='outline'>状态预览</Badge><span>{shotScenario !== 'actual' ? `${selected?.name} · ${selectedState?.label}` : chatScenario !== 'actual' ? chatStates.find(item => item[0] === chatScenario)?.[1] : saveScenario !== 'actual' ? effectiveSaved : exportStates.find(item => item[0] === exportScenario)?.[1]}</span>{canAdvancePreview ? <Button size='sm' variant='outline' onClick={() => setShotScenario(nextPreviewStatus(shotScenario, selectedState?.hasVideo))}>推进状态<ArrowRight /></Button> : null}{saveScenario === '保存中' ? <Button size='sm' variant='outline' onClick={() => setSaveScenario('已保存')}>展示保存成功</Button> : null}<Button size='sm' variant='ghost' onClick={resetStatePreview}>退出预览<X /></Button></div> : null}
      <div className='workspace-body'>
        <section className='chat-panel' aria-label='创作对话'>
          <div className='chat-heading'><span><MessageCircle />创作对话</span><IconAction label='收起对话' onClick={() => setChatVisible(false)}><PanelLeftClose /></IconAction></div>
          <Conversation className='chat-conversation'><ConversationContent className='chat-messages'>
            <div className='conversation-date'>今天 · 示例对话</div>
            <Message from='user'><MessageContent>制作一支 16 秒的茶品牌短片。从山间晨雾，到新茶叶片，再到一盏热茶。画面安静，保留自然的呼吸感。</MessageContent></Message>
            <Message from='assistant'><MessageContent><div className='assistant-name'><Logo src={logoSrc} />Pixoma</div><p>按「山间 → 茶叶 → 茶盏」安排三个分镜。</p><div className='chat-shot-list'>{seedProject().shots.map((shot, index) => <button key={shot.id} className='chat-shot-row' onClick={() => { selectShot(shot.id); setPanel('editor'); setMobileView('editor') }}><span className='chat-shot-index'>{String(index + 1).padStart(2, '0')}</span><div><strong>{shot.name}</strong><small>{index === 0 ? '远景 · 缓慢推进' : index === 1 ? '特写 · 轻微横移' : '近景 · 固定镜头'}</small></div><span>{shot.duration / FPS}s</span><ChevronRight /></button>)}</div><p className='message-muted'>时间轴中的画面为分镜示意图。</p></MessageContent></Message>
            <Message from='user'><MessageContent>第二个镜头保留露珠，把注意力放在叶片的纹路上。</MessageContent></Message>
            <Message from='assistant'><MessageContent><div className='assistant-name'><Logo src={logoSrc} />Pixoma</div><p>可以在分镜说明中调整构图，也可以引用这个分镜继续对话。</p><Button size='sm' variant='outline' onClick={() => { selectShot('shot-2', 'storyboard'); addToChat('shot-2') }}><Film />引用「一叶一晨」<ArrowRight /></Button></MessageContent></Message>
            {messages.map(message => <Message key={message.id} from={message.role}><MessageContent>{message.shotIds?.length ? <div className='message-scope'>{message.shotIds.map(id => <button key={id} onClick={() => { selectShot(id); setMobileView('editor') }}><Film />{shots.find(shot => shot.id === id)?.name ?? '已删除分镜'}</button>)}</div> : null}<p>{message.text}</p>{message.pending ? <span className='message-muted'>请求已保存 · 对话服务未连接</span> : null}</MessageContent></Message>)}
            {chatScenario !== 'actual' ? <div className='chat-lifecycle' data-chat-state={chatScenario}><div><StatusDot state='warn' label={chatStates.find(item => item[0] === chatScenario)?.[1] || '对话状态'} /><strong>{chatStates.find(item => item[0] === chatScenario)?.[1]}</strong><Badge variant='outline'>状态预览</Badge></div><p>{chatScenario === 'waiting_approval' ? `生成视频 · ${scopeLabel}` : chatScenario === 'failed' ? '模型请求超时，输入和已收到的内容保留。' : chatScenario === 'cancelled' ? '已保留回复内容与创作要求。' : scopeLabel}</p><div>{chatActive ? <Button variant='outline' size='sm' onClick={() => setChatScenario('cancelled')}>{chatScenario === 'queued' ? '取消排队' : '停止回复'}</Button> : null}{chatScenario === 'waiting_approval' ? <><Button size='sm' onClick={() => setChatScenario('running')}>批准调用</Button><Button variant='outline' size='sm' onClick={() => setChatScenario('cancelled')}>拒绝调用</Button></> : null}{chatScenario === 'failed' || chatScenario === 'cancelled' ? <Button size='sm' variant='outline' onClick={() => setChatScenario('running')}>{chatScenario === 'failed' ? '重试回复' : '继续对话'}</Button> : null}</div></div> : null}
          </ConversationContent></Conversation>
          <div className='composer-area'>
            <div className='composer-scope'><span>对话范围</span>{scope.length ? <Button variant='ghost' size='sm' onClick={() => setScope([])}>恢复整个视频<X /></Button> : <span>整个视频</span>}</div>
            {scope.length ? <div className='scope-chips'>{scopedShots.map(shot => <Badge variant='secondary' key={shot.id}><Film />{shot.name}<button aria-label={`移除${shot.name}引用`} onClick={() => setScope(current => current.filter(id => id !== shot.id))}><X /></button></Badge>)}</div> : null}
            <InputGroup className='chat-composer'>
              <InputGroupTextarea ref={draftInput} value={draft} onChange={event => setDraft(event.target.value)} placeholder={scope.length ? '描述这个分镜的修改…' : '描述你想创作的视频…'} aria-label='创作要求' onKeyDown={event => { if (event.key === 'Enter' && !event.shiftKey && !event.nativeEvent.isComposing) { event.preventDefault(); if (!chatActive && chatScenario !== 'waiting_approval') sendMessage() } }} />
              <InputGroupAddon align='block-end'><IconAction label='浏览项目资产' onClick={() => { setPanel('assets'); setMobileView('editor') }}><Plus /></IconAction><Button variant='ghost' size='sm' onClick={() => setModal('connection')}>DeepSeek Flash<ChevronDown /></Button><div className='composer-spacer' />
                {chatActive ? <IconAction label={chatScenario === 'queued' ? '取消排队' : '停止回复'} variant='default' onClick={() => setChatScenario('cancelled')}><Square /></IconAction> : <IconAction label='发送创作要求' variant='default' disabled={!draft.trim() || chatScenario === 'waiting_approval'} onClick={sendMessage}><ArrowUp /></IconAction>}
              </InputGroupAddon>
            </InputGroup>
            <div className='composer-footer'><span><ShieldCheck />按请求审批</span><span>Enter 发送</span></div>
          </div>
        </section>
        <div className='chat-resizer' role='separator' aria-label='调整对话宽度' aria-orientation='vertical' onPointerDown={event => { resizeOrigin.current = { x: event.clientX, width: chatWidth }; event.currentTarget.setPointerCapture(event.pointerId) }} onPointerMove={event => { if (resizeOrigin.current) setChatWidth(Math.min(580, Math.max(300, resizeOrigin.current.width + event.clientX - resizeOrigin.current.x))) }} onPointerUp={() => { resizeOrigin.current = null }} />
        <section className='editor-panel' aria-label='视频编辑器'>
          <Tabs value={panel} onValueChange={setPanel} className='editor-tabs'>
            <div className='editor-tabbar'>
              <TabsList aria-label='会话工作区'><TabsTrigger value='editor'><Film />视频编辑器</TabsTrigger><TabsTrigger value='assets'>项目资产</TabsTrigger><TabsTrigger value='tasks'>会话任务{requests.length ? <span className='request-count'>{requests.length}</span> : null}</TabsTrigger></TabsList>
              <div className='editor-panel-actions'>
                {!chatVisible ? <IconAction label='展开对话' onClick={() => setChatVisible(true)}><MessageCircle /></IconAction> : null}
                <div className='history-actions'><IconAction label='撤销' disabled={!history.past.length} onClick={undo}><Undo2 /></IconAction><IconAction label='重做' disabled={!history.future.length} onClick={redo}><Redo2 /></IconAction></div>
                <DropdownMenu><DropdownMenuTrigger asChild><Button variant='ghost' size='icon-sm' aria-label='视频编辑器更多操作'><MoreHorizontal /></Button></DropdownMenuTrigger><DropdownMenuContent align='end'><DropdownMenuGroup>
                  <DropdownMenuItem onSelect={() => setModal('state-preview')}><Settings2 />状态预览</DropdownMenuItem>
                  <DropdownMenuItem onSelect={() => { setProjectName(project.name); setProjectRatio(project.ratio); setModal('rename') }}><Video />视频设置</DropdownMenuItem>
                  <DropdownMenuItem onSelect={() => setExpanded(value => !value)}>{expanded ? <Minimize2 /> : <Maximize2 />}{expanded ? '恢复工作区布局' : '放大编辑器'}</DropdownMenuItem>
                </DropdownMenuGroup></DropdownMenuContent></DropdownMenu>
                <Button variant='outline' size='sm' onClick={() => setModal('export')}><ArrowDownToLine />导出</Button>
              </div>
            </div>
            <TabsContent value='editor' className='editor-content'>
              <div className='preview-layout'>
                <div className='preview-area'>
                  <div className='preview-stage'>
                    {previewAsset ? <div className='asset-preview'>{previewAsset.kind === 'audio' ? <audio src={previewAsset.src} controls /> : previewAsset.kind === 'video' ? <video src={previewAsset.src} controls /> : <MediaThumb item={previewAsset} />}<Button variant='secondary' size='sm' className='return-preview' onClick={() => setPreviewAsset(null)}><ArrowLeft />返回成片</Button></div> : <Player ref={player} component={Composition} inputProps={{ project: previewProject }} durationInFrames={Math.max(1, previewDuration)} compositionWidth={previewWidth} compositionHeight={1080} fps={FPS} loop={previewMode === 'shot'} controls={false} clickToPlay={false} style={{ width: '100%', maxHeight: '100%', aspectRatio: `${previewWidth} / 1080` }} />}
                    {!previewAsset && playbackState?.canPreview && !playbackShot.media ? <span className='preview-example'>分镜示意</span> : null}
                    {!previewAsset && playbackState && !playbackState.canPreview ? <div className='preview-empty-state'><Video /><strong>{playbackState.label}</strong><span>{playbackShot.name}</span><Button size='sm' variant='outline' onClick={() => { selectShot(playbackShot.id); openInspector('video') }}>编辑此分镜</Button></div> : null}
                    {!previewAsset && playbackState?.hasVideo && ['queued', 'running', 'collecting', 'failed', 'candidate', 'submission_unknown'].includes(playbackState.status) ? <span className='preview-current-version'>预览当前采用版本</span> : null}
                  </div>
                  <div className='playback-toolbar'>
                    <div className='playback-context'><button className='video-name' onClick={() => { setProjectName(project.name); setProjectRatio(project.ratio); setModal('rename') }}>{project.name}<ChevronDown /></button><span className='project-lifecycle' data-project-state={projectState.status} title={`${projectState.label}${projectState.pendingCount ? ` · ${projectState.pendingCount} 个任务处理中` : ''}${projectState.candidateCount ? ` · ${projectState.candidateCount} 个新版本待采用` : ''}`}><StatusDot state={projectState.tone as 'ok' | 'warn'} label={projectState.label} /><span>{projectState.label}</span></span></div>
                    <div className='transport-controls'><div className='playback-actions'><IconAction label='返回开头' onClick={() => player.current?.seekTo(0)}><SkipBack /></IconAction><IconAction label={playing ? '暂停预览' : '播放预览'} onClick={() => player.current?.toggle()}>{playing ? <Pause /> : <Play />}</IconAction><IconAction label='逐帧后退' onClick={() => player.current?.seekTo(Math.max(frame - 1, 0))}><ArrowLeft /></IconAction><IconAction label='逐帧前进' onClick={() => player.current?.seekTo(Math.min(frame + 1, previewDuration - 1))}><ArrowRight /></IconAction></div><span className='timecode'>{timecode(frame)}<span> / {timecode(previewDuration)}</span></span></div>
                    <div className='playback-actions'>{previewMode === 'shot' ? <Button className='loop-indicator' size='sm' variant='secondary' onClick={() => setPreviewMode('full')} aria-label='退出分镜循环'>{selected?.name}<X /></Button> : null}<IconAction label={muted ? '打开预览声音' : '关闭预览声音'} onClick={() => { setMuted(value => !value); muted ? player.current?.unmute() : player.current?.mute() }}>{muted ? <VolumeX /> : <Volume2 />}</IconAction><IconAction label='全屏预览' onClick={() => player.current?.requestFullscreen()}><Maximize2 /></IconAction></div>
                  </div>
                </div>
              </div>
              <div className='timeline-section'>
                <div className='timeline-toolbar'><div className='timeline-title'><Film /><strong>时间轴</strong><span>{shots.length} 个分镜 · {(displayDuration / FPS).toFixed(1)}s</span></div><div className='timeline-tools'><IconAction label='复制分镜' disabled={!selected || previewing} onClick={copySelected}><Copy /></IconAction><IconAction label='在游标处分割' disabled={!selected || previewing} onClick={splitSelected}><Scissors /></IconAction><IconAction label='删除分镜' disabled={!selected} onClick={deleteSelected}><Trash2 /></IconAction><span className='toolbar-divider' /><IconAction label='缩小时间轴' onClick={() => setZoom(value => Math.max(0.5, value - 0.25))}><ZoomOut /></IconAction><span className='zoom-value'>{Math.round(zoom * 100)}%</span><IconAction label='放大时间轴' onClick={() => setZoom(value => Math.min(3, value + 0.25))}><ZoomIn /></IconAction><Button size='sm' variant='outline' onClick={addShot}><Plus />新增分镜</Button></div></div>
                <div className='timeline-content'>
                {inspectorOpen ? <aside className='inspector' aria-label='分镜属性'>
                  {selected ? <><div className='inspector-title'><span><span className='shot-index'>{String(selectedIndex + 1).padStart(2, '0')}</span>{selected.name}</span><IconAction label='关闭分镜编辑' onClick={() => setInspectorOpen(false)}><X /></IconAction></div><Tabs value={propertyTab} onValueChange={setPropertyTab}><TabsList className='inspector-tabs'><TabsTrigger value='shot'>剧本</TabsTrigger><TabsTrigger value='storyboard'>分镜</TabsTrigger><TabsTrigger value='reference'>参考</TabsTrigger><TabsTrigger value='video'>视频</TabsTrigger></TabsList></Tabs>
                    <div className='inspector-body'>
                      <div className='shot-lifecycle' data-shot-state={selectedState.status}><div><StatusDot state={selectedState.tone as 'ok' | 'warn'} label={selectedState.label} /><strong>{selectedState.label}</strong></div>{selectedState.detail ? <p>{selectedState.detail}</p> : null}{selectedState.hasVideo && ['queued', 'running', 'collecting', 'cancelling', 'submission_unknown', 'failed'].includes(selectedState.status) ? <small>当前版本仍在成片中</small> : null}</div>
                      <fieldset disabled={shotScenario !== 'actual'} className='lifecycle-fields'>
                      <FieldGroup className='property-fields'>
                        {propertyTab === 'shot' ? <>
                          <Field><FieldLabel htmlFor='shot-name'>分镜名称</FieldLabel><Input id='shot-name' value={presentationShot.name} onChange={event => { if (event.target.value.trim()) patchShot({ name: event.target.value }) }} /></Field>
                          <Field><FieldLabel htmlFor='shot-script'>剧本</FieldLabel><Textarea id='shot-script' value={presentationShot.script} placeholder='描述这个分镜发生的内容' onChange={event => patchShot({ script: event.target.value })} /></Field>
                          <Field><FieldLabel htmlFor='shot-subtitle'>字幕</FieldLabel><Textarea id='shot-subtitle' value={presentationShot.subtitle} placeholder='添加对白或旁白字幕' onChange={event => patchShot({ subtitle: event.target.value })} /></Field>
                          <Field><FieldLabel htmlFor='shot-duration'>时长（秒）</FieldLabel><Input id='shot-duration' type='number' step='0.1' min='0.1' max={Math.floor((selected.sourceDuration - selected.sourceIn) / selected.speed / FPS * 10) / 10} value={selected.duration / FPS} onChange={event => { const duration = Math.round(Number(event.target.value) * FPS); if (duration > 0 && selected.sourceIn + Math.ceil(duration * selected.speed) <= selected.sourceDuration) patchShot({ duration }) }} /></Field>
                        </> : null}
                        {propertyTab === 'storyboard' ? <>
                          <div className='storyboard-art'><SceneArt scene={presentationShot.scene} /><span>分镜示意</span></div>
                          <Field><FieldLabel htmlFor='shot-storyboard'>画面与运镜</FieldLabel><Textarea id='shot-storyboard' value={presentationShot.storyboard} placeholder='描述景别、构图和镜头运动' onChange={event => patchShot({ storyboard: event.target.value })} /></Field>
                          <Button size='sm' variant='outline' onClick={() => { addToChat(selected.id); setDraft('为这个分镜设计构图和运镜。') }}><MessageCircle />通过对话设计</Button>
                        </> : null}
                        {propertyTab === 'reference' ? <><div className='reference-grid'>{selected.references.map((item: any) => <div className='reference-item' key={item.id}><button onClick={() => setPreviewAsset(item)}><MediaThumb item={item} /></button><div><span>{item.name}</span><small>{item.role}</small></div><IconAction label={`移除参考${item.name}`} onClick={() => patchShot({ references: selected.references.filter((ref: any) => ref.id !== item.id) })}><X /></IconAction></div>)}</div><Button size='sm' variant='outline' onClick={() => setModal('assets')}><Plus />添加参考资产</Button></> : null}
                        {propertyTab === 'video' ? <>
                          <Button size='sm' variant='outline' onClick={() => openImport('video')}><Upload />导入视频</Button>
                          {presentationShot.media && selectedState.canPreview ? <>
                            <p className='media-filename'>{presentationShot.media.name}</p>
                            <Button size='sm' variant='outline' onClick={() => { setTrimStart(String(selected.sourceIn / FPS)); setTrimDuration(String(selected.duration / FPS)); setModal('trim') }}><Scissors />裁剪视频</Button>
                            <Field><FieldLabel>播放速度</FieldLabel><Select value={String(selected.speed)} onValueChange={value => { const speed = Number(value); const duration = Math.min(selected.duration, Math.floor((selected.sourceDuration - selected.sourceIn) / speed)); patchShot({ speed, duration }) }}><SelectTrigger><SelectValue /></SelectTrigger><SelectContent><SelectGroup>{['0.5', '1', '1.5', '2'].map(value => <SelectItem key={value} value={value}>{value}×</SelectItem>)}</SelectGroup></SelectContent></Select></Field>
                            <div className='boundary-frames'>{selectedFrames.map(item => <button key={item.name} onClick={() => setPreviewAsset({ name: `${selected.name} · ${item.name}`, kind: 'image', src: item.src })}><img src={item.src} alt={item.name} /><span>{item.name}</span></button>)}</div>
                            <Button size='sm' variant='outline' disabled={!selectedState.canUseTailFrame} onClick={chainFrame}><ArrowRight />尾帧用于下一分镜</Button>
                          </> : <p className='property-note'>{selectedState.status === 'media_missing' ? '重新导入视频后继续编辑。' : '导入视频后可裁剪并查看首尾画面。'}</p>}
                        </> : null}
                      </FieldGroup>
                      </fieldset>
                      {propertyTab === 'video' && selectedState.hasVideo ? <Button variant='outline' size='sm' className='versions-button' onClick={() => setModal('versions')}><Video />查看视频版本（{Math.max(1, presentationShot.versions.length)}）</Button> : null}
                    </div></> : <Empty><EmptyHeader><EmptyTitle>选择一个分镜</EmptyTitle></EmptyHeader><EmptyContent><Button size='sm' variant='outline' onClick={addShot}>新增分镜</Button></EmptyContent></Empty>}
                </aside> : null}
                <div className='timeline-scroll'>
                  <div className='timeline-grid' style={{ '--timeline-width': `${timelineWidth}px` } as CSSProperties}>
                    <div className='track-label ruler-label'><span>轨道</span></div><div className='ruler' aria-label='时间刻度' onPointerDown={seekTimeline}>{Array.from({ length: Math.ceil(displayDuration / FPS / 2) + 1 }, (_, index) => <span key={index} style={{ left: `${index * 2 * FPS / displayDuration * 100}%` }}>{String(index * 2).padStart(2, '0')}s</span>)}</div>
                    {[
                      { key: 'script', name: '剧本', icon: FileText, tab: 'shot' },
                      { key: 'storyboard', name: '分镜', icon: Film, tab: 'storyboard' },
                      { key: 'reference', name: '参考资产', icon: ImageIcon, tab: 'reference' },
                      { key: 'video', name: '视频', icon: Video, tab: 'video' },
                    ].map(track => <div className='track-row' key={track.key}><div className='track-label'><track.icon /><span>{track.name}</span></div><div className={cn('track-lane', `track-${track.key}`)}>{displayShots.map((shot, index) => <div className='timeline-block-shell' key={shot.id} style={{ left: `${shot.from / displayDuration * 100}%`, width: `${shot.duration / displayDuration * 100}%` }}>
                      <button draggable={track.key === 'video' && !previewing} onDragStart={() => { dragShot.current = shot.id }} onDragEnd={() => { dragShot.current = null }} onDragOver={event => event.preventDefault()} onDrop={event => { event.preventDefault(); if (dragShot.current) { commit(moveShot(project, dragShot.current, shot.id)); dragShot.current = null } }} className={cn('timeline-block', selectedId === shot.id && 'is-selected', !shot.ready && 'is-empty')} aria-label={`${track.name}：${shot.name}`} aria-pressed={selectedId === shot.id} onClick={() => { selectShot(shot.id, track.tab); if (track.key === 'reference' && !previewing) setModal('assets') }} onDoubleClick={() => { if (track.key === 'script' || track.key === 'storyboard') beginInlineEdit(shot, track.key); else if (track.key === 'video') openInspector('video') }}>
                      {track.key === 'script' ? <><span className='block-number'>{String(index + 1).padStart(2, '0')}</span><span className='block-text'>{shot.script || '添加剧本'}</span></> : null}
                      {track.key === 'storyboard' ? <><span className='block-number'>{String(index + 1).padStart(2, '0')}</span><strong>{shot.name}</strong><span className='block-description'>{shot.storyboard || '设计画面与运镜'}</span></> : null}
                      {track.key === 'reference' ? shot.references.length ? <><div className='reference-thumbs'>{shot.references.slice(0, 3).map((item: any) => <span key={item.id}><MediaThumb item={item} /></span>)}</div><span>{shot.references.length} 个参考{shot.references.some((item: any) => item.role === '首帧') ? ' · 含首帧' : ''}</span></> : <span className='block-placeholder'><Plus />添加参考</span> : null}
                      {track.key === 'video' ? shot.ready ? <><div className='filmstrip'>{Array.from({ length: 4 }, (_, number) => <span key={number}>{shot.media?.firstFrame ? <img src={shot.media.firstFrame} alt='' /> : <SceneArt scene={shot.scene} frame={number * 30} />}</span>)}</div><div className='video-block-caption'><span>{String(index + 1).padStart(2, '0')} · {shot.name}</span><span>{shotLifecycle(shot).label} · {(shot.duration / FPS).toFixed(1)}s</span></div></> : <span className='block-placeholder'><Video />{shotLifecycle(shot).label} · {(shot.duration / FPS).toFixed(1)}s</span> : null}
                      </button>
                      {inlineEdit?.id === shot.id && inlineEdit.field === track.key ? <Textarea autoFocus className='timeline-inline-editor' aria-label={track.key === 'script' ? '编辑剧本' : '编辑画面与运镜'} value={inlineEdit.value} onChange={event => setInlineEdit({ ...inlineEdit, value: event.target.value })} onBlur={finishInlineEdit} onKeyDown={event => { event.stopPropagation(); if (event.key === 'Escape') { inlineCancelled.current = true; setInlineEdit(null) }; if (event.key === 'Enter' && (event.metaKey || event.ctrlKey)) event.currentTarget.blur() }} /> : null}
                      {track.key === 'video' && selectedId === shot.id && !previewing ? <span className='clip-resize-handle' role='slider' tabIndex={0} aria-label={`调整${shot.name}时长`} aria-valuemin={1 / FPS} aria-valuemax={Math.floor((shot.sourceDuration - shot.sourceIn) / shot.speed) / FPS} aria-valuenow={shot.duration / FPS} aria-valuetext={`${(shot.duration / FPS).toFixed(2)} 秒`} onPointerDown={event => startClipResize(event, shot)} onPointerMove={updateClipResize} onPointerUp={() => finishClipResize()} onPointerCancel={() => finishClipResize(true)} onKeyDown={event => { if (!['ArrowLeft', 'ArrowRight'].includes(event.key)) return; event.preventDefault(); event.stopPropagation(); const next = Math.max(1, Math.min(Math.floor((shot.sourceDuration - shot.sourceIn) / shot.speed), shot.duration + (event.key === 'ArrowRight' ? 1 : -1) * (event.shiftKey ? FPS : 1))); if (next !== shot.duration) commit(editShot(project, shot.id, { duration: next })) }} /> : null}
                    </div>)}</div></div>)}
                    <div className='track-row track-optional'><div className='track-label'><AudioLines /><span>成片音频</span></div><div className='track-lane'>{project.audio ? <button className='audio-item' onClick={() => setPreviewAsset(project.audio)}><AudioLines /><span>{project.audio.name}</span><span>0s — {(duration / FPS).toFixed(1)}s</span></button> : <button className='add-track-content' onClick={() => openImport('audio')}><Plus />添加配音或音乐</button>}</div></div>
                    <div className='track-row track-optional'><div className='track-label'><FileText /><span>字幕</span></div><div className='track-lane subtitle-lane'>{project.subtitles ? displayShots.map(shot => <div key={shot.id} className='timeline-block-shell' style={{ left: `${shot.from / displayDuration * 100}%`, width: `${shot.duration / displayDuration * 100}%` }}><button aria-label={`字幕：${shot.name}`} className={cn('subtitle-item', selectedId === shot.id && 'is-selected')} onClick={() => selectShot(shot.id, 'shot')} onDoubleClick={() => beginInlineEdit(shot, 'subtitle')}>{shot.subtitle || '添加字幕'}</button>{inlineEdit?.id === shot.id && inlineEdit.field === 'subtitle' ? <Textarea autoFocus className='timeline-inline-editor' aria-label='编辑字幕' value={inlineEdit.value} onChange={event => setInlineEdit({ ...inlineEdit, value: event.target.value })} onBlur={finishInlineEdit} onKeyDown={event => { event.stopPropagation(); if (event.key === 'Escape') { inlineCancelled.current = true; setInlineEdit(null) }; if (event.key === 'Enter' && (event.metaKey || event.ctrlKey)) event.currentTarget.blur() }} /> : null}</div>) : <button className='add-track-content' onClick={() => commit({ ...project, subtitles: true })}><Plus />显示字幕</button>}</div></div>
                    <div className='timeline-cursor' style={{ left: `calc(106px + ${Math.min(displayDuration, previewMode === 'shot' ? frame + (selected?.from ?? 0) : frame) / displayDuration * timelineWidth}px)` }}><span /></div>
                  </div>
                </div>
                </div>
                <div className='timeline-bottom'>
                  <div className='selection-summary'>{selected ? <><span className='selected-shot-name'>{selected.name}</span><button className='selection-state' data-shot-state={selectedState.status} title={selectedState.detail || selectedState.label} onClick={() => openInspector()}><StatusDot state={selectedState.tone as 'ok' | 'warn'} label={selectedState.label} /><span>{selectedState.label}</span></button><div className='duration-control'><Input aria-label='分镜时长（秒）' type='number' min={1 / FPS} step='0.1' value={resizePreview?.id === selected.id ? String(resizePreview.duration / FPS) : durationDraft} disabled={previewing} onChange={event => setDurationDraft(event.target.value)} onBlur={applyDuration} onKeyDown={event => { if (event.key === 'Enter') event.currentTarget.blur(); if (event.key === 'Escape') setDurationDraft(String(selected.duration / FPS)) }} /><span>秒</span></div></> : <span>选择分镜开始编辑</span>}</div>
                  <div className='selection-actions'>
                    <IconAction label='向前移动分镜' disabled={!selected || selectedIndex === 0 || previewing} onClick={() => moveSelected(-1)}><ArrowLeft /></IconAction><IconAction label='向后移动分镜' disabled={!selected || selectedIndex === shots.length - 1 || previewing} onClick={() => moveSelected(1)}><ArrowRight /></IconAction>
                    <IconAction label='循环播放此分镜' disabled={!selected || !selectedState.canPreview || previewAsset} onClick={() => setPreviewMode('shot')}><Play /></IconAction>
                    <Button variant='ghost' size='sm' disabled={!selected} onClick={() => openInspector()}>编辑分镜</Button>
                    <IconAction label='加入对话' disabled={!selected} onClick={() => selected && addToChat(selected.id)}><MessageCircle /></IconAction>
                    {selectedState?.canCancel ? <Button size='sm' variant='outline' onClick={() => setModal('cancel-generation')}>取消生成</Button> : null}
                    {selectedState?.frameAction ? <Button size='sm' variant='outline' onClick={() => setShotScenario('frames_processing')}>{selectedState.frameAction}</Button> : null}
                    {selectedState ? <Button size='sm' disabled={selectedState.primary.disabled || importing} onClick={openGeneration}><Sparkles />{selectedState.primary.label}</Button> : null}
                  </div>
                </div>
              </div>
            </TabsContent>
            <TabsContent value='assets' forceMount className='resource-panel project-library'>
              <div className='resource-header'><div><h2>项目资产库</h2><span>山间茶事 · {assets.length} 个资产</span></div><Button size='sm' variant='outline' onClick={() => openImport('library')}><Upload />导入资产</Button></div>
              <div className='library-controls'><Input aria-label='搜索项目资产' placeholder='搜索项目资产' value={assetSearch} onChange={event => setAssetSearch(event.target.value)} /><Tabs value={assetFilter} onValueChange={setAssetFilter}><TabsList><TabsTrigger value='all'>全部</TabsTrigger><TabsTrigger value='image'>图片</TabsTrigger><TabsTrigger value='video'>视频</TabsTrigger><TabsTrigger value='audio'>音频</TabsTrigger></TabsList></Tabs></div>
              {filteredAssets.length ? <div className='asset-grid'>{filteredAssets.map(item => <div key={item.id} className='asset-card'><button aria-label={`预览${item.name}`} onClick={() => setLibraryPreview(item)}><MediaThumb item={item} /></button><div><strong>{item.name}</strong><span>{item.src ? '项目资产' : '示例资产'}</span></div><Button size='sm' variant='ghost' onClick={() => openAssignment(item)} disabled={!shots.length}><Plus />引用到分镜</Button></div>)}</div> : <Empty><EmptyHeader><EmptyTitle>{assetSearch || assetFilter !== 'all' ? '没有符合条件的资产' : '暂无项目资产'}</EmptyTitle></EmptyHeader></Empty>}
            </TabsContent>
            <TabsContent value='tasks' className='resource-panel'><div className='resource-header'><div><h2>会话任务</h2><span>{requests.length} 个请求草稿</span></div></div>
              {shotScenario !== 'actual' && selectedState ? <div className='preview-task'><div><Badge variant='outline'>状态预览</Badge><strong>{selected?.name}</strong><StatusDot state={selectedState.tone as 'ok' | 'warn'} label={selectedState.label} /><span>{selectedState.label}</span></div><p>{selectedState.detail || '视频与资料按当前分镜状态展示。'}</p>{selectedState.canCancel ? <Button size='sm' variant='outline' onClick={() => setModal('cancel-generation')}>取消生成</Button> : null}<Button size='sm' variant='outline' onClick={() => setPanel('editor')}>定位分镜</Button></div> : null}
              {requests.length ? <div className='request-list'>{requests.map(request => <div className='request-item' key={request.id}><div><Badge variant='outline'>请求草稿</Badge><strong>{request.kind === 'chat' ? '对话请求' : '视频生成请求'}</strong></div><p>{request.instruction}</p><span>{request.shotIds.map(id => shots.find(shot => shot.id === id)?.name ?? '已删除分镜').join('、') || '整个视频'}</span><Button size='sm' variant='outline' onClick={() => downloadJson(request, `请求-${request.id}.json`)}><Download />下载请求</Button></div>)}</div> : shotScenario === 'actual' ? <Empty><EmptyHeader><EmptyTitle>暂无请求</EmptyTitle></EmptyHeader><EmptyContent><Button size='sm' variant='outline' onClick={() => setPanel('editor')}>返回编辑器</Button></EmptyContent></Empty> : null}
            </TabsContent>
          </Tabs>
        </section>
      </div>
    </main>

    <input ref={fileInput} type='file' accept='image/*,video/*,audio/*' hidden onChange={event => { const file = event.target.files?.[0]; if (file) onImport(file).catch(cause => { setImporting(false); setError(cause.message) }); event.target.value = '' }} />
    {notice ? <div className='notice' role='status'><Check />{notice}</div> : null}
    {importing ? <div className='notice' role='status'>读取媒体与提取画面中…</div> : null}
    <Dialog open={Boolean(libraryPreview)} onOpenChange={open => { if (!open) setLibraryPreview(null) }}><DialogContent className='demo-dialog library-preview-dialog'>
      <DialogHeader><DialogTitle>{libraryPreview?.name}</DialogTitle><DialogDescription>山间茶事 · 项目资产</DialogDescription></DialogHeader>
      {libraryPreview ? <div className='library-preview-media'>{libraryPreview.kind === 'video' ? <video src={libraryPreview.src} controls /> : libraryPreview.kind === 'audio' ? <audio src={libraryPreview.src} controls /> : <MediaThumb item={libraryPreview} />}</div> : null}
      <DialogFooter><Button size='sm' variant='outline' onClick={() => setLibraryPreview(null)}>关闭预览</Button><Button size='sm' disabled={!shots.length} onClick={() => openAssignment(libraryPreview)}><Plus />引用到分镜</Button></DialogFooter>
    </DialogContent></Dialog>
    <Dialog open={Boolean(error)} onOpenChange={open => { if (!open) setError('') }}><DialogContent><DialogHeader><DialogTitle>操作未提交</DialogTitle><DialogDescription>{error}</DialogDescription></DialogHeader><DialogFooter><Button size='sm' onClick={() => setError('')}>知道了</Button></DialogFooter></DialogContent></Dialog>
    <Dialog open={Boolean(modal)} onOpenChange={open => { if (!open) setModal(null) }}><DialogContent className={cn('demo-dialog', modal === 'assets' && 'asset-dialog')}>
      {modal === 'state-preview' ? <><DialogHeader><DialogTitle>状态预览</DialogTitle><DialogDescription>切换演示状态，不提交生成或渲染任务。</DialogDescription></DialogHeader><FieldGroup>
        <Field><FieldLabel>分镜状态</FieldLabel><Select value={shotScenario} onValueChange={setShotScenario} disabled={!selected}><SelectTrigger aria-label='分镜状态预览'><SelectValue /></SelectTrigger><SelectContent><SelectGroup><SelectItem value='actual'>当前数据</SelectItem>{shotPreviewStates.map(([value, label]) => <SelectItem key={value} value={value}>{label}</SelectItem>)}</SelectGroup></SelectContent></Select></Field>
        <Field><FieldLabel>Chat 状态</FieldLabel><Select value={chatScenario} onValueChange={setChatScenario}><SelectTrigger aria-label='Chat 状态预览'><SelectValue /></SelectTrigger><SelectContent><SelectGroup><SelectItem value='actual'>当前数据</SelectItem>{chatStates.map(([value, label]) => <SelectItem key={value} value={value}>{label}</SelectItem>)}</SelectGroup></SelectContent></Select></Field>
        <Field><FieldLabel>保存状态</FieldLabel><Select value={saveScenario} onValueChange={setSaveScenario}><SelectTrigger aria-label='保存状态预览'><SelectValue /></SelectTrigger><SelectContent><SelectGroup><SelectItem value='actual'>当前数据</SelectItem>{['保存中', '已保存', '保存失败'].map(value => <SelectItem key={value} value={value}>{value}</SelectItem>)}</SelectGroup></SelectContent></Select></Field>
        <Field><FieldLabel>导出状态</FieldLabel><Select value={exportScenario} onValueChange={setExportScenario}><SelectTrigger aria-label='导出状态预览'><SelectValue /></SelectTrigger><SelectContent><SelectGroup><SelectItem value='actual'>当前数据</SelectItem>{exportStates.map(([value, label]) => <SelectItem key={value} value={value}>{label}</SelectItem>)}</SelectGroup></SelectContent></Select></Field>
      </FieldGroup><DialogFooter><Button size='sm' variant='outline' onClick={resetStatePreview}>恢复当前数据</Button><Button size='sm' onClick={() => { setModal(exportScenario === 'actual' ? null : 'export'); setPanel('editor') }}>查看状态</Button></DialogFooter></> : null}
      {modal === 'cancel-generation' ? <><DialogHeader><DialogTitle>取消生成？</DialogTitle><DialogDescription>{selectedState?.hasVideo ? '当前采用的视频和创作资料会保留。' : '创作资料会保留。'}服务确认后显示取消结果。</DialogDescription></DialogHeader><DialogFooter><Button size='sm' variant='outline' onClick={() => setModal(null)}>继续等待</Button><Button size='sm' onClick={() => { setShotScenario('cancelling'); setModal(null) }}>确认取消</Button></DialogFooter></> : null}
      {modal === 'delete-active' ? <><DialogHeader><DialogTitle>删除正在生成的分镜？</DialogTitle><DialogDescription>已有视频保存在项目资产库。原任务返回的结果保留到任务记录。</DialogDescription></DialogHeader><DialogFooter><Button size='sm' variant='outline' onClick={() => setModal(null)}>保留分镜</Button><Button size='sm' onClick={finishDelete}>删除分镜</Button></DialogFooter></> : null}
      {modal === 'versions' ? <><DialogHeader><DialogTitle>视频版本</DialogTitle><DialogDescription>{selected?.name}{shotScenario !== 'actual' ? ' · 状态预览' : ''}</DialogDescription></DialogHeader><div className='video-versions'>
        {selectedState?.hasVideo ? <div className='version-entry'><div className='version-image'>{selected?.media ? <MediaThumb item={selected.media} /> : <SceneArt scene={selected?.scene} />}</div><div><strong>当前版本</strong><span>{selected?.media?.name || '示例视频'}</span><Badge variant='secondary'>当前采用</Badge></div></div> : null}
        {presentationShot?.versions.filter((version: any) => version.status !== 'active').map((version: any) => <div className='version-entry' key={version.id}><div className='version-image'>{version.media ? <MediaThumb item={version.media} /> : <SceneArt scene={selected?.scene} />}</div><div><strong>{version.media?.name || '新版本'}</strong><span>{version.isPreview ? '示例候选 · 状态预览' : version.status === 'candidate' ? '待采用' : '历史版本'}</span><Button size='sm' variant='outline' onClick={() => adoptVersion(version)}>{version.media && version.media.duration < selected.duration ? `采用并调整为 ${(version.media.duration / FPS).toFixed(1)}s` : '采用此版本'}</Button></div></div>)}
      </div><DialogFooter>{selectedState?.status === 'candidate' ? <Button size='sm' variant='outline' onClick={() => { if (shotScenario !== 'actual') setShotScenario(selectedState.hasVideo ? 'ready' : 'prepared'); setModal(null) }}>保留当前版本</Button> : <Button size='sm' variant='outline' onClick={() => setModal(null)}>返回编辑器</Button>}</DialogFooter></> : null}
      {modal === 'assets' ? <><DialogHeader><DialogTitle>选择项目资产</DialogTitle><DialogDescription>{selected ? `引用到「${selected.name}」` : '选择分镜后添加参考'}</DialogDescription></DialogHeader><div className='asset-picker'>{assets.map(item => <button key={item.id} onClick={() => { useAsset(item); setModal(null) }}><MediaThumb item={item} /><span>{item.name}</span></button>)}</div><DialogFooter><Button size='sm' variant='outline' onClick={() => { setModal(null); openImport('reference') }}><Upload />导入本地文件</Button></DialogFooter></> : null}
      {modal === 'assign' ? <><DialogHeader><DialogTitle>引用项目资产</DialogTitle><DialogDescription>{assignmentAsset?.name}</DialogDescription></DialogHeader><Field><FieldLabel>目标分镜</FieldLabel><Select value={assignmentTarget} onValueChange={setAssignmentTarget}><SelectTrigger aria-label='目标分镜'><SelectValue placeholder='选择目标分镜' /></SelectTrigger><SelectContent><SelectGroup>{shots.map(shot => <SelectItem key={shot.id} value={shot.id}>{shot.name}</SelectItem>)}</SelectGroup></SelectContent></Select></Field><DialogFooter><Button size='sm' variant='outline' onClick={() => setModal(null)}>取消</Button><Button size='sm' disabled={!shots.some(shot => shot.id === assignmentTarget)} onClick={() => { useAsset(assignmentAsset, assignmentTarget); setModal(null) }}>添加引用</Button></DialogFooter></> : null}
      {modal === 'generate' ? <><DialogHeader><DialogTitle>{selectedState?.hasVideo ? '重新生成分镜视频' : '生成分镜视频'}</DialogTitle><DialogDescription>{selected?.name} · {selected ? selected.duration / FPS : 0} 秒 · {project.ratio}{shotScenario !== 'actual' ? ' · 状态预览' : ''}</DialogDescription></DialogHeader><FieldGroup><Field><FieldLabel htmlFor='generation-prompt'>生成要求</FieldLabel><Textarea id='generation-prompt' rows={5} value={generationPrompt} onChange={event => setGenerationPrompt(event.target.value)} /></Field><div className='request-references'>{selected?.references.map((item: any) => <Badge key={item.id} variant='secondary'>{item.name} · {item.role}</Badge>)}</div>{selectedState?.hasVideo ? <p className='property-note'>新结果先进入版本列表，采用后替换当前视频。</p> : null}<div className='service-state'><StatusDot state='warn' label={shotScenario !== 'actual' ? '状态预览' : '生成服务未连接'} /><span>{shotScenario !== 'actual' ? '状态预览' : '生成服务未连接'}</span></div></FieldGroup><DialogFooter><Button size='sm' variant='outline' onClick={() => setModal(null)}>取消</Button><Button size='sm' disabled={!generationPrompt.trim()} onClick={() => { if (shotScenario !== 'actual') { setShotScenario('queued'); setModal(null) } else saveGeneration() }}>{shotScenario !== 'actual' ? '演示提交' : <><Download />保存生成请求</>}</Button></DialogFooter></> : null}
      {modal === 'export' ? <>
        <DialogHeader><DialogTitle>导出视频</DialogTitle><DialogDescription>{project.name} · {(duration / FPS).toFixed(1)} 秒{exportScenario !== 'actual' ? ' · 状态预览' : ''}</DialogDescription></DialogHeader>
        <fieldset disabled={['checking', 'queued', 'rendering', 'succeeded'].includes(exportScenario)} className='lifecycle-fields'><FieldGroup>
          <Field><FieldLabel>导出范围</FieldLabel><Select value={exportRange} onValueChange={setExportRange}><SelectTrigger><SelectValue /></SelectTrigger><SelectContent><SelectGroup><SelectItem value='all'>整个视频</SelectItem><SelectItem value='selected' disabled={!selected}>当前分镜{scope.length > 1 ? '与引用分镜' : ''}</SelectItem></SelectGroup></SelectContent></Select></Field>
          <Field><FieldLabel>分辨率</FieldLabel><Select value={exportResolution} onValueChange={setExportResolution}><SelectTrigger><SelectValue /></SelectTrigger><SelectContent><SelectGroup><SelectItem value='1080p'>1080p</SelectItem><SelectItem value='720p'>720p</SelectItem></SelectGroup></SelectContent></Select></Field>
          <Field orientation='horizontal'><FieldLabel htmlFor='subtitles-switch'>包含字幕</FieldLabel><Switch id='subtitles-switch' checked={project.subtitles} onCheckedChange={value => commit({ ...project, subtitles: value })} /></Field>
        </FieldGroup></fieldset>
        <div className='export-status-panel' data-export-state={exportScenario}><div className='export-check'><StatusDot state={exportScenario === 'succeeded' ? 'ok' : 'warn'} label={exportScenario === 'actual' ? '导出检查' : exportStates.find(item => item[0] === exportScenario)?.[1] || '导出状态'} /><strong>{exportScenario === 'actual' ? exportCheck.canExport ? '剪辑检查通过' : '导出条件未满足' : exportStates.find(item => item[0] === exportScenario)?.[1]}</strong></div>
          {exportScenario === 'actual' ? <p>{missingExport.length ? `${missingExport.length} 个分镜使用示意画面。` : ''}渲染服务未连接，可下载剪辑方案。</p> : null}
          {effectiveSaved !== '已保存' ? <p>当前修改尚未保存，保存后才能导出。</p> : null}
          {exportCheck.missingShotIds.length || exportScenario === 'blocked' ? <div className='export-issues'>{(exportCheck.missingShotIds.length ? exportCheck.missingShotIds : [selectedId]).filter(Boolean).map(id => <button key={id} onClick={() => { selectShot(id); if (exportScenario === 'blocked') setShotScenario('media_missing'); setModal(null) }}><Video />{shots.find(shot => shot.id === id)?.name} · 视频不可用<ArrowRight /></button>)}{!shots.length ? <p>新增分镜后再导出。</p> : null}</div> : null}
          {exportScenario === 'checking' ? <p>核对已保存的作品、视频、字幕与声音。</p> : null}
          {exportScenario === 'queued' || exportScenario === 'rendering' ? <p>使用提交时的作品版本。继续编辑不会改变本次导出。</p> : null}
          {exportScenario === 'failed' ? <p>编码失败，原素材和剪辑设置已保留。</p> : null}
          {exportScenario === 'cancelled' ? <p>本次导出已取消，作品仍可继续编辑。</p> : null}
          {exportScenario === 'stale' ? <p>已有导出版本保持不变。重新导出可包含当前修改。</p> : null}
          {exportScenario === 'succeeded' ? <div className='export-result-preview'><SceneArt scene='cup' /><span>成片示意 · 状态预览</span></div> : null}
        </div>
        <DialogFooter>
          {['queued', 'rendering'].includes(exportScenario) ? <Button size='sm' variant='outline' onClick={() => setExportScenario('cancelled')}>取消导出</Button> : <Button size='sm' variant='outline' onClick={() => setModal(null)}>返回编辑器</Button>}
          {['checking', 'queued', 'rendering'].includes(exportScenario) ? <Button size='sm' onClick={() => setExportScenario(exportScenario === 'checking' ? exportCheck.canExport ? 'queued' : 'blocked' : exportScenario === 'queued' ? 'rendering' : 'succeeded')}>推进导出状态<ArrowRight /></Button> : null}
          {['failed', 'cancelled', 'stale'].includes(exportScenario) ? <Button size='sm' onClick={() => setExportScenario('checking')}>{exportScenario === 'failed' ? '重试导出' : '重新导出'}</Button> : null}
          {exportScenario === 'actual' ? <Button size='sm' disabled={!exportCheck.canExport} onClick={() => { downloadJson({ ...project, shots: exportShots, export: { resolution: exportResolution, format: 'mp4', status: 'draft' } }, `${project.name}-剪辑方案.json`); setNotice('已下载剪辑方案'); setModal(null) }}><Download />下载剪辑方案</Button> : null}
        </DialogFooter>
      </> : null}
      {modal === 'trim' ? <><DialogHeader><DialogTitle>裁剪视频</DialogTitle><DialogDescription>{selected?.media?.name}</DialogDescription></DialogHeader><FieldGroup><Field><FieldLabel htmlFor='trim-start'>源视频开始（秒）</FieldLabel><Input id='trim-start' type='number' min='0' step='0.1' value={trimStart} onChange={event => setTrimStart(event.target.value)} /></Field><Field><FieldLabel htmlFor='trim-duration'>片段时长（秒）</FieldLabel><Input id='trim-duration' type='number' min='0.1' step='0.1' value={trimDuration} onChange={event => setTrimDuration(event.target.value)} /></Field></FieldGroup><DialogFooter><Button size='sm' variant='outline' onClick={() => setModal(null)}>取消</Button><Button size='sm' onClick={() => { const sourceIn = Math.round(Number(trimStart) * FPS); const duration = Math.round(Number(trimDuration) * FPS); if (!selected || sourceIn < 0 || duration < 1 || sourceIn + Math.ceil(duration * selected.speed) > selected.sourceDuration) { setError('剪辑范围超出源视频'); return }; patchShot({ sourceIn, duration }); setModal(null) }}>应用裁剪</Button></DialogFooter></> : null}
      {modal === 'create' || modal === 'rename' ? <>
        <DialogHeader><DialogTitle>{modal === 'create' ? '创建视频' : '视频设置'}</DialogTitle><DialogDescription className='sr-only'>设置视频名称和画幅</DialogDescription></DialogHeader>
        <FieldGroup>
          <Field><FieldLabel htmlFor='project-name'>视频名称</FieldLabel><Input id='project-name' value={projectName} placeholder='填写视频名称' onChange={event => setProjectName(event.target.value)} /></Field>
          <Field><FieldLabel>画幅</FieldLabel><Select value={projectRatio} onValueChange={setProjectRatio}><SelectTrigger><SelectValue /></SelectTrigger><SelectContent><SelectGroup><SelectItem value='16:9'>16:9 横屏</SelectItem><SelectItem value='9:16'>9:16 竖屏</SelectItem><SelectItem value='1:1'>1:1 方形</SelectItem></SelectGroup></SelectContent></Select></Field>
        </FieldGroup>
        <DialogFooter><Button size='sm' variant='outline' onClick={() => setModal(null)}>取消</Button><Button size='sm' disabled={!projectName.trim()} onClick={() => {
          commit(modal === 'create' ? { ...seedProject(), assets: project.assets, name: projectName.trim(), ratio: projectRatio, shots: [] } : { ...project, name: projectName.trim(), ratio: projectRatio })
          setSelectedId(modal === 'create' ? '' : selectedId)
          setModal(null)
          setPanel('editor')
        }}>{modal === 'create' ? '创建视频' : '保存设置'}</Button></DialogFooter>
      </> : null}
      {modal === 'connection' || modal === 'settings' || modal === 'workflows' || modal === 'skills' ? <><DialogHeader><DialogTitle>{modal === 'connection' ? '对话模型' : modal === 'workflows' ? '生成工作流' : modal === 'skills' ? '技能与专家' : '演示设置'}</DialogTitle><DialogDescription>{modal === 'connection' ? 'DeepSeek Flash · 对话服务未连接' : modal === 'workflows' ? '分镜生成使用工作流中配置的模型与参考输入。' : modal === 'skills' ? '创作技能与专家沿用 Studio 的已有入口。' : '编辑内容与导入素材保存在当前浏览器。'}</DialogDescription></DialogHeader>{modal === 'settings' ? <Field orientation='horizontal'><FieldLabel htmlFor='theme-switch'>暗色主题</FieldLabel><Switch id='theme-switch' checked={dark} onCheckedChange={setDark} /></Field> : null}<DialogFooter><Button variant='outline' size='sm' onClick={() => setModal(null)}>关闭</Button></DialogFooter></> : null}
    </DialogContent></Dialog>
  </div></TooltipProvider>
}

const root = document.getElementById('root')
if (!root) throw new Error('页面入口不存在')
createRoot(root).render(<Demo />)
