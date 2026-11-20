import { useLayoutEffect, useRef, useState } from 'react'
import { useMutation, useQuery, useQueryClient } from '@tanstack/react-query'
import { useNavigate, useSearch } from '@tanstack/react-router'
import { ArrowLeft, Bot, Plus, Search, Sparkles, Upload } from 'lucide-react'
import { useTranslation } from 'react-i18next'
import { toast } from 'sonner'
import { expertAvatarURL, getAssetVersion, listAgentSkills, listAssetVersions, listExperts, setExpertEnabled, type AgentAssetKind, type AgentSkillSummary, type AssetScope, type Expert, type ExpertVersion } from '@/lib/api/agent-assets'
import { inspectStudioSkillZip, sendStudioMessage, updateStudioSkillEnabled, type StudioSkill, type StudioSkillVersion } from '@/lib/api/studio'
import { Button } from '@/components/ui/button'
import { Dialog, DialogContent, DialogDescription, DialogFooter, DialogHeader, DialogTitle } from '@/components/ui/dialog'
import { Empty, EmptyHeader, EmptyMedia, EmptyTitle } from '@/components/ui/empty'
import { Field, FieldGroup, FieldLabel } from '@/components/ui/field'
import { Input } from '@/components/ui/input'
import { Textarea } from '@/components/ui/textarea'
import { Tabs, TabsContent, TabsList, TabsTrigger } from '@/components/ui/tabs'
import { CodeEditor } from '@/components/code-editor'
import { LoadingSkeleton } from '@/components/feedback/loading-skeleton'
import { StatusDot } from '@/components/status-dot'
import { SkillWorkspace } from '@/features/studio/skill-workspace'
import { Choice } from './expert-editor'
import { expertPartsText } from './expert-form'

type CatalogItem = AgentSkillSummary | Expert

