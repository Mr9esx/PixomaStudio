import { useLayoutEffect, useRef } from 'react'
import { useQuery } from '@tanstack/react-query'
import { Link, useNavigate, useSearch } from '@tanstack/react-router'
import { ArrowLeft, Workflow } from 'lucide-react'
import { useTranslation } from 'react-i18next'
import { getMarketWorkflow, listMarketWorkflows } from '@/lib/api/workflow-market'
import { Button } from '@/components/ui/button'
import { Input } from '@/components/ui/input'
import { Badge } from '@/components/ui/badge'
import { Skeleton } from '@/components/ui/skeleton'
import { FilterSegment } from '@/components/filters/filter-segment'
import { WorkflowCover, WorkflowIntroduction } from '@/features/cases/workflow-introduction'

export function StudioMarket({ workflowId, listPositionRef }: { workflowId?: string; listPositionRef: { current: number } }) {
  const { t } = useTranslation()
  const navigate = useNavigate()
  const search = useSearch({ strict: false })
  const scrollRef = useRef<HTMLDivElement>(null)
  const market = useQuery({ queryKey: ['workflow-market'], queryFn: listMarketWorkflows })
  const detail = useQuery({ queryKey: ['workflow-market', workflowId], queryFn: () => getMarketWorkflow(workflowId!), enabled: Boolean(workflowId) })
  useLayoutEffect(() => {
    if (scrollRef.current) scrollRef.current.scrollTop = workflowId ? 0 : listPositionRef.current
  }, [workflowId, market.data, listPositionRef])
  const categories = [...new Set(market.data?.flatMap((workflow) => workflow.categories).filter((category) => category.trim()) ?? [])].sort()
  const workflows = market.data?.filter((workflow) => {
    const matchesCategory = !search.category || workflow.categories.includes(search.category)
    const text = `${workflow.name} ${workflow.description} ${workflow.tags.join(' ')}`.toLocaleLowerCase()
    return matchesCategory && text.includes((search.q ?? '').toLocaleLowerCase())
  }) ?? []
  const returnSearch = { q: search.q, category: search.category }
  const openWorkflowDraft = (example?: number) => {
    if (!detail.data) return
    void navigate({ to: '/studio', search: { workflow: detail.data.id, example } })
  }
  return (
    <div className='flex min-h-0 min-w-0 flex-1 p-3 sm:p-4'>
      <main id='main-content' className='flex min-h-0 min-w-0 flex-1 flex-col overflow-hidden rounded-2xl border bg-card'>
        <header className='flex min-h-16 flex-wrap items-center justify-between gap-3 border-b px-5 py-3 pl-16 lg:pl-5'>
          {workflowId ? <div className='flex min-w-0 items-center gap-3'><Button asChild size='icon-sm' variant='ghost' aria-label={t('common.backToList')}><Link to='/studio/workflows' search={returnSearch}><ArrowLeft /></Link></Button><h1 className='truncate text-sm font-semibold'>{detail.data?.name ?? t('workflowMarket.title')}</h1></div> : <h1 className='text-sm font-semibold'>{t('workflowMarket.title')}</h1>}
          {workflowId && detail.data ? <Button size='sm' onClick={() => openWorkflowDraft()}>{t('workflowMarket.useWorkflow')}</Button> : null}
        </header>
        <div ref={scrollRef} className='min-h-0 flex-1 overflow-y-auto p-5' onScroll={(event) => { if (!workflowId) listPositionRef.current = event.currentTarget.scrollTop }}>
          {workflowId ? detail.isPending ? <Skeleton className='h-80 w-full' /> : detail.isError ? <div className='flex flex-col items-start gap-3'><p role='alert' className='text-sm text-destructive'>{detail.error.message}</p><Button size='sm' variant='outline' onClick={() => void detail.refetch()}>{t('common.retry')}</Button></div> : <div className='mx-auto max-w-4xl'><WorkflowIntroduction workflow={detail.data} inputs={detail.data.input_fields} outputs={detail.data.output_fields} onUseExample={(prompt) => openWorkflowDraft(detail.data.examples.findIndex((example) => example.prompt === prompt))} /></div> : (
            <div className='flex flex-col gap-5'>
              <div className='flex flex-col gap-3'>
                <Input aria-label={t('workflowMarket.search')} placeholder={t('workflowMarket.search')} value={search.q ?? ''} onChange={(event) => void navigate({ to: '/studio/workflows', search: { ...returnSearch, q: event.target.value }, replace: true })} />
                <FilterSegment value={search.category ?? ''} options={[{ value: '', label: t('workflowMarket.allCategories') }, ...categories.map((category) => ({ value: category, label: category }))]} aria-label={t('workflowMarket.allCategories')} onValueChange={(category) => void navigate({ to: '/studio/workflows', search: { ...returnSearch, category: category || undefined }, replace: true })} />
              </div>
              {market.isPending ? <div className='grid gap-4 min-[920px]:grid-cols-2 xl:grid-cols-3'>{[0, 1, 2].map((key) => <Skeleton key={key} className='h-64' />)}</div> : market.isError ? <div className='flex flex-col items-start gap-3'><p role='alert' className='text-sm text-destructive'>{market.error.message}</p><Button size='sm' variant='outline' onClick={() => void market.refetch()}>{t('common.retry')}</Button></div> : workflows.length ? <div className='grid gap-4 min-[920px]:grid-cols-2 xl:grid-cols-3'>
                {workflows.map((workflow) => <Link key={workflow.id} to='/studio/workflows/$workflowId' params={{ workflowId: workflow.id }} search={returnSearch} className='flex min-w-0 flex-col gap-3 rounded-lg border bg-card p-3 outline-none hover:border-primary focus-visible:ring-2 focus-visible:ring-ring'>
                  <WorkflowCover preview={workflow.preview} name={workflow.name} compact />
                  <h2 className='text-sm font-semibold'>{workflow.name}</h2>
                  <p className='line-clamp-2 text-sm text-muted-foreground'>{workflow.description}</p>
                  <div className='flex flex-wrap gap-2'>{workflow.categories.map((category) => <Badge key={category} variant='secondary'>{category}</Badge>)}</div>
                </Link>)}
              </div> : <div className='flex flex-col items-center gap-3 py-16 text-muted-foreground'><Workflow className='size-10' aria-hidden='true' /><p className='text-sm'>{t(market.data?.length ? 'workflowMarket.noResults' : 'workflowMarket.empty')}</p></div>}
            </div>
          )}
        </div>
      </main>
    </div>
  )
}
