import { useEffect, useRef, useState } from 'react'
import { useMutation, useQuery, useQueryClient } from '@tanstack/react-query'
import { useMatchRoute, useNavigate, useSearch } from '@tanstack/react-router'
import { usePanelRef } from 'react-resizable-panels'
import { ListTree, Menu, PanelRightClose, PanelRightOpen, Workflow } from 'lucide-react'
import { ApiError } from '@/lib/api/client'
import {
  createStudioTextAsset,
  getStudioSession,
  listStudioModels,
  listStudioProjects,
  listStudioLibraryAssets,
  listStudioSkills,
  referenceStudioAsset,
  uploadStudioAsset,
  updateStudioTextAsset,
  type StudioPermissionMode,
  type StudioSessionDetail,
  type StudioAsset,
} from '@/lib/api/studio'
import { Button } from '@/components/ui/button'
import { Badge } from '@/components/ui/badge'
import {
  Empty,
  EmptyContent,
  EmptyDescription,
  EmptyHeader,
  EmptyTitle,
} from '@/components/ui/empty'
import { IconButtonTooltip } from '@/components/ui/icon-button-tooltip'
import {
  Sheet,
  SheetContent,
  SheetDescription,
  SheetTitle,
  SheetTrigger,
} from '@/components/ui/sheet'
import { Skeleton } from '@/components/ui/skeleton'
import { Tabs, TabsContent, TabsList, TabsTrigger } from '@/components/ui/tabs'
import { ResizableHandle, ResizablePanel, ResizablePanelGroup } from '@/components/ui/resizable'
import { StudioAssetActions, StudioAssets, type AssetLayout } from './studio-assets'
import { StudioSessionTasks } from './studio-session-tasks'
import { isActiveWorkflowExecution } from './studio-session-tasks-data'
import { StudioChat, type StudioEditorRequest } from './studio-chat'
import { StudioVideoEditor } from './video-editor'
import { getStudioVideoWork, getVideoMaterialContext, type StudioVideoWork, type VideoReferenceLocation } from '@/lib/api/video-projects'
import { VideoMaterialContext } from './video-material-context'
import { VideoScriptRequests } from './video-script-requests'
import { updateVideoScriptRequests, type VideoScriptRequest } from './video-script-request'
import { StudioLibrary } from './studio-library'
import { StudioMarket } from './studio-market'
import type { WorkflowEditorState } from '@/features/cases/workflow-editor'
import { AssetCatalog } from '@/features/agent-assets/asset-catalog'
import { ExpertEditor } from '@/features/agent-assets/expert-editor'
import { getWorkflowReference } from '@/lib/api/workflows'
import { StudioNewSession } from './studio-new-session'
import { StudioSettings, type SettingSection } from './studio-settings'
import { StudioSidebar, type StudioView } from './studio-sidebar'
import { StudioTraceTabs } from './studio-trace-tabs'
import { StudioWorkbenchBackground } from './studio-workbench-background'
import { StudioPage } from './studio-page'

type SelectedAsset = { assetId: string; assetVersionId: string }
const viewedRunsStorageKey = 'studio.viewedRunIds'

