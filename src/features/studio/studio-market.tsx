import { useLayoutEffect, useRef, useState } from 'react'
import { useMutation, useQuery, useQueryClient } from '@tanstack/react-query'
import { Link, useNavigate, useSearch } from '@tanstack/react-router'
import { ArrowLeft, Plus, PenLine, Settings2, Tags, FolderOpen, ZoomIn, Play, MoreHorizontal, Trash2, Workflow as WorkflowIcon } from 'lucide-react'
import { useTranslation } from 'react-i18next'
import { deleteWorkflow, updateWorkflow, getWorkflow, listWorkflows, listWorkflowExecutions, type Workflow } from '@/lib/api/workflows'
import { Button } from '@/components/ui/button'
import { Input } from '@/components/ui/input'
import { Badge } from '@/components/ui/badge'
import { Skeleton } from '@/components/ui/skeleton'
import { Empty, EmptyHeader, EmptyMedia, EmptyTitle } from '@/components/ui/empty'
import { Tabs, TabsContent, TabsList, TabsTrigger } from '@/components/ui/tabs'
import { FilterSegment } from '@/components/filters/filter-segment'
import { ConfirmDialog } from '@/components/confirm-dialog'
import { DropdownMenu, DropdownMenuContent, DropdownMenuGroup, DropdownMenuItem, DropdownMenuTrigger } from '@/components/ui/dropdown-menu'
import { WorkflowCover, WorkflowIntroduction } from '@/features/cases/workflow-introduction'
import { WorkflowEditor, type WorkflowEditorState } from '@/features/cases/workflow-editor'
import { WorkflowVersions } from '@/features/cases/workflow-versions'
import { WorkflowConfigView } from '@/features/cases/sections/workflow-config-view'
import { WorkflowExecutionPanel, WorkflowExecutionResult, executionStatusLabels, executionFinished } from '@/features/cases/workflow-execution'
import { formatDateTime } from '@/lib/format'
import { cn } from '@/lib/utils'
import { getComputeNode } from '@/lib/api/compute-nodes'
import { resolveMediaKey } from '@/lib/api/media'
import { useMediaObjectUrl } from '@/features/cases/lib/use-media-object-url'
import { MediaLightbox } from '@/features/cases/components/media-lightbox'
import { LongText } from '@/components/long-text'
import { MetaChip } from '@/components/meta-chip'
import { SectionHead } from '@/components/section-head'
import { StatusDot } from '@/components/status-dot'
import { kit } from '@/features/edges/kit-classes'