export function AssetCatalog({ kind, scope }: { kind: AgentAssetKind; scope: AssetScope }) {
  const { t } = useTranslation()
  const client = useQueryClient()
  const navigate = useNavigate()
  const routeSearch = useSearch({ strict: false })
  const search = routeSearch.q ?? ''
  const [imported, setImported] = useState<Pick<StudioSkill, 'name' | 'description' | 'files'>>()
  const [selected, setSelected] = useState<CatalogItem>()
  const [usage, setUsage] = useState<Expert>()
  const [importError, setImportError] = useState('')
  const importRef = useRef<HTMLInputElement>(null)
  const listRef = useRef<HTMLDivElement>(null)
  const listPosition = useRef(0)
  useLayoutEffect(() => { if (!routeSearch.edit && !selected && listRef.current) listRef.current.scrollTop = listPosition.current }, [routeSearch.edit, selected])
  const assets = useQuery({ queryKey: ['agent-assets', scope, kind], queryFn: async (): Promise<CatalogItem[]> => kind === 'skills' ? listAgentSkills(scope) : listExperts(scope) })
  const toggle = useMutation({
    mutationFn: async (item: CatalogItem) => kind === 'skills' ? updateStudioSkillEnabled(item.id, !item.enabled, scope) : setExpertEnabled(scope, item.id, !item.enabled),
    onSuccess: async () => { await client.invalidateQueries({ queryKey: ['agent-assets'] }); await client.invalidateQueries({ queryKey: ['studio', 'skills'] }) },
    onError: cause => toast.error(cause.message),
  })
  const visible = (assets.data ?? []).filter(item => `${item.name} ${item.description}`.toLocaleLowerCase().includes(search.toLocaleLowerCase()))
  const Icon = kind === 'skills' ? Sparkles : Bot
  const openEditor = (id?: string) => {
    if (kind === 'skills') { setImported(undefined); void navigate({ to: '/studio/skills', search: { q: search || undefined, edit: id ?? 'new' } }); return }
    void navigate({ to: '/studio/expert-editor/$expertId', params: { expertId: id ?? 'new' } })
  }
  if (kind === 'skills' && typeof routeSearch.edit === 'string') return <SkillWorkspace key={routeSearch.edit} scope={scope} skillId={routeSearch.edit === 'new' ? undefined : routeSearch.edit} imported={imported} open onOpenChange={open => { if (!open) { setImported(undefined); void navigate({ to: '/studio/skills', search: { q: search || undefined } }) } }} />
  if (selected) return <AssetHistoryPage kind={kind} scope={scope} item={selected} onUse={selected.available === true ? expert => { setSelected(undefined); setUsage(expert) } : undefined} onClose={() => setSelected(undefined)} />
  return <Tabs value={kind} onValueChange={value => void navigate({ to: value === 'skills' ? '/studio/skills' : '/studio/experts', search: {} })} className='flex min-h-0 min-w-0 flex-1 flex-col gap-0 overflow-hidden bg-card'>
    <header data-tauri-drag-region='deep' className='studio-page-header studio-continuous-header justify-between'>
      <h1 className='flex min-w-0 items-center gap-2 text-sm font-semibold'><Sparkles className='size-4 shrink-0' /><span className='truncate'>{t('agentAssets.catalogTitle')}</span></h1>
      <div className='flex gap-2'>
        {kind === 'skills' ? <>
          <input ref={importRef} type='file' accept='.zip' className='hidden' aria-label={t('agentAssets.importSkill')} onChange={event => {
            const file = event.target.files?.[0]; event.target.value = ''; if (!file) return
            setImportError('')
            void inspectStudioSkillZip(file, scope).then(imported => { setImported(imported); void navigate({ to: '/studio/skills', search: { q: search || undefined, edit: 'new' } }) }, cause => setImportError(cause instanceof Error ? cause.message : t('agentAssets.importFailed')))
          }} />
          <Button size='sm' variant='outline' onClick={() => importRef.current?.click()}><Upload />{t('agentAssets.importSkill')}</Button>
        </> : null}
        <Button size='sm' onClick={() => openEditor()}><Plus />{t(kind === 'skills' ? 'agentAssets.createSkill' : 'agentAssets.createExpert')}</Button>
      </div>
    </header>
    <div className='flex shrink-0 flex-wrap items-center gap-3 px-5 pb-3'>
      <TabsList aria-label={t('agentAssets.catalogTitle')}>
        <TabsTrigger value='skills'><Sparkles />{t('agentAssets.skill')}</TabsTrigger>
        <TabsTrigger value='experts'><Bot />{t('agentAssets.expert')}</TabsTrigger>
      </TabsList>
      <div className='relative ms-auto min-w-44'><Search className='absolute start-3 top-2.5 size-4 text-muted-foreground' /><Input aria-label={t('agentAssets.search')} placeholder={t('agentAssets.search')} className='ps-9' value={search} onChange={event => void navigate({ to: kind === 'skills' ? '/studio/skills' : '/studio/experts', search: { q: event.target.value || undefined }, replace: true })} /></div>
    </div>
    <TabsContent ref={listRef} value={kind} className='m-0 flex min-h-0 flex-1 flex-col overflow-y-auto px-5 pb-5' onScroll={event => { listPosition.current = event.currentTarget.scrollTop }}>
      {importError ? <p role='alert' className='mb-4 text-sm text-destructive'>{importError}</p> : null}
      {assets.isPending ? <LoadingSkeleton /> : assets.isError ? <div role='alert' className='flex items-center gap-3 text-sm'><span>{assets.error.message}</span><Button size='sm' variant='outline' onClick={() => void assets.refetch()}>{t('agentAssets.retry')}</Button></div> : visible.length === 0 ? <Empty><EmptyHeader><EmptyMedia variant='icon'><Icon /></EmptyMedia><EmptyTitle>{t('agentAssets.empty')}</EmptyTitle></EmptyHeader></Empty> :
        <div className='grid grid-cols-1 gap-x-8 gap-y-10 lg:grid-cols-2 xl:grid-cols-3'>{visible.map(item => <article key={item.id} className='flex min-w-0 flex-col gap-4 py-3'>
          <div className='flex flex-col gap-3'><h2><button type='button' className='flex w-full items-center gap-2 text-left text-sm font-semibold hover:underline' onClick={() => setSelected(item)}>
            {'avatar_asset_id' in item && item.avatar_asset_id ? <img alt='' className='size-8 shrink-0 rounded-full border object-cover' src={expertAvatarURL(scope, item.id, item.version)} /> : <Icon className='size-4 shrink-0' />}{item.name}
          </button></h2><StatusDot state={item.available === true ? 'ok' : 'warn'} label={item.unavailable_reason || t(item.enabled ? 'agentAssets.enabled' : 'agentAssets.disabled')} /></div>
          <div className='flex flex-1 flex-col gap-2'><p className='line-clamp-3 text-sm text-muted-foreground'>{item.description}</p><span className='font-mono text-xs text-muted-foreground'>{t('agentAssets.savedVersion')} v{item.version}</span>{item.unavailable_reason ? <p className='text-xs text-warning'>{item.unavailable_reason}</p> : null}</div>
          <div className='flex flex-wrap gap-2'><Button size='sm' variant='outline' onClick={() => openEditor(item.id)}>{t('agentAssets.edit')}</Button><Button size='sm' variant='ghost' disabled={toggle.isPending} onClick={() => toggle.mutate(item)}>{t(item.enabled ? 'agentAssets.disable' : 'agentAssets.enable')}</Button>{'sop' in item ? <Button size='sm' disabled={item.available !== true} onClick={() => setUsage(item)}>{t('agentAssets.useExpert')}</Button> : null}</div>
        </article>)}</div>}
    </TabsContent>
    {usage ? <ExpertUseDialog expert={usage} onClose={() => setUsage(undefined)} /> : null}
  </Tabs>
}

