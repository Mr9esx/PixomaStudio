import { useRef, useState } from 'react'
import { useMutation, useQuery, useQueryClient } from '@tanstack/react-query'
import { useNavigate } from '@tanstack/react-router'
import { Bot, Plus, Search, Sparkles, Upload } from 'lucide-react'
import { useTranslation } from 'react-i18next'
import { toast } from 'sonner'
import {
  changeAssetPublication,
  expertAvatarURL,
  getAssetSettings,
  getAssetVersion,
  listAgentSkills,
  listAssetPublications,
  listAssetVersions,
  listExperts,
  listMarketExperts,
  listMarketSkills,
  saveAssetSettings,
  setExpertEnabled,
  testOfficialExpert,
  type AgentAssetKind,
  type AgentSkillSummary,
  type AssetScope,
  type Expert,
  type ExpertVersion,
} from '@/lib/api/agent-assets'
import {
  inspectStudioSkillZip,
  sendStudioMessage,
  updateStudioSkillEnabled,
  type StudioSkill,
  type StudioSkillVersion,
} from '@/lib/api/studio'
import { Badge } from '@/components/ui/badge'
import { Button } from '@/components/ui/button'
import {
  Card,
  CardContent,
  CardFooter,
  CardHeader,
  CardTitle,
} from '@/components/ui/card'
import {
  Dialog,
  DialogContent,
  DialogDescription,
  DialogFooter,
  DialogHeader,
  DialogTitle,
} from '@/components/ui/dialog'
import {
  Empty,
  EmptyHeader,
  EmptyMedia,
  EmptyTitle,
} from '@/components/ui/empty'
import { Field, FieldGroup, FieldLabel } from '@/components/ui/field'
import { Input } from '@/components/ui/input'
import { Switch } from '@/components/ui/switch'
import { Textarea } from '@/components/ui/textarea'
import { CodeEditor } from '@/components/code-editor'
import { LoadingSkeleton } from '@/components/feedback/loading-skeleton'
import { FilterSegment } from '@/components/filters/filter-segment'
import { StatusDot } from '@/components/status-dot'
import { SkillWorkspaceDialog } from '@/features/studio/skill-workspace-dialog'
import { Choice } from './expert-editor'
import { expertPartsText } from './expert-form'

type CatalogItem = AgentSkillSummary | Expert
type Selection = 'all' | 'official' | 'user' | 'mine'