export function StudioMarket({ workflowId, listPositionRef, editorState, onEditorStateChange, onManageNodes, onSaved }: {
  workflowId?: string
  listPositionRef: { current: number }
  editorState?: WorkflowEditorState
  onEditorStateChange: (state: WorkflowEditorState) => void
  onManageNodes: (mode: 'create' | 'manage') => void
  onSaved: () => void
}) {
  const { t } = useTranslation()
  const navigate = useNavigate()
  const search = useSearch({ strict: false })
  const scrollRef = useRef<HTMLDivElement>(null)
  const creating = workflowId === 'new'
  const editing = creating || Boolean(search.edit)
  const editingSection = search.section === 'info' || search.section === 'workflow' ? search.section : undefined
  const market = useQuery({ queryKey: ['studio', 'workflows'], queryFn: () => listWorkflows() })
  const detail = useQuery({ queryKey: ['studio', 'workflow', workflowId], queryFn: () => getWorkflow(workflowId!), enabled: Boolean(workflowId) && !creating })
  useLayoutEffect(() => {
    if (scrollRef.current) scrollRef.current.scrollTop = workflowId ? 0 : listPositionRef.current
  }, [workflowId, market.data, listPositionRef])
  const categories = [...new Set(market.data?.map((workflow) => workflow.category).filter(Boolean) ?? [])].sort()
  const workflows = market.data?.filter((workflow) => {
    const matchesCategory = !search.category || workflow.category === search.category
    const text = `${workflow.name} ${workflow.draft.description ?? ''} ${workflow.tags.join(' ')}`.toLocaleLowerCase()
    return matchesCategory && (!search.type || workflow.type === search.type) && text.includes((search.q ?? '').toLocaleLowerCase())
  }) ?? []
  const returnSearch = { q: search.q, category: search.category, type: search.type }
  const saved = (workflow: Workflow) => {
    onSaved()
    void navigate({ to: '/studio/workflows/$workflowId', params: { workflowId: workflow.id }, search: returnSearch })
  }
  return (
      <section className='flex min-h-0 min-w-0 flex-1 flex-col overflow-hidden'>
        <header data-tauri-drag-region='deep' className='studio-page-header studio-continuous-header justify-between'>
          {workflowId ? <div className='flex min-w-0 flex-1 items-center gap-3'><Button asChild size='icon-sm' variant='ghost' aria-label={t('common.backToList')}><Link to='/studio/workflows' search={returnSearch}><ArrowLeft /></Link></Button><h1 className='truncate text-sm font-semibold'>{creating ? '创建工作流' : detail.data?.name ?? '工作流'}</h1></div> : <h1 className='text-sm font-semibold'>工作流</h1>}
          {!workflowId ? <Button asChild size='sm'><Link to='/studio/workflows/$workflowId' params={{ workflowId: 'new' }} search={returnSearch}><Plus />创建工作流</Link></Button> : null}
        </header>
        <div ref={scrollRef} className={cn('relative flex min-h-0 flex-1 flex-col', workflowId && !editing && !creating ? '' : 'px-5 pb-5', editing && workflowId ? 'overflow-hidden' : 'overflow-y-auto')} onScroll={(event) => { if (!workflowId) listPositionRef.current = event.currentTarget.scrollTop }}>
          {workflowId ? creating || (detail.data && editing) ? <WorkflowEditor key={`${workflowId}:${editingSection ?? ''}`} section={editingSection} initial={creating ? undefined : detail.data} state={editorState} onStateChange={onEditorStateChange} onManageNodes={onManageNodes} onSaved={saved} onDiscard={onSaved} onCancel={() => { if (creating) void navigate({ to: '/studio/workflows', search: returnSearch }); else void navigate({ to: '/studio/workflows/$workflowId', params: { workflowId }, search: returnSearch }) }} />
            : detail.isPending ? <Skeleton className='h-80 w-full' /> : detail.isError ? <div className='flex flex-col items-start gap-3'><p role='alert' className='text-sm text-destructive'>{detail.error.message}</p><Button size='sm' variant='outline' onClick={() => void detail.refetch()}>{t('common.retry')}</Button></div>
              : <WorkflowDetail key={detail.data.id} workflow={detail.data} /> : (
            <div className='flex flex-1 flex-col gap-5'>
              <div className='flex flex-wrap items-center justify-between gap-4'>
                <Input className='w-full sm:max-w-xs' aria-label={t('workflowMarket.search')} placeholder={t('workflowMarket.search')} value={search.q ?? ''} onChange={(event) => void navigate({ to: '/studio/workflows', search: { ...returnSearch, q: event.target.value }, replace: true })} />
                <div className='flex flex-wrap gap-2'>
                  <FilterSegment value={search.type ?? ''} options={[{ value: '', label: '全部类型' }, { value: 'comfyui', label: '自建 ComfyUI' }, { value: 'online', label: '在线模型' }]} aria-label='工作流类型' onValueChange={(type) => void navigate({ to: '/studio/workflows', search: { ...returnSearch, type: type === 'comfyui' || type === 'online' ? type : undefined }, replace: true })} />
                  <FilterSegment value={search.category ?? ''} options={[{ value: '', label: t('workflowMarket.allCategories') }, ...categories.map((category) => ({ value: category, label: category === 'image' ? '图片' : category === 'video' ? '视频' : category }))]} aria-label={t('workflowMarket.allCategories')} onValueChange={(category) => void navigate({ to: '/studio/workflows', search: { ...returnSearch, category: category || undefined }, replace: true })} />
                </div>
              </div>
              {market.isPending ? <div className='grid gap-4 min-[920px]:grid-cols-2 xl:grid-cols-3'>{[0, 1, 2].map((key) => <Skeleton key={key} className='h-64' />)}</div> : market.isError ? <div className='flex flex-col items-start gap-3'><p role='alert' className='text-sm text-destructive'>{market.error.message}</p><Button size='sm' variant='outline' onClick={() => void market.refetch()}>{t('common.retry')}</Button></div> : workflows.length ? <div className='grid gap-4 min-[920px]:grid-cols-2 xl:grid-cols-3'>
                {workflows.map((workflow) => <Link key={workflow.id} to='/studio/workflows/$workflowId' params={{ workflowId: workflow.id }} search={returnSearch} className='flex min-w-0 flex-col gap-3 rounded-lg p-2 outline-none hover:bg-muted focus-visible:ring-2 focus-visible:ring-ring'>
                  <WorkflowCover preview={workflow.draft.preview} name={workflow.name} compact />
                  <h2 className='text-sm font-semibold'>{workflow.name}</h2>
                  <p className='line-clamp-2 text-sm text-muted-foreground'>{workflow.draft.description}</p>
                  <div className='flex flex-wrap gap-2'><Badge variant='secondary'>{workflow.type === 'comfyui' ? '自建 ComfyUI' : '在线模型'}</Badge><Badge variant='outline'>{workflow.category === 'image' ? '图片' : workflow.category === 'video' ? '视频' : workflow.category}</Badge></div>
                </Link>)}
              </div> : <Empty><EmptyHeader><EmptyMedia variant='icon'><WorkflowIcon /></EmptyMedia><EmptyTitle>{market.data?.length ? '没有匹配的工作流' : '暂无工作流'}</EmptyTitle></EmptyHeader></Empty>}
            </div>
          )}
        </div>
      </section>
  )
}