function AssetHistoryPage({ scope, kind, item, onUse, onClose }: { scope: AssetScope; kind: AgentAssetKind; item: CatalogItem; onUse?: (expert: Expert) => void; onClose: () => void }) {
  const { t } = useTranslation()
  const [version, setVersion] = useState(item.version)
  const versions = useQuery({ queryKey: ['agent-assets', scope, kind, item.id, 'versions'], queryFn: () => listAssetVersions(scope, kind, item.id) })
  const detail = useQuery({ queryKey: ['agent-assets', scope, kind, item.id, 'version', version], queryFn: () => getAssetVersion(scope, kind, item.id, version) })
  const selected = detail.data
  return <section className='flex min-h-0 flex-1 flex-col'>
    <header data-tauri-drag-region='deep' className='studio-page-header studio-continuous-header'><Button size='icon-sm' variant='ghost' aria-label={t('common.backToList')} onClick={onClose}><ArrowLeft /></Button><h1 className='truncate text-sm font-semibold'>{item.name}</h1></header>
    <div className='relative flex min-h-0 flex-1 flex-col gap-3 overflow-y-auto px-5 pb-3'>
      <p className='text-sm text-muted-foreground'>{item.description}</p>
      <Choice ariaLabel={t('agentAssets.version')} value={version} onChange={setVersion} options={(versions.data ?? []).map(item => ({ value: item.version, label: `v${item.version}` }))} />
      {versions.error || detail.error ? <p role='alert' className='text-sm text-destructive'>{(versions.error ?? detail.error)?.message}</p> : null}
      {detail.isPending ? <LoadingSkeleton /> : selected && kind === 'experts' ? <>
        <ExpertVersionPreview expert={selected as ExpertVersion} />
        <details><summary className='cursor-pointer text-sm'>{t('agentAssets.versionContent')}</summary><CodeEditor value={JSON.stringify(selected, null, 2)} language='json' readOnly editable={false} maxHeight={400} /></details>
      </> : selected ? <CodeEditor aria-label={t('agentAssets.versionContent')} value={(selected as StudioSkillVersion).files?.find(file => file.path === 'SKILL.md')?.content ?? (selected as StudioSkillVersion).prompt} language='markdown' readOnly editable={false} maxHeight={400} /> : null}
    </div>
    {selected && kind === 'experts' && onUse ? <div className='px-5 pb-5'><Button size='sm' onClick={() => onUse({ ...item, ...(selected as ExpertVersion), id: item.id } as Expert)}>{t('agentAssets.useExpert')}</Button></div> : null}
  </section>
}