export function AssetCatalog({
  kind,
  scope,
  market = false,
}: {
  kind: AgentAssetKind
  scope: AssetScope
  market?: boolean
}) {
  const { t } = useTranslation()
  const client = useQueryClient()
  const navigate = useNavigate()
  const [filter, setFilter] = useState<Selection>(
    market || scope === 'operations' ? 'all' : 'mine'
  )
  const [search, setSearch] = useState('')
  const [editor, setEditor] = useState<{
    id?: string
    imported?: Pick<StudioSkill, 'name' | 'description' | 'files'>
  }>()
  const [selected, setSelected] = useState<CatalogItem>()
  const [publication, setPublication] = useState<{
    item: CatalogItem
    action: 'publish' | 'unpublish' | 'unlock'
  }>()
  const [usage, setUsage] = useState<CatalogItem>()
  const [importError, setImportError] = useState('')
  const importRef = useRef<HTMLInputElement>(null)
  const personal = !market || filter === 'mine'
  const assets = useQuery({
    queryKey: [
      'agent-assets',
      scope,
      kind,
      market && !personal ? 'market' : 'owned',
    ],
    queryFn: async (): Promise<CatalogItem[]> =>
      kind === 'skills'
        ? market && !personal
          ? listMarketSkills()
          : listAgentSkills(scope)
        : market && !personal
          ? listMarketExperts()
          : listExperts(scope),
  })
  const settings = useQuery({
    queryKey: ['agent-assets', 'studio', 'settings'],
    queryFn: () => getAssetSettings('studio'),
    enabled: scope === 'studio' && kind === 'experts',
  })
  const toggle = useMutation({
    mutationFn: async (item: CatalogItem) =>
      kind === 'skills'
        ? updateStudioSkillEnabled(
            item.id,
            !item.enabled,
            scope === 'studio' ? 'studio' : 'official'
          )
        : setExpertEnabled(scope, item.id, !item.enabled),
    onSuccess: async () => {
      await client.invalidateQueries({ queryKey: ['agent-assets'] })
      if (kind === 'skills')
        await client.invalidateQueries({ queryKey: ['studio', 'skills'] })
    },
    onError: (cause) => toast.error(cause.message),
  })
  const visible = (assets.data ?? []).filter(
    (item) =>
      ((filter !== 'official' && filter !== 'user') ||
        item.source === filter) &&
      `${item.name} ${item.description}`
        .toLocaleLowerCase()
        .includes(search.toLocaleLowerCase())
  )
  const canCreate =
    scope !== 'operations' &&
    (kind === 'skills' ||
      scope === 'official' ||
      settings.data?.allow_user_create_experts === true)
  const canEdit = scope === 'official' || (scope === 'studio' && personal)
  const Icon = kind === 'skills' ? Sparkles : Bot
  const title = t(
    kind === 'skills'
      ? market || scope === 'operations'
        ? 'agentAssets.skillMarket'
        : 'agentAssets.skillManagement'
      : market || scope === 'operations'
        ? 'agentAssets.expertMarket'
        : 'agentAssets.expertManagement'
  )
  const openEditor = (id?: string) => {
    if (kind === 'skills') { setEditor({ id }); return }
    void navigate({ to: '/studio/expert-editor/$expertId', params: { expertId: id ?? 'new' } })
  }
  return (
    <section className='flex min-h-0 flex-1 flex-col overflow-hidden bg-card'>
      <header className='flex flex-wrap items-center justify-between gap-3 border-b p-5'>
        <h1 className='flex items-center gap-2 text-sm font-semibold'>
          <Icon className='size-4' />
          {title}
        </h1>
        <div className='flex gap-2'>
          {canCreate && kind === 'skills' ? (
            <>
              <input
                ref={importRef}
                type='file'
                accept='.zip'
                className='hidden'
                aria-label={t('agentAssets.importSkill')}
                onChange={(event) => {
                  const file = event.target.files?.[0]
                  event.target.value = ''
                  if (!file) return
                  setImportError('')
                  void inspectStudioSkillZip(
                    file,
                    scope === 'studio' ? 'studio' : 'official'
                  ).then(
                    (imported) => setEditor({ imported }),
                    (cause) =>
                      setImportError(
                        cause instanceof Error
                          ? cause.message
                          : t('agentAssets.importFailed')
                      )
                  )
                }}
              />
              <Button
                size='sm'
                variant='outline'
                onClick={() => importRef.current?.click()}
              >
                <Upload />
                {t('agentAssets.importSkill')}
              </Button>
            </>
          ) : null}
          {canCreate ? (
            <Button size='sm' onClick={() => openEditor()}>
              <Plus />
              {t(
                kind === 'skills'
                  ? 'agentAssets.createSkill'
                  : 'agentAssets.createExpert'
              )}
            </Button>
          ) : null}
        </div>
      </header>
      <div className='flex flex-wrap items-center gap-3 p-5 pb-0'>
        {market || scope === 'operations' ? (
          <FilterSegment
            aria-label={t('agentAssets.source')}
            value={filter}
            onValueChange={setFilter}
            options={(
              [
                'all',
                'official',
                'user',
                ...(scope === 'studio' ? ['mine'] : []),
              ] as Selection[]
            ).map((value) => ({ value, label: t(`agentAssets.${value}`) }))}
            className='w-full sm:w-72'
          />
        ) : null}
        <div className='relative ms-auto min-w-44'>
          <Search className='absolute start-3 top-2.5 size-4 text-muted-foreground' />
          <Input
            aria-label={t('agentAssets.search')}
            placeholder={t('agentAssets.search')}
            className='ps-9'
            value={search}
            onChange={(event) => setSearch(event.target.value)}
          />
        </div>
      </div>
      <div className='min-h-0 flex-1 overflow-y-auto p-5'>
        {importError ? (
          <p role='alert' className='mb-4 text-sm text-destructive'>
            {importError}
          </p>
        ) : null}
        {assets.isPending ? (
          <LoadingSkeleton />
        ) : assets.isError ? (
          <div role='alert' className='flex items-center gap-3 text-sm'>
            <span>{assets.error.message}</span>
            <Button
              size='sm'
              variant='outline'
              onClick={() => void assets.refetch()}
            >
              {t('agentAssets.retry')}
            </Button>
          </div>
        ) : visible.length === 0 ? (
          <Empty>
            <EmptyHeader>
              <EmptyMedia variant='icon'>
                <Icon />
              </EmptyMedia>
              <EmptyTitle>{t('agentAssets.empty')}</EmptyTitle>
            </EmptyHeader>
          </Empty>
        ) : (
          <div className='grid grid-cols-1 gap-4 lg:grid-cols-2 xl:grid-cols-3'>
            {visible.map((item) => (
              <Card key={item.id} className='flex flex-col shadow-none'>
                <CardHeader>
                  <CardTitle>
                    <button
                      type='button'
                      className='flex w-full items-center gap-2 text-left text-sm hover:underline'
                      onClick={() => setSelected(item)}
                    >
                      {'avatar_asset_id' in item && item.avatar_asset_id ? (
                        <img
                          alt=''
                          className='size-8 shrink-0 rounded-full border object-cover'
                          src={expertAvatarURL(
                            scope,
                            item.id,
                            item.version,
                            market && !personal
                          )}
                        />
                      ) : (
                        <Icon className='size-4 shrink-0' />
                      )}
                      {item.name}
                    </button>
                  </CardTitle>
                  <div className='flex flex-wrap items-center gap-2'>
                    <Badge variant='secondary'>
                      {t(`agentAssets.${item.source}`)}
                    </Badge>
                    <StatusDot
                      state={item.available === true ? 'ok' : 'warn'}
                      label={
                        item.unavailable_reason ||
                        (item.enabled
                          ? t('agentAssets.enabled')
                          : t('agentAssets.disabled'))
                      }
                    />
                    <span className='text-xs text-muted-foreground'>
                      {t(
                        item.published
                          ? 'agentAssets.published'
                          : 'agentAssets.unpublished'
                      )}
                    </span>
                  </div>
                </CardHeader>
                <CardContent className='flex flex-1 flex-col gap-2'>
                  <p className='line-clamp-3 text-sm text-muted-foreground'>
                    {item.description}
                  </p>
                  <div className='flex flex-wrap gap-3 font-mono text-xs text-muted-foreground'>
                    <span>
                      {t(
                        canEdit || scope === 'operations'
                          ? 'agentAssets.savedVersion'
                          : 'agentAssets.version'
                      )}{' '}
                      v{item.version}
                    </span>
                    {item.published_version &&
                    (canEdit || scope === 'operations') ? (
                      <span>
                        {t('agentAssets.marketVersion')} v
                        {item.published_version}
                      </span>
                    ) : null}
                  </div>
                  {item.unavailable_reason ? (
                    <p className='text-xs text-warning'>
                      {item.unavailable_reason}
                    </p>
                  ) : null}
                  {item.market_locked ? (
                    <p className='text-xs text-warning'>
                      {t('agentAssets.marketLocked')}
                      {item.market_lock_reason
                        ? `：${item.market_lock_reason}`
                        : ''}
                    </p>
                  ) : null}
                  {scope === 'operations' ? (
                    <p className='truncate text-xs text-muted-foreground'>
                      {t('agentAssets.creator')}：{item.account_id}
                    </p>
                  ) : null}
                </CardContent>
                <CardFooter className='flex flex-wrap gap-2'>
                  {canEdit ? (
                    <>
                      <Button
                        size='sm'
                        variant='outline'
                        onClick={() => openEditor(item.id)}
                      >
                        {t('agentAssets.edit')}
                      </Button>
                      <Button
                        size='sm'
                        variant='ghost'
                        disabled={toggle.isPending}
                        onClick={() => toggle.mutate(item)}
                      >
                        {t(
                          item.enabled
                            ? 'agentAssets.disable'
                            : 'agentAssets.enable'
                        )}
                      </Button>
                    </>
                  ) : null}
                  {canEdit || scope === 'operations' ? (
                    <>
                      <Button
                        size='sm'
                        variant='outline'
                        disabled={!item.enabled || item.market_locked}
                        onClick={() =>
                          setPublication({ item, action: 'publish' })
                        }
                      >
                        {t(
                          item.published
                            ? 'agentAssets.updatePublication'
                            : 'agentAssets.publish'
                        )}
                      </Button>
                      {item.published ||
                      (scope === 'operations' && !item.market_locked) ? (
                        <Button
                          size='sm'
                          variant='ghost'
                          onClick={() =>
                            setPublication({ item, action: 'unpublish' })
                          }
                        >
                          {t('agentAssets.unpublish')}
                        </Button>
                      ) : null}
                      {scope === 'operations' && item.market_locked ? (
                        <Button
                          size='sm'
                          variant='ghost'
                          onClick={() =>
                            setPublication({ item, action: 'unlock' })
                          }
                        >
                          {t('agentAssets.unlock')}
                        </Button>
                      ) : null}
                    </>
                  ) : null}
                  {kind === 'experts' && scope !== 'operations' ? (
                    <Button
                      size='sm'
                      disabled={item.available !== true}
                      onClick={() => setUsage(item)}
                    >
                      {t(
                        canEdit ? 'agentAssets.test' : 'agentAssets.useExpert'
                      )}
                    </Button>
                  ) : null}
                </CardFooter>
              </Card>
            ))}
          </div>
        )}
      </div>
      {editor && kind === 'skills' ? (
        <SkillWorkspaceDialog
          scope={scope === 'studio' ? 'studio' : 'official'}
          skillId={editor.id}
          imported={editor.imported}
          open
          onOpenChange={(open) => {
            if (!open) setEditor(undefined)
          }}
        />
      ) : null}
      {publication ? (
        <PublicationDialog
          kind={kind}
          scope={scope}
          item={publication.item}
          action={publication.action}
          onClose={() => setPublication(undefined)}
        />
      ) : null}
      {selected ? (
        <AssetHistoryDialog
          kind={kind}
          scope={scope}
          market={market && !personal}
          item={selected}
          onUse={
            kind === 'experts' && scope !== 'operations' && selected.enabled
              ? (expert) => {
                  setSelected(undefined)
                  setUsage(expert)
                }
              : undefined
          }
          onClose={() => setSelected(undefined)}
        />
      ) : null}
      {usage && 'sop' in usage ? (
        <ExpertUseDialog
          expert={usage}
          scope={scope === 'official' ? 'official' : 'studio'}
          onClose={() => setUsage(undefined)}
        />
      ) : null}
    </section>
  )
}

