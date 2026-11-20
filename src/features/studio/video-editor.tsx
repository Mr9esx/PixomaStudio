import { useCallback, useEffect, useLayoutEffect, useMemo, useRef, useState, type ReactNode } from 'react'
import { useIsMutating, useMutation, useQuery, useQueryClient } from '@tanstack/react-query'
import { useBlocker } from '@tanstack/react-router'
import type { PlayerRef } from '@remotion/player'
import { Film, Plus, Play, Pause, Settings2, MessageCircle, Download, Trash2, Undo2, Redo2, X, StepBack, StepForward, SkipBack, SkipForward, Maximize, Volume2, VolumeX, Keyboard, Scan, Magnet } from 'lucide-react'
import { Button } from '@/components/ui/button'
import { Input } from '@/components/ui/input'
import { Checkbox } from '@/components/ui/checkbox'
import { Textarea } from '@/components/ui/textarea'
import { Field, FieldError, FieldGroup, FieldLabel } from '@/components/ui/field'
import { Select, SelectContent, SelectGroup, SelectItem, SelectTrigger, SelectValue } from '@/components/ui/select'
import { Popover, PopoverContent, PopoverTrigger } from '@/components/ui/popover'
import { Switch } from '@/components/ui/switch'
import { Alert, AlertTitle } from '@/components/ui/alert'
import { useMediaResource } from '@/features/cases/lib/use-media-object-url'
import { WorkflowParameterFields } from '@/features/cases/workflow-parameter-fields'
import { chooseVideoShotWorkflow, editVideoShotInput, videoDescriptionInputState } from './video-input-sources'
import { VideoWorkflowBatchDialog } from './video-workflow-batch-dialog'
import { applyVideoWorkflowBatch } from './video-workflow-batch'
import { VideoGenerationBatchDialog } from './video-generation-batch-dialog'
import { validateVideoGenerationBatchSelection, videoGenerationBatchPendingSchema, videoGenerationConnectionIssue, type VideoGenerationBatchPending } from './video-generation-batch-review'
import { createVideoGenerationBatch, getVideoGenerationBatch, type VideoGenerationDependency } from '@/lib/api/video-projects'
import { createStudioSkill, createStudioTextAsset, exportStudioAssetVersion, getStudioAsset, studioLibraryAssetsPath, uploadStudioAsset, type StudioAsset, type StudioLibraryAssetsPage, type StudioSkillSummary } from '@/lib/api/studio'
import type { StudioEditorRequest } from './studio-chat'
import { cancelWorkflowExecution, getWorkflowExecution, getWorkflowVersion, listWorkflowVersions, listWorkflows, stopWaitingForWorkflowExecution, type WorkflowExecution, type WorkflowVersion } from '@/lib/api/workflows'
import { VideoWorkflowVersionDialog } from './video-workflow-version-dialog'
import { applyVideoWorkflowVersion } from './video-workflow-version'
import { getVideoEditorState, saveVideoEditorState } from '@/lib/api/video-editor-state'
import { ApiError, apiFetch, desktopInvoke } from '@/lib/api/client'
import { registerDesktopExitGuard } from '@/lib/desktop-exit'
import type { VideoTextDraft } from './video-text-draft'
import { getComputeNode } from '@/lib/api/compute-nodes'
import { listGenerationConnections } from '@/lib/api/generation'
import { collectVideoMedia, emptyVideoProject, fitVideoProjectTiming, newVideoShot, normalizeVideoProject, previewVideoShot, removeVideoReference, resolveVideoReferenceSource, reviseVideoShotContent, setVideoReference, videoReferenceRoleLabels, videoReferenceRoles, validateVideoProject, videoTimelineFrames, videoGenerationChanged, type VideoAudio, type VideoMedia, type VideoProject, type VideoShot, type VideoReference } from './video-editor-model'
import { VideoTimeline } from './video-timeline'
import { videoProjectTiming, videoShotDuration } from './video-editor-model'
import { duplicateVideoShot, serializeVideoContent, videoReferencePosition, videoReferenceSourceChanged } from './video-editor-model'
import { fitTimelineZoom, type VideoTrackDisplay } from './video-timeline-model'
import { VideoEditorLayout } from './video-editor-layout'
import { VideoScriptPanel } from './video-script-panel'
import { VideoScriptSources } from './video-script-sources'
import { addVideoScriptBlock, normalizeProjectScript, importVideoScript, type VideoScript } from './video-script'
import { VideoProjectSettings } from './video-project-settings'
import { restoreVideoEditorHistory, updateVideoGenerationState, videoHistoryProblem, type VideoGenerationState } from './video-editor-history'
import { VideoPropertyField } from './video-property-field'
import { editVideoNumber, renameVideoShot, setVideoAudioLoop, type VideoNameDraft, type VideoNumberDraft, type VideoNumberTarget } from './video-property-edit'
import { applyUploadedVideoInput, applyUploadedVideoMedia, videoGenerationInputIssues, type VideoInputUploadTarget } from './video-generation-inputs'
import { applyVideoPromptDraft, createVideoPromptDraft, videoPromptDraftKey, videoPromptFields, type VideoPromptDraft } from './video-prompt-draft'
import { VideoPromptDialog } from './video-prompt-dialog'
import { VideoAdoptionDialog } from './video-adoption-dialog'
import { createVideoAdoption, previewVideoAdoption, type VideoAdoption } from './video-adoption'
import { VideoMaterialChats } from './video-material-chats'
import { VideoFrameReference, type VideoFrameSelection } from './video-frame-reference'
import { VideoReferenceVersions } from './video-reference-versions'
import { VideoShotStructureDialog, type VideoStructureRequest } from './video-shot-structure-dialog'
import { createVideoStructureDraft, videoStructureDraftKey, type VideoStructureDraft } from './video-structure-draft'
import { applyVideoShotStructure } from './video-shot-structure'
import { applyVideoReferenceVersion } from './video-reference-update'
import { createVideoGeneration, listVideoGenerations, videoGenerationPending, createStudioVideoFrame, type StudioVideoWork, type VideoReferenceLocation } from '@/lib/api/video-projects'
import { VideoGenerationRecords, videoGenerationLabels } from './video-generation-records'
import { VideoExportDialog } from './video-export-dialog'
import { VideoExportHistory } from './video-export-history'
import { VideoProjectRecovery } from './video-project-recovery'
import { applyVideoExportStatus, createVideoExportTask, videoExportPending, videoExportStatusSchema, type VideoExportIssue, type VideoExportSnapshot, type VideoExportTask } from './video-export-model'
import { storeVideoExportTask, videoExportTaskOptions } from './video-export-state'
import { exportVideoWork, getVideoWorkExport } from '@/lib/api/video-work-exports'
import { VideoAssetLibrary, type VideoAssetImport } from './video-asset-library'
import { VideoAssetInfo } from './video-asset-info'
import { VideoAssetPreview } from './video-asset-preview'
import { VideoMediaError } from './video-media-error'
import { VideoProjectPreview } from './video-project-preview'
import { Dialog, DialogContent, DialogHeader, DialogTitle, DialogFooter } from '@/components/ui/dialog'
import scriptSkillContent from './skills/pixoma-studio-video-creator/SKILL.md?raw'
import { serializeDraft } from './skill-document'
import { mergeVideoProjects } from './video-project-merge'
import { restoreVideoEditorView, selectedVideoAsset, videoPreviewFrame, type VideoAssetBrowserState, type VideoAssetPreviewState, type VideoEditorViewState, type VideoTimelineViewport } from './video-editor-view'
import './video-editor.css'

function FrameImage({ media, label }: { media?: { url: string }; label: string }) {
  const resource = useMediaResource(media?.url)
  const [failedSource, setFailedSource] = useState<string>()
  const failure = resource.error?.message || (resource.src && failedSource === resource.src ? `${label}无法显示` : '')
  if (failure) return <VideoMediaError message={failure} loading={resource.isFetching} onRetry={() => { void resource.refetch() }} />
  return resource.src ? <img className='w-full rounded border' src={resource.src} alt={label} onError={() => setFailedSource(resource.src)} /> : <span className='text-xs text-muted-foreground'>{media ? `正在读取${label}` : `${label}尚未就绪`}</span>
}
function VideoVersionPreview({ media, onReady }: { media: VideoMedia; onReady?: (ready: boolean) => void }) {
  const resource = useMediaResource(media.url)
  const [failedSource, setFailedSource] = useState<string>()
  const failure = resource.error?.message || (resource.src && failedSource === resource.src ? '视频版本无法播放' : '')
  useEffect(() => onReady?.(false), [resource.src, failure, onReady])
  if (failure) return <VideoMediaError message={failure} loading={resource.isFetching} onRetry={() => { setFailedSource(undefined); onReady?.(false); void resource.refetch() }} />
  return resource.src ? <video className='w-full rounded border' src={resource.src} controls preload={onReady ? 'auto' : 'metadata'} aria-label='预览视频版本' onCanPlay={event => onReady?.(Number.isFinite(event.currentTarget.duration) && event.currentTarget.duration > 0 && event.currentTarget.videoWidth > 0)} onError={() => { setFailedSource(resource.src); onReady?.(false) }} /> : <span className='text-xs text-muted-foreground'>正在读取视频版本</span>
}
const labels: Record<string, string> = { pending: '等待执行', dispatching: '正在提交', running: '生成中', collecting: '正在保存视频', succeeded: '视频已生成', failed: '生成失败', cancelled: '已取消', result_unknown: '结果无法确认', submission_unknown: '提交结果未知' }
type ProjectRecord = { asset_id: string | null; asset_version: number; project: VideoProject }
const active = (status?: string) => Boolean(status && !['succeeded', 'failed', 'cancelled', 'result_unknown'].includes(status))