export function StudioWorkspace() {
  const queryClient = useQueryClient()
  const navigate = useNavigate()
  const matchRoute = useMatchRoute()
  const search = useSearch({ strict: false })
  const libraryMatch = matchRoute({ to: '/studio/library' })
  const settingsMatch = matchRoute({ to: '/studio/settings/$section' })
  const marketMatch = matchRoute({ to: '/studio/workflows', fuzzy: true })
  const marketDetailMatch = matchRoute({ to: '/studio/workflows/$workflowId' })
  const skillMarketMatch = matchRoute({ to: '/studio/skills' })
  const expertMarketMatch = matchRoute({ to: '/studio/experts' })
  const expertEditorMatch = matchRoute({ to: '/studio/expert-editor/$expertId' })
  const sessionMatch = matchRoute({
    to: '/studio/sessions/$sessionId',
    fuzzy: true,
  })
  const view: StudioView = skillMarketMatch ? 'skill-market' : expertMarketMatch || expertEditorMatch ? 'expert-market' : marketMatch
    ? 'market'
    : libraryMatch
    ? 'library'
    : settingsMatch
      ? 'settings'
      : 'chat'
  const activeSessionId = sessionMatch ? sessionMatch.sessionId : undefined
  const marketListPosition = useRef(0)
  const [workflowEditorState, setWorkflowEditorState] = useState<WorkflowEditorState>()
  const [createNode, setCreateNode] = useState(false)
  const marketReturnSearch = useRef<{ q?: string; category?: string }>({})
  useEffect(() => {
    if (view === 'market') marketReturnSearch.current = { q: search.q, category: search.category }
  }, [view, search.q, search.category])
  const draftWorkflowId = typeof search.workflow === 'string' ? search.workflow : undefined
  const marketDraft = useQuery({ queryKey: ['studio', 'workflow-reference', draftWorkflowId], queryFn: () => getWorkflowReference(draftWorkflowId!), enabled: view === 'chat' && !activeSessionId && Boolean(draftWorkflowId) })
  const traceOpen = Boolean(
    matchRoute({ to: '/studio/sessions/$sessionId/trace' })
  )
  const [assetLayout, setAssetLayout] = useState<AssetLayout>('adaptive')
  const [rightOpen, setRightOpen] = useState(true)
  const editorSender = useRef<{ sessionId: string; send: (request: StudioEditorRequest) => void } | undefined>(undefined)
  const pendingEditorRequest = useRef<{ sessionId: string; request: StudioEditorRequest; resolve: () => void; reject: (error: unknown) => void } | undefined>(undefined)
  const [scriptRequests, setScriptRequests] = useState<VideoScriptRequest[]>(() => JSON.parse(window.sessionStorage.getItem('studio.video-script-requests') ?? '[]') as VideoScriptRequest[])
  useEffect(() => { window.sessionStorage.setItem('studio.video-script-requests', JSON.stringify(scriptRequests)) }, [scriptRequests])
  const workbenchOpen = rightOpen && !traceOpen
  const [workbenchDocked, setWorkbenchDocked] = useState(() => window.matchMedia('(min-width: 1280px)').matches)
  const chatPanel = usePanelRef()
  const workbenchPanel = usePanelRef()
  const workbenchGroup = useRef<HTMLDivElement>(null)
  const preferredChatWidth = useRef(380)
  const workbenchVisible = workbenchDocked && workbenchOpen
  useEffect(() => {
    const media = window.matchMedia('(min-width: 1280px)')
    const update = () => setWorkbenchDocked(media.matches)
    media.addEventListener('change', update)
    return () => media.removeEventListener('change', update)
  }, [])
  useEffect(() => {
    if (view !== 'chat' || !activeSessionId) return
    const resizeWorkspace = () => {
      if (workbenchVisible) {
        workbenchPanel.current?.expand()
        chatPanel.current?.resize(preferredChatWidth.current)
      } else {
        workbenchPanel.current?.collapse()
        chatPanel.current?.resize('100%')
      }
    }
    resizeWorkspace()
    const observer = new ResizeObserver(resizeWorkspace)
    if (workbenchGroup.current) observer.observe(workbenchGroup.current)
    return () => observer.disconnect()
  }, [view, activeSessionId, workbenchVisible, chatPanel, workbenchPanel])
  const [mobileMenuOpen, setMobileMenuOpen] = useState(false)
  const [workbenchSheetOpen, setWorkbenchSheetOpen] = useState(false)
  const [locateMessage, setLocateMessage] = useState<{
    id: string
    alternateId?: string
    request: number
  }>()
  const [viewedRunIds, setViewedRunIds] = useState<Record<string, string>>(() =>
    JSON.parse(window.localStorage.getItem(viewedRunsStorageKey) ?? '{}')
  )
  const chatOpenGeneration = useRef(0)
  const [newSessionKey, setNewSessionKey] = useState(0)
  const [libraryEntryProjectId, setLibraryEntryProjectId] = useState<string>()
  const [libraryEntrySessionId, setLibraryEntrySessionId] = useState<string>()
  const [libraryEntrySessionTitle, setLibraryEntrySessionTitle] = useState<string>()
  const [modelSelection, setModelSelection] = useState<{
    sessionId: string
    modelConfigId: string
  }>()
  const [skillSelection, setSkillSelection] = useState<{
    sessionId: string
    skillIds: string[]
  }>()
  const [assetSelection, setAssetSelection] = useState<{
    sessionId: string
    assets: SelectedAsset[]
  }>()
  const [permissionOverride, setPermissionOverride] = useState<{
    sessionId: string
    mode: StudioPermissionMode
  }>()

  const models = useQuery({
    queryKey: ['studio', 'models'],
    queryFn: listStudioModels,
  })
  const skills = useQuery({
    queryKey: ['studio', 'skills'],
    queryFn: async () => {
      return (await listStudioSkills()).filter(skill => skill.enabled)
    },
  })
  const startNewSession = (projectId = '') => {
    ++chatOpenGeneration.current
    setNewSessionKey((current) => current + 1)
    void navigate({ to: '/studio', search: { project: projectId } })
  }
  const sessionId = activeSessionId
  const videoId = search.video
  const videoWork = useQuery({ queryKey: ['studio', 'video-work', videoId], queryFn: () => getStudioVideoWork(videoId!), enabled: view === 'chat' && Boolean(videoId) })
  const [materialVersions, setMaterialVersions] = useState<Record<string, string>>({})
  const materialChat = videoWork.data?.chats.find(chat => chat.session_id === sessionId)
  const material = videoWork.data?.materials.find(material => material.id === materialChat?.material_id)
  const materialVersion = sessionId ? materialVersions[sessionId] : undefined
  const materialContext = useQuery({ queryKey: ['studio', 'material-context', videoId, sessionId, materialVersion], queryFn: () => getVideoMaterialContext(videoId!, sessionId!, materialVersion), enabled: Boolean(material && videoId && sessionId), retry: false })
  const videoSessionId = videoWork.data?.main_session_id ?? sessionId
  const videoSession = useQuery({ queryKey: ['studio', 'session', videoSessionId], queryFn: () => getStudioSession(videoSessionId!), enabled: view === 'chat' && Boolean(videoId && videoSessionId && videoSessionId !== sessionId) })
  async function openVideoChat(work: StudioVideoWork, targetSessionId: string, location?: VideoReferenceLocation) {
    await queryClient.fetchQuery({ queryKey: ['studio', 'session', targetSessionId], queryFn: () => getStudioSession(targetSessionId) })
    queryClient.setQueryData(['studio', 'video-work', work.id], work)
    await navigate({ to: '/studio/sessions/$sessionId', params: { sessionId: targetSessionId }, search: { panel: 'video', video: work.id, ...location } })
  }
  async function sendVideoEditorRequest(request: StudioEditorRequest) {
    if (!videoSessionId) throw new Error('视频对话尚未就绪')
    if (pendingEditorRequest.current) throw new Error('正在打开视频对话')
    if (sessionId === videoSessionId) {
      if (editorSender.current?.sessionId !== videoSessionId) throw new Error('视频对话尚未就绪')
      editorSender.current.send(request)
      return
    }
    if (!videoWork.data) throw new Error('视频作品尚未读取')
    const work = videoWork.data
    await new Promise<void>((resolve, reject) => {
      pendingEditorRequest.current = { sessionId: videoSessionId, request, resolve, reject }
      void openVideoChat(work, videoSessionId).catch(error => { pendingEditorRequest.current = undefined; reject(error) })
    })
  }

  const detail = useQuery({
    queryKey: ['studio', 'session', sessionId],
    queryFn: () => getStudioSession(sessionId!),
    enabled: Boolean(sessionId) && view === 'chat',
    staleTime: 0,
    refetchOnMount: 'always',
    refetchOnWindowFocus: true,
    refetchInterval: (query) =>
      query.state.data?.workflow_executions?.some(
        (execution) => execution.status === 'submitted'
      ) || query.state.data?.pending_clarifications?.some(
        (clarification) => clarification.reason === 'input_required'
      ) || ['queued', 'running', 'waiting_workflow'].includes(query.state.data?.session.latest_run?.status ?? '')
        ? 2500
        : false,
  })
  const projects = useQuery({
    queryKey: ['studio', 'projects'],
    queryFn: listStudioProjects,
    enabled: view === 'chat' && Boolean(sessionId),
  })
  const currentProject = projects.data?.find(project => project.id === detail.data?.session.project_id)
  const panel = search.panel ?? (currentProject?.project_type === 'video' ? 'video' : 'assets')
  useEffect(() => {
    window.localStorage.setItem(
      viewedRunsStorageKey,
      JSON.stringify(viewedRunIds)
    )
  }, [viewedRunIds])

  useEffect(() => {
    const viewedSessionId = detail.data?.session.id
    const run = detail.data?.session.latest_run
    if (
      view !== 'chat' ||
      traceOpen ||
      !viewedSessionId ||
      viewedSessionId !== sessionId ||
      run?.status !== 'succeeded' ||
      viewedRunIds[viewedSessionId] === run.id
    ) {
      return
    }
    const frame = window.requestAnimationFrame(() => {
      setViewedRunIds((current) => ({
        ...current,
        [viewedSessionId]: run.id,
      }))
    })
    return () => window.cancelAnimationFrame(frame)
  }, [detail.data, sessionId, traceOpen, view, viewedRunIds])

  const sessionPermissionMode =
    permissionOverride && permissionOverride.sessionId === sessionId
      ? permissionOverride.mode
      : (detail.data?.session.permission_mode ?? 'request_approval')

  const openChatWithFreshDetail = (
    id: string,
    onReady: (sessionDetail?: StudioSessionDetail) => void
  ) => {
    const generation = ++chatOpenGeneration.current
    // The chat runtime decides whether to resume from its first history load.
    // Mount it only after the selected session's current run has been fetched.
    void queryClient
      .fetchQuery({
        queryKey: ['studio', 'session', id],
        queryFn: () => getStudioSession(id),
        staleTime: 0,
      })
      .catch(() => undefined)
      .then((sessionDetail) => {
        if (chatOpenGeneration.current === generation) onReady(sessionDetail)
      })
  }

  useEffect(() => {
    const reconcile = () => {
      if (document.visibilityState !== 'visible' || !navigator.onLine) return
      void queryClient.invalidateQueries({ queryKey: ['studio', 'sessions'] })
      if (sessionId && view === 'chat') {
        void queryClient.invalidateQueries({
          queryKey: ['studio', 'session', sessionId],
        })
      }
    }
    const onVisibilityChange = () => {
      if (document.visibilityState === 'visible') reconcile()
    }
    const onOnline = () => reconcile()
    document.addEventListener('visibilitychange', onVisibilityChange)
    window.addEventListener('online', onOnline)
    return () => {
      document.removeEventListener('visibilitychange', onVisibilityChange)
      window.removeEventListener('online', onOnline)
    }
  }, [queryClient, sessionId, view])

  const createTextAsset = useMutation({
    mutationFn: (input: { name: string; content: string; requestId: string }) =>
      createStudioTextAsset({ sessionId: sessionId!, ...input }),
    onSuccess: () => {
      void queryClient.invalidateQueries({
        queryKey: ['studio', 'session', sessionId],
      })
      void queryClient.invalidateQueries({ queryKey: ['studio', 'library'] })
    },
  })
  const updateTextAsset = useMutation({
    mutationFn: (input: { assetId: string; content: string; requestId: string }) =>
      updateStudioTextAsset(input.assetId, input.content, input.requestId),
    onSuccess: () => {
      void queryClient.invalidateQueries({
        queryKey: ['studio', 'session', sessionId],
      })
    },
  })
  const referenceAsset = useMutation({
    mutationFn: ({ assetId, assetVersionId, requestId, sourceProjectAssetId }: SelectedAsset & { requestId: string; sourceProjectAssetId?: string }) => {
      if (!sessionId) throw new Error('请先创建或选择一个对话')
      return referenceStudioAsset(
        sessionId,
        assetId,
        assetVersionId,
        requestId,
        sourceProjectAssetId
      )
    },
    onSuccess: () => {
      void queryClient.invalidateQueries({
        queryKey: ['studio', 'session', sessionId],
      })
      void queryClient.invalidateQueries({ queryKey: ['studio', 'library'] })
    },
  })
  const currentSessionDetail =
    detail.data?.session.id === sessionId ? detail.data : undefined
  const projectAssets = useQuery({
    queryKey: ['studio', 'workspace-project-assets', currentSessionDetail?.session.project_id, currentSessionDetail?.assets.map(asset => `${asset.id}:${asset.current_version}`).join(',')],
    enabled: view === 'chat' && Boolean(currentSessionDetail),
    queryFn: async () => {
      const assets: StudioAsset[] = []
      let cursor: string | undefined
      do {
        const page = await listStudioLibraryAssets({ projectId: currentSessionDetail!.session.project_id ?? '', limit: 100, cursor })
        assets.push(...page.items.map(item => item.asset))
        cursor = page.next_cursor
      } while (cursor)
      return [...new Map(assets.map(asset => [asset.id, asset])).values()]
    },
  })
  const activeWorkflowCount = currentSessionDetail?.workflow_executions?.filter(isActiveWorkflowExecution).length ?? 0
  const uploadAsset = useMutation({
    mutationFn: (file: File) => uploadStudioAsset(file, sessionId),
    onSuccess: () => {
      void queryClient.invalidateQueries({
        queryKey: ['studio', 'session', sessionId],
      })
      void queryClient.invalidateQueries({ queryKey: ['studio', 'library'] })
    },
  })

  const sidebarProps = {
    activeSessionId: sessionId,
    viewedRunIds,
    view,
    onNewSession: startNewSession,
    onSelectSession: (id: string) => {
      openChatWithFreshDetail(id, () => {
        void navigate({
          to: '/studio/sessions/$sessionId',
          params: { sessionId: id },
          search: videoWork.data && (videoWork.data.main_session_id === id || videoWork.data.chats.some(chat => chat.session_id === id)) ? { video: videoId, panel } : {},
        })
      })
    },
    onViewChange: (nextView: StudioView) => {
      if (nextView === 'library') {
        ++chatOpenGeneration.current
        setLibraryEntryProjectId(undefined)
        setLibraryEntrySessionId(undefined)
        setLibraryEntrySessionTitle(undefined)
        void navigate({ to: '/studio/library', search: {} })
      } else if (nextView === 'market') {
        ++chatOpenGeneration.current
        void navigate({ to: '/studio/workflows', search: marketReturnSearch.current })
      } else if (nextView === 'skill-market' || nextView === 'expert-market') {
        ++chatOpenGeneration.current
        void navigate({ to: nextView === 'skill-market' ? '/studio/skills' : '/studio/experts', search: {} })
      } else if (nextView === 'settings') {
        ++chatOpenGeneration.current
        void navigate({
          to: '/studio/settings/$section',
          params: { section: 'general' },
          search: {},
        })
      } else {
        startNewSession()
      }
    },
  }
  const mobileSidebarProps = {
    ...sidebarProps,
    onNewSession: (projectId?: string) => {
      setMobileMenuOpen(false)
      sidebarProps.onNewSession(projectId)
    },
    onSelectSession: (id: string) => {
      setMobileMenuOpen(false)
      sidebarProps.onSelectSession(id)
    },
    onViewChange: (nextView: StudioView) => {
      setMobileMenuOpen(false)
      sidebarProps.onViewChange(nextView)
    },
  }

  const renderWorkbenchContent = (kind: 'video' | 'assets' | 'tasks', editorActive = false) => {
    if (!sessionId || (detail.isError && !currentSessionDetail)) return null
    if (!currentSessionDetail) {
      return (
        <div aria-busy='true' className='h-full min-h-0 p-4'>
          <Skeleton className='h-full w-full' />
        </div>
      )
    }
    if (currentSessionDetail.session.project_id && projects.isPending) return <Skeleton className='h-full w-full' />
    if (currentSessionDetail.session.project_id && projects.isError && !currentProject) return <Empty><EmptyHeader><EmptyTitle>读取项目失败</EmptyTitle></EmptyHeader><EmptyContent><Button variant='outline' size='sm' onClick={() => void projects.refetch()}>重新读取</Button></EmptyContent></Empty>
    if (kind === 'video' && videoId && videoWork.isError) return <Empty><EmptyHeader><EmptyTitle>{videoWork.error.message}</EmptyTitle></EmptyHeader></Empty>
    if (kind === 'video' && videoId && videoWork.isPending) return <Skeleton className='h-full w-full' />
    const editorDetail = videoSessionId === sessionId ? currentSessionDetail : videoSession.data
    if (kind === 'video' && !editorDetail) return <Skeleton className='h-full w-full' />
    if (kind === 'video') return <StudioVideoEditor
      key={videoSessionId} active={editorActive} sessionId={videoSessionId!} activeChatId={sessionId} videoId={videoId} projectId={videoWork.data?.project_id ?? editorDetail!.session.project_id} assets={editorDetail!.assets}
      onOpenVideoChat={openVideoChat} referenceLocation={search.shot && search.reference ? { shot: search.shot, reference: search.reference } : undefined}
      skills={editorDetail!.session.expert_id ? editorDetail!.expert_capabilities.skills : skills.data ?? []} canInstallScriptSkill={!editorDetail!.session.expert_id}
      scriptRequests={<VideoScriptRequests requests={scriptRequests.filter(request => request.sessionId === videoSessionId)} onLocate={(messageId, clientMessageId) => {
        setWorkbenchSheetOpen(false)
        if (messageId) setLocateMessage({ id: messageId, alternateId: clientMessageId, request: Date.now() })
      }} />}
      onScriptRequest={sendVideoEditorRequest}
      onReference={asset => { const version = asset.versions.find(version => version.version === asset.current_version); if (version) setAssetSelection(current => ({ sessionId, assets: [...(current?.sessionId === sessionId ? current.assets : []), { assetId: asset.id, assetVersionId: version.id }] })); void queryClient.invalidateQueries({ queryKey: ['studio', 'session', sessionId] }) }}
    />
    if (kind === 'tasks') return <StudioSessionTasks executions={currentSessionDetail.workflow_executions ?? []} />
    if (projectAssets.isPending) return <Skeleton className='h-full w-full' />
    if (projectAssets.isError) return <Empty><EmptyHeader><EmptyTitle>{projectAssets.error.message}</EmptyTitle></EmptyHeader></Empty>
    return (
      <StudioAssets
        assets={projectAssets.data ?? []}
        layout={assetLayout}
        messages={currentSessionDetail.messages}
        onLocateMessage={(id) => {
          setLocateMessage((current) => ({
            id,
            request: (current?.request ?? 0) + 1,
          }))
          setWorkbenchSheetOpen(false)
        }}
        onUpdateTextAsset={(input) => updateTextAsset.mutateAsync(input)}
      />
    )
  }

  const renderAssetActions = () => currentSessionDetail ? <StudioAssetActions
    layout={assetLayout} onLayoutChange={setAssetLayout}
    onCreateTextAsset={(input) => createTextAsset.mutateAsync(input)}
    onUploadAsset={(file) => uploadAsset.mutate(file)}
    onOpenLibrary={() => {
      setLibraryEntryProjectId(currentSessionDetail.session.project_id ?? '')
      setLibraryEntrySessionId(currentSessionDetail.session.id)
      setLibraryEntrySessionTitle(currentSessionDetail.session.title)
      void navigate({ to: '/studio/library', search: {} })
    }} uploading={uploadAsset.isPending}
  /> : null

  const workbenchTabs = () => (
    <Tabs value={panel} onValueChange={(value) => void navigate({ to: '/studio/sessions/$sessionId', params: { sessionId: sessionId! }, search: { video: videoId, panel: value === 'video' || value === 'tasks' ? value : 'assets' } })} className='relative min-h-0 flex-1 gap-0'>
      <StudioWorkbenchBackground />
      <TabsList className='absolute top-3 left-4 z-10'>
        <TabsTrigger value='video'>视频编辑器</TabsTrigger>
        <TabsTrigger value='assets'>项目资产</TabsTrigger>
        <TabsTrigger value='tasks'>会话任务</TabsTrigger>
      </TabsList>
      {panel === 'assets' ? <div className='absolute top-14 right-4 z-10 sm:top-3'>{renderAssetActions()}</div> : null}
      <TabsContent value='video' forceMount className='relative z-0 m-0 min-h-0 pt-14 data-[state=inactive]:hidden'>{renderWorkbenchContent('video', workbenchSheetOpen && panel === 'video')}</TabsContent>
      <TabsContent value='assets' className='relative z-0 m-0 min-h-0 pt-28 sm:pt-16'>
        {renderWorkbenchContent('assets')}
      </TabsContent>
      <TabsContent value='tasks' className='relative z-0 m-0 min-h-0 pt-16'>{renderWorkbenchContent('tasks')}</TabsContent>
    </Tabs>
  )

  return (
    <div className='flex h-svh min-h-0 w-full overflow-hidden'>
      <div data-slot='studio-desktop-sidebar' className='hidden lg:flex'>
        <StudioSidebar {...sidebarProps} />
      </div>
      <div data-slot='studio-mobile-menu' className='fixed top-6 left-6 z-30 sm:top-7 sm:left-7 lg:hidden'>
        <Sheet open={mobileMenuOpen} onOpenChange={setMobileMenuOpen}>
          <IconButtonTooltip label='打开 Studio 菜单'>
            <SheetTrigger asChild>
              <Button
                variant='outline'
                size='icon'
                className='size-11'
                aria-label='打开 Studio 菜单'
              >
                <Menu />
              </Button>
            </SheetTrigger>
          </IconButtonTooltip>
          <SheetContent side='left' className='w-64 bg-muted/30 p-0'>
            <SheetTitle className='sr-only'>Studio 菜单</SheetTitle>
            <SheetDescription className='sr-only'>
              切换对话、资产库、工作流和设置。
            </SheetDescription>
            <StudioSidebar {...mobileSidebarProps} />
          </SheetContent>
        </Sheet>
      </div>

      <StudioPage>
      {view === 'library' ? (
        <StudioLibrary
          onOpenVideoWork={async (work, location) => {
            if (!work.main_session_id) throw new Error('视频主对话尚未创建')
            await queryClient.invalidateQueries({ queryKey: ['studio', 'sessions'] })
            await openVideoChat(work, work.main_session_id, location)
          }}
          initialProjectId={libraryEntryProjectId}
          initialSessionId={libraryEntrySessionId}
          initialSessionTitle={libraryEntrySessionTitle}
          onOpenSession={(id, sourceRunId) => {
            openChatWithFreshDetail(id, (sessionDetail) => {
              const sourceMessageId = sourceRunId
                ? sessionDetail?.messages.find(
                    (message) =>
                      message.role === 'user' && message.run_id === sourceRunId
                  )?.id
                : undefined
              if (sourceMessageId) {
                setLocateMessage((current) => ({
                  id: sourceMessageId,
                  request: (current?.request ?? 0) + 1,
                }))
              }
              void navigate({
                to: '/studio/sessions/$sessionId',
                params: { sessionId: id },
                search: {},
              })
            })
          }}
        />
      ) : null}
      {view === 'market' ? <StudioMarket workflowId={marketDetailMatch ? marketDetailMatch.workflowId : undefined} listPositionRef={marketListPosition}
        editorState={workflowEditorState} onEditorStateChange={setWorkflowEditorState} onSaved={() => setWorkflowEditorState(undefined)}
        onManageNodes={(mode) => { setCreateNode(mode === 'create'); void navigate({ to: '/studio/settings/$section', params: { section: 'compute-nodes' } }) }} /> : null}
      {view === 'skill-market' ? <AssetCatalog scope='studio' kind='skills' /> : null}
      {view === 'expert-market' ? expertEditorMatch ? <ExpertEditor scope='studio' id={expertEditorMatch.expertId === 'new' ? undefined : expertEditorMatch.expertId} onClose={() => void navigate({ to: '/studio/experts' })} /> : <AssetCatalog scope='studio' kind='experts' /> : null}
      {view === 'settings' && settingsMatch ? (
        <StudioSettings
          section={settingsMatch.section as SettingSection}
          onSectionChange={section => void navigate({ to: '/studio/settings/$section', params: { section } })}
          createNode={createNode}
          onCreateNodeHandled={() => setCreateNode(false)}
          onReturnWorkflow={workflowEditorState ? () => void navigate({ to: '/studio/workflows/$workflowId', params: { workflowId: workflowEditorState.workflowId ?? 'new' }, search: { ...marketReturnSearch.current, edit: true, section: workflowEditorState.kind !== 'online' ? workflowEditorState.section : undefined } }) : undefined}
          onSessionsCleared={() => {
            ++chatOpenGeneration.current
            setViewedRunIds({})
            setLocateMessage(undefined)
            setModelSelection(undefined)
            setSkillSelection(undefined)
            setAssetSelection(undefined)
            setPermissionOverride(undefined)
          }}
        />
      ) : null}
      {view === 'chat' && !sessionId && search.workflow && marketDraft.isPending ? <Skeleton className='m-4 h-80 flex-1' /> : null}
      {view === 'chat' && !sessionId && marketDraft.isError ? <p role='alert' className='p-5 text-sm text-destructive'>{marketDraft.error.message}</p> : null}
      {view === 'chat' && !sessionId && (!search.workflow || marketDraft.isSuccess) ? (
        <StudioNewSession
          key={`${newSessionKey}:${search.project ?? ''}:${search.workflow ?? ''}:${search.example ?? ''}`}
          models={models.data ?? []}
          skills={skills.data ?? []}
          initialProjectId={search.project}
          initialWorkflow={marketDraft.data ? { id: marketDraft.data.id, name: marketDraft.data.name, prompt: typeof search.example === 'number' ? marketDraft.data.examples[search.example]?.prompt : undefined } : undefined}
        />
      ) : null}
      {view === 'chat' && sessionId ? (
          <section
            data-slot='studio-workbench'
            className='flex min-h-0 min-w-0 flex-1 overflow-hidden'
          >
            <ResizablePanelGroup elementRef={workbenchGroup} orientation='horizontal' className='min-h-0 flex-1' onLayoutChanged={(layout, { isUserInteraction }) => {
              if (!isUserInteraction || !workbenchVisible) return
              if (layout['studio-workbench-pane'] === 0) setRightOpen(false)
              else if (chatPanel.current && workbenchPanel.current) {
                const availableWidth = chatPanel.current.getSize().inPixels + workbenchPanel.current.getSize().inPixels
                preferredChatWidth.current = availableWidth * layout['studio-chat-pane'] / 100
              }
            }}>
            <ResizablePanel id='studio-chat-pane' panelRef={chatPanel} defaultSize={380} minSize={380} groupResizeBehavior='preserve-pixel-size' className='flex min-w-0 flex-col'>
              <header data-tauri-drag-region='deep' className='studio-page-header justify-between'>
                <div className='min-w-0 flex-1'>
                  <div className='flex min-w-0 items-center gap-2'>
                    <h1 className='min-w-0 truncate text-sm font-semibold'>{detail.data?.session.title ?? '新对话'}</h1>
                    {material && sessionId ? <VideoMaterialContext material={material} context={materialContext.data} sessionId={sessionId} assets={detail.data?.assets ?? []} error={materialContext.error?.message} loading={materialContext.isFetching} onVersion={id => setMaterialVersions(current => ({ ...current, [sessionId]: id }))} /> : null}
                    {videoWork.data?.main_session_id && videoWork.data.main_session_id !== sessionId ? <Button size='sm' variant='ghost' onClick={() => void openVideoChat(videoWork.data!, videoWork.data!.main_session_id!)}>返回视频对话</Button> : null}
                    {activeWorkflowCount > 0 ? <Badge asChild variant='secondary' className='h-6 cursor-pointer text-sm hover:bg-secondary/80'>
                      <button type='button' aria-label={`工作流运行中，查看会话任务；当前 ${activeWorkflowCount} 个工作流`} onClick={() => {
                        setRightOpen(true)
                        setWorkbenchSheetOpen(window.matchMedia('(max-width: 1279px)').matches)
                        void navigate({ to: '/studio/sessions/$sessionId', params: { sessionId: sessionId! }, search: { panel: 'tasks' } })
                      }}><Workflow className='motion-safe:animate-pulse' />工作流运行中</button>
                    </Badge> : null}
                  </div>
                  <p className='text-xs text-muted-foreground'>
                    自动保存 · 后台运行
                  </p>
                </div>
                <div className='flex items-center gap-1'>
                  <Button
                    variant={traceOpen ? 'secondary' : 'ghost'}
                    size='sm'
                    disabled={!sessionId}
                    onClick={() => {
                      if (!sessionId) return
                      if (traceOpen) {
                        openChatWithFreshDetail(sessionId, () => {
                          void navigate({
                            to: '/studio/sessions/$sessionId',
                            params: { sessionId },
                            search: {},
                          })
                        })
                      } else {
                        ++chatOpenGeneration.current
                        void navigate({
                          to: '/studio/sessions/$sessionId/trace',
                          params: { sessionId },
                          search: {},
                        })
                      }
                    }}
                    aria-pressed={traceOpen}
                  >
                    <ListTree />
                    {traceOpen ? '对话' : '轨迹'}
                  </Button>
                  {!traceOpen && !workbenchDocked ? (
                    <Sheet
                      open={workbenchSheetOpen}
                      onOpenChange={setWorkbenchSheetOpen}
                    >
                      <IconButtonTooltip label='打开创作工作台'>
                        <SheetTrigger asChild>
                          <Button
                            variant='ghost'
                            size='icon'
                            className='size-11 xl:hidden'
                            aria-label='打开创作工作台'
                          >
                            <PanelRightOpen />
                          </Button>
                        </SheetTrigger>
                      </IconButtonTooltip>
                      <SheetContent
                        side='right'
                        className='w-full max-w-none gap-0 bg-card p-0 sm:w-[95vw] sm:max-w-none'
                      >
                        <SheetTitle className='px-4 pt-4 text-sm'>
                          创作工作台
                        </SheetTitle>
                        <SheetDescription className='sr-only'>
                          查看会话资产和会话任务。
                        </SheetDescription>
                        {workbenchTabs()}
                      </SheetContent>
                    </Sheet>
                  ) : null}
                  {!workbenchOpen && !traceOpen ? (
                    <IconButtonTooltip label='展开右侧面板'>
                      <Button
                        variant='ghost'
                        size='icon'
                        className='hidden xl:inline-flex'
                        onClick={() => setRightOpen(true)}
                        aria-label='展开右侧面板'
                      >
                        <PanelRightOpen />
                      </Button>
                    </IconButtonTooltip>
                  ) : null}
                </div>
              </header>
              {detail.error instanceof ApiError &&
              detail.error.status === 404 ? (
                <Empty>
                  <EmptyHeader>
                    <EmptyTitle>对话不存在或已清空</EmptyTitle>
                  </EmptyHeader>
                </Empty>
              ) : traceOpen && sessionId ? (
                <StudioTraceTabs key={sessionId} sessionId={sessionId} />
              ) : detail.isLoading ? (
                <ChatSkeleton />
              ) : detail.isError || !detail.data ? (
                <Empty>
                  <EmptyHeader>
                    <EmptyTitle>对话读取失败</EmptyTitle>
                    <EmptyDescription>检查连接后重试。</EmptyDescription>
                  </EmptyHeader>
                  <EmptyContent>
                    <Button
                      variant='outline'
                      className='min-h-11'
                      disabled={detail.isFetching}
                      onClick={() => void detail.refetch()}
                    >
                      重试读取
                    </Button>
                  </EmptyContent>
                </Empty>
              ) : (
                <StudioChat
                  key={sessionId}
                  onEditorSenderChange={send => {
                    editorSender.current = send ? { sessionId, send } : undefined
                    const pending = pendingEditorRequest.current
                    if (send && pending?.sessionId === sessionId) {
                      pendingEditorRequest.current = undefined
                      try { send(pending.request); pending.resolve() } catch (error) { pending.reject(error) }
                    }
                  }}
                  onEditorRequestChange={request => setScriptRequests(current => updateVideoScriptRequests(current, request))}
                  sessionId={sessionId}
                  materialContext={material && materialContext.data ? { video_id: materialContext.data.video_id, material_id: materialContext.data.material_id, asset_id: materialContext.data.asset_id, asset_version_id: materialContext.data.asset_version?.id ?? null } : undefined}
                  materialContextPending={Boolean((videoId && (videoWork.isPending || videoWork.isError)) || (materialChat && (!materialContext.data || materialContext.isFetching || materialContext.isError)))}
                  projectId={detail.data.session.project_id ?? ''}
                  messages={detail.data.messages}
                  transcript={detail.data.transcript}
                  latestRun={detail.data.session.latest_run}
                  runProgress={detail.data.run_progress}
                  pendingApprovals={detail.data.pending_approvals}
                  pendingClarifications={detail.data.pending_clarifications}
                  expertId={detail.data.session.expert_id}
                  expertName={detail.data.session.expert_name}
                  expertVersion={detail.data.session.expert_version}
                  expertWorkflows={detail.data.expert_capabilities.workflows}
                  expertTasks={detail.data.expert_tasks}
                  expertCards={detail.data.expert_cards}
                  models={models.data ?? []}
                  modelConfigId={
                    modelSelection?.sessionId === sessionId
                      ? modelSelection.modelConfigId
                      : detail.data.session.model_config_id
                  }
                  permissionMode={sessionPermissionMode}
                  skills={detail.data.session.expert_id ? detail.data.expert_capabilities.skills : skills.data ?? []}
                  assets={detail.data.assets}
                  locateMessage={locateMessage}
                  selectedSkillIds={
                    skillSelection?.sessionId === sessionId
                      ? skillSelection.skillIds
                      : []
                  }
                  selectedAssets={
                    assetSelection?.sessionId === sessionId
                      ? assetSelection.assets
                      : []
                  }
                  onModelChange={(modelConfigId) => {
                    setModelSelection({ sessionId, modelConfigId })
                  }}
                  onPermissionChange={(mode) => {
                    if (sessionId) setPermissionOverride({ sessionId, mode })
                  }}
                  onSkillChange={(skillIds) => {
                    setSkillSelection({ sessionId, skillIds })
                  }}
                  onAssetChange={(assets) => {
                    setAssetSelection({ sessionId, assets })
                  }}
                  onReferenceAsset={(selection) =>
                    referenceAsset.mutateAsync(selection)
                  }
                  onUploadAsset={(file) => uploadAsset.mutateAsync(file)}
                  onRunFinished={() => {
                    void queryClient.invalidateQueries({
                      queryKey: ['studio', 'session', sessionId],
                    })
                    void queryClient.invalidateQueries({
                      queryKey: ['studio', 'sessions'],
                    })
                    void queryClient.invalidateQueries({
                      queryKey: ['studio', 'skills'],
                    })
                  }}
                  onRuntimeStateChange={() => {
                    void queryClient.invalidateQueries({ queryKey: ['studio', 'session', sessionId] })
                    void queryClient.invalidateQueries({
                      queryKey: ['studio', 'sessions'],
                    })
                  }}
                />
              )}
            </ResizablePanel>
            <ResizableHandle aria-label='调整对话与工作台宽度' disabled={!workbenchVisible} disableDoubleClick onDoubleClick={() => { preferredChatWidth.current = 380; chatPanel.current?.resize(380) }} className={workbenchVisible ? 'after:w-3' : 'hidden'} />
            <ResizablePanel id='studio-workbench-pane' panelRef={workbenchPanel} minSize={400} collapsible className='min-w-0'>
            <aside
              data-slot='studio-workbench-track'
              aria-hidden={!workbenchVisible}
              inert={!workbenchVisible}
              className='flex h-full min-w-0 flex-col overflow-hidden'
            >
              <div
                data-slot='studio-workbench-panel'
                className='flex h-full w-full min-w-80 flex-col bg-muted/20'
              >
                <Tabs
                  value={panel}
                  onValueChange={(nextPanel) =>
                    void navigate({
                      to: '/studio/sessions/$sessionId',
                      params: { sessionId: sessionId! },
                      search: {
                        panel: nextPanel === 'video' || nextPanel === 'assets' || nextPanel === 'tasks' ? nextPanel : undefined,
                      },
                    })
                  }
                  className='relative min-h-0 min-w-80 flex-1 gap-0'
                >
                  <StudioWorkbenchBackground />
                  <TabsList className='absolute top-3 left-4 z-10'>
                    <TabsTrigger value='video' disabled={!sessionId}>视频编辑器</TabsTrigger>
                    <TabsTrigger value='assets' disabled={!sessionId}>项目资产</TabsTrigger>
                    <TabsTrigger value='tasks' disabled={!sessionId}>会话任务</TabsTrigger>
                  </TabsList>
                  <div className='absolute top-3 right-5 z-10 flex items-center gap-1'>
                  <IconButtonTooltip label='收起右侧面板'>
                    <Button
                      variant='ghost'
                      size='icon'
                      onClick={() => setRightOpen(false)}
                      aria-label='收起右侧面板'
                    >
                      <PanelRightClose />
                    </Button>
                  </IconButtonTooltip>
                  {panel === 'assets' ? renderAssetActions() : null}
                  </div>
                  <TabsContent value='video' forceMount className='relative z-0 m-0 min-h-0 pt-14 data-[state=inactive]:hidden'>{workbenchDocked ? renderWorkbenchContent('video', workbenchVisible && panel === 'video') : null}</TabsContent>
                  <TabsContent
                    value='assets'
                    className='relative z-0 m-0 min-h-0 pt-16'
                  >
                    {renderWorkbenchContent('assets')}
                  </TabsContent>
                  <TabsContent value='tasks' className='relative z-0 m-0 min-h-0 pt-16'>{renderWorkbenchContent('tasks')}</TabsContent>
                </Tabs>
              </div>
            </aside>
            </ResizablePanel>
            </ResizablePanelGroup>
          </section>
      ) : null}
      </StudioPage>
    </div>
  )
}

function ChatSkeleton() {
  return (
    <div className='mx-auto flex w-full max-w-3xl flex-1 flex-col gap-6 p-8'>
      <Skeleton className='mt-10 h-20 w-3/4 self-end rounded-2xl' />
      <Skeleton className='h-28 w-4/5 rounded-2xl' />
      <div className='flex-1' />
      <Skeleton className='h-32 w-full rounded-2xl' />
    </div>
  )
}