function PublicationDialog({
  scope,
  kind,
  item,
  action,
  onClose,
}: {
  scope: AssetScope
  kind: AgentAssetKind
  item: CatalogItem
  action: 'publish' | 'unpublish' | 'unlock'
  onClose: () => void
}) {
  const { t } = useTranslation()
  const client = useQueryClient()
  const [version, setVersion] = useState(item.published_version || item.version)
  const [reason, setReason] = useState('')
  const requests = useRef(new Map<string, string>())
  const versions = useQuery({
    queryKey: ['agent-assets', scope, kind, item.id, 'versions'],
    queryFn: () => listAssetVersions(scope, kind, item.id),
    enabled: action === 'publish',
  })
  const submit = useMutation({
    mutationFn: () => {
      const key = JSON.stringify({ version, reason, action })
      const requestId = requests.current.get(key) ?? crypto.randomUUID()
      requests.current.set(key, requestId)
      return changeAssetPublication(
        scope,
        kind,
        item.id,
        action,
        action === 'publish' ? version : undefined,
        reason,
        requestId
      )
    },
    onSuccess: async () => {
      await client.invalidateQueries({ queryKey: ['agent-assets'] })
      await client.invalidateQueries({ queryKey: ['studio', 'skills'] })
      toast.success(t(`agentAssets.${action}Done`))
      onClose()
    },
  })
  return (
    <Dialog
      open
      onOpenChange={(open) => {
        if (!open && !submit.isPending) onClose()
      }}
    >
      <DialogContent>
        <DialogHeader>
          <DialogTitle>
            {t(`agentAssets.${action}`)} · {item.name}
          </DialogTitle>
          <DialogDescription className='sr-only'>
            {item.description}
          </DialogDescription>
        </DialogHeader>
        {action === 'publish' ? (
          <Field>
            <FieldLabel htmlFor='publication-version'>
              {t('agentAssets.version')}
            </FieldLabel>
            <Choice
              id='publication-version'
              value={version}
              onChange={setVersion}
              options={(versions.data ?? []).map((item) => ({
                value: item.version,
                label: `v${item.version}`,
              }))}
            />
          </Field>
        ) : null}
        {scope === 'operations' && action === 'unpublish' ? (
          <Field>
            <FieldLabel htmlFor='publication-reason'>
              {t('agentAssets.unpublishReason')}
            </FieldLabel>
            <Textarea
              id='publication-reason'
              value={reason}
              onChange={(event) => setReason(event.target.value)}
            />
          </Field>
        ) : null}
        {submit.error || versions.error ? (
          <p role='alert' className='text-sm text-destructive'>
            {(submit.error ?? versions.error)?.message}
          </p>
        ) : null}
        <DialogFooter>
          <Button
            size='sm'
            variant='outline'
            disabled={submit.isPending}
            onClick={onClose}
          >
            {t('common.cancel')}
          </Button>
          <Button
            size='sm'
            disabled={
              submit.isPending ||
              (action === 'publish' &&
                (versions.isPending ||
                  versions.isError ||
                  !versions.data?.some((item) => item.version === version))) ||
              (scope === 'operations' &&
                action === 'unpublish' &&
                !reason.trim())
            }
            onClick={() => submit.mutate()}
          >
            {t(`agentAssets.${action}`)}
          </Button>
        </DialogFooter>
      </DialogContent>
    </Dialog>
  )
}