export function StudioVideoEditor({ active: editorActive, sessionId, activeChatId, videoId, projectId, assets: sessionAssets, skills, canInstallScriptSkill, scriptRequests, onScriptRequest: sendScriptRequest, onReference, onOpenVideoChat, referenceLocation }: { active: boolean; sessionId: string; activeChatId?: string; videoId?: string; projectId?: string; assets: StudioAsset[]; skills: StudioSkillSummary[]; canInstallScriptSkill: boolean; scriptRequests: ReactNode; onScriptRequest: (request: StudioEditorRequest) => Promise<void>; onReference: (asset: StudioAsset) => void; onOpenVideoChat?: (work: StudioVideoWork, sessionId: string) => Promise<void>; referenceLocation?: VideoReferenceLocation }) {
  const client = useQueryClient()
  const projectAssets = useQuery({ queryKey: ['studio', 'video-project-assets', projectId ?? ''], queryFn: async () => {
    const found: StudioAsset[] = []; let cursor: string | undefined
    do { const page = await apiFetch<StudioLibraryAssetsPage>(studioLibraryAssetsPath({ projectId: projectId ?? '', limit: 100, cursor })); found.push(...page.items.map(item => item.asset)); cursor = page.next_cursor } while (cursor)
    return found
  } })
  const assets = useMemo(() => [...new Map([...(projectAssets.data ?? []), ...sessionAssets].map(asset => [asset.id, asset])).values()], [projectAssets.data, sessionAssets])
  const [project, setProject] = useState<VideoProject>(emptyVideoProject)
  const [selectedId, setSelectedId] = useState('')
  const [selectedAudioId, setSelectedAudioId] = useState<string>()
  const [error, setError] = useState('')
  const [dirty, setDirty] = useState(false)
  const [leaving, setLeaving] = useState(false)
  const [leaveError, setLeaveError] = useState('')
  const [desktopExitPending, setDesktopExitPending] = useState(false)
  const [version, setVersion] = useState(-1)
  const currentVersion = useRef(version); currentVersion.current = version
  const [inspector, setInspector] = useState(false)
  const [tab, setTab] = useState('shot')
  const [past, setPast] = useState<VideoProject[]>([])
  const [future, setFuture] = useState<VideoProject[]>([])
  const [frame, setFrame] = useState(0)
  const [playing, setPlaying] = useState(false)
  const [loop, setLoop] = useState(false)
  const [previewShotId, setPreviewShotId] = useState('')
  const [monitorVolume, setMonitorVolume] = useState(1)
  const audibleVolume = useRef(1)
  const [shortcutsOpen, setShortcutsOpen] = useState(false)
  const [timeInput, setTimeInput] = useState<string>()
  const [timeError, setTimeError] = useState('')
  const previewPosition = useRef(0)
  const [zoom, setZoom] = useState(1)
  const [snap, setSnap] = useState(true)
  const [fitRequest, setFitRequest] = useState(0)
  const [batchSelection, setBatchSelection] = useState<string[]>([])
  const [workflowBatchOpen, setWorkflowBatchOpen] = useState(false)
  const [generationBatchOpen, setGenerationBatchOpen] = useState(false)
  const [pendingGenerationBatch, setPendingGenerationBatch] = useState<VideoGenerationBatchPending>()
  const timelineBody = useRef<HTMLDivElement>(null)
  const [library, setLibrary] = useState(false)
  const [previewOpen, setPreviewOpen] = useState(true)
  const [assetPreview, setAssetPreview] = useState<VideoAssetPreviewState>()
  const [assetVersions, setAssetVersions] = useState<Record<string, string>>({})
  const [assetBrowser, setAssetBrowser] = useState<VideoAssetBrowserState>({ search: '', assetKind: 'all', assetView: 'grid', assetScrollTop: 0 })
  const [timelineViewport, setTimelineViewport] = useState<VideoTimelineViewport>({ scrollLeft: 0, scrollTop: 0 })
  const [trackDisplay, setTrackDisplay] = useState<VideoTrackDisplay>({ trackHeights: {}, collapsedTracks: [] })
  const [numberDrafts, setNumberDrafts] = useState<Record<string, VideoNumberDraft>>({})
  const [nameDrafts, setNameDrafts] = useState<Record<string, VideoNameDraft>>({})
  const [promptDrafts, setPromptDrafts] = useState<Record<string, VideoPromptDraft>>({})
  const [promptTarget, setPromptTarget] = useState<string>()
  const promptDraft = promptTarget ? promptDrafts[promptTarget] : undefined
  const promptWorkflow = useQuery({ queryKey: ['studio', 'video-workflow-version', promptDraft?.workflowId, promptDraft?.versionId], enabled: Boolean(promptDraft), queryFn: () => getWorkflowVersion(promptDraft!.workflowId, promptDraft!.versionId), retry: false })
  const previewRequest = useQuery({ queryKey: ['studio', 'video-preview-asset', assetPreview?.assetId], enabled: editorActive && Boolean(assetPreview), queryFn: () => getStudioAsset(assetPreview!.assetId), retry: false })
  const previewSource = previewRequest.isError ? undefined : previewRequest.data
  const previewAsset = previewSource && assetPreview ? selectedVideoAsset(previewSource, assetPreview.versionId) : undefined
  const [assetImports, setAssetImports] = useState<VideoAssetImport[]>([])
  const importsRunning = useRef(false)
  const [seekFrame, setSeekFrame] = useState<number>()
  const [assetAction, setAssetAction] = useState<{ asset: StudioAsset; versionId: string; shotId: string; track: string; time: number; newShot?: VideoShot; error?: string }>()
  const [scriptImport, setScriptImport] = useState<{ baseline: VideoScript; content: string; name: string; error?: string }>()
  const [recovery, setRecovery] = useState<{ snapshot: VideoExportSnapshot; baseline: VideoProject }>()
  const [referenceRole, setReferenceRole] = useState<VideoReference['role']>('reference')
  const [referenceInputTarget, setReferenceInputTarget] = useState<VideoInputUploadTarget>()
  const referenceShot = project.shots.find(shot => shot.id === assetAction?.shotId)
  const referenceWorkflow = useQuery({ queryKey: ['studio', 'video-workflow-version', referenceShot?.workflowId, referenceShot?.versionId],
    enabled: Boolean(assetAction && assetAction.track !== 'video' && referenceShot?.workflowId && referenceShot.versionId),
    queryFn: () => getWorkflowVersion(referenceShot!.workflowId!, referenceShot!.versionId!), retry: false })
  const referenceSource = referenceWorkflow.data?.definition
  const [scriptTarget, setScriptTarget] = useState<{ scope: 'script' } | { scope: 'storyboard'; shotIds?: string[] }>()
  const [ideas, setIdeas] = useState('')
  const [textDraft, setTextDraft] = useState<VideoTextDraft>()
  const [scriptSkillId, setScriptSkillId] = useState('')
  const [scriptSending, setScriptSending] = useState(false)
  const [scriptError, setScriptError] = useState('')
  const [generationSettingsOpen, setGenerationSettingsOpen] = useState(false)
  const [workflowVersionTarget, setWorkflowVersionTarget] = useState<{ shot: VideoShot; from: WorkflowVersion; targetId: string }>()
  const [generationRecordsOpen, setGenerationRecordsOpen] = useState(false)
  const [removeRequest, setRemoveRequest] = useState<VideoShot>()
  const [exportSnapshot, setExportSnapshot] = useState<VideoExportSnapshot>()
  const [exportOpen, setExportOpen] = useState(false)
  const [exportHistoryOpen, setExportHistoryOpen] = useState(false)
  const [timingChange, setTimingChange] = useState<{ previous: VideoProject; next: VideoProject; audio: VideoAudio[]; group?: string; title?: string }>()
  const [adoption, setAdoption] = useState<VideoAdoption>()
  const [materialVideoId, setMaterialVideoId] = useState<string>()
  const [frameTargetId, setFrameTargetId] = useState<string>()
  const [referenceVersionTarget, setReferenceVersionTarget] = useState<VideoReference>()
  const locatedReference = useRef<string | undefined>(undefined)
  const [copyRequest, setCopyRequest] = useState<{ source: VideoShot; reuseMedia: boolean; error?: string }>()
  const [structureDrafts, setStructureDrafts] = useState<Record<string, VideoStructureDraft>>({})
  const [structureTarget, setStructureTarget] = useState<string>()
  const [structureMenuOpen, setStructureMenuOpen] = useState(false)
  const [scriptFocus, setScriptFocus] = useState<{ blockId: string; requestId: string }>()
  const scriptSkills = skills.filter(skill => skill.enabled && /PixomaStudioVideoCreator|脚本|剧本|screenplay|script.?writ/i.test(`${skill.name} ${skill.description}`))
  const chosenScriptSkill = scriptSkillId || skills.find(skill => skill.enabled && skill.name === 'pixoma-studio-video-creator')?.id || (scriptSkills.length === 1 ? scriptSkills[0].id : '')
  const installScriptSkill = useMutation({ mutationFn: () => createStudioSkill({ ...serializeDraft({ files: [{ path: 'SKILL.md', content: scriptSkillContent }], enabled: true }), version: '1.0.0' }, 'studio'), onSuccess: async skill => { setScriptSkillId(skill.id); await client.invalidateQueries({ queryKey: ['studio', 'skills'] }) }, onError: cause => setScriptError(cause.message) })
  const workflowChoices = useRef(new Map<string, number>())
  const editorRoot = useRef<HTMLElement>(null)
  const player = useRef<PlayerRef>(null)
  const [playerTarget, setPlayerTarget] = useState<PlayerRef | null>(null)
  const mountPlayer = useCallback((instance: PlayerRef | null) => { player.current = instance; setPlayerTarget(instance) }, [])
  useEffect(() => {
    if (editorActive && previewOpen) return
    player.current?.pause()
    editorRoot.current?.querySelectorAll<HTMLMediaElement>('video, audio').forEach(media => media.pause())
  }, [editorActive, previewOpen])
  const current = useRef(project); current.current = project
  const savedProject = useRef(project)
  const historyGroup = useRef<string | undefined>(undefined)
  const restoredDraft = useRef(false)
  const draftKey = `video-editor-draft:${sessionId}`
  const viewKey = `video-editor-view:${sessionId}`
  const [viewLoaded, setViewLoaded] = useState(false)
  const viewSnapshot = useRef<VideoEditorViewState | undefined>(undefined)
  const lastSavedView = useRef<string | undefined>(undefined)
  const lastFailedView = useRef<string | undefined>(undefined)
  const [viewError, setViewError] = useState('')
  const selected = project.shots.find(shot => shot.id === selectedId)
  const timing = videoProjectTiming(project)
  const total = timing.knownDuration
  const sequence = timing.unknownCount > 0
  const previousSequence = useRef(sequence)
  const timelineFrames = videoTimelineFrames(project.shots, project.fps)
  const base = videoId ? `/api/v1/studio/video-projects/${encodeURIComponent(videoId)}` : `/api/v1/studio/sessions/${encodeURIComponent(sessionId)}/video-project`
  const recordKey = ['studio', 'video-project', sessionId, sessionAssets.find(asset => asset.name === 'video-project.json.md')?.current_version]
  const record = useQuery({ queryKey: recordKey, queryFn: () => apiFetch<ProjectRecord>(base) })
  const workId = videoId ?? record.data?.asset_id
  const editorState = useQuery({ queryKey: ['studio', 'video-editor-state', workId], enabled: Boolean(workId), queryFn: () => getVideoEditorState(workId!), retry: false, refetchOnWindowFocus: false })
  const saveEditorState = useMutation({ mutationKey: ['studio', 'video-editor-state', workId], scope: { id: `video-editor-state:${workId ?? sessionId}` }, mutationFn: saveVideoEditorState,
    onSuccess: (state, input) => { lastSavedView.current = JSON.stringify(input.state); lastFailedView.current = undefined; setViewError(''); client.setQueryData(['studio', 'video-editor-state', input.videoId], state) },
    onError: (_cause, input) => { lastFailedView.current = JSON.stringify(input.state) } })
  const persistEditorView = useCallback(async () => {
    if (!viewSnapshot.current) return
    const state = structuredClone(viewSnapshot.current)
    const serialized = JSON.stringify(state)
    localStorage.setItem(viewKey, serialized)
    if (!workId || (serialized === lastSavedView.current && !client.isMutating({ mutationKey: ['studio', 'video-editor-state', workId] }))) return
    if (!editorState.isSuccess) throw new Error('编辑状态尚未读取完成')
    await saveEditorState.mutateAsync({ videoId: workId, sessionId: activeChatId ?? sessionId, state })
  }, [viewKey, workId, editorState.isSuccess, activeChatId, sessionId, saveEditorState.mutateAsync, client])
  const exportOptions = videoExportTaskOptions(sessionId, workId ?? undefined)
  const exportRecord = useQuery(exportOptions)
  const exportTask = exportRecord.data
  const exportPending = videoExportPending(exportTask)
  const visibleExportTask = exportTask && (!exportSnapshot || exportSnapshot.id === exportTask.request.request_id) ? exportTask : undefined
  const dialogSnapshot = exportSnapshot ?? (exportTask ? { id: exportTask.request.request_id, project: exportTask.project, sourceVersion: exportTask.sourceVersion } : undefined)
  useEffect(() => {
    if (!workId) return
    const saved = localStorage.getItem(`video-generation-batch-submit:${workId}`)
    setPendingGenerationBatch(saved ? videoGenerationBatchPendingSchema.parse(JSON.parse(saved)) : undefined)
  }, [workId])
  const generationRecords = useQuery({ queryKey: ['studio', 'video-generations', workId], enabled: Boolean(workId), queryFn: () => listVideoGenerations(workId!),
    refetchInterval: query => query.state.data?.some(videoGenerationPending) || project.shots.some(shot => shot.generationRequest) ? 2000 : false, retry: false })
  const external = Boolean(record.data && version >= 0 && record.data.asset_version !== version)
  const enabledWorkflows = useQuery({ queryKey: ['studio', 'workflows', 'enabled-video-editor'], queryFn: () => listWorkflows({ enabled: true }), refetchInterval: 5000 })
  const workflows = useQuery({ queryKey: ['studio', 'video-workflows', enabledWorkflows.data?.map(item => `${item.id}:${item.default_version_id}`).join(',')], enabled: enabledWorkflows.isSuccess, queryFn: async () => {
    const published = await Promise.all((enabledWorkflows.data ?? []).filter(item => item.default_version_id).map(async item => ({ workflow: item, version: await getWorkflowVersion(item.id, item.default_version_id!) })))
    return published.filter(item => item.version.definition.outputs.some(output => output.type === 'video')).map(item => item.workflow)
  } })
  const definition = useQuery({ queryKey: ['studio', 'video-workflow-version', selected?.workflowId, selected?.versionId], enabled: Boolean(selected?.workflowId && selected?.versionId), queryFn: () => getWorkflowVersion(selected!.workflowId!, selected!.versionId!) })
  const workflowVersions = useQuery({ queryKey: ['studio', 'video-workflow-versions', selected?.workflowId], enabled: Boolean(generationSettingsOpen && selected?.workflowId), queryFn: () => listWorkflowVersions(selected!.workflowId!), refetchInterval: generationSettingsOpen ? 5000 : false })
  const executions = useQuery({ queryKey: ['studio', 'video-executions', sessionId, project.shots.map(shot => shot.executionId).join(',')], enabled: project.shots.some(shot => shot.executionId), queryFn: async () => Promise.all(project.shots.filter(shot => shot.executionId).map(async shot => ({ shotId: shot.id, execution: await getWorkflowExecution(shot.executionId!) }))), refetchInterval: query => query.state.data?.some(item => active(item.execution.status) && item.execution.waiting) ? 2000 : false, retry: false })
  const generationJobs = useMemo(() => [...new Map<string, { shotId: string; execution: WorkflowExecution; recorded: boolean }>([
    ...(executions.data ?? []).map(item => [item.execution.id, { ...item, recorded: false }] as const),
    ...(generationRecords.data ?? []).flatMap(item => item.execution ? [[item.execution.id, { shotId: item.shot_id, execution: item.execution, recorded: true }] as const] : []),
  ]).values()].sort((a, b) => b.execution.created_at.localeCompare(a.execution.created_at)), [executions.data, generationRecords.data])
  const selectedGeneration = generationRecords.data?.find(item => item.shot_id === selectedId)
  const execution = selectedGeneration ? selectedGeneration.execution ?? undefined : executions.data?.find(item => item.shotId === selectedId)?.execution
  const busy = active(execution?.status) && execution?.waiting !== false
  const source = definition.data?.definition
  const nodeId = source?.type === 'comfyui' ? source.execution.node_id : undefined
  const node = useQuery({ queryKey: ['studio', 'compute-node', nodeId], enabled: Boolean(nodeId), queryFn: () => getComputeNode(nodeId!), refetchInterval: 5000 })
  const waitingForNode = source?.type === 'comfyui' && (!node.isSuccess || node.data.healthy !== true)
  const connections = useQuery({ queryKey: ['studio', 'generation-connections'], queryFn: listGenerationConnections, enabled: source?.type === 'online', refetchInterval: 5000 })
  const connectionIssue = source?.type === 'online' && !selected?.generationRequest ? connections.error?.message || videoGenerationConnectionIssue(source, connections.isSuccess ? connections.data : undefined) : undefined
  const boundWorkflow = enabledWorkflows.data?.find(item => item.id === selected?.workflowId)
  const workflowOptions = boundWorkflow && source?.outputs.some(output => output.type === 'video') && !workflows.data?.some(item => item.id === boundWorkflow.id) ? [...(workflows.data ?? []), boundWorkflow] : workflows.data ?? []
  function applyCommit(next: VideoProject, group?: string) { const previous = current.current; if (JSON.stringify(previous) === JSON.stringify(next)) return; next = { ...next, shots: next.shots.map(shot => reviseVideoShotContent(previous.shots.find(item => item.id === shot.id), shot)) }; current.current = next; if (!group || historyGroup.current !== group) setPast(history => [...history, previous].slice(-50)); historyGroup.current = group; setFuture([]); setProject(next); setDirty(true) }
  function commit(next: VideoProject, group?: string) {
    const normalized = fitVideoProjectTiming(next), previous = current.current
    const audio = previous.audio?.filter(item => { const changed = normalized.audio?.find(audio => audio.id === item.id); return !changed || changed.duration < item.duration }) ?? []
    if (audio.length) { setTimingChange({ previous, next: normalized, audio, group }); return }
    applyCommit(normalized, group)
  }
  function patchShot(changes: Partial<VideoShot>, id = selectedId, group?: string) { commit({ ...current.current, shots: current.current.shots.map(shot => shot.id === id ? { ...shot, ...changes } : shot) }, group ? `${id}:${group}` : undefined) }
  function patchGenerationState(changes: Partial<VideoGenerationState>, shotId: string) {
    const next = updateVideoGenerationState(current.current, shotId, changes)
    if (next === current.current) return next
    current.current = next; setProject(next); setDirty(true)
    setPast(history => history.map(snapshot => updateVideoGenerationState(snapshot, shotId, changes)))
    setFuture(history => history.map(snapshot => updateVideoGenerationState(snapshot, shotId, changes)))
    return next
  }
  function select(id: string, section?: string, openInspector = true) {
    setSelectedId(id)
    if (section) { setTab(section); setInspector(openInspector) }
    player.current?.pause(); setAssetPreview(undefined)
    const index = current.current.shots.findIndex(shot => shot.id === id)
    if (index < 0) return
    const timing = videoProjectTiming(current.current)
    setLoop(false); setPreviewShotId(timing.unknownCount ? id : '')
    setSeekFrame(timing.unknownCount ? 0 : Math.round((timing.starts[index] ?? 0) * current.current.fps))
  }
  function restore(source: VideoProject) { const next = normalizeVideoProject(source); current.current = next; savedProject.current = next; setProject(next); setSelectedId(id => next.shots.some(shot => shot.id === id) ? id : next.shots[0]?.id ?? ''); setPast([]); setFuture([]); setDirty(false) }
  useEffect(() => {
    if (!record.data) return
    if (!restoredDraft.current) {
      restoredDraft.current = true
      const saved = localStorage.getItem(draftKey)
      if (saved) {
        const draft = JSON.parse(saved) as { version: number; project: VideoProject; baseline?: VideoProject }
        restore(draft.project); savedProject.current = normalizeVideoProject(draft.baseline ?? record.data.project); setVersion(draft.version); setDirty(true); return
      }
    }
    if (!dirty && record.data.asset_version !== version) { restore(record.data.project); setVersion(record.data.asset_version) }
  }, [record.data, dirty, version, draftKey])
  useEffect(() => {
    if (version < 0) return
    if (dirty) localStorage.setItem(draftKey, JSON.stringify({ version, project, baseline: savedProject.current }))
    else localStorage.removeItem(draftKey)
  }, [dirty, version, project, draftKey])
  useEffect(() => {
    if (version < 0 || viewLoaded || (workId && !editorState.isSuccess)) return
    const cached = localStorage.getItem(viewKey)
    const saved = workId && editorState.data !== null ? editorState.data : cached ? JSON.parse(cached) : {}
    if (workId && editorState.data !== null) lastSavedView.current = JSON.stringify(editorState.data)
    const view = restoreVideoEditorView(current.current, saved)
    setSelectedId(view.selectedId); setSelectedAudioId(view.selectedAudioId || undefined); setTab(view.tab)
    setInspector(view.inspector); setPreviewShotId(view.previewShotId); setLoop(view.loop)
    setZoom(view.zoom); setSnap(view.snap); setLibrary(view.library); setMonitorVolume(view.monitorVolume)
    setAssetVersions(view.assetPreview ? { ...view.assetVersions, [view.assetPreview.assetId]: view.assetPreview.versionId } : view.assetVersions)
    setAssetPreview(view.assetPreview)
    setAssetBrowser({ search: view.search, assetKind: view.assetKind, assetView: view.assetView, assetScrollTop: view.assetScrollTop })
    setTimelineViewport({ scrollLeft: view.scrollLeft, scrollTop: view.scrollTop })
    setTrackDisplay({ trackHeights: view.trackHeights, collapsedTracks: view.collapsedTracks })
    setBatchSelection(view.batchSelection)
    setNumberDrafts(view.numberDrafts)
    setNameDrafts(view.nameDrafts)
    setPromptDrafts(view.promptDrafts)
    setStructureDrafts(view.structureDrafts)
    setIdeas(view.scriptIdeas)
    setTextDraft(view.textDraft)
    if (view.monitorVolume > 0) audibleVolume.current = view.monitorVolume
    previewPosition.current = view.frame; setFrame(view.frame); setSeekFrame(view.frame); setViewLoaded(true)
  }, [version, viewLoaded, viewKey, workId, editorState.isSuccess, editorState.data])
  useEffect(() => {
    if (!viewLoaded || version < 0 || !referenceLocation) { if (!referenceLocation) locatedReference.current = undefined; return }
    const locationKey = JSON.stringify(referenceLocation)
    if (locatedReference.current === locationKey) return
    locatedReference.current = locationKey
    const shot = current.current.shots.find(shot => shot.id === referenceLocation.shot)
    const reference = shot?.references?.find(reference => reference.id === referenceLocation.reference)
    if (!shot || !reference) { setError('目标分镜或素材引用已删除'); return }
    select(shot.id, 'reference')
    setReferenceVersionTarget(structuredClone(reference))
  }, [viewLoaded, version, referenceLocation])
  useEffect(() => {
    if (!viewLoaded || !loop || project.shots.some(shot => shot.id === previewShotId)) return
    const view = restoreVideoEditorView(project, viewSnapshot.current ?? {})
    previewPosition.current = view.playheadFrame
    setLoop(false); setPreviewShotId(''); setFrame(view.playheadFrame); setSeekFrame(view.playheadFrame)
  }, [viewLoaded, loop, previewShotId, project])
  useLayoutEffect(() => {
    if (!viewLoaded || previousSequence.current === sequence) { previousSequence.current = sequence; return }
    previousSequence.current = sequence
    const view = restoreVideoEditorView(project, viewSnapshot.current ?? {})
    player.current?.pause()
    setFrame(view.frame); setSeekFrame(view.frame); setPreviewShotId(view.previewShotId)
    previewPosition.current = view.frame
  }, [viewLoaded, sequence, project])
  useLayoutEffect(() => {
    if (!viewLoaded) return
    const index = project.shots.findIndex(shot => shot.id === previewShotId)
    const start = !sequence && loop && index >= 0 ? timelineFrames[index] : 0
    const selectedShot = project.shots.find(shot => shot.id === selectedId)
    const end = sequence ? Math.round((selectedShot ? videoShotDuration(selectedShot) ?? 0 : 0) * project.fps) : timelineFrames.at(-1)!
    viewSnapshot.current = { version: 1, selectedId, batchSelection, selectedAudioId: selectedAudioId ?? '', tab: tab as VideoEditorViewState['tab'], inspector, previewShotId, assetPreview, loop, playheadShotId: sequence ? selectedId : undefined, playheadFrame: Math.max(0, Math.min(end - 1, start + frame)), zoom, snap, library, previewOpen, monitorVolume, assetVersions, numberDrafts, nameDrafts, promptDrafts, structureDrafts, scriptIdeas: ideas, textDraft, ...assetBrowser, ...timelineViewport, ...trackDisplay }
  }, [viewLoaded, selectedId, batchSelection, selectedAudioId, tab, inspector, previewShotId, assetPreview, loop, frame, zoom, snap, library, previewOpen, monitorVolume, assetVersions, numberDrafts, nameDrafts, promptDrafts, structureDrafts, ideas, textDraft, assetBrowser, timelineViewport, trackDisplay, project])
  useEffect(() => {
    const persist = () => {
      if (!viewSnapshot.current || JSON.stringify(viewSnapshot.current) === lastFailedView.current || client.isMutating({ mutationKey: ['studio', 'video-editor-state', workId] })) return
      void persistEditorView().catch(cause => setViewError(cause instanceof Error ? cause.message : String(cause)))
    }
    const timer = window.setInterval(persist, 500)
    window.addEventListener('pagehide', persist)
    return () => { window.clearInterval(timer); window.removeEventListener('pagehide', persist); persist() }
  }, [persistEditorView, client, workId])
  useEffect(() => { playerTarget?.setVolume(monitorVolume) }, [playerTarget, monitorVolume])
  useEffect(() => { const target = playerTarget; setPlaying(target?.isPlaying() ?? false); if (!target) return; if (seekFrame === undefined) target.seekTo(previewPosition.current); const update = (event: { detail: { frame: number } }) => { previewPosition.current = event.detail.frame; setFrame(event.detail.frame) }; const play = () => setPlaying(true); const pause = () => setPlaying(false); target.addEventListener('frameupdate', update); target.addEventListener('play', play); target.addEventListener('pause', pause); target.addEventListener('ended', pause); return () => { target.removeEventListener('frameupdate', update); target.removeEventListener('play', play); target.removeEventListener('pause', pause); target.removeEventListener('ended', pause) } }, [playerTarget, project.shots.length, loop])
  useEffect(() => { if (seekFrame !== undefined && playerTarget) { playerTarget.seekTo(seekFrame); setSeekFrame(undefined) } }, [seekFrame, loop, playerTarget])
  const save = useMutation({ scope: { id: `video-project:${sessionId}` }, mutationFn: async (snapshot: VideoProject) => { validateVideoProject(snapshot); return apiFetch<ProjectRecord>(base, { method: 'PUT', body: JSON.stringify({ session_id: activeChatId ?? sessionId, request_id: crypto.randomUUID(), expected_version: currentVersion.current, project: snapshot }) }) }, onSuccess: result => { currentVersion.current = result.asset_version; savedProject.current = normalizeVideoProject(result.project); setVersion(result.asset_version); client.setQueryData(recordKey, result); if (JSON.stringify(result.project) === JSON.stringify(current.current)) setDirty(false); void client.invalidateQueries({ queryKey: ['studio', 'session', sessionId] }) }, onError: cause => { if (cause instanceof ApiError && cause.status === 409) return client.invalidateQueries({ queryKey: recordKey, exact: true }) } })
  const saveBeforeLeaving = useCallback(async () => {
    setLeaving(true); setLeaveError('')
    try {
      if (editorRoot.current?.querySelector('[aria-label="文字编辑冲突"]')) throw new Error('处理剧本或字幕的编辑冲突后继续')
      await persistEditorView()
      if (!dirty) return true
      if (external) throw new Error('解决项目编辑冲突后继续切换')
      const snapshot = current.current
      await save.mutateAsync(snapshot)
      if (JSON.stringify(snapshot) !== JSON.stringify(current.current)) throw new Error('保存期间内容发生变化，请保存最新修改')
      await persistEditorView()
      localStorage.removeItem(draftKey)
      return true
    } catch (cause) { setLeaveError(cause instanceof Error ? cause.message : String(cause)); return false }
    finally { setLeaving(false) }
  }, [dirty, external, save.mutateAsync, draftKey, persistEditorView])
  useLayoutEffect(() => registerDesktopExitGuard(async () => {
    const saved = await saveBeforeLeaving()
    if (!saved) setDesktopExitPending(true)
    return saved
  }), [saveBeforeLeaving])
  const navigation = useBlocker({ shouldBlockFn: useCallback(async () => !await saveBeforeLeaving(), [saveBeforeLeaving]), enableBeforeUnload: dirty, withResolver: true })
  function continueLeaving() {
    if (desktopExitPending) void desktopInvoke<void>('studio_confirm_exit').catch(cause => setLeaveError(cause instanceof Error ? cause.message : String(cause)))
    else navigation.proceed?.()
  }
  function cancelLeaving() { setDesktopExitPending(false); navigation.reset?.() }
  async function discardUnsavedAndLeave() {
    setLeaving(true); setLeaveError('')
    localStorage.removeItem(draftKey); setNumberDrafts({}); setNameDrafts({}); setPromptDrafts({}); setStructureDrafts({}); setIdeas(''); setTextDraft(undefined)
    if (viewSnapshot.current) { viewSnapshot.current.numberDrafts = {}; viewSnapshot.current.nameDrafts = {}; viewSnapshot.current.promptDrafts = {}; viewSnapshot.current.structureDrafts = {}; viewSnapshot.current.scriptIdeas = ''; viewSnapshot.current.textDraft = undefined }
    try { await persistEditorView(); setDirty(false); continueLeaving() }
    catch (cause) { setLeaveError(cause instanceof Error ? cause.message : String(cause)) }
    finally { setLeaving(false) }
  }
  const pendingMerge = external && dirty && record.data ? mergeVideoProjects(savedProject.current, project, normalizeVideoProject(record.data.project)) : undefined
  function acceptMerge(preference: 'local' | 'remote') {
    if (!record.data) return
    const remote = normalizeVideoProject(record.data.project)
    const merged = mergeVideoProjects(savedProject.current, current.current, remote, preference)
    validateVideoProject(merged.project)
    savedProject.current = remote; currentVersion.current = record.data.asset_version; setVersion(record.data.asset_version)
    applyCommit(merged.project); save.reset()
  }
  useEffect(() => {
    if (!pendingMerge || pendingMerge.conflicts.length || save.isPending) return
    try { acceptMerge('local') } catch (cause) { setError(cause instanceof Error ? cause.message : String(cause)) }
  }, [record.data, project, external, dirty, save.isPending])
  useEffect(() => { if (!dirty || version < 0 || external || save.isPending || save.error || leaving || navigation.status === 'blocked') return; const timer = setTimeout(() => save.mutate(project), 700); return () => clearTimeout(timer) }, [project, dirty, version, external, save.isPending, save.error, leaving, navigation.status])
  const collecting = useRef(new Set<string>())
  useEffect(() => {
    for (const shot of project.shots) {
      const saved = generationRecords.data?.find(record => record.shot_id === shot.id && record.id === shot.generationRequest?.id)
      if (!saved || (!saved.execution_id && !saved.submission_error)) continue
      patchGenerationState({ generationRequest: undefined, ...(saved.execution_id ? { executionId: saved.execution_id, executionInputs: JSON.stringify(saved.snapshot.inputs) } : {}) }, shot.id)
    }
  }, [generationRecords.data, project.shots])
  useEffect(() => { for (const { shotId, execution: job, recorded } of generationJobs) {
    const shot = project.shots.find(value => value.id === shotId)
    const outputs = job.outputs.filter(value => value.media_type.startsWith('video/'))
    if (job.status !== 'succeeded' || !shot || !outputs.some(output => shot.media?.versionId !== output.asset_version_id && !shot.versions?.some(value => value.media.versionId === output.asset_version_id)) || collecting.current.has(job.id)) continue
    collecting.current.add(job.id)
    Promise.all(job.outputs.map(async item => { const asset = await getStudioAsset(item.asset_id); const mediaVersion = asset.versions.find(value => value.id === item.asset_version_id); if (!mediaVersion) throw new Error('生成资产版本缺失'); return { output: item, version: mediaVersion, ref: { assetId: asset.id, versionId: mediaVersion.id, url: mediaVersion.content_url } } })).then(results => {
      if (!job.workflow_version_id || !job.inputs) throw new Error('生成任务缺少工作流版本或输入快照')
      const previous = current.current
      const shot = previous.shots.find(item => item.id === shotId)
      if (!shot || (!recorded && shot.executionId !== job.id)) { collecting.current.delete(job.id); return }
      const generation = { executionId: job.id, workflowId: job.workflow_id, versionId: job.workflow_version_id, inputs: job.inputs }
      const collected = results.filter(item => item.output.media_type.startsWith('video/')).reduce((current, video) => collectVideoMedia(current, recorded ? { shotId, executionId: job.id } : job.id, {
        ...video.ref, duration: typeof video.version.metadata?.duration === 'number' ? video.version.metadata.duration : undefined,
        firstFrame: results.find(item => item.output.field_key === 'first_frame' && item.output.index === video.output.index)?.ref,
        lastFrame: results.find(item => item.output.field_key === 'last_frame' && item.output.index === video.output.index)?.ref,
        generation,
      }), shot)
      if (collected !== shot) {
        const next = { ...previous, shots: previous.shots.map(item => item.id === shotId ? collected : item) }
        current.current = next; setProject(next); setDirty(true)
      }
      void client.invalidateQueries({ queryKey: ['studio', 'session', sessionId] })
      collecting.current.delete(job.id)
    }).catch(cause => { collecting.current.delete(job.id); setError(String(cause)) })
  } }, [generationJobs, project.shots])
  const generate = useMutation({ mutationFn: async (shot: VideoShot) => {
    if (!shot.workflowId || !shot.versionId) throw new Error('选择一个已发布工作流')
    const request = shot.generationRequest ?? { id: crypto.randomUUID(), workflowId: shot.workflowId, versionId: shot.versionId, inputs: structuredClone(shot.inputs) }
    if (!shot.generationRequest) {
      const workflow = await getWorkflowVersion(request.workflowId, request.versionId)
      const issues = videoGenerationInputIssues(workflow.definition.inputs, request.inputs, undefined, workflow.definition.input_schema)
      if (issues.length) throw new Error(issues.map(issue => issue.key ? `${workflow.definition.inputs.find(field => field.key === issue.key)?.description || issue.key}：${issue.message}` : issue.message).join('；'))
      const mediaIds = workflow.definition.inputs.filter(field => ['image', 'video', 'audio', 'file'].includes(field.type))
        .flatMap(field => { const input = request.inputs[field.key] as { asset_id: string } | undefined; return input && typeof input === 'object' ? [input.asset_id] : [] })
      const checkedAssets = await Promise.all([...new Set(mediaIds)].map(id => getStudioAsset(id)))
      const mediaIssues = videoGenerationInputIssues(workflow.definition.inputs, request.inputs, checkedAssets)
      if (mediaIssues.length) throw new Error(mediaIssues.map(issue => `${workflow.definition.inputs.find(field => field.key === issue.key)?.description || issue.key}：${issue.message}`).join('；'))
    }
    if (!current.current.shots.some(item => item.id === shot.id)) throw new Error('分镜已被删除')
    const snapshot = patchGenerationState({ generationRequest: request }, shot.id)
    const saved = await save.mutateAsync(snapshot)
    if (!saved.asset_id) throw new Error('视频作品标识缺失')
    const result = await createVideoGeneration(saved.asset_id, { request_id: request.id, session_id: activeChatId ?? sessionId, shot_id: shot.id, expected_version: saved.asset_version })
    return { result, shotId: shot.id }
  }, onSuccess: ({ result, shotId }) => {
    patchGenerationState({ ...(result.execution_id ? { executionId: result.execution_id, executionInputs: JSON.stringify(result.snapshot.inputs) } : {}), generationRequest: undefined }, shotId)
    if (result.submission_error) setError(result.submission_error)
  }, onSettled: () => client.invalidateQueries({ queryKey: ['studio', 'video-generations'] }), onError: (cause, shot) => {
    if (cause instanceof ApiError && cause.status >= 400 && cause.status < 500 && cause.status !== 409) patchGenerationState({ generationRequest: undefined }, shot.id)
  } })
  const submitGenerationBatch = useMutation({ mutationFn: async (selection?: { baseline: VideoProject; ids: string[]; dependencies: VideoGenerationDependency[] }) => {
    let submission = pendingGenerationBatch
    if (submission) {
      try { return await getVideoGenerationBatch(submission.videoId, submission.body.request_id) }
      catch (cause) { if (!(cause instanceof ApiError && cause.status === 404)) throw cause }
    } else {
      if (!selection) throw new Error('重新选择生成范围')
      validateVideoGenerationBatchSelection(selection.baseline, current.current, selection.ids, selection.dependencies)
      const saved = await save.mutateAsync(current.current)
      if (!saved.asset_id) throw new Error('视频作品标识缺失')
      submission = { videoId: saved.asset_id, body: { request_id: crypto.randomUUID(), session_id: activeChatId ?? sessionId, expected_version: saved.asset_version, shot_ids: selection.ids, dependencies: selection.dependencies } }
      localStorage.setItem(`video-generation-batch-submit:${submission.videoId}`, JSON.stringify(submission))
      setPendingGenerationBatch(submission)
    }
    return createVideoGenerationBatch(submission.videoId, submission.body)
  }, onSuccess: result => {
    localStorage.removeItem(`video-generation-batch-submit:${result.video_id}`)
    setPendingGenerationBatch(undefined); setGenerationBatchOpen(false); setGenerationRecordsOpen(true)
    void client.invalidateQueries({ queryKey: ['studio', 'video-generations'] })
    void client.invalidateQueries({ queryKey: ['studio', 'video-generation-batches'] })
  }, onError: cause => {
    if (cause instanceof ApiError && [400, 404, 409].includes(cause.status) && workId) {
      localStorage.removeItem(`video-generation-batch-submit:${workId}`); setPendingGenerationBatch(undefined)
    }
  } })
  const upload = useMutation({
    mutationFn: ({ file, requestId }: { key?: string; file: File; requestId?: string }) => uploadStudioAsset(file, sessionId, projectId, requestId),
    onMutate: input => {
      if (!input.key) return
      const shot = current.current.shots.find(shot => shot.id === selectedId)
      const field = source?.inputs.find(field => field.key === input.key)
      if (!shot?.workflowId || !shot.versionId || !field) throw new Error('读取目标工作流后上传素材')
      return { target: { shotId: shot.id, workflowId: shot.workflowId, versionId: shot.versionId, field: { ...field }, baseline: structuredClone(shot.inputs[input.key]) } }
    },
    onSuccess: (asset, _input, context) => {
      const mediaVersion = asset.versions.find(item => item.version === asset.current_version)
      if (!mediaVersion) throw new Error('上传资产的版本不存在')
      setAssetVersions(current => ({ ...current, [asset.id]: mediaVersion.id }))
      void client.invalidateQueries({ queryKey: ['studio', 'session', sessionId] })
      void client.invalidateQueries({ queryKey: ['studio', 'video-project-assets', projectId ?? ''] })
      if (context) { const next = applyUploadedVideoInput(current.current, context.target, asset); if (next !== current.current) commit(next) }
    },
  })
  async function importAssets(items: VideoAssetImport[]) {
    if (importsRunning.current || !items.length) return
    importsRunning.current = true
    setAssetImports(previous => [...previous.filter(item => !items.some(next => next.id === item.id)), ...items.map(item => ({ ...item, status: 'queued' as const, error: undefined }))])
    try {
      for (const item of items) {
        setAssetImports(previous => previous.map(entry => entry.id === item.id ? { ...entry, status: 'uploading' } : entry))
        try {
          const asset = await upload.mutateAsync({ file: item.file, requestId: item.id })
          setAssetImports(previous => previous.map(entry => entry.id === item.id ? { ...entry, asset, status: asset.already_exists ? 'existing' : 'succeeded' } : entry))
        } catch (cause) {
          setAssetImports(previous => previous.map(entry => entry.id === item.id ? { ...entry, status: 'failed', error: cause instanceof Error ? cause.message : String(cause) } : entry))
        }
      }
    } finally { importsRunning.current = false }
  }
  const exportVideo = useMutation({ mutationKey: ['studio', 'video-export', workId ?? sessionId], networkMode: 'always', retry: false, mutationFn: async (task: VideoExportTask) => {
    if (videoExportPending(client.getQueryData(exportOptions.queryKey))) throw new Error('当前导出尚未完成')
    task = { ...task, status: 'checking', problems: [], error: undefined }
    const { request, project: output } = task
    try {
      storeVideoExportTask(client, task, workId ?? undefined)
      validateVideoProject(output)
      if (!task.submitted) {
        const clips = [
          ...output.shots.map(shot => ({ kind: 'shot' as const, id: shot.id, assetId: shot.media!.assetId, versionId: shot.media!.versionId, sourceIn: shot.sourceIn, duration: shot.duration, loop: false })),
          ...(output.audio ?? []).map(audio => ({ kind: 'audio' as const, ...audio })),
        ]
        const checkedAssets = new Map<string, Promise<StudioAsset>>()
        const checkedFiles = new Map<string, Promise<void>>()
        const issues: VideoExportIssue[] = []
        await Promise.all(clips.map(async clip => {
          try {
            if (!checkedAssets.has(clip.assetId)) checkedAssets.set(clip.assetId, getStudioAsset(clip.assetId))
            const asset = await checkedAssets.get(clip.assetId)!
            const mediaVersion = asset.versions.find(version => version.id === clip.versionId)
            if (!mediaVersion) throw new Error('素材版本已不存在')
            if (!mediaVersion.mime_type.startsWith(clip.kind === 'shot' ? 'video/' : 'audio/')) throw new Error('素材类型不符合轨道要求')
            const duration = mediaVersion.metadata?.duration
            const tolerance = clip.kind === 'shot' ? 1 / output.fps : .05
            if (typeof duration === 'number' && (clip.sourceIn >= duration || (!clip.loop && clip.sourceIn + clip.duration > duration + tolerance))) throw new Error('剪辑范围超出源素材')
            const key = `${clip.assetId}:${clip.versionId}`
            if (!checkedFiles.has(key)) checkedFiles.set(key, (async () => {
              const response = await fetch(mediaVersion.content_url, { headers: { Range: 'bytes=0-0' } })
              if (!response.ok) throw new Error(`素材文件无法读取（${response.status}）`)
              await response.body?.cancel()
            })())
            await checkedFiles.get(key)
          } catch (cause) { issues.push({ kind: clip.kind, id: clip.id, message: cause instanceof Error ? cause.message : String(cause) }) }
        }))
        task = { ...task, problems: issues }
        if (issues.length) throw new Error(`修正 ${issues.length} 项素材问题后重试`)
      }
      task = { ...task, status: 'exporting', submitted: true }; storeVideoExportTask(client, task, workId ?? undefined)
      let asset: StudioAsset
      if (workId) {
        const result = await exportVideoWork(workId, activeChatId ?? sessionId, task)
        if (!result.result) throw new Error('导出服务没有返回成片版本')
        const selected = selectedVideoAsset(await getStudioAsset(result.result.assetId), result.result.versionId)
        if (!selected) throw new Error('成片版本已不存在')
        asset = selected
      } else asset = await apiFetch<StudioAsset>('/api/v1/studio/video-exports', { method: 'POST', body: JSON.stringify(request) })
      const mediaVersion = asset.versions.find(item => item.version === asset.current_version)
      if (!mediaVersion) throw new Error('成片版本不存在')
      task = { ...task, status: 'completed', result: { assetId: asset.id, versionId: mediaVersion.id } }; storeVideoExportTask(client, task, workId ?? undefined)
      void client.invalidateQueries({ queryKey: ['studio', 'video-export-history'] })
      void client.invalidateQueries({ queryKey: ['studio', 'session', sessionId] })
      void client.invalidateQueries({ queryKey: ['studio', 'video-project-assets'] })
      return asset
    } catch (cause) {
      storeVideoExportTask(client, { ...task, status: 'failed', error: cause instanceof Error ? cause.message : String(cause) }, workId ?? undefined)
      throw cause
    }
  } })
  const exportMutations = useIsMutating({ mutationKey: ['studio', 'video-export', workId ?? sessionId] })
  const exportStatus = useQuery({
    queryKey: ['studio', 'video-export-status', workId ?? sessionId, exportTask?.request.request_id],
    enabled: editorActive && Boolean(exportTask?.submitted && exportTask.status !== 'completed') && exportMutations === 0,
    retry: false,
    queryFn: async () => {
      const requestId = exportTask!.request.request_id
      if (workId) {
        const task = await getVideoWorkExport(workId, requestId)
        return { requestId, status: task.status, task }
      }
      const status = videoExportStatusSchema.parse(await apiFetch<unknown>(`/api/v1/studio/sessions/${encodeURIComponent(sessionId)}/video-exports/${encodeURIComponent(requestId)}`))
      return { requestId, ...status }
    },
    refetchInterval: query => query.state.data?.status === 'exporting' ? 2000 : false,
  })
  useEffect(() => {
    if (!exportStatus.data || exportMutations > 0) return
    const task = client.getQueryData<VideoExportTask | null>(exportOptions.queryKey)
    if (!task) return
    const next = 'task' in exportStatus.data ? exportStatus.data.task! : applyVideoExportStatus(task, exportStatus.data.requestId, exportStatus.data)
    if (next === task) return
    storeVideoExportTask(client, next, workId ?? undefined)
    if (next.status === 'completed') {
      exportVideo.reset()
      void client.invalidateQueries({ queryKey: ['studio', 'session', sessionId] })
      void client.invalidateQueries({ queryKey: ['studio', 'video-project-assets'] })
    }
  }, [exportStatus.data, exportMutations, exportTask, client, sessionId, workId, exportVideo.reset])
  const previewExport = useMutation({ mutationFn: async (result: NonNullable<VideoExportTask['result']>) => {
    const asset = selectedVideoAsset(await getStudioAsset(result.assetId), result.versionId)
    if (!asset) throw new Error('成片版本已不存在')
    return asset
  }, onSuccess: asset => { openAssetPreview(asset); setExportOpen(false) } })
  const saveExport = useMutation({ mutationFn: async (asset: StudioAsset) => {
    const version = asset.versions.find(item => item.version === asset.current_version)
    if (!version) throw new Error('成片版本不存在')
    return exportStudioAssetVersion(asset.id, version.id)
  } })
  async function refreshExportSnapshot() {
    if (timingChange || exportPending || save.isPending) return
    if (pendingInputCount) { setExportOpen(false); locatePendingInput(); return }
    const snapshot = structuredClone(current.current)
    try {
      const sourceVersion = dirty ? (await save.mutateAsync(snapshot)).asset_version : currentVersion.current
      if (sourceVersion < 0) throw new Error('作品读取完成后再导出')
      exportVideo.reset(); previewExport.reset()
      setExportSnapshot({ id: crypto.randomUUID(), project: snapshot, sourceVersion }); setExportOpen(true)
    } catch (cause) { setError(cause instanceof Error ? cause.message : String(cause)) }
  }
  function openAssetPreview(asset: StudioAsset) {
    const version = asset.versions.find(item => item.version === asset.current_version)
    if (!version) { setError('资产版本已不可用，请重新选择'); return }
    player.current?.pause()
    client.setQueryData(['studio', 'video-preview-asset', asset.id], asset)
    void client.invalidateQueries({ queryKey: ['studio', 'video-preview-asset', asset.id] })
    setAssetVersions(current => ({ ...current, [asset.id]: version.id }))
    setAssetPreview(current => current?.assetId === asset.id && current.versionId === version.id ? current : { assetId: asset.id, versionId: version.id, time: 0 })
    setPreviewOpen(true)
    setLibrary(false); setInspector(false)
  }
  function selectAssetVersion(assetId: string, versionId: string) {
    setAssetVersions(current => ({ ...current, [assetId]: versionId }))
    setAssetPreview(current => current?.assetId === assetId && current.versionId !== versionId ? { assetId, versionId, time: 0 } : current)
  }
  const reference = useMutation({ mutationFn: (scope: 'shot' | 'project') => createStudioTextAsset({ sessionId, name: `${scope === 'shot' ? selected?.name ?? project.name : project.name} · 创作上下文`, content: JSON.stringify({ scope, shotId: scope === 'shot' ? selected?.id : undefined, project }, null, 2) }), onSuccess: onReference })
  async function chooseWorkflow(id: string) {
    const shotId = selectedId; const workflow = workflows.data?.find(value => value.id === id)
    if (!workflow?.default_version_id || !shotId) return
    if (selected?.generationRequest) { setError('先恢复当前生成请求，再更换工作流'); return }
    const choice = (workflowChoices.current.get(shotId) ?? 0) + 1; workflowChoices.current.set(shotId, choice)
    const published = await getWorkflowVersion(id, workflow.default_version_id)
    const shot = current.current.shots.find(item => item.id === shotId)
    if (!shot || workflowChoices.current.get(shotId) !== choice) return
    patchShot(chooseVideoShotWorkflow(shot, published), shot.id)
  }
  function reviewWorkflowVersion(targetId: string) {
    if (!selected || !definition.data || targetId === selected.versionId) return
    if (pendingInputCount) { locatePendingInput(); return }
    setWorkflowVersionTarget({ shot: structuredClone(selected), from: definition.data, targetId }); setGenerationSettingsOpen(false)
  }
  function undo() {
    const previous = past.at(-1)
    if (!previous) return
    try {
      const next = restoreVideoEditorHistory(previous, current.current, current.current.shots.filter(shot => !canRemoveShot(shot.id)).map(shot => shot.id))
      historyGroup.current = undefined; setPast(past.slice(0, -1)); setFuture([current.current, ...future]); current.current = next; setProject(next); setDirty(true)
    } catch (cause) { setError(cause instanceof Error ? cause.message : String(cause)) }
  }
  function redo() {
    if (!future[0]) return
    try {
      const next = restoreVideoEditorHistory(future[0], current.current, current.current.shots.filter(shot => !canRemoveShot(shot.id)).map(shot => shot.id))
      historyGroup.current = undefined; setPast([...past, current.current]); current.current = next; setProject(next); setFuture(future.slice(1)); setDirty(true)
    } catch (cause) { setError(cause instanceof Error ? cause.message : String(cause)) }
  }
  function audioSourceDuration(audio: VideoAudio) {
    const duration = assets.find(asset => asset.id === audio.assetId)?.versions.find(version => version.id === audio.versionId)?.metadata?.duration
    if (duration === undefined) return audio.sourceDuration
    if (typeof duration !== 'number' || !Number.isFinite(duration) || duration <= 0) throw new Error('音频时长无效')
    return duration
  }
  function numberKey(target: VideoNumberTarget) { return `${target.kind}:${target.id}:${target.field}` }
  function numberField(target: VideoNumberTarget, label: string, value: number) {
    const key = numberKey(target)
    return <VideoPropertyField key={key} id={`video-number-${sessionId}-${key}`} label={label} value={value} displayValue={target.field === 'plannedDuration' && value === 0 ? '' : undefined} placeholder={target.field === 'plannedDuration' ? '时长未定' : undefined} draft={numberDrafts[key]} disabled={version < 0 || external || leaving}
      onDraftChange={draft => setNumberDrafts(current => { const next = { ...current }; if (draft) next[key] = draft; else delete next[key]; return next })}
      onApply={draft => {
        const source = current.current
        const withMetadata = target.kind === 'audio' ? { ...source, audio: source.audio?.map(audio => audio.id === target.id ? { ...audio, sourceDuration: audioSourceDuration(audio) } : audio) } : source
        commit(editVideoNumber(withMetadata, target, draft))
      }} />
  }
  function changeAudioLoop(id: string, loop: boolean) {
    const previous = current.current
    const audio = previous.audio?.find(item => item.id === id)
    if (!audio) throw new Error('音频已被删除')
    const next = setVideoAudioLoop(previous, id, loop, audioSourceDuration(audio))
    if (next.audio!.find(item => item.id === id)!.duration < audio.duration) {
      setTimingChange({ previous, next, audio: [audio], title: '关闭音频循环' })
      return
    }
    commit(next)
  }
  function addAsset(assetId: string, versionId: string, track: string, shotId: string | undefined, time = playhead) {
    const asset = assets.find(item => item.id === assetId)
    if (!asset?.versions.some(item => item.id === versionId)) { setError('资产版本不可用，请重新选择'); return }
    if (track.startsWith('audio')) {
      if (asset.kind !== 'audio') { setError('成片音频轨道仅接受音频资产'); return }
      const media = asset.versions.find(item => item.id === versionId)!
      const sourceDuration = media.metadata?.duration
      if (typeof sourceDuration !== 'number' || sourceDuration <= 0) { setError('音频时长尚未就绪'); return }
      const start = Math.round(time * project.fps) / project.fps
      if (start >= total) { setError('将音频放在成片范围内'); return }
      const audio = { id: crypto.randomUUID(), assetId, versionId, url: media.content_url, start, duration: Math.min(sourceDuration, total - start), sourceDuration, sourceIn: 0, volume: 1, loop: false }
      commit({ ...current.current, audio: [...(current.current.audio ?? []), audio] }); setSelectedAudioId(audio.id); setTab('audio'); setInspector(true); return
    }
    if (track === 'video' && !shotId && asset.kind === 'video') {
      const duration = asset.versions.find(item => item.id === versionId)?.metadata?.duration
      if (typeof duration !== 'number' || duration < 1 / project.fps) { setError('视频时长尚未就绪'); return }
      const shot = newVideoShot(asset.name)
      setAssetAction({ asset, versionId, shotId: shot.id, track, time, newShot: shot }); return
    }
    if (track === 'script' && asset.kind === 'document') {
      const mediaVersion = asset.versions.find(item => item.id === versionId)!
      if (!mediaVersion.mime_type.startsWith('text/')) { setError('选择文本或 Markdown 文档作为脚本'); return }
      const baseline = structuredClone(normalizeProjectScript(current.current).script!)
      void apiFetch<string>(mediaVersion.content_url).then(content => {
        if (!content.trim()) throw new Error('文档没有可用的脚本内容')
        setScriptImport({ baseline, content, name: asset.name })
      }).catch(cause => setError(String(cause))); return
    }
    const target = current.current.shots.find(shot => shot.id === shotId)
    if (!target) { setError('选择一个分镜'); return }
    if (!['reference', 'shot', 'video'].includes(track)) { setError('将资产放入参考、分镜或视频轨道'); return }
    if ((track === 'shot' && asset.kind !== 'image') || (track === 'video' && asset.kind !== 'video') || !['image', 'audio', 'video', 'document'].includes(asset.kind)) { setError('资产类型与轨道不匹配'); return }
    setReferenceRole(track === 'shot' ? 'storyboard' : 'reference')
    setReferenceInputTarget(undefined); setSelectedId(target.id)
    setAssetAction({ asset, versionId, shotId: target.id, track, time })
  }
  function chooseReferenceInput(key: string) {
    if (!assetAction) return
    setAssetAction({ ...assetAction, error: undefined })
    if (key === 'none') { setReferenceInputTarget(undefined); return }
    const shot = current.current.shots.find(shot => shot.id === assetAction.shotId)
    const field = referenceSource?.inputs.find(field => field.key === key)
    if (!shot?.workflowId || !shot.versionId || !field || referenceWorkflow.data?.id !== shot.versionId || referenceWorkflow.data.workflow_id !== shot.workflowId) {
      setAssetAction({ ...assetAction, error: '生成参数已改变，请重新读取后选择' }); return
    }
    setReferenceInputTarget({ shotId: shot.id, workflowId: shot.workflowId, versionId: shot.versionId, field: { ...field }, baseline: structuredClone(shot.inputs[key]) })
  }
  function applyImportedScript(mode: 'append' | 'replace') {
    if (!scriptImport) return
    try {
      const id = crypto.randomUUID()
      commit(importVideoScript(current.current, scriptImport.baseline, id, scriptImport.content, mode))
      setScriptImport(undefined)
      setScriptFocus({ blockId: id, requestId: crypto.randomUUID() })
    } catch (cause) { setScriptImport({ ...scriptImport, error: cause instanceof Error ? cause.message : String(cause) }) }
  }
  function applyAsset() {
    if (!assetAction) return
    const { asset, versionId, shotId, track } = assetAction
    const shot = assetAction.newShot ?? current.current.shots.find(item => item.id === shotId)
    const currentAsset = assets.find(item => item.id === asset.id)
    const mediaVersion = currentAsset?.versions.find(item => item.id === versionId)
    try {
      if (!shot || !currentAsset || !mediaVersion) throw new Error('分镜或资产版本已不存在，请重新选择')
      const media = { assetId: asset.id, versionId, url: mediaVersion.content_url }
      if (track === 'video') {
        setAdoption(createVideoAdoption(current.current, shot, { ...media, duration: typeof mediaVersion.metadata?.duration === 'number' ? mediaVersion.metadata.duration : undefined }, Boolean(assetAction.newShot)))
      } else {
        const bound = referenceInputTarget ? applyUploadedVideoInput(current.current, referenceInputTarget, { ...currentAsset, current_version: mediaVersion.version }).shots.find(shot => shot.id === shotId)! : shot
        const reference = { ...media, id: crypto.randomUUID(), kind: currentAsset.kind as VideoReference['kind'], role: referenceRole, inputKey: referenceInputTarget?.field.key }
        patchShot(setVideoReference(bound, reference), shotId)
      }
      if (!assetAction.newShot) select(shotId, track === 'shot' ? 'reference' : track)
      setReferenceInputTarget(undefined); setAssetAction(undefined)
    } catch (cause) { setAssetAction({ ...assetAction, error: cause instanceof Error ? cause.message : String(cause) }) }
  }
  async function sendScriptIdeas() {
    if (scriptSending || !scriptTarget || !ideas.trim() || !chosenScriptSkill) return
    setScriptSending(true); setScriptError('')
    try {
      if (external) throw new Error('项目已更新，请读取最新项目后生成')
      const snapshot = normalizeProjectScript(current.current)
      const record = dirty || !videoId ? await save.mutateAsync(snapshot) : undefined
      const workId = videoId ?? record?.asset_id ?? undefined
      if (scriptTarget.scope === 'script') {
        await onScriptRequest({ skillId: chosenScriptSkill, videoEditContext: { scope: 'script', video_id: workId, project: structuredClone(snapshot) }, text: `根据以下想法创作完整脚本。\n想法：${ideas.trim()}\n视频：${snapshot.name}\n读取 studio_read_video_project 获取最新 asset_version，调用 studio_propose_video_script 保存有序文本块提案。已有文本块沿用 id，新增内容使用新 id。只创作脚本，等待用户在编辑器采用。` })
      } else {
        const ids = scriptTarget.shotIds
        if (ids && (!ids.length || ids.some(id => !snapshot.shots.some(shot => shot.id === id)))) throw new Error('重新选择有效的目标分镜，输入内容已保留')
        const scope = ids ? snapshot.shots.filter(shot => ids.includes(shot.id)).map(shot => `${shot.name}，shotId=${shot.id}`).join('；') : '全部脚本'
        await onScriptRequest({ skillId: chosenScriptSkill, videoEditContext: { scope: 'storyboard', video_id: workId, project: structuredClone(snapshot), ...(ids ? { shot_ids: [...ids] } : {}) }, text: `根据完整脚本生成分镜提案。\n要求：${ideas.trim()}\n范围：${scope}\n读取 studio_read_video_project 获取最新 asset_version，调用 studio_propose_video_storyboard。每项提供 name、storyboard 和 sourceBlockIds，已有分镜使用 update 和原 shotId，新增分镜使用 add 和新 shotId。保持人物与动作衔接，等待用户在编辑器采用。` })
      }
      setScriptTarget(undefined); setIdeas('')
    } catch (cause) { setScriptError(cause instanceof Error ? cause.message : String(cause)) }
    finally { setScriptSending(false) }
  }
  function onScriptRequest(request: Omit<StudioEditorRequest, 'label'>) {
    const context = request.videoEditContext
    const label = context.scope === 'script' ? '创作脚本' : context.scope === 'storyboard' ? '生成分镜提案' : '视频创作'
    return sendScriptRequest({ ...request, label })
  }
  function openScriptIdeas(shotId?: string) { setScriptTarget(shotId ? { scope: 'storyboard', shotIds: [shotId] } : { scope: 'script' }); setScriptError('') }
  const selectedPromptKey = selected ? videoPromptDraftKey(selected) : ''
  const hasPromptDraft = Boolean(promptDrafts[selectedPromptKey])
  const canPreparePrompt = Boolean(selected && definition.data && (hasPromptDraft || ((selected.storyboard?.trim() || selected.script.trim()) && source && videoPromptFields(source).length)))
  function openPromptDraft() {
    if (!selected || !definition.data) return
    const key = videoPromptDraftKey(selected)
    try {
      const draft = promptDrafts[key] ?? createVideoPromptDraft(selected, definition.data)
      setPromptDrafts(current => ({ ...current, [key]: draft })); setPromptTarget(key); setGenerationSettingsOpen(false)
    } catch (cause) { setError(cause instanceof Error ? cause.message : String(cause)) }
  }
  function discardPromptDraft() {
    if (!promptTarget) return
    setPromptDrafts(current => { const next = { ...current }; delete next[promptTarget]; return next })
    setPromptTarget(undefined)
  }
  function writeScriptManually() {
    if (!scriptTarget) return
    commit(addVideoScriptBlock(current.current, crypto.randomUUID(), ideas.trim()))
    setScriptTarget(undefined); setIdeas('')
  }
  const frameReference = useMutation({ mutationFn: async (selection: VideoFrameSelection | { sourceId: string; targetId: string; referenceId: string }) => {
    const { sourceId, targetId } = selection
    const referenceId = 'referenceId' in selection ? selection.referenceId : undefined
    const sourceShot = structuredClone(current.current.shots.find(shot => shot.id === sourceId))
    const target = structuredClone(current.current.shots.find(shot => shot.id === targetId))
    const fps = current.current.fps
    if (!sourceShot?.media || !target) throw new Error('来源视频或目标分镜已不存在')
    const original = referenceId ? target.references?.find(reference => reference.id === referenceId) : undefined
    if (referenceId && original?.sourceShotId !== sourceId) throw new Error('来源引用已修改，请重新选择')
    const chosen = 'sourcePosition' in selection ? selection : original
    if (!chosen) throw new Error('参考画面选择已不存在')
    const inputKey = chosen.inputKey
    if (inputKey) {
      if (!target.workflowId || !target.versionId) throw new Error('选择目标分镜的图生视频工作流')
      const workflow = await getWorkflowVersion(target.workflowId, target.versionId)
      if (!workflow.definition.inputs.some(field => field.key === inputKey && field.type === 'image')) throw new Error('目标工作流没有对应的图片输入')
    }
    const position = videoReferencePosition(chosen, sourceShot, fps)
    const asset = await createStudioVideoFrame(sourceShot.media.assetId, sourceShot.media.versionId, { session_id: sessionId, ...position })
    const frame = asset.versions.find(version => version.version === 1)
    if (!frame) throw new Error('提取的画面版本不存在')
    const latestSource = current.current.shots.find(shot => shot.id === sourceId)
    const latest = current.current.shots.find(shot => shot.id === targetId)
    if (!latest || !latestSource?.media || latestSource.media.versionId !== sourceShot.media.versionId || serializeVideoContent(videoReferencePosition(chosen, latestSource, current.current.fps)) !== serializeVideoContent(position)) throw new Error('来源剪辑或目标分镜已修改，请重新采用画面')
    const relevant = (shot: VideoShot) => ({ workflowId: shot.workflowId, versionId: shot.versionId, value: inputKey ? shot.inputs[inputKey] : undefined, references: shot.references?.filter(reference => referenceId ? reference.id === referenceId : reference.role === chosen.role || Boolean(inputKey && reference.inputKey === inputKey)) })
    if (serializeVideoContent(relevant(latest)) !== serializeVideoContent(relevant(target))) throw new Error('目标参考或生成输入已修改，请重新采用画面')
    const media = { assetId: asset.id, versionId: frame.id, url: frame.content_url }
    const referenced = referenceId ? resolveVideoReferenceSource(latest, referenceId, latestSource, true, current.current.fps, media) : setVideoReference(latest, { ...media, id: crypto.randomUUID(), kind: 'image', role: chosen.role, inputKey, sourcePosition: chosen.sourcePosition, sourceShotId: sourceId, sourceMediaVersionId: latestSource.media.versionId, confirmedSourceVersionId: latestSource.media.versionId, sourceFrame: position.frame, sourceFps: position.fps, confirmedSourceFrame: position.frame, confirmedSourceFps: position.fps })
    patchShot(referenced, targetId)
    select(targetId)
  }, onSuccess: () => { setFrameTargetId(undefined); void client.invalidateQueries({ queryKey: ['studio', 'video-project-assets'] }); void client.invalidateQueries({ queryKey: ['studio', 'session', sessionId] }) }, onError: cause => setError(cause.message) })
  const inputIssues = useMemo(() => source && selected ? videoGenerationInputIssues(source.inputs, selected.inputs, projectAssets.isSuccess ? assets : undefined, source.input_schema) : [], [source, selected?.inputs, projectAssets.isSuccess, assets])
  const inputErrors = Object.fromEntries((source?.inputs ?? []).flatMap(field => {
    const message = inputIssues.filter(issue => issue.key === field.key).map(issue => issue.message).join('；')
    return message ? [[field.key, message]] : []
  }))
  const formErrors = inputIssues.filter(issue => !issue.key)
  const invalidInputs = source?.inputs.filter(field => inputErrors[field.key]) ?? []
  const invalidInputCount = invalidInputs.length + formErrors.length
  const pendingNumberTargets: VideoNumberTarget[] = [
    ...project.shots.flatMap(shot => (['plannedDuration', 'duration', 'sourceIn', 'volume'] as const).map(field => ({ kind: 'shot' as const, id: shot.id, field }))),
    ...(project.audio ?? []).flatMap(audio => (['start', 'duration', 'sourceIn', 'volume'] as const).map(field => ({ kind: 'audio' as const, id: audio.id, field }))),
  ].filter(target => Boolean(numberDrafts[numberKey(target)]))
  const pendingNames = project.shots.filter(shot => Boolean(nameDrafts[shot.id]))
  const pendingInputCount = pendingNames.length + pendingNumberTargets.length
  function locatePendingInput() {
    const shot = pendingNames[0]
    if (shot) {
      select(shot.id, 'shot')
      requestAnimationFrame(() => document.getElementById(`video-name-${sessionId}-${shot.id}`)?.focus())
      return
    }
    const target = pendingNumberTargets[0]
    if (!target) return
    if (target.kind === 'shot') select(target.id, target.field === 'plannedDuration' ? 'shot' : 'video')
    else { setSelectedId(selected?.id ?? project.shots[0]?.id ?? ''); setSelectedAudioId(target.id); setTab('audio'); setInspector(true) }
    requestAnimationFrame(() => document.getElementById(`video-number-${sessionId}-${numberKey(target)}`)?.focus())
  }
  const staleReferences = selected?.references?.filter(reference => videoReferenceSourceChanged(reference, project.shots.find(shot => shot.id === reference.sourceShotId), project.fps)) ?? []
  const inputUploading = upload.isPending && upload.context?.target.shotId === selectedId
  const canGenerate = !busy && !generationRecords.data?.some(item => item.shot_id === selectedId && videoGenerationPending(item)) && (!workId || generationRecords.isSuccess) && !generate.isPending && !inputUploading && !save.isPending && !timingChange && Boolean(source?.outputs.some(output => output.type === 'video')) && !waitingForNode && !connectionIssue && !external && version >= 0 && (!selected?.executionId || executions.isSuccess) && enabledWorkflows.isSuccess && Boolean(boundWorkflow) && (Boolean(selected?.generationRequest) || (!inputIssues.length && !staleReferences.length))
  function shotStatus(shot: VideoShot) {
    const generation = generationRecords.data?.find(item => item.shot_id === shot.id)
    const job = generation ? generation.execution : executions.data?.find(item => item.shotId === shot.id)?.execution
    if (generation && !job) return videoGenerationLabels[generation.status]
    if (shot.generationRequest) return '生成请求等待恢复'
    if (job && active(job.status) && !job.waiting) return '已停止等待'
    if (job && (active(job.status) || ['failed', 'cancelled'].includes(job.status))) return job.cancel_requested && active(job.status) ? '正在取消' : labels[job.status]
    if (shot.versions?.some(value => value.status === 'candidate')) return '新版本待采用'
    if (videoGenerationChanged(shot)) return '生成输入已修改'
    if (Object.keys(shot.inputSources ?? {}).some(key => videoDescriptionInputState(shot, key)?.changed)) return '描述待同步'
    return shot.media ? '视频已就绪' : shot.workflowId ? '待生成' : '待选择工作流'
  }
  const failure = error || save.error?.message || generate.error?.message || submitGenerationBatch.error?.message || generationRecords.error?.message || exportRecord.error?.message || exportVideo.error?.message || saveExport.error?.message || reference.error?.message || definition.error?.message || executions.error?.message || workflows.error?.message || enabledWorkflows.error?.message || node.error?.message || record.error?.message || upload.error?.message || projectAssets.error?.message || execution?.error
  const previewShot = project.shots.find(shot => shot.id === previewShotId)
  const displayProject = sequence ? { ...project, shots: selected ? [selected] : [], audio: [] } : loop && previewShot ? previewVideoShot(project, previewShot.id) : project
  const displayFrames = videoTimelineFrames(displayProject.shots, project.fps).at(-1)!
  const displaySeconds = displayFrames / project.fps
  const previewStart = (timelineFrames[project.shots.findIndex(shot => shot.id === previewShotId)] ?? 0) / project.fps
  const playhead = sequence ? Math.min(displaySeconds, frame / project.fps) : Math.min(total, (loop && previewShot ? previewStart : 0) + frame / project.fps)
  function seekPreview(seconds: number) { player.current?.pause(); player.current?.seekTo(videoPreviewFrame(seconds, project.fps, displayFrames)); setTimeInput(undefined); setTimeError('') }
  function stepPreview(delta: number) { seekPreview((frame + delta) / project.fps) }
  function seekProject(seconds: number) { player.current?.pause(); setAssetPreview(undefined); setLoop(false); setSeekFrame(videoPreviewFrame(seconds, project.fps, sequence ? displayFrames : timelineFrames.at(-1)!)); setTimeInput(undefined); setTimeError('') }
  function seekShot(direction: -1 | 1) {
    if (sequence) {
      const target = project.shots[project.shots.findIndex(shot => shot.id === selectedId) + direction]
      if (target) select(target.id, tab, false)
      return
    }
    const position = Math.round(playhead * project.fps)
    const starts = timelineFrames.slice(0, -1)
    const target = direction < 0 ? [...starts].reverse().find(start => start < position) ?? 0 : starts.find(start => start > position) ?? Math.max(0, timelineFrames.at(-1)! - 1)
    seekProject(target / project.fps)
  }
  function applyPreviewTime() {
    if (timeInput === undefined) return
    const seconds = Number(timeInput)
    if (!timeInput.trim() || !Number.isFinite(seconds) || seconds < 0 || Math.round(seconds * project.fps) >= displayFrames) { setTimeError('输入预览范围内的时间'); return }
    seekPreview(seconds); setTimeInput(undefined); setTimeError('')
  }
  function duplicateShot(shotId = selectedId) {
    const source = current.current.shots.find(shot => shot.id === shotId)
    if (!source) return
    setCopyRequest({ source: structuredClone(source), reuseMedia: false })
  }
  function applyCopy() {
    if (!copyRequest) return
    try {
      const source = current.current.shots.find(shot => shot.id === copyRequest.source.id)
      if (!source) throw new Error('原分镜已被删除')
      const id = crypto.randomUUID()
      const copy = duplicateVideoShot(source, id, copyRequest.reuseMedia)
      if (JSON.stringify(copy) !== JSON.stringify(duplicateVideoShot(copyRequest.source, id, copyRequest.reuseMedia))) throw new Error('分镜内容已修改，请重新打开复制窗口')
      commit({ ...current.current, shots: current.current.shots.flatMap(shot => shot.id === source.id ? [shot, copy] : [shot]) })
      select(copy.id, 'shot'); setCopyRequest(undefined)
    } catch (cause) { setCopyRequest({ ...copyRequest, error: cause instanceof Error ? cause.message : String(cause) }) }
  }
  function canRemoveShot(shotId: string) {
    const shot = current.current.shots.find(item => item.id === shotId)
    if (generationRecords.data?.some(item => item.shot_id === shotId && (!shot?.generationRequest || item.id === shot.generationRequest.id))) return Boolean(shot)
    const job = executions.data?.find(item => item.shotId === shotId)?.execution
    return Boolean(shot && !shot.generationRequest && !(generate.isPending && generate.variables.id === shotId) && (!shot.executionId || (job && !(active(job.status) && job.waiting !== false))))
  }
  function removeShot(shotId: string) {
    if (!canRemoveShot(shotId)) return
    setRemoveRequest(structuredClone(current.current.shots.find(shot => shot.id === shotId)!))
  }
  function applyRemoveShot() {
    if (!removeRequest) return
    const shotId = removeRequest.id
    if (!canRemoveShot(shotId)) { setError('分镜生成状态已改变，读取任务状态后重试'); return }
    if (JSON.stringify(current.current.shots.find(shot => shot.id === shotId)) !== JSON.stringify(removeRequest)) { setRemoveRequest(undefined); setError('分镜内容已改变，重新打开删除窗口'); return }
    const index = current.current.shots.findIndex(shot => shot.id === shotId)
    const shots = current.current.shots.filter(shot => shot.id !== shotId)
    commit({ ...current.current, shots }); setSelectedId(shots[Math.min(index, shots.length - 1)]?.id ?? ''); setRemoveRequest(undefined)
  }
  function splitShot(shotId: string) {
    if (!canRemoveShot(shotId)) return
    const source = current.current
    if (videoProjectTiming(source).unknownCount) return
    const index = source.shots.findIndex(shot => shot.id === shotId)
    const shot = source.shots[index]
    const offset = playhead - videoTimelineFrames(source.shots, source.fps)[index] / source.fps
    if (offset <= 0 || offset >= shot.duration) { setError('将播放游标放在当前分镜内部'); return }
    const second = { ...structuredClone(shot), id: crypto.randomUUID(), name: `${shot.name} · 后半段`, sourceIn: shot.sourceIn + offset, duration: shot.duration - offset, executionId: undefined, executionInputs: undefined, generationRequest: undefined }
    commit({ ...source, shots: source.shots.flatMap(item => item.id === shotId ? [{ ...shot, duration: offset }, second] : [item]) })
  }
  function openShotStructure(mode: VideoStructureRequest['mode'], shotId: string) {
    if (pendingInputCount) { setError('处理当前未保存的输入后修改分镜结构'); return }
    if (!canRemoveShot(shotId) || !current.current.shots.some(shot => shot.id === shotId)) { setError('分镜状态已改变，请重新读取'); return }
    const key = videoStructureDraftKey(mode, shotId)
    if (!structureDrafts[key]) setStructureDrafts(drafts => ({ ...drafts, [key]: createVideoStructureDraft(current.current, mode, shotId) }))
    setStructureTarget(key)
  }
  function discardStructureDraft(key: string) {
    setStructureDrafts(current => { const next = { ...current }; delete next[key]; return next })
    setStructureTarget(undefined)
  }
  function deleteSelection() {
    if (tab === 'audio' && selectedAudioId) { commit({ ...project, audio: project.audio?.filter(audio => audio.id !== selectedAudioId) }); setSelectedAudioId(undefined); return }
    if (!selected) return
    if (tab === 'script' || tab === 'subtitle') { patchShot({ [tab]: '' }); return }
    if (tab === 'reference') return
    removeShot(selected.id)
  }
  const candidates = selected?.versions?.filter(value => value.status === 'candidate').length ?? 0
  const protectedShotIds = project.shots.filter(shot => !canRemoveShot(shot.id)).map(shot => shot.id)
  const undoProblem = past.length ? videoHistoryProblem(past.at(-1)!, project, protectedShotIds) : undefined
  const redoProblem = future.length ? videoHistoryProblem(future[0], project, protectedShotIds) : undefined
  const properties = source?.input_schema.properties as Record<string, { type?: string; title?: string; enum?: unknown[] }> | undefined
  const summary = source?.inputs.filter(field => field.type === 'number' || field.type === 'boolean' || properties?.[field.key]?.enum).map(field => `${properties?.[field.key]?.title || field.description || field.key} ${typeof selected?.inputs[field.key] === 'boolean' ? selected.inputs[field.key] ? '开启' : '关闭' : selected?.inputs[field.key] ?? '未设置'}`).join(' · ')
  return <section ref={editorRoot} className='studio-video-editor' aria-label='视频编辑器' tabIndex={-1} onBlurCapture={() => { historyGroup.current = undefined }} onKeyDown={event => {
    if (event.defaultPrevented || !event.currentTarget.contains(event.target as Node) || (event.target as HTMLElement).closest('input,textarea,select,video,audio,.sve-library,.sve-timeline-labels,[contenteditable="true"],[role="combobox"],[role="slider"],[role="separator"],[role="menu"]')) return
    if ((event.metaKey || event.ctrlKey) && event.key.toLowerCase() === 'z') { event.preventDefault(); if (event.shiftKey) redo(); else undo() }
    else if ((event.metaKey || event.ctrlKey) && event.key.toLowerCase() === 'd') { event.preventDefault(); duplicateShot() }
    else if ((event.key === 'Delete' || event.key === 'Backspace') && (event.target as HTMLElement).closest('.sve-timeline')) { event.preventDefault(); deleteSelection() }
    else if (!assetPreview && event.key === 'ArrowLeft') { event.preventDefault(); stepPreview(-1) }
    else if (!assetPreview && event.key === 'ArrowRight') { event.preventDefault(); stepPreview(1) }
    else if (!assetPreview && event.key === 'Home') { event.preventDefault(); seekPreview(0) }
    else if (!assetPreview && event.key === 'End') { event.preventDefault(); seekPreview(Math.max(0, displayFrames - 1) / project.fps) }
    else if (event.code === 'Space' && !(event.target as HTMLElement).closest('button')) { event.preventDefault(); if (previewOpen) player.current?.toggle() }
  }}>
    <header className='sve-header'>
      <VideoProjectSettings project={project} assets={assets} disabled={version < 0 || external || leaving} onApply={next => applyCommit(next)} />
      <span className='sve-muted'>{total.toFixed(1)} 秒{sequence ? ` + ${timing.unknownCount} 个分镜时长未定` : ''} · {project.shots.filter(shot => shot.media).length}/{project.shots.length} 分镜可用</span>
      <div className='sve-actions'>
        {scriptRequests}
        <Button size='sm' variant='ghost' disabled={!workId} onClick={() => { setGenerationRecordsOpen(true); void generationRecords.refetch() }}>生成记录</Button>
        <Button size='sm' variant='ghost' disabled={!workId} onClick={() => setExportHistoryOpen(true)}>导出记录</Button>
        <Button size='icon-sm' variant='ghost' aria-label='撤销视频编辑' disabled={!past.length || Boolean(undoProblem)} title={undoProblem} onClick={undo}><Undo2 /></Button>
        <Button size='icon-sm' variant='ghost' aria-label='重做视频编辑' disabled={!future.length || Boolean(redoProblem)} title={redoProblem} onClick={redo}><Redo2 /></Button>
        <Button size='sm' variant='ghost' disabled={save.isPending || (!dirty && !pendingInputCount) || external || Boolean(timingChange)} onClick={() => { if (pendingInputCount) { locatePendingInput(); return }; save.reset(); save.mutate(current.current) }}>{save.isPending ? '保存中' : pendingInputCount ? `处理 ${pendingInputCount} 项输入` : dirty ? '保存' : '已保存'}</Button>
        {editorState.error ? <><FieldError>{editorState.error.message}</FieldError><Button size='sm' variant='outline' onClick={() => void editorState.refetch()}>重新读取编辑状态</Button></> : null}
        {viewError ? <><FieldError>{viewError}</FieldError><Button size='sm' variant='outline' disabled={saveEditorState.isPending} onClick={() => void persistEditorView().catch(cause => setViewError(cause instanceof Error ? cause.message : String(cause)))}>重试草稿保存</Button></> : null}
        <Button size='sm' variant='outline' disabled={!dialogSnapshot && (external || !project.shots.length || Boolean(timingChange) || save.isPending)} onClick={() => { if (dialogSnapshot) setExportOpen(true); else void refreshExportSnapshot() }}><Download />{exportPending ? '查看导出' : visibleExportTask?.status === 'completed' ? '查看成片' : visibleExportTask?.status === 'failed' ? '重试导出' : visibleExportTask ? '恢复导出' : '导出成片'}</Button>
      </div>
    </header>
    {pendingMerge ? <Alert aria-label='项目编辑冲突'><AlertTitle>同一内容出现不同修改</AlertTitle><div className='max-h-36 overflow-auto'>{pendingMerge.conflicts.map(conflict => <div key={`${conflict.object}:${conflict.field}`} className='grid grid-cols-2 gap-2 border-b py-2 text-xs'><span className='col-span-2'>{project.shots.find(shot => shot.id === conflict.object)?.name ?? '作品'} · {conflict.field}</span><pre className='whitespace-pre-wrap break-all'>{typeof conflict.local === 'string' ? conflict.local : JSON.stringify(conflict.local, null, 2) ?? '已删除'}</pre><pre className='whitespace-pre-wrap break-all'>{typeof conflict.remote === 'string' ? conflict.remote : JSON.stringify(conflict.remote, null, 2) ?? '已删除'}</pre></div>)}</div><div className='flex gap-2'><Button size='sm' variant='outline' onClick={() => { try { acceptMerge('remote') } catch (cause) { setError(String(cause)) } }}>采用更新的冲突内容</Button><Button size='sm' variant='outline' onClick={() => { try { acceptMerge('local') } catch (cause) { setError(String(cause)) } }}>保留当前冲突内容</Button></div></Alert> : null}
    {failure ? <Alert><AlertTitle>{failure}</AlertTitle><Button variant='ghost' size='sm' onClick={() => { setError(''); if (!save.isPending) save.reset(); if (!generate.isPending) generate.reset(); if (!exportVideo.isPending) exportVideo.reset(); if (!saveExport.isPending) saveExport.reset(); if (!upload.isPending) upload.reset() }}>关闭提示</Button></Alert> : null}
    <VideoEditorLayout scriptRequest={scriptFocus?.requestId} script={<VideoScriptPanel project={project} selectedShotId={selectedId} focus={scriptFocus} disabled={version < 0 || external || leaving || Boolean(timingChange)} onCommit={commit} onCreateScript={() => openScriptIdeas()}
      onImportScript={() => { setLibrary(true); setAssetBrowser(current => ({ ...current, assetKind: 'document', search: '', assetScrollTop: 0 })) }}
      canRemoveShot={canRemoveShot}
      onCreateStoryboard={shotIds => { setScriptTarget({ scope: 'storyboard', shotIds }); setIdeas(shotIds ? '根据当前脚本更新选定分镜，保持人物与动作衔接。' : '根据当前脚本生成分镜，保持角色、场景与动作连续。'); setScriptError('') }} onSelectShot={id => select(id, 'shot')} />} libraryOpen={library} onCloseInspector={() => setInspector(false)} onLibraryChange={setLibrary} onPreviewVisibilityChange={setPreviewOpen} onResetLayout={() => setTrackDisplay({ trackHeights: {}, collapsedTracks: [] })} previewTools={previewSource ? <VideoAssetInfo asset={previewSource} versionId={assetPreview?.versionId} onVersionChange={id => selectAssetVersion(previewSource.id, id)} /> : null} preview={
    <aside className='sve-preview-pane' aria-label='视频预览区域'>
    <div className='sve-preview'>{!viewLoaded ? <span className='sve-muted'>正在读取预览状态</span> : assetPreview ? previewRequest.isError ? <div className='flex flex-col items-center gap-2'>
      <span role='alert' className='sve-muted'>{previewRequest.error instanceof ApiError && previewRequest.error.status === 404 ? '预览资产已不可用' : previewRequest.error.message}</span>
      <Button size='sm' variant='outline' disabled={previewRequest.isFetching} onClick={() => void previewRequest.refetch()}>重新读取</Button>
    </div> : !previewSource ? <span className='sve-muted'>正在读取资产</span> : previewAsset ? <VideoAssetPreview key={`${previewAsset.id}:${assetPreview.versionId}`} asset={previewAsset} time={assetPreview.time} onTimeChange={time => setAssetPreview(current => current?.assetId === previewAsset.id && current.versionId === assetPreview.versionId && current.time !== time ? { ...current, time } : current)} /> : <span role='alert' className='sve-muted'>资产版本已不可用，请重新选择。</span> : project.shots.length ? <VideoProjectPreview project={displayProject} frame={frame} loop={loop} playerRef={mountPlayer} /> : <Button size='sm' variant='outline' disabled={version < 0} onClick={() => openScriptIdeas()}><Film />从想法创建视频</Button>}</div>
    <div className='sve-transport'><span className='sve-muted'>{previewSource?.name ?? (assetPreview ? '资产预览' : `${project.width}×${project.height} · ${project.fps} fps`)}</span><div className='sve-actions'>{assetPreview ? <>{previewAsset && exportTask?.result?.assetId === previewAsset.id && exportTask.result.versionId === assetPreview.versionId ? <Button size='sm' variant='outline' disabled={saveExport.isPending} onClick={() => saveExport.mutate(previewAsset)}><Download />{saveExport.isPending ? '正在保存' : '保存成片'}</Button> : null}<Button size='sm' variant='outline' onClick={() => setAssetPreview(undefined)}>返回视频预览</Button></> : <>
      <Button size='icon-sm' variant='ghost' aria-label='上一分镜' disabled={!project.shots.length || (sequence ? selectedId === project.shots[0]?.id : playhead === 0)} onClick={() => seekShot(-1)}><SkipBack /></Button>
      <Button size='icon-sm' variant='ghost' aria-label='上一帧' disabled={!project.shots.length || frame === 0} onClick={() => stepPreview(-1)}><StepBack /></Button>
      <Button size='icon-sm' variant='ghost' disabled={!displayFrames} aria-label={playing ? '暂停视频' : '播放视频'} onClick={() => player.current?.toggle()}>{playing ? <Pause /> : <Play />}</Button>
      <Button size='icon-sm' variant='ghost' aria-label='下一帧' disabled={!project.shots.length || frame >= displayFrames - 1} onClick={() => stepPreview(1)}><StepForward /></Button>
      <Button size='icon-sm' variant='ghost' aria-label='下一分镜' disabled={!project.shots.length || (sequence ? selectedId === project.shots.at(-1)?.id : Math.round(playhead * project.fps) >= timelineFrames.at(-1)! - 1)} onClick={() => seekShot(1)}><SkipForward /></Button>
      <Input className='h-7 w-24 text-xs' type='number' aria-label='预览时间（秒）' aria-invalid={Boolean(timeError)} disabled={!displayFrames} min={0} step={1 / project.fps} value={displayFrames ? timeInput ?? (frame / project.fps).toFixed(3) : ''} placeholder='时长未定' onFocus={() => player.current?.pause()} onChange={event => { setTimeInput(event.target.value); setTimeError('') }} onBlur={applyPreviewTime} onKeyDown={event => { if (event.key === 'Enter') { event.preventDefault(); event.currentTarget.blur() } else if (event.key === 'Escape') { event.preventDefault(); setTimeInput(undefined); setTimeError('') } }} />
      <span aria-label='预览时长'>{displayFrames ? `/ ${displaySeconds.toFixed(2)} 秒${sequence ? ' · 当前分镜' : ''}` : '时长未定'}</span>
      <Popover><PopoverTrigger asChild><Button size='icon-sm' variant='ghost' aria-label='监听音量'>{monitorVolume === 0 ? <VolumeX /> : <Volume2 />}</Button></PopoverTrigger><PopoverContent className='w-52'><FieldGroup><Field><FieldLabel>监听音量（%）</FieldLabel><Input type='number' min={0} max={100} value={Math.round(monitorVolume * 100)} onChange={event => { const value = event.target.valueAsNumber; if (Number.isFinite(value) && value >= 0 && value <= 100) { setMonitorVolume(value / 100); if (value > 0) audibleVolume.current = value / 100 } }} /></Field><Button size='sm' variant='outline' onClick={() => setMonitorVolume(monitorVolume === 0 ? audibleVolume.current : 0)}>{monitorVolume === 0 ? '恢复监听声音' : '静音监听'}</Button></FieldGroup></PopoverContent></Popover>
      <Button size='icon-sm' variant='ghost' aria-label='全屏预览' disabled={!project.shots.length} onClick={() => player.current?.requestFullscreen()}><Maximize /></Button>
      {loop ? <Button variant='secondary' size='sm' onClick={() => seekProject(playhead)}>退出分镜循环<X /></Button> : null}
    </>}
      <Button size='icon-sm' variant='ghost' aria-label='查看编辑器快捷键' onClick={() => setShortcutsOpen(true)}><Keyboard /></Button>
    </div>{timeError ? <span role='alert' className='text-xs text-muted-foreground'>{timeError}</span> : null}</div>
    </aside>} assets={
    <aside className='sve-assets-pane' aria-label='项目资产区域'>
    <VideoAssetLibrary assets={assets} assetVersions={assetVersions} browser={assetBrowser} onBrowserChange={setAssetBrowser} onVersionChange={selectAssetVersion} imports={assetImports} uploading={upload.isPending || assetImports.some(item => item.status === 'queued' || item.status === 'uploading')} selected={Boolean(selected)} onReference={onReference} onPreview={openAssetPreview} onAdd={(asset, track) => { const mediaVersion = asset.versions.find(item => item.version === asset.current_version); if (mediaVersion) addAsset(asset.id, mediaVersion.id, track, selectedId || undefined) }} onUpload={files => void importAssets(files.map(file => ({ id: crypto.randomUUID(), file, status: 'queued' })))} onRetry={id => { const item = assetImports.find(entry => entry.id === id); if (item) void importAssets([item]) }} onDismissImports={() => setAssetImports([])} />
    </aside>} timeline={<>
    <div className='sve-timeline-header'><strong>时间轴</strong><div className='sve-actions'>
      {batchSelection.filter(id => project.shots.some(shot => shot.id === id)).length ? <><span>已选择 {batchSelection.filter(id => project.shots.some(shot => shot.id === id)).length} 个分镜</span><Button size='sm' variant='ghost' onClick={() => setBatchSelection([])}>清除选择</Button></> : null}
      <Button size='sm' variant='outline' disabled={!project.shots.length || version < 0 || external || leaving || Boolean(timingChange)} onClick={() => { if (pendingInputCount) { locatePendingInput(); return }; setWorkflowBatchOpen(true) }}>批量设置工作流</Button>
      <Button size='sm' variant='outline' disabled={version < 0 || external || leaving || Boolean(timingChange) || submitGenerationBatch.isPending || (!pendingGenerationBatch && !project.shots.length)} onClick={() => { if (pendingGenerationBatch) { submitGenerationBatch.mutate(undefined); return }; if (pendingInputCount) { locatePendingInput(); return }; submitGenerationBatch.reset(); setGenerationBatchOpen(true) }}>{pendingGenerationBatch ? '恢复批量提交' : '批量生成'}</Button>
      {Object.keys(structureDrafts).length ? <Popover open={structureMenuOpen} onOpenChange={setStructureMenuOpen}><PopoverTrigger asChild><Button size='sm' variant='ghost'>查看结构草稿 · {Object.keys(structureDrafts).length}</Button></PopoverTrigger><PopoverContent className='flex max-h-80 flex-col gap-1 overflow-y-auto shadow-lg' aria-label='分镜结构草稿'>{Object.entries(structureDrafts).map(([key, draft]) => <Button key={key} size='sm' variant='ghost' className='justify-start' onClick={() => { setStructureMenuOpen(false); setStructureTarget(key) }}>{draft.request.mode === 'split' ? '分成多个分镜' : '合并分镜'} · {draft.request.project.shots.find(shot => shot.id === draft.request.shotId)?.name}</Button>)}</PopoverContent></Popover> : null}
      {project.audio?.length || project.shots.some(shot => shot.subtitle) ? <Popover><PopoverTrigger asChild><Button size='sm' variant='ghost'>已有内容</Button></PopoverTrigger><PopoverContent align='end' className='max-h-80 overflow-y-auto shadow-lg' aria-label='已有字幕与音频'><div className='flex flex-col gap-1'>
        {project.shots.filter(shot => shot.subtitle).map(shot => <Button key={shot.id} size='sm' variant='ghost' className='justify-start' onClick={() => select(shot.id, 'subtitle')}>字幕 · {shot.name}</Button>)}
        {project.audio?.map(audio => <Button key={audio.id} size='sm' variant='ghost' className='justify-start' onClick={() => { setSelectedAudioId(audio.id); setInspector(true); setTab('audio') }}>音频 · {assets.find(asset => asset.id === audio.assetId)?.name ?? '未命名音频'}</Button>)}
      </div></PopoverContent></Popover> : null}
      <Button size='icon-sm' variant={snap ? 'secondary' : 'ghost'} aria-label='时间轴吸附' aria-pressed={snap} onClick={() => setSnap(!snap)}><Magnet /></Button>
      <Button size='icon-sm' variant='ghost' aria-label='适应全部时间轴' onClick={() => { const width = timelineBody.current?.clientWidth; if (width) { setZoom(fitTimelineZoom(width, sequence ? project.shots.length : total, sequence ? 240 : 48)); setFitRequest(value => value + 1) } }}><Scan /></Button>
      <Button size='icon-sm' variant='ghost' aria-label='缩小时间轴' onClick={() => setZoom(Math.max(.00001, zoom / 1.5))}>−</Button><span>{(zoom * 100).toLocaleString(undefined, { maximumFractionDigits: 2 })}%</span><Button size='icon-sm' variant='ghost' aria-label='放大时间轴' onClick={() => setZoom(Math.min(4, zoom * 1.5))}>+</Button><Button size='sm' variant='outline' onClick={() => { const shot = newVideoShot(); commit({ ...project, shots: selected ? project.shots.flatMap(item => item.id === selected.id ? [item, shot] : [item]) : [...project.shots, shot] }); select(shot.id, 'shot') }}><Plus />新增分镜</Button></div></div>
    <div ref={timelineBody} className='sve-timeline-body'>{viewLoaded ? <VideoTimeline
      project={project} selectedId={selectedId} selectedTrack={tab} selectedAudioId={selectedAudioId} time={playhead}
      batchSelection={batchSelection} onBatchSelectionChange={(id, checked) => setBatchSelection(current => checked ? [...new Set([...current, id])] : current.filter(value => value !== id))}
      zoom={zoom} snap={snap} fitRequest={fitRequest} viewport={timelineViewport} onViewportChange={setTimelineViewport}
      display={trackDisplay} onDisplayChange={setTrackDisplay} onCommit={commit} onSelect={select} onError={setError}
      textDraft={textDraft} onTextDraftChange={setTextDraft}
      onAssetDrop={addAsset} onScriptIdeas={openScriptIdeas} onDuplicateShot={duplicateShot} onSplitShot={splitShot} onStructure={openShotStructure} onRemoveShot={removeShot} canRemoveShot={canRemoveShot}
      onRemoveAudio={id => { commit({ ...current.current, audio: current.current.audio?.filter(audio => audio.id !== id) }); if (selectedAudioId === id) setSelectedAudioId(undefined) }}
      onAudio={(id, openInspector = true) => { setSelectedAudioId(id); setInspector(openInspector); setTab('audio') }}
      audioName={id => { const audio = project.audio?.find(item => item.id === id); return assets.find(asset => asset.id === audio?.assetId)?.name ?? '音频' }}
      onSeek={seekProject}
    /> : <span className='sve-muted'>正在读取时间轴</span>}</div></>} inspector={
      inspector && selected ? <aside className='sve-inspector' aria-label='分镜属性'><div className='flex items-center justify-between gap-2'><strong className='truncate text-sm'>{({ script: '剧本', shot: '分镜', reference: '参考资产', video: '视频', subtitle: '字幕', audio: '成片音频' } as Record<string, string>)[tab]} · {tab === 'audio' ? assets.find(asset => asset.id === project.audio?.find(audio => audio.id === selectedAudioId)?.assetId)?.name ?? '添加音频' : selected.name}</strong><Button size='icon-sm' variant='ghost' aria-label='关闭分镜属性' onClick={() => setInspector(false)}><X /></Button></div><div className='sve-inspector-fields'>
        {tab === 'script' || tab === 'subtitle' ? <FieldGroup><Field><FieldLabel>{tab === 'script' ? '剧本' : '字幕'}</FieldLabel><Textarea aria-label={tab === 'script' ? '剧本内容' : '字幕内容'} value={selected[tab]} onChange={event => patchShot({ [tab]: event.target.value }, selected.id, tab)} /></Field>{tab === 'script' ? <><Button variant='outline' size='sm' onClick={() => openScriptIdeas(selected.id)}><MessageCircle />根据想法生成剧本</Button><Button variant='outline' size='sm' disabled={!canPreparePrompt} onClick={openPromptDraft}>{hasPromptDraft ? '查看待采用提示词' : '根据剧本更新提示词'}</Button></> : null}</FieldGroup> : null}
        {tab === 'shot' ? <FieldGroup>
          <VideoPropertyField key={`${selected.id}:name`} id={`video-name-${sessionId}-${selected.id}`} label='分镜名称' value={selected.name} draft={nameDrafts[selected.id]} disabled={version < 0 || external || leaving}
            onDraftChange={draft => setNameDrafts(current => { const next = { ...current }; if (draft) next[selected.id] = draft; else delete next[selected.id]; return next })}
            onApply={draft => commit(renameVideoShot(current.current, selected.id, draft))} />
          <Field><FieldLabel htmlFor={`video-storyboard-${sessionId}-${selected.id}`}>画面描述</FieldLabel><Textarea id={`video-storyboard-${sessionId}-${selected.id}`} value={selected.storyboard ?? ''} onChange={event => patchShot({ storyboard: event.target.value }, selected.id, 'storyboard')} /></Field>
          {numberField({ kind: 'shot', id: selected.id, field: 'plannedDuration' }, '计划时长（秒）', selected.plannedDuration ?? 0)}
          <VideoScriptSources key={selected.id} project={project} shot={selected} disabled={version < 0 || external || leaving} onCommit={commit} onLocate={blockId => { setScriptFocus({ blockId, requestId: crypto.randomUUID() }); setLibrary(false); setInspector(false) }} />
          <div className='flex flex-wrap gap-2'><Button size='sm' variant='outline' disabled={version < 0 || external || leaving || !canRemoveShot(selected.id) || Boolean(timingChange)} onClick={() => openShotStructure('split', selected.id)}>分成多个分镜</Button><Button size='sm' variant='outline' disabled={version < 0 || external || leaving || project.shots.length < 2 || !canRemoveShot(selected.id) || Boolean(timingChange)} onClick={() => openShotStructure('merge', selected.id)}>合并分镜</Button></div>
        </FieldGroup> : null}
        {tab === 'reference' ? <>{onOpenVideoChat ? <Button size='sm' variant='outline' disabled={version < 0 || external || save.isPending || leaving} onClick={async () => {
          try {
            const saved = dirty || !record.data?.asset_id ? await save.mutateAsync(current.current) : record.data
            if (!saved?.asset_id) throw new Error('保存视频作品后创建素材对话')
            setMaterialVideoId(saved.asset_id)
          } catch (cause) { setError(cause instanceof Error ? cause.message : String(cause)) }
        }}><MessageCircle />素材创作对话</Button> : null}{selected.references?.map(reference => {
          const asset = assets.find(item => item.id === reference.assetId)
          const assetVersion = asset?.versions.find(item => item.id === reference.versionId)
          const sourceShot = project.shots.find(item => item.id === reference.sourceShotId)
          const stale = staleReferences.some(item => item.id === reference.id)
          return <div className='sve-reference-item' key={reference.id}>
            <span className='truncate'>{asset?.name ?? '参考资产'}</span>
            <span className='sve-muted'>{videoReferenceRoleLabels[reference.role]}{assetVersion ? ` · 版本 ${assetVersion.version}` : ''}</span>
            {reference.sourceShotId ? <span className='sve-muted'>{sourceShot?.name ?? '来源分镜已删除'} · {reference.sourcePosition === 'first' ? '剪辑首帧' : reference.sourcePosition === 'fixed' ? '指定画面' : '剪辑尾帧'}{reference.sourceFrame !== undefined && reference.sourceFps ? ` · ${(reference.sourceFrame / reference.sourceFps).toFixed(3)} 秒` : ''}</span> : null}
            {reference.inputKey ? <span className='sve-muted'>生成参数：{source?.inputs.find(field => field.key === reference.inputKey)?.description || reference.inputKey}</span> : null}
            {reference.kind === 'image' ? <FrameImage media={reference} label='参考图片' /> : null}
            {stale ? <Alert><AlertTitle>{sourceShot ? '来源分镜已更新' : '来源分镜已删除'}</AlertTitle><div className='flex flex-col gap-2'>
              {sourceShot?.media ? <Button size='sm' variant='outline' disabled={frameReference.isPending} onClick={() => frameReference.mutate({ sourceId: sourceShot.id, targetId: selected.id, referenceId: reference.id })}>{frameReference.isPending ? '正在提取画面' : '采用新画面'}</Button> : null}
              <Button size='sm' variant='outline' disabled={frameReference.isPending} onClick={() => patchShot(resolveVideoReferenceSource(selected, reference.id, sourceShot, false, project.fps))}>保留原参考</Button>
            </div></Alert> : null}
            <Button size='sm' variant='ghost' onClick={() => patchShot(removeVideoReference(selected, reference.id))}>移除参考</Button>
            <Button size='sm' variant='outline' disabled={external || leaving} onClick={() => setReferenceVersionTarget(structuredClone(reference))}>{assetVersion && asset && assetVersion.version < asset.current_version ? '有新版本 · 选择应用范围' : '选择素材版本与范围'}</Button>
          </div>
        })}<Button size='sm' variant='outline' onClick={() => { setLibrary(true); setInspector(false) }}>添加项目资产作为参考</Button><Button size='sm' variant='outline' disabled={frameReference.isPending || !project.shots.some(shot => shot.id !== selected.id && shot.media)} onClick={() => { frameReference.reset(); setFrameTargetId(selected.id) }}>引用分镜画面</Button></> : null}
        {tab === 'video' ? <FieldGroup>
          {selected.media ? (['duration', 'sourceIn', 'volume'] as const).map(field => numberField({ kind: 'shot', id: selected.id, field }, field === 'duration' ? '使用时长（秒）' : field === 'sourceIn' ? '源视频开始（秒）' : '视频音量', selected[field] ?? 1)) : <span className='text-sm text-muted-foreground'>未采用视频</span>}
          <Field><FieldLabel htmlFor={`video-import-${sessionId}-${selected.id}`}>导入视频版本</FieldLabel><Input id={`video-import-${sessionId}-${selected.id}`} type='file' accept='video/*' disabled={upload.isPending || version < 0 || external || leaving || Boolean(timingChange)} onChange={async event => {
            const file = event.currentTarget.files?.[0]
            event.currentTarget.value = ''
            if (!file) return
            const shot = current.current.shots.find(shot => shot.id === selected.id)
            if (!shot) { setError('目标分镜已删除'); return }
            const target = structuredClone(shot)
            try {
              const asset = await upload.mutateAsync({ file })
              const next = applyUploadedVideoMedia(current.current, target, asset)
              if (next !== current.current) commit(next)
            } catch (cause) { setError(cause instanceof Error ? cause.message : String(cause)) }
          }} /></Field>
          {selected.versions?.map(item => <div key={item.id} className='flex flex-col gap-2 rounded border p-2'><span className='text-xs'>{item.status === 'candidate' ? '新版本待采用' : '历史版本'}</span><VideoVersionPreview media={item.media} /><Button size='sm' variant='outline' onClick={() => setAdoption(createVideoAdoption(current.current, selected, item.media))}>采用此版本</Button></div>)}
        </FieldGroup> : null}
        {tab === 'audio' ? <FieldGroup>
          <Field><FieldLabel>添加项目音频</FieldLabel><Select onValueChange={id => { const asset = assets.find(item => item.id === id)!; const mediaVersion = asset.versions.find(item => item.version === asset.current_version)!; addAsset(id, mediaVersion.id, 'audio', selectedId) }}><SelectTrigger><SelectValue placeholder='选择音频资产' /></SelectTrigger><SelectContent><SelectGroup>{assets.filter(asset => asset.kind === 'audio').map(asset => <SelectItem key={asset.id} value={asset.id}>{asset.name}</SelectItem>)}</SelectGroup></SelectContent></Select></Field>
          {project.audio?.filter(audio => audio.id === selectedAudioId).map(audio => <div key={audio.id} className='flex flex-col gap-3'>
            {(['start', 'duration', 'sourceIn', 'volume'] as const).map(field => numberField({ kind: 'audio', id: audio.id, field }, field === 'start' ? '时间轴开始（秒）' : field === 'duration' ? '使用时长（秒）' : field === 'sourceIn' ? '音频开始（秒）' : '音量', audio[field]))}
            <Field orientation='horizontal'><FieldLabel htmlFor={`video-audio-loop-${sessionId}-${audio.id}`}>循环播放</FieldLabel><Switch id={`video-audio-loop-${sessionId}-${audio.id}`} checked={audio.loop} disabled={version < 0 || external || leaving || Boolean(timingChange)} onCheckedChange={value => { try { changeAudioLoop(audio.id, value) } catch (cause) { setError(cause instanceof Error ? cause.message : String(cause)) } }} /></Field><Button variant='ghost' size='sm' onClick={() => commit({ ...current.current, audio: current.current.audio!.filter(item => item.id !== audio.id) })}>移除音频</Button>
          </div>)}
        </FieldGroup> : null}
      </div>
    {tab !== 'audio' ? <footer className='sve-footer'><span className='sve-muted'>{shotStatus(selected)}</span><div className='sve-actions'>
      {staleReferences.length ? <Button size='sm' variant='outline' onClick={() => { setInspector(true); setTab('reference') }}>确认 {staleReferences.length} 项参考更新</Button> : null}
      <Button variant='ghost' size='sm' disabled={reference.isPending} onClick={() => reference.mutate('shot')}><MessageCircle />与分镜对话</Button><Button size='sm' variant='ghost' disabled={videoShotDuration(selected) === undefined} onClick={() => { setAssetPreview(undefined); setPreviewShotId(selected.id); setLoop(!loop); setSeekFrame(loop ? Math.round(playhead * project.fps) : 0); setLibrary(false); setInspector(false) }}>{loop ? '退出分镜循环' : '循环分镜'}</Button>
      <Popover open={generationSettingsOpen} onOpenChange={setGenerationSettingsOpen}>
        <PopoverTrigger asChild><Button size='sm' variant='outline' title={summary || boundWorkflow?.name}><Settings2 />生成设置</Button></PopoverTrigger>
        <PopoverContent className='max-h-[70vh] w-[min(420px,calc(100vw-24px))] overflow-y-auto' onCloseAutoFocus={event => { if (promptTarget) event.preventDefault() }} onOpenAutoFocus={event => {
          const target = invalidInputs[0] ? document.getElementById(`video-generation-${selected.id}-${invalidInputs[0].key}`) : document.getElementById(`video-generation-errors-${selected.id}`)
          if (target) { event.preventDefault(); target.focus(); target.scrollIntoView({ block: 'nearest' }) }
        }}>
          <FieldGroup>
            {formErrors.length ? <FieldError id={`video-generation-errors-${selected.id}`} tabIndex={-1} errors={formErrors} /> : null}
            <Field><FieldLabel>生成工作流</FieldLabel><Select value={selected.workflowId} disabled={generate.isPending} onValueChange={id => void chooseWorkflow(id).catch(cause => setError(String(cause)))}><SelectTrigger><SelectValue placeholder='选择已发布的视频工作流' /></SelectTrigger><SelectContent><SelectGroup>{workflowOptions.map(item => <SelectItem key={item.id} value={item.id} disabled={!workflows.data?.some(workflow => workflow.id === item.id)}>{item.name}{!workflows.data?.some(workflow => workflow.id === item.id) ? ' · 固定版本' : ''}</SelectItem>)}</SelectGroup></SelectContent></Select></Field>
            {definition.data ? <Field><FieldLabel>固定发布版本 · {definition.data.version}</FieldLabel><Select value={selected.versionId} disabled={generate.isPending || Boolean(selected.generationRequest) || !workflowVersions.data} onValueChange={reviewWorkflowVersion}><SelectTrigger aria-label='分镜工作流发布版本'><SelectValue placeholder='正在读取发布版本' /></SelectTrigger><SelectContent><SelectGroup>{workflowVersions.data?.map(version => <SelectItem key={version.id} value={version.id}>{version.version}{version.id === boundWorkflow?.default_version_id ? ' · 默认' : ''}</SelectItem>)}</SelectGroup></SelectContent></Select>
              {boundWorkflow?.default_version_id && boundWorkflow.default_version_id !== selected.versionId ? <Button size='sm' variant='outline' disabled={generate.isPending || Boolean(selected.generationRequest)} onClick={() => reviewWorkflowVersion(boundWorkflow.default_version_id!)}>默认版本已更新 · 查看参数变化</Button> : null}
              {workflowVersions.error ? <FieldError>{workflowVersions.error.message}</FieldError> : null}
            </Field> : null}
            {invalidInputs.length ? <div className='flex flex-wrap gap-1' aria-label='待处理的生成参数'>{invalidInputs.map(field => <Button key={field.key} size='sm' variant='outline' onClick={() => {
              const target = document.getElementById(`video-generation-${selected.id}-${field.key}`)
              target?.focus(); target?.scrollIntoView({ block: 'nearest' })
            }}>{properties?.[field.key]?.title || field.description || field.key}</Button>)}</div> : null}
            {source ? <WorkflowParameterFields prefix={`video-generation-${selected.id}`} definition={source} values={selected.inputs} assets={assets} errors={inputErrors} disabled={generate.isPending || upload.isPending} onUploadAsset={(key, file) => upload.mutate({ key, file })} onChange={(key, value) => {
              const field = source.inputs.find(field => field.key === key)
              const continuous = !properties?.[key]?.enum && (field?.type === 'string' || field?.type === 'number')
              patchShot(editVideoShotInput(selected, key, value), selected.id, continuous ? `input:${key}` : undefined)
            }} /> : null}
            {source?.inputs.flatMap(field => {
              const state = videoDescriptionInputState(selected, field.key)
              if (!state) return []
              const origin = selected.inputSources![field.key]
              return <div key={field.key} className='flex flex-wrap items-center gap-2 text-xs text-muted-foreground' aria-label={`${field.key} 输入来源`}><span>{properties?.[field.key]?.title || field.description || field.key} · {state.changed ? '描述待同步' : '来自分镜描述'}{origin.revision ? ` · 来源修订 ${origin.revision}` : ''}{state.edited ? ' · 已手工修改' : ''}</span></div>
            })}
            {inputUploading ? <span role='status' className='text-xs text-muted-foreground'>正在导入素材</span> : null}
            <Button size='sm' variant='outline' disabled={!canPreparePrompt} onClick={openPromptDraft}>{hasPromptDraft ? '查看待采用提示词' : '根据分镜描述更新提示词'}</Button>
          </FieldGroup>
        </PopoverContent>
      </Popover>
      {invalidInputCount && !selected.generationRequest ? <Button size='sm' variant='outline' onClick={() => setGenerationSettingsOpen(true)}>处理 {invalidInputCount} 项参数</Button> : null}
      {connectionIssue ? <FieldError>{connectionIssue}</FieldError> : null}
      {candidates ? <Button size='sm' variant='outline' onClick={() => { setInspector(true); setTab('video') }}>查看新版本</Button> : null}
      {busy ? <>
        <Button size='sm' variant='outline' disabled={execution?.cancel_requested || execution?.can_cancel === false} title={execution?.can_cancel === false ? '此任务当前仅支持停止等待' : undefined} onClick={() => void cancelWorkflowExecution(execution!.id).then(() => Promise.all([client.invalidateQueries({ queryKey: ['studio', 'video-executions'] }), client.invalidateQueries({ queryKey: ['studio', 'video-generations'] })])).catch(cause => setError(String(cause)))}>{execution?.cancel_requested ? '正在取消' : '取消生成'}</Button>
        {execution?.waiting ? <Button size='sm' variant='ghost' onClick={() => void stopWaitingForWorkflowExecution(execution!.id).then(() => Promise.all([client.invalidateQueries({ queryKey: ['studio', 'video-executions'] }), client.invalidateQueries({ queryKey: ['studio', 'video-generations'] })])).catch(cause => setError(String(cause)))}>停止等待</Button> : null}
      </> : null}
      <Button size='sm' disabled={!canGenerate} title={waitingForNode ? node.data?.issues.join('、') || node.error?.message : undefined} onClick={() => { if (pendingInputCount) { locatePendingInput(); return }; generate.mutate(selected) }}><Play />{busy ? labels[execution!.status] : waitingForNode ? '等待节点就绪' : selected.media || selectedGeneration || execution ? '使用当前设置重新生成' : '生成此分镜'}</Button></div></footer> : null}
      </aside> : null} />
    <Dialog open={desktopExitPending || navigation.status === 'blocked'} onOpenChange={open => { if (!open && !leaving) cancelLeaving() }}><DialogContent aria-describedby={undefined} className='shadow-lg'><DialogHeader><DialogTitle>视频修改尚未保存</DialogTitle></DialogHeader>{leaveError ? <p role='alert' className='text-sm'>{leaveError}</p> : null}<DialogFooter>
      <Button variant='ghost' disabled={leaving || save.isPending} onClick={() => void discardUnsavedAndLeave()}>放弃未保存修改</Button>
      <Button variant='outline' disabled={leaving} onClick={cancelLeaving}>继续编辑</Button>
      <Button disabled={leaving || save.isPending || external} onClick={() => { void saveBeforeLeaving().then(saved => { if (saved) continueLeaving() }) }}>{leaving ? '保存中' : '保存并继续'}</Button>
    </DialogFooter></DialogContent></Dialog>
    <Dialog open={shortcutsOpen} onOpenChange={setShortcutsOpen}><DialogContent aria-describedby={undefined} className='max-h-[80vh] overflow-y-auto shadow-lg'><DialogHeader><DialogTitle>编辑器快捷键</DialogTitle></DialogHeader><dl className='flex flex-col gap-3 text-sm'>
      {[
        ['Space', '播放或暂停'], ['← / →', '上一帧或下一帧'], ['Home / End', '预览开始或结束'],
        ['⌘ / Ctrl + Z', '撤销'], ['⌘ / Ctrl + Shift + Z', '重做'], ['⌘ / Ctrl + D', '复制分镜'],
        ['Delete / Backspace', '删除分镜或音频，清空选中剧本或字幕'],
        ['Enter', '编辑选中的剧本或字幕'], ['视频轨道：← / →', '调整使用时长一帧'],
        ['视频轨道：Alt + ← / →', '调整素材开始位置一帧'], ['视频轨道：Shift + ← / →', '调整分镜顺序'],
        ['音频轨道：← / →', '移动音频一帧'], ['音频轨道：Shift + ← / →', '调整音频结束位置一帧'],
      ].map(([key, action]) => <div key={key} className='flex items-start justify-between gap-4'><dt className='shrink-0 font-mono text-xs'>{key}</dt><dd className='text-right text-muted-foreground'>{action}</dd></div>)}
    </dl></DialogContent></Dialog>
    <Dialog open={Boolean(assetAction)} onOpenChange={open => { if (!open) { setReferenceInputTarget(undefined); setAssetAction(undefined) } }}><DialogContent aria-describedby={undefined} className='max-h-[85vh] overflow-y-auto shadow-lg'>
      <DialogHeader><DialogTitle>{assetAction?.track === 'video' ? '采用视频' : '添加参考资产'}</DialogTitle></DialogHeader>
      <dl className='grid grid-cols-[auto_minmax(0,1fr)] gap-x-3 gap-y-2 text-sm'><dt className='text-muted-foreground'>分镜</dt><dd className='truncate'>{referenceShot?.name ?? assetAction?.newShot?.name ?? '目标分镜不可用'}</dd><dt className='text-muted-foreground'>资产</dt><dd className='min-w-0 break-words'>{assetAction?.asset.name} · v{assetAction?.asset.versions.find(version => version.id === assetAction.versionId)?.version}</dd></dl>
      {assetAction?.track === 'video' ? <p className='text-sm text-muted-foreground'>采用后按素材可用时长更新分镜，原视频保留为历史版本。</p> : <FieldGroup className='gap-3'>
        <Field className='gap-1.5'><FieldLabel htmlFor='video-reference-role'>参考用途</FieldLabel><Select value={referenceRole} onValueChange={value => { setReferenceRole(value as VideoReference['role']); setAssetAction(current => current ? { ...current, error: undefined } : current) }}><SelectTrigger id='video-reference-role'><SelectValue /></SelectTrigger><SelectContent><SelectGroup>{videoReferenceRoles.filter(role => role === 'reference' || (assetAction?.asset.kind === 'image' && ['storyboard', 'character', 'scene', 'style', 'first_frame', 'last_frame'].includes(role)) || (assetAction?.asset.kind === 'audio' && ['voice', 'rhythm'].includes(role))).map(role => <SelectItem key={role} value={role}>{videoReferenceRoleLabels[role]}</SelectItem>)}</SelectGroup></SelectContent></Select></Field>
        {referenceShot?.workflowId || referenceInputTarget ? <Field className='gap-1.5'><FieldLabel htmlFor='video-reference-input'>生成参数</FieldLabel><Select value={referenceInputTarget?.field.key ?? 'none'} onValueChange={chooseReferenceInput}><SelectTrigger id='video-reference-input'><SelectValue /></SelectTrigger><SelectContent><SelectGroup><SelectItem value='none'>仅用于创作参考</SelectItem>{referenceSource?.inputs.filter(field => field.type === assetAction?.asset.kind || field.type === 'file').map(field => <SelectItem key={field.key} value={field.key}>{field.description || field.key}</SelectItem>)}</SelectGroup></SelectContent></Select>
          {referenceWorkflow.isFetching ? <span role='status' className='text-xs text-muted-foreground'>正在读取生成参数</span> : null}
          {referenceWorkflow.error ? <><FieldError>{referenceWorkflow.error.message}</FieldError><Button size='sm' variant='outline' disabled={referenceWorkflow.isFetching} onClick={() => void referenceWorkflow.refetch()}>重新读取生成参数</Button></> : null}
        </Field> : null}
      </FieldGroup>}
      {assetAction?.error ? <div className='flex flex-col gap-2'><FieldError>{assetAction.error}</FieldError>{referenceInputTarget ? <Button size='sm' variant='outline' onClick={() => { setReferenceInputTarget(undefined); setAssetAction({ ...assetAction, error: undefined }) }}>重新选择生成参数</Button> : null}</div> : null}
      {pendingMerge?.conflicts.length ? <FieldError>项目存在编辑冲突，关闭窗口后处理</FieldError> : null}
      <DialogFooter><Button variant='outline' onClick={() => { setReferenceInputTarget(undefined); setAssetAction(undefined) }}>取消</Button><Button disabled={version < 0 || external || leaving || Boolean(timingChange)} onClick={applyAsset}>确认添加</Button></DialogFooter>
    </DialogContent></Dialog>
    {promptDraft && promptTarget ? <VideoPromptDialog key={promptTarget} draft={promptDraft} shot={project.shots.find(shot => shot.id === promptDraft.shotId)} workflow={promptWorkflow.data} loading={promptWorkflow.isFetching} loadError={promptWorkflow.error?.message} disabled={version < 0 || external || leaving || Boolean(timingChange) || generate.isPending}
      onChange={draft => setPromptDrafts(current => ({ ...current, [promptTarget]: draft }))} onClose={() => setPromptTarget(undefined)} onDiscard={discardPromptDraft} onRetry={() => void promptWorkflow.refetch()} onApply={() => {
        if (!promptWorkflow.data) throw new Error('工作流尚未读取完成')
        commit(applyVideoPromptDraft(current.current, promptDraft, promptWorkflow.data))
        discardPromptDraft()
      }} /> : null}
    {structureTarget && structureDrafts[structureTarget] ? <VideoShotStructureDialog key={structureTarget} draft={structureDrafts[structureTarget]} currentProject={project} assets={assets} disabled={version < 0 || external || leaving || Boolean(timingChange)} canRemove={canRemoveShot} onChange={draft => setStructureDrafts(current => ({ ...current, [structureTarget]: draft }))} onClose={() => setStructureTarget(undefined)} onDiscard={() => discardStructureDraft(structureTarget)} onRestart={() => {
      const { mode, shotId } = structureDrafts[structureTarget].request
      setStructureDrafts(drafts => ({ ...drafts, [structureTarget]: createVideoStructureDraft(current.current, mode, shotId) }))
    }} onApply={(sources, replacements) => {
      if (pendingInputCount) throw new Error('处理当前未保存的输入后修改分镜结构')
      if (sources.some(shot => !canRemoveShot(shot.id))) throw new Error('分镜生成状态已改变，请重新读取任务记录')
      commit(applyVideoShotStructure(current.current, sources, replacements))
      discardStructureDraft(structureTarget)
      select(replacements[0].id, 'shot')
    }} /> : null}
    <Dialog open={Boolean(copyRequest)} onOpenChange={open => { if (!open) setCopyRequest(undefined) }}><DialogContent aria-describedby={undefined} className='shadow-lg'>
      <DialogHeader><DialogTitle>复制分镜 · {copyRequest?.source.name}</DialogTitle></DialogHeader>
      <p className='whitespace-pre-wrap text-sm'>{copyRequest?.source.storyboard || '尚未填写画面描述'}</p>
      <label className='flex items-center gap-2 text-sm'><Checkbox checked={copyRequest?.reuseMedia ?? false} disabled={!copyRequest?.source.media} onCheckedChange={checked => setCopyRequest(current => current ? { ...current, reuseMedia: checked === true, error: undefined } : current)} />复用当前视频</label>
      {copyRequest?.error ? <FieldError>{copyRequest.error}</FieldError> : null}
      <DialogFooter><Button variant='outline' onClick={() => setCopyRequest(undefined)}>取消</Button><Button disabled={version < 0 || external || leaving || Boolean(timingChange)} onClick={applyCopy}>复制分镜</Button></DialogFooter>
    </DialogContent></Dialog>
    <Dialog open={Boolean(scriptTarget)} onOpenChange={open => { if (!open && !scriptSending) setScriptTarget(undefined) }}><DialogContent aria-describedby={undefined} className='shadow-lg'>
      <DialogHeader><DialogTitle>{scriptTarget?.scope === 'script' ? '创作脚本' : '生成分镜提案'}</DialogTitle></DialogHeader>
      <FieldGroup><Field><FieldLabel>创作范围</FieldLabel><Select value={scriptTarget?.scope === 'storyboard' && scriptTarget.shotIds ? 'selection' : scriptTarget?.scope} disabled={scriptSending} onValueChange={value => { if (value === 'script') setScriptTarget({ scope: 'script' }); else setScriptTarget({ scope: 'storyboard', ...(value === 'selection' ? { shotIds: selected ? [selected.id] : [] } : {}) }) }}><SelectTrigger aria-label='创作范围'><SelectValue /></SelectTrigger><SelectContent><SelectGroup><SelectItem value='script'>完整脚本</SelectItem><SelectItem value='storyboard'>全部分镜</SelectItem><SelectItem value='selection' disabled={!project.shots.length}>指定分镜</SelectItem></SelectGroup></SelectContent></Select></Field>
        {scriptTarget?.scope === 'storyboard' && scriptTarget.shotIds ? <div className='flex max-h-48 flex-col gap-2 overflow-y-auto' aria-label='分镜创作范围'>{project.shots.map(shot => <label key={shot.id} className='flex items-center gap-2 text-sm'><Checkbox aria-label={`更新分镜 ${shot.name}`} disabled={scriptSending} checked={scriptTarget.shotIds!.includes(shot.id)} onCheckedChange={checked => setScriptTarget(current => current?.scope === 'storyboard' && current.shotIds ? { ...current, shotIds: checked ? [...current.shotIds, shot.id] : current.shotIds.filter(id => id !== shot.id) } : current)} />{shot.name}</label>)}</div> : null}
        <Field><FieldLabel htmlFor='video-script-ideas'>创作要求</FieldLabel><Textarea id='video-script-ideas' value={ideas} onChange={event => setIdeas(event.target.value)} placeholder='主题、人物、情节或希望表达的感觉' disabled={scriptSending} /></Field><Field><FieldLabel>创作 Skill</FieldLabel><Select value={chosenScriptSkill} onValueChange={setScriptSkillId} disabled={scriptSending}><SelectTrigger aria-label='创作 Skill'><SelectValue placeholder='选择创作 Skill' /></SelectTrigger><SelectContent><SelectGroup>{skills.filter(skill => skill.enabled).map(skill => <SelectItem key={skill.id} value={skill.id}>{skill.name}</SelectItem>)}</SelectGroup></SelectContent></Select></Field></FieldGroup>
      {!skills.some(skill => skill.enabled && skill.name === 'pixoma-studio-video-creator') && canInstallScriptSkill ? <Button variant='outline' size='sm' disabled={installScriptSkill.isPending} onClick={() => installScriptSkill.mutate()}>{installScriptSkill.isPending ? '正在安装' : '安装 PixomaStudioVideoCreator'}</Button> : null}
      {!skills.some(skill => skill.enabled) && !canInstallScriptSkill ? <p role='alert'>当前专家尚未配置剧本 Skill。</p> : null}
      {scriptError ? <p role='alert'>{scriptError}</p> : null}
      <DialogFooter>{scriptTarget?.scope === 'script' ? <Button variant='ghost' disabled={scriptSending || version < 0} onClick={writeScriptManually}>手动编写</Button> : null}<Button variant='outline' disabled={scriptSending} onClick={() => setScriptTarget(undefined)}>取消</Button><Button disabled={scriptSending || !ideas.trim() || !chosenScriptSkill || version < 0 || (scriptTarget?.scope === 'storyboard' && scriptTarget.shotIds?.length === 0)} onClick={() => void sendScriptIdeas()}>{scriptSending ? '正在发送' : '发送到 Chat 生成'}</Button></DialogFooter>
    </DialogContent></Dialog>
    {recovery ? <VideoProjectRecovery snapshot={recovery.snapshot} current={recovery.baseline} disabled={external || leaving || Boolean(timingChange) || save.isPending} onClose={() => setRecovery(undefined)} onRestore={() => {
      if (serializeVideoContent(current.current) !== serializeVideoContent(recovery.baseline)) throw new Error('作品已修改，请重新查看来源作品后恢复')
      if (pendingInputCount) throw new Error('处理当前未保存的输入后恢复作品')
      commit(restoreVideoEditorHistory(recovery.snapshot.project, current.current, protectedShotIds))
    }} /> : null}
    {dialogSnapshot ? <VideoExportDialog key={dialogSnapshot.id} sessionId={sessionId} snapshot={dialogSnapshot} assets={assets} open={exportOpen} pending={exportPending} refreshing={save.isPending} previewPending={previewExport.isPending} task={visibleExportTask} error={previewExport.error?.message || (visibleExportTask && visibleExportTask.status !== 'completed' ? exportStatus.error?.message : undefined) || exportVideo.error?.message} onOpenChange={setExportOpen} onOpenSource={() => { setRecovery({ snapshot: structuredClone(dialogSnapshot), baseline: structuredClone(current.current) }); setExportOpen(false) }} onEdit={() => exportVideo.reset()} onRefresh={() => void refreshExportSnapshot()} onExport={submission => {
      const task = createVideoExportTask(submission, dialogSnapshot.sourceVersion)
      setExportSnapshot({ id: task.request.request_id, project: task.project, sourceVersion: task.sourceVersion })
      exportVideo.mutate(task, { onSuccess: asset => { openAssetPreview(asset); setExportOpen(false); setExportSnapshot(undefined) } })
    }} onResume={() => { if (visibleExportTask) exportVideo.mutate(visibleExportTask, { onSuccess: asset => { openAssetPreview(asset); setExportOpen(false); setExportSnapshot(undefined) } }) }} onPreview={() => { if (visibleExportTask?.result) previewExport.mutate(visibleExportTask.result) }} onLocate={issue => {
      setExportOpen(false)
      if (issue.kind === 'shot') {
        if (!current.current.shots.some(shot => shot.id === issue.id)) { setError('分镜已删除，请使用当前作品重新检查导出'); return }
        select(issue.id, issue.track ?? 'video')
      } else if (issue.kind === 'audio') {
        if (!current.current.audio?.some(audio => audio.id === issue.id)) { setError('音频已删除，请使用当前作品重新检查导出'); return }
        setSelectedAudioId(issue.id); setTab('audio'); setInspector(true)
      }
    }} /> : null}
    {generationRecordsOpen ? <VideoGenerationRecords videoId={workId ?? undefined} records={generationRecords.data ?? []} shots={project.shots} sessionId={activeChatId ?? sessionId} loading={generationRecords.isPending} error={generationRecords.error?.message} onRefresh={() => { void generationRecords.refetch() }} onClose={() => setGenerationRecordsOpen(false)} onSelectShot={id => select(id, 'video')} onPreview={(assetId, versionId) => setAssetPreview({ assetId, versionId, time: 0 })} /> : null}
    <Dialog open={Boolean(removeRequest)} onOpenChange={open => { if (!open) setRemoveRequest(undefined) }}><DialogContent aria-describedby={undefined} className='shadow-lg'><DialogHeader><DialogTitle>删除分镜</DialogTitle></DialogHeader>
      {removeRequest ? <div className='flex flex-col gap-2'><p>{removeRequest.name}</p><p>{videoShotDuration(removeRequest) === undefined ? '此分镜时长未定' : `成片减少 ${videoShotDuration(removeRequest)?.toFixed(2)} 秒`}</p>
        {generationRecords.data?.some(item => item.shot_id === removeRequest.id) ? <p>生成任务与结果保留在生成记录中。</p> : null}
        {project.shots.filter(shot => shot.references?.some(reference => reference.sourceShotId === removeRequest.id)).map(shot => <p key={shot.id}>「{shot.name}」引用了此分镜画面，删除后需要检查引用。</p>)}
      </div> : null}
      <DialogFooter><Button size='sm' variant='outline' onClick={() => setRemoveRequest(undefined)}>取消</Button><Button size='sm' disabled={external || leaving || Boolean(timingChange)} onClick={applyRemoveShot}>删除分镜</Button></DialogFooter>
    </DialogContent></Dialog>
    {materialVideoId && onOpenVideoChat ? <VideoMaterialChats videoId={materialVideoId} assets={assets} shotId={selectedId || undefined} onOpen={onOpenVideoChat} onUse={(material, asset, versionId) => {
      if (!selectedId || !current.current.shots.some(shot => shot.id === selectedId)) { setError('目标分镜已删除'); return }
      setReferenceRole(asset.kind === 'image' && ['character', 'scene', 'style'].includes(material.kind) ? material.kind as VideoReference['role'] : asset.kind === 'audio' && material.kind === 'voice' ? 'voice' : 'reference')
      setReferenceInputTarget(undefined)
      setAssetAction({ asset, versionId, shotId: selectedId, track: 'reference', time: 0 })
    }} onClose={() => setMaterialVideoId(undefined)} /> : null}
    {frameTargetId ? <VideoFrameReference key={frameTargetId} project={project} targetId={frameTargetId} pending={frameReference.isPending} error={frameReference.error?.message} onApply={selection => frameReference.mutate(selection)} onClose={() => setFrameTargetId(undefined)} /> : null}
    {referenceVersionTarget ? <VideoReferenceVersions project={project} reference={referenceVersionTarget} disabled={version < 0 || external || leaving || Boolean(timingChange)} onApply={(targets, replacement) => commit(applyVideoReferenceVersion(current.current, targets, replacement))} onClose={() => setReferenceVersionTarget(undefined)} /> : null}
    {adoption ? <VideoAdoptionDialog project={project} request={adoption} preview={onReady => <VideoVersionPreview media={adoption.media} onReady={onReady} />} disabled={version < 0 || external || leaving || Boolean(timingChange)} onClose={() => setAdoption(undefined)} onApply={range => {
      const next = previewVideoAdoption(current.current, adoption, range)
      applyCommit(next)
      select(adoption.shot.id, 'video')
      setAdoption(undefined)
    }} /> : null}
    <Dialog open={Boolean(timingChange)} onOpenChange={open => { if (!open) setTimingChange(undefined) }}><DialogContent aria-describedby={undefined} className='shadow-lg'><DialogHeader><DialogTitle>{timingChange?.title ?? '调整成片范围'}</DialogTitle></DialogHeader>{timingChange?.audio.map(audio => {
      const next = timingChange.next.audio?.find(item => item.id === audio.id)
      return <div key={audio.id} className='flex flex-col gap-1'><p>{assets.find(asset => asset.id === audio.assetId)?.name ?? '音频'}：{audio.duration.toFixed(2)} 秒 → {next ? `${next.duration.toFixed(2)} 秒` : '移除'}</p>{next && next.loop !== audio.loop ? <p className='text-xs text-muted-foreground'>{audio.loop ? '循环播放' : '单次播放'} → {next.loop ? '循环播放' : '单次播放'}</p> : null}</div>
    })}<DialogFooter><Button variant='outline' onClick={() => setTimingChange(undefined)}>取消</Button><Button onClick={() => { if (!timingChange) return; if (current.current !== timingChange.previous) { setError('项目已更新，请重新进行时长调整'); setTimingChange(undefined); return }; applyCommit(timingChange.next, timingChange.group); setTimingChange(undefined) }}>确认调整</Button></DialogFooter></DialogContent></Dialog>
    <Dialog open={Boolean(scriptImport)} onOpenChange={open => { if (!open) setScriptImport(undefined) }}><DialogContent aria-describedby={undefined} className='max-h-[85vh] overflow-y-auto shadow-lg sm:max-w-2xl'><DialogHeader><DialogTitle>导入脚本</DialogTitle></DialogHeader><p>{scriptImport?.name}</p>{scriptImport?.baseline.blocks.length ? <div className='flex flex-col gap-2'><strong className='text-sm'>当前脚本</strong><Textarea aria-label='当前完整脚本' readOnly value={scriptImport.baseline.blocks.map(block => block.text).join('\n\n')} className='max-h-40' /></div> : null}<Textarea aria-label='待采用脚本' readOnly value={scriptImport?.content ?? ''} className='max-h-72 min-h-40' />{scriptImport?.error ? <p role='alert' className='text-sm'>{scriptImport.error}</p> : null}<DialogFooter><Button variant='outline' onClick={() => setScriptImport(undefined)}>取消</Button>{scriptImport?.baseline.blocks.length ? <Button variant='outline' disabled={external || leaving || Boolean(timingChange)} onClick={() => applyImportedScript('replace')}>替换完整脚本</Button> : null}<Button disabled={external || leaving || Boolean(timingChange)} onClick={() => applyImportedScript('append')}>{scriptImport?.baseline.blocks.length ? '追加到脚本' : '采用脚本'}</Button></DialogFooter></DialogContent></Dialog>
    {workflowBatchOpen ? <VideoWorkflowBatchDialog project={project} initialIds={batchSelection.filter(id => project.shots.some(shot => shot.id === id)).length ? batchSelection.filter(id => project.shots.some(shot => shot.id === id)) : selectedId ? [selectedId] : []} workflows={workflows.data ?? []} assets={assets} disabled={version < 0 || external || leaving || Boolean(timingChange) || save.isPending} onClose={() => setWorkflowBatchOpen(false)} onApply={batch => { commit(applyVideoWorkflowBatch(current.current, batch)); setWorkflowBatchOpen(false) }} /> : null}
    {generationBatchOpen ? <VideoGenerationBatchDialog project={project} initialIds={batchSelection.filter(id => project.shots.some(shot => shot.id === id)).length ? batchSelection.filter(id => project.shots.some(shot => shot.id === id)) : selectedId ? [selectedId] : []} assets={assets} enabledWorkflowIds={(enabledWorkflows.data ?? []).map(workflow => workflow.id)} records={generationRecords.data ?? []} recordsReady={generationRecords.isSuccess && enabledWorkflows.isSuccess} disabled={version < 0 || external || leaving || Boolean(timingChange) || save.isPending || Boolean(pendingInputCount)} pending={submitGenerationBatch.isPending} error={submitGenerationBatch.error?.message} onClose={() => setGenerationBatchOpen(false)} onConfirm={(baseline, ids, dependencies) => submitGenerationBatch.mutate({ baseline, ids, dependencies })} /> : null}
    {exportHistoryOpen && workId ? <VideoExportHistory videoId={workId} onClose={() => setExportHistoryOpen(false)} onOpen={task => { storeVideoExportTask(client, task, workId); setExportSnapshot({ id: task.request.request_id, project: task.project, sourceVersion: task.sourceVersion }); setExportHistoryOpen(false); setExportOpen(true) }} onSource={task => { setRecovery({ snapshot: { id: task.request.request_id, project: structuredClone(task.project), sourceVersion: task.sourceVersion }, baseline: structuredClone(current.current) }); setExportHistoryOpen(false) }} /> : null}
    {workflowVersionTarget ? <VideoWorkflowVersionDialog {...workflowVersionTarget} assets={assets} disabled={version < 0 || external || leaving || Boolean(timingChange) || save.isPending || Boolean(pendingInputCount)} onClose={() => setWorkflowVersionTarget(undefined)} onApply={review => { commit(applyVideoWorkflowVersion(current.current, review)); setWorkflowVersionTarget(undefined) }} /> : null}
  </section>
}