function ExpertVersionPreview({ expert }: { expert: ExpertVersion }) {
  const { t } = useTranslation()
  return <div className='flex flex-col gap-5'>
    {expert.examples.length > 0 ? <section className='flex flex-col gap-2'><h3 className='text-sm font-medium'>{t('agentAssets.examples')}</h3>{expert.examples.map((example, index) => <p key={index} className='rounded-lg border bg-muted p-3 text-sm whitespace-pre-wrap'>{example}</p>)}</section> : null}
    <section className='flex flex-col gap-2'><h3 className='text-sm font-medium'>专家提示词</h3><p className='text-sm whitespace-pre-wrap'>{expertPartsText(expert.prompt_parts)}</p></section>
    <section className='flex flex-col gap-3'><h3 className='text-sm font-medium'>{t('agentAssets.sop')}</h3>{expert.sop.map((step, index) => <div key={step.id} className='flex flex-col gap-2 rounded-lg border bg-muted p-3'><p className='text-sm font-medium'>{index + 1}. {step.title}</p><p className='text-sm whitespace-pre-wrap'>{expertPartsText(step.instruction_parts)}</p></div>)}</section>
  </div>
}

function ExpertUseDialog({ expert, onClose }: { expert: Expert; onClose: () => void }) {
  const { t, i18n } = useTranslation()
  const client = useQueryClient()
  const navigate = useNavigate()
  const [text, setText] = useState('')
  const requests = useRef(new Map<string, string>())
  const start = useMutation({
    mutationFn: () => {
      const requestId = requests.current.get(text) ?? crypto.randomUUID()
      requests.current.set(text, requestId)
      return sendStudioMessage({ text, locale: i18n.language.startsWith('en') ? 'en' : 'zh', requestId, permissionMode: 'request_approval', expertId: expert.id, expertVersion: expert.version })
    },
    onSuccess: async turn => { await client.invalidateQueries({ queryKey: ['studio'] }); onClose(); void navigate({ to: '/studio/sessions/$sessionId', params: { sessionId: turn.session.id } }) },
  })
  return <Dialog open onOpenChange={open => { if (!open && !start.isPending) onClose() }}><DialogContent className='flex max-h-[90dvh] flex-col overflow-hidden sm:max-w-2xl'>
    <DialogHeader><DialogTitle>{expert.name}</DialogTitle><DialogDescription>{expert.description}</DialogDescription></DialogHeader>
    <FieldGroup className='overflow-y-auto'><Field><FieldLabel htmlFor='expert-goal'>{t('agentAssets.goal')}</FieldLabel><Textarea id='expert-goal' placeholder={t('agentAssets.goalPlaceholder')} value={text} onChange={event => setText(event.target.value)} /></Field></FieldGroup>
    {start.error ? <p role='alert' className='text-sm text-destructive'>{start.error.message}</p> : null}
    <DialogFooter><Button size='sm' variant='outline' disabled={start.isPending} onClick={onClose}>{t('common.cancel')}</Button><Button size='sm' disabled={start.isPending || !text.trim()} onClick={() => start.mutate()}>{t('agentAssets.start')}</Button></DialogFooter>
  </DialogContent></Dialog>
}