function AssetHistoryDialog({
  scope,
  kind,
  item,
  market,
  onUse,
  onClose,
}: {
  scope: AssetScope
  kind: AgentAssetKind
  item: CatalogItem
  market: boolean
  onUse?: (expert: Expert) => void
  onClose: () => void
}) {
  const { t } = useTranslation()
  const [version, setVersion] = useState(item.version)
  const versions = useQuery({
    queryKey: ['agent-assets', scope, kind, item.id, 'versions', market],
    queryFn: () => listAssetVersions(scope, kind, item.id, market),
  })
  const records = useQuery({
    queryKey: ['agent-assets', scope, kind, item.id, 'publications'],
    queryFn: () => listAssetPublications(scope, kind, item.id),
    enabled: !market,
  })
  const detail = useQuery({
    queryKey: [
      'agent-assets',
      scope,
      kind,
      item.id,
      'version',
      version,
      market,
    ],
    queryFn: () => getAssetVersion(scope, kind, item.id, version, market),
    enabled: Boolean(version),
  })
  const selected = detail.data
  return (
    <Dialog
      open
      onOpenChange={(open) => {
        if (!open) onClose()
      }}
    >
      <DialogContent className='flex max-h-[85dvh] flex-col sm:max-w-3xl'>
        <DialogHeader>
          <DialogTitle>{item.name}</DialogTitle>
          <DialogDescription>{item.description}</DialogDescription>
        </DialogHeader>
        <div className='flex min-h-0 flex-1 flex-col gap-4 overflow-y-auto'>
          <Choice
            ariaLabel={t('agentAssets.version')}
            value={version}
            onChange={setVersion}
            options={(versions.data ?? []).map((item) => ({
              value: item.version,
              label: `v${item.version}`,
            }))}
          />
          {versions.error || detail.error || records.error ? (
            <p role='alert' className='text-sm text-destructive'>
              {(versions.error ?? detail.error ?? records.error)?.message}
            </p>
          ) : null}
          {selected && kind === 'experts' ? (
            <>
              <ExpertVersionPreview expert={selected as ExpertVersion} />
              {!market ? (
                <details>
                  <summary className='cursor-pointer text-sm'>
                    {t('agentAssets.versionContent')}
                  </summary>
                  <CodeEditor
                    value={JSON.stringify(selected, null, 2)}
                    language='json'
                    readOnly
                    editable={false}
                    maxHeight={400}
                  />
                </details>
              ) : null}
            </>
          ) : selected ? (
            <CodeEditor
              aria-label={t('agentAssets.versionContent')}
              value={
                kind === 'skills'
                  ? ((selected as StudioSkillVersion).files?.find(
                      (file) => file.path === 'SKILL.md'
                    )?.content ?? (selected as StudioSkillVersion).prompt)
                  : JSON.stringify(selected as ExpertVersion, null, 2)
              }
              language={kind === 'skills' ? 'markdown' : 'json'}
              readOnly
              editable={false}
              maxHeight={400}
            />
          ) : null}
          {!market ? (
            <section className='flex flex-col gap-2'>
              <h3 className='text-sm font-medium'>
                {t('agentAssets.publicationHistory')}
              </h3>
              {records.data?.map((record) => (
                <div
                  key={record.id}
                  className='flex flex-wrap gap-3 border-b py-2 text-xs text-muted-foreground'
                >
                  <span>{t(`agentAssets.${record.action}`)}</span>
                  <span>v{record.version}</span>
                  <span>{record.created_by}</span>
                  <time>{new Date(record.created_at).toLocaleString()}</time>
                  {record.reason ? <span>{record.reason}</span> : null}
                </div>
              ))}
            </section>
          ) : null}
        </div>
        {selected && kind === 'experts' && onUse ? (
          <DialogFooter>
            <Button
              size='sm'
              onClick={() =>
                onUse({
                  ...item,
                  ...(selected as ExpertVersion),
                  id: item.id,
                } as Expert)
              }
            >
              {t(market ? 'agentAssets.useExpert' : 'agentAssets.test')}
            </Button>
          </DialogFooter>
        ) : null}
      </DialogContent>
    </Dialog>
  )
}