function WorkflowDetail({ workflow }: { workflow: Workflow }) {
  const { t } = useTranslation()
  const client = useQueryClient()
  const navigate = useNavigate()
  const [deleting, setDeleting] = useState(false)
  const [running, setRunning] = useState(false)
  const [selectedExecution, setSelectedExecution] = useState<string>()
  const [previewOpen, setPreviewOpen] = useState(false)
  const previewUrl = useMediaObjectUrl(workflow.draft.preview)
  const previewKey = resolveMediaKey(workflow.draft.preview)
  const previewIsVideo = /\.(mp4|webm)$/i.test(previewKey ?? '')
  const nodeId = workflow.draft.type === 'comfyui' ? workflow.draft.execution.node_id : undefined
  const node = useQuery({ queryKey: ['studio', 'compute-node', nodeId], queryFn: () => getComputeNode(nodeId!), enabled: Boolean(nodeId), refetchInterval: 5000 })
  const executions = useQuery({ queryKey: ['studio', 'executions', workflow.id], queryFn: () => listWorkflowExecutions({ workflow_id: workflow.id }),
    refetchInterval: (query) => query.state.data?.some((item) => !executionFinished(item.status)) ? 1500 : false,
  })
  const toggle = useMutation({ mutationFn: () => updateWorkflow(workflow.id, { expected_revision: workflow.revision, enabled: !workflow.enabled }), onSuccess: async (next) => {
    client.setQueryData(['studio', 'workflow', workflow.id], next)
    await client.invalidateQueries({ queryKey: ['studio', 'workflows'] })
  } })
  const remove = useMutation({ mutationFn: () => deleteWorkflow(workflow.id), onSuccess: async () => {
    setDeleting(false)
    await client.invalidateQueries({ queryKey: ['studio', 'workflows'] })
    void navigate({ to: '/studio/workflows' })
  } })
  if (running) return <WorkflowExecutionPanel workflow={workflow} onClose={() => setRunning(false)} />
  const editSection = (section: 'info' | 'workflow') => void navigate({ to: '/studio/workflows/$workflowId', params: { workflowId: workflow.id }, search: { edit: true, section } })
  return <section className={kit.pageSection} data-testid='case-detail-panel'>
    <div className='flex min-w-0 items-stretch gap-4'>
      {previewKey ? <Button variant='outline' onClick={() => setPreviewOpen(true)} aria-label={t('media.zoom')} className='group h-[84px] w-[84px] shrink-0 rounded-lg bg-muted p-0'>
        {previewIsVideo ? <video src={previewUrl ? `${previewUrl}#t=0.1` : undefined} muted playsInline preload='metadata' disablePictureInPicture className='h-full w-full object-contain' /> : <img src={previewUrl} alt={workflow.name} className='h-full w-full object-contain' />}
        <span className='absolute right-1.5 bottom-1.5 flex size-7 items-center justify-center rounded-md bg-foreground/45 text-background opacity-0 transition-opacity group-hover:opacity-100'><ZoomIn className='size-4' /></span>
      </Button> : null}
      <div className='flex min-w-0 flex-1 flex-col gap-[6px]'>
        <div className='flex min-w-0 flex-wrap items-center justify-between gap-3'>
          <div className='flex min-w-0 flex-wrap items-center gap-2'><h2 className={`${kit.title} min-w-0 truncate`}>{workflow.name || workflow.id}</h2></div>
          <div className='flex shrink-0 flex-wrap gap-2'>
            <Button type='button' size='sm' onClick={() => editSection('info')}><PenLine className='size-3.5' strokeWidth={2} />{t('cases.edit')}</Button>
            <Button type='button' variant='outline' size='sm' onClick={() => editSection('workflow')}><Settings2 className='size-3.5' strokeWidth={2} />{t('cases.editWorkflow')}</Button>
            <Button size='sm' variant='outline' disabled={!workflow.enabled} onClick={() => setRunning(true)}><Play />运行草稿</Button>
          <DropdownMenu><DropdownMenuTrigger asChild><Button size='icon-sm' variant='outline' aria-label='工作流操作'><MoreHorizontal /></Button></DropdownMenuTrigger><DropdownMenuContent align='end'><DropdownMenuGroup><DropdownMenuItem disabled={toggle.isPending} onSelect={() => toggle.mutate()}>{workflow.enabled ? '停用' : '启用'}</DropdownMenuItem><DropdownMenuItem variant='destructive' onSelect={() => { remove.reset(); setDeleting(true) }}><Trash2 />删除工作流</DropdownMenuItem></DropdownMenuGroup></DropdownMenuContent></DropdownMenu>
          </div>
        </div>
        {workflow.draft.description ? <LongText className='max-w-full text-sm text-muted-foreground'>{workflow.draft.description}</LongText> : null}
        <div className='mt-1 flex max-w-full flex-wrap items-center gap-2 text-xs'><MetaChip icon={<Tags className='size-3.5' strokeWidth={2} />} label={t('cases.fieldTags')} value={workflow.tags.join(', ')} divider /><MetaChip icon={<FolderOpen className='size-3.5' strokeWidth={2} />} label={t('cases.fieldCategories')} value={workflow.category} /></div>
      </div>
    </div>
    <Tabs defaultValue='configuration' className='min-w-0 gap-5'>
      <TabsList className='h-auto w-full flex-wrap justify-start' aria-label={t('cases.title')}><TabsTrigger value='configuration' className='data-[state=active]:shadow-none'>{t('cases.sectionConfig')}</TabsTrigger><TabsTrigger value='versions' className='data-[state=active]:shadow-none'>{t('workflowMarket.versions')}</TabsTrigger><TabsTrigger value='executions' className='data-[state=active]:shadow-none'>执行记录</TabsTrigger><TabsTrigger value='introduction' className='data-[state=active]:shadow-none'>介绍</TabsTrigger></TabsList>
      {toggle.error ? <p role='alert' className='text-sm text-destructive'>{toggle.error.message}</p> : null}
      <TabsContent value='introduction'><WorkflowIntroduction workflow={{ ...workflow.draft, name: workflow.name, categories: [workflow.category], tags: workflow.tags }} inputs={workflow.draft.inputs} outputs={workflow.draft.outputs} /></TabsContent>
      <TabsContent value='configuration' className='flex flex-col gap-5'>
        <section className='flex flex-col gap-4'><SectionHead title={t('cases.sectionConfig')} /><WorkflowConfigView definition={workflow.draft} workflow={workflow} /></section>
        {workflow.draft.type === 'comfyui' ? <section id='case-routing-section' className='flex flex-col gap-4'><SectionHead title='计算节点' /><div className='flex flex-wrap items-center justify-between gap-3 rounded-lg border bg-card p-4'><div className='flex min-w-0 items-center gap-2'><StatusDot label={node.data?.healthy ? '节点就绪' : node.error?.message ?? node.data?.issues.join('；') ?? '选择计算节点'} problems={node.isSuccess && node.data.healthy ? 0 : 1} /><span className='text-sm font-medium'>{node.data?.name || nodeId || '暂未选择'}</span></div><Button variant='outline' size='sm' className='h-7 gap-1.5 px-2.5 text-xs' onClick={() => editSection('workflow')}><PenLine className='size-3.5' />编辑节点</Button>{node.data ? <p className='w-full break-all text-xs text-muted-foreground'>{node.data.comfyui_base_url}</p> : null}{node.error ? <p role='alert' className='text-sm text-destructive'>{node.error.message}</p> : null}</div></section> : null}
      </TabsContent>
      <TabsContent value='versions'><WorkflowVersions record={workflow} /></TabsContent>
      <TabsContent value='executions' className='flex flex-col'><div className='flex flex-1 flex-col gap-4'>
        {executions.isPending ? <Skeleton className='h-40' /> : executions.error ? <p role='alert' className='text-sm text-destructive'>{executions.error.message}</p> : executions.data.length ? <div className='flex flex-col gap-2'>
          {executions.data.map((execution) => <Button key={execution.id} size='sm' variant={selectedExecution === execution.id ? 'secondary' : 'ghost'} className='h-auto justify-between gap-3 py-3' onClick={() => setSelectedExecution(execution.id)}><span>{executionStatusLabels[execution.status]}</span><span className='tabular-nums'>{formatDateTime(execution.created_at)}</span></Button>)}
        </div> : <Empty><EmptyHeader><EmptyTitle>暂无执行记录</EmptyTitle></EmptyHeader></Empty>}
        {selectedExecution ? <WorkflowExecutionResult key={selectedExecution} executionId={selectedExecution} /> : null}
      </div></TabsContent>
    </Tabs>
    <MediaLightbox open={previewOpen} onOpenChange={setPreviewOpen} src={previewUrl} isVideo={previewIsVideo} alt={workflow.name} />
    <ConfirmDialog open={deleting} onOpenChange={setDeleting} title='删除工作流？' desc={<div className='flex flex-col gap-2'><p>{workflow.name}</p>{remove.error ? <p role='alert' className='text-destructive'>{remove.error.message}</p> : null}</div>} confirmText='删除' cancelBtnText='取消' destructive isLoading={remove.isPending} handleConfirm={() => remove.mutate()} />
  </section>
}