function ExpertVersionPreview({ expert }: { expert: ExpertVersion }) {
  const { t } = useTranslation()
  return (
    <div className='flex flex-col gap-5'>
      {expert.examples.length > 0 ? (
        <section className='flex flex-col gap-2'>
          <h3 className='text-sm font-medium'>{t('agentAssets.examples')}</h3>
          {expert.examples.map((example, index) => (
            <p
              key={index}
              className='rounded-lg border bg-muted p-3 text-sm whitespace-pre-wrap'
            >
              {example}
            </p>
          ))}
        </section>
      ) : null}
      <section className='flex flex-col gap-2'>
        <h3 className='text-sm font-medium'>专家提示词</h3>
        <p className='text-sm whitespace-pre-wrap'>{expertPartsText(expert.prompt_parts)}</p>
      </section>
      <section className='flex flex-col gap-3'>
        <h3 className='text-sm font-medium'>{t('agentAssets.sop')}</h3>
        {expert.sop.map((step, index) => (
          <div
            key={step.id}
            className='flex flex-col gap-2 rounded-lg border bg-muted p-3'
          >
            <p className='text-sm font-medium'>
              {index + 1}. {step.title}
            </p>
            <p className='text-sm whitespace-pre-wrap'>{expertPartsText(step.instruction_parts)}</p>
          </div>
        ))}
      </section>
    </div>
  )
}

function ExpertUseDialog({
  expert,
  scope,
  onClose,
}: {
  expert: Expert
  scope: 'official' | 'studio'
  onClose: () => void
}) {
  const { t, i18n } = useTranslation()
  const client = useQueryClient()
  const navigate = useNavigate()
  const [text, setText] = useState('')
  const requests = useRef(new Map<string, string>())
  const start = useMutation({
    mutationFn: () => {
      const key = JSON.stringify({ text })
      const requestId = requests.current.get(key) ?? crypto.randomUUID()
      requests.current.set(key, requestId)
      const locale = i18n.language.startsWith('en') ? 'en' : 'zh'
      return scope === 'official'
        ? testOfficialExpert(
            expert.id,
            expert.version,
            text,
            requestId,
            locale
          )
        : sendStudioMessage({
            text,
            locale,
            requestId,
            permissionMode: 'request_approval',
            expertId: expert.id,
            expertVersion: expert.version,
          }).then((turn) => ({ run: turn.run, session_id: turn.session.id }))
    },
    onSuccess: async (turn) => {
      await client.invalidateQueries({ queryKey: ['studio'] })
      onClose()
      void navigate({
        to: '/studio/sessions/$sessionId',
        params: { sessionId: turn.session_id },
      })
    },
  })
  return (
    <Dialog
      open
      onOpenChange={(open) => {
        if (!open && !start.isPending) onClose()
      }}
    >
      <DialogContent className='flex max-h-[90dvh] flex-col overflow-hidden sm:max-w-2xl'>
        <DialogHeader>
          <DialogTitle>{expert.name}</DialogTitle>
          <DialogDescription>{expert.description}</DialogDescription>
        </DialogHeader>
        {
          <>
            <FieldGroup className='overflow-y-auto'>
              <Field>
                <FieldLabel htmlFor='expert-goal'>
                  {t('agentAssets.goal')}
                </FieldLabel>
                <Textarea
                  id='expert-goal'
                  placeholder={t('agentAssets.goalPlaceholder')}
                  value={text}
                  onChange={(event) => setText(event.target.value)}
                />
              </Field>
            </FieldGroup>
            {start.error ? (
              <p role='alert' className='text-sm text-destructive'>
                {start.error.message}
              </p>
            ) : null}
            <DialogFooter>
              <Button
                size='sm'
                variant='outline'
                disabled={start.isPending}
                onClick={onClose}
              >
                {t('common.cancel')}
              </Button>
              <Button
                size='sm'
                disabled={start.isPending || !text.trim()}
                onClick={() => start.mutate()}
              >
                {t(
                  scope === 'official'
                    ? 'agentAssets.test'
                    : 'agentAssets.start'
                )}
              </Button>
            </DialogFooter>
          </>
        }
      </DialogContent>
    </Dialog>
  )
}

export function AssetSettingsPage() {
  const { t } = useTranslation()
  const client = useQueryClient()
  const settings = useQuery({
    queryKey: ['agent-assets', 'operations', 'settings'],
    queryFn: () => getAssetSettings('operations'),
  })
  const save = useMutation({
    mutationFn: (enabled: boolean) =>
      saveAssetSettings({ allow_user_create_experts: enabled }),
    onSuccess: () => client.invalidateQueries({ queryKey: ['agent-assets'] }),
  })
  return (
    <section className='flex flex-col gap-5 bg-card p-5'>
      <h1 className='text-sm font-semibold'>{t('agentAssets.settings')}</h1>
      {settings.data ? (
        <label className='flex items-center justify-between gap-4 text-sm'>
          <span>{t('agentAssets.allowUserCreateExperts')}</span>
          <Switch
            checked={settings.data.allow_user_create_experts}
            disabled={save.isPending}
            onCheckedChange={(enabled) => save.mutate(enabled)}
          />
        </label>
      ) : settings.isPending ? (
        <LoadingSkeleton />
      ) : null}
      {settings.error || save.error ? (
        <p role='alert' className='text-sm text-destructive'>
          {(settings.error ?? save.error)?.message}
        </p>
      ) : null}
    </section>
  )
}
