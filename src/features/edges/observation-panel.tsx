import { useState } from 'react'
import { useQuery } from '@tanstack/react-query'
import { Link } from '@tanstack/react-router'
import { Activity, BadgeCheck, ListTodo, Timer, TriangleAlert } from 'lucide-react'
import { useTranslation } from 'react-i18next'
import { Area, AreaChart, CartesianGrid, Line, LineChart, XAxis, YAxis } from 'recharts'
import { zhCN } from 'date-fns/locale'
import { getComputeNodeMetrics, getComputeNodeObservation, type MetricsRange } from '@/lib/api/compute-nodes'
import type { EdgeMetrics } from '@/lib/api/types'
import { formatDateTime } from '@/lib/format'
import { Alert, AlertTitle } from '@/components/ui/alert'
import { Badge } from '@/components/ui/badge'
import { Button } from '@/components/ui/button'
import { Calendar } from '@/components/ui/calendar'
import { Card } from '@/components/ui/card'
import { ChartTooltip, ChartTooltipContent, type ChartConfig } from '@/components/ui/chart'
import { Empty, EmptyHeader, EmptyMedia, EmptyTitle } from '@/components/ui/empty'
import { Label } from '@/components/ui/label'
import { Input } from '@/components/ui/input'
import { Popover, PopoverContent, PopoverTrigger } from '@/components/ui/popover'
import { Table, TableBody, TableCell, TableHead, TableHeader, TableRow } from '@/components/ui/table'
import { Tabs, TabsList, TabsTrigger } from '@/components/ui/tabs'
import { ErrorBanner } from '@/components/feedback/error-banner'
import { MonitorCard, MonitorCardSkeleton, type MonitorStat } from '@/components/monitor-card'
import { SectionHead } from '@/components/section-head'
import { kit } from './kit-classes'
import { WorkflowExecutionResult, executionStatusLabels } from '@/features/cases/workflow-execution'
import { formatBytes, formatFullTime, formatRuntime, ioAxisTicks, metricDevices, seriesStats } from './observation'

const CHART_MARGIN = { top: 12, right: 4, bottom: 0, left: 4 }
const TICK_PROPS = { tickLine: false, axisLine: false, tickMargin: 4 } as const
const GPU_SERIES_COLORS = ['var(--primary)', 'var(--chart-2)', 'var(--chart-3)', 'var(--chart-4)', 'var(--chart-5)']
type ChartPoint = { time: number; [key: string]: number | null }

function MetricCard({ title, data, config, bytes = false, mixed = false }: {
  title: string; data: ChartPoint[]; config: ChartConfig; bytes?: boolean; mixed?: boolean
}) {
  const { t } = useTranslation()
  const stats: MonitorStat[] = []
  const keys = Object.keys(config)
  for (const key of keys) {
    if (mixed && key.endsWith('Used')) continue
    const result = seriesStats(data.map(point => point[key]), bytes ? formatBytes : value => `${value.toFixed(1)}%`)
    const prefix = keys.length > (mixed ? 2 : 1) ? `GPU${key.match(/\d+/)?.[0] ?? ''} · ` : ''
    stats.push({ label: `${prefix}${t('edges.monitorCurrent')}`, value: result.current }, { label: `${prefix}${t('edges.monitorMax')}`, value: result.max }, { label: `${prefix}${t('edges.monitorAvg')}`, value: result.avg })
  }
  if (bytes) {
    const read = seriesStats(data.map(point => point.read), formatBytes)
    const write = seriesStats(data.map(point => point.write), formatBytes)
    const ioStats = [{ label: t('edges.monitorIoReadCurrent'), value: read.current }, { label: t('edges.monitorIoWriteCurrent'), value: write.current }, { label: t('edges.monitorIoReadMax'), value: read.max }, { label: t('edges.monitorIoWriteMax'), value: write.max }, { label: t('edges.monitorIoReadAvg'), value: read.avg }, { label: t('edges.monitorIoWriteAvg'), value: write.avg }]
    const axis = ioAxisTicks(data.flatMap(point => [point.read, point.write]).filter((value): value is number => typeof value === 'number' && Number.isFinite(value)))
    return <MonitorCard title={title} config={config} stats={ioStats} compact><AreaChart data={data} margin={CHART_MARGIN}>
      <defs><linearGradient id='readGradient' x1='0' y1='0' x2='0' y2='1'><stop offset='0%' stopColor='var(--color-read)' stopOpacity={0.3} /><stop offset='100%' stopColor='var(--color-read)' stopOpacity={0.05} /></linearGradient><linearGradient id='writeGradient' x1='0' y1='0' x2='0' y2='1'><stop offset='0%' stopColor='var(--color-write)' stopOpacity={0.2} /><stop offset='100%' stopColor='var(--color-write)' stopOpacity={0.02} /></linearGradient></defs>
      <CartesianGrid vertical={false} /><XAxis dataKey='time' tickFormatter={formatFullTime} {...TICK_PROPS} height={20} /><YAxis ticks={axis.ticks} tickFormatter={axis.format} width={48} {...TICK_PROPS} /><ChartTooltip content={<ChartTooltipContent labelFormatter={(_, payload) => payload[0]?.payload.time ? formatFullTime(payload[0].payload.time) : ''} formatter={(value,name) => `${config[String(name)]?.label ?? name}: ${typeof value === 'number' ? formatBytes(value) : '—'}`} />} /><Area dataKey='read' type='monotone' fill='url(#readGradient)' stroke='var(--color-read)' strokeWidth={2} isAnimationActive={false} /><Area dataKey='write' type='monotone' fill='url(#writeGradient)' stroke='var(--color-write)' strokeWidth={2} isAnimationActive={false} />
    </AreaChart></MonitorCard>
  }
  return <MonitorCard title={title} config={config} stats={stats}>
    <LineChart data={data} margin={CHART_MARGIN}>
      <CartesianGrid vertical={false} />
      <XAxis dataKey='time' tickFormatter={formatFullTime} {...TICK_PROPS} height={20} />
      <YAxis yAxisId='rate' width={bytes ? 70 : 36} domain={bytes ? ['auto', 'auto'] : [0, 100]} tickFormatter={(value: number) => bytes ? formatBytes(value) : `${value}%`} {...TICK_PROPS} />
      {mixed ? <YAxis yAxisId='used' orientation='right' tickFormatter={(value: number) => formatBytes(value)} width={70} {...TICK_PROPS} /> : null}
      <ChartTooltip content={<ChartTooltipContent labelFormatter={(_, payload) => payload[0]?.payload.time ? formatDateTime(new Date(payload[0].payload.time).toISOString()) : ''} formatter={(value, name) => `${config[String(name)]?.label ?? name}: ${typeof value === 'number' ? bytes || String(name).endsWith('Used') ? formatBytes(value) : `${value.toFixed(1)}%` : '—'}`} />} />
      {keys.map(key => <Line key={key} yAxisId={mixed && key.endsWith('Used') ? 'used' : 'rate'} dataKey={key} type='monotone' stroke={`var(--color-${key})`} strokeWidth={2} dot={false} connectNulls={false} isAnimationActive={false} />)}
    </LineChart>
  </MonitorCard>
}

function MetricCharts({ series }: { series: EdgeMetrics[] }) {
  const { t } = useTranslation()
  const devices = metricDevices(series)
  const label = (device: typeof devices[number]) => devices.length > 1 ? `GPU ${device.id} · ${device.name}` : device.name
  const deviceMetric = (point: EdgeMetrics, id: string) => point.gpus?.find((gpu, index) => (gpu.id ?? String(index)) === id)
  const gpuConfig = Object.fromEntries(devices.map((device, index) => [`gpu${index}`, { label: label(device), color: GPU_SERIES_COLORS[index % GPU_SERIES_COLORS.length] }]))
  const gpuData = series.map(point => ({ time: Date.parse(point.collected_at), ...Object.fromEntries(devices.map((device, index) => [`gpu${index}`, deviceMetric(point, device.id)?.usage_percent ?? null])) }))
  const errors = [...new Set(series.at(-1)?.collection_errors ?? [])]
  return <div className='flex flex-col gap-4' data-testid='edge-metrics-charts'>
    {errors.length ? <Alert variant='warn'><TriangleAlert /><AlertTitle>部分监控数据暂不可用</AlertTitle><p className='text-sm'>{errors.join('；')}</p></Alert> : null}
    <div className='flex min-w-0 flex-col gap-4 xl:flex-row'>
      <MetricCard title={t('edges.monitorCpu')} data={series.map(point => ({ time: Date.parse(point.collected_at), cpu: point.cpu_usage_percent ?? null }))} config={{ cpu: { label: t('edges.monitorCpu'), color: 'var(--primary)' } }} />
      <MetricCard title={t('edges.monitorMemRate')} mixed data={series.map(point => ({ time: Date.parse(point.collected_at), memRate: point.mem_usage_percent ?? null, memUsed: point.mem_used_bytes ?? null }))} config={{ memRate: { label: t('edges.monitorMemRate'), color: 'var(--primary)' }, memUsed: { label: t('edges.monitorMemUsed'), color: 'color-mix(in oklch, var(--primary) 75%, var(--background))' } }} />
    </div>
    {devices.length ? <div className='flex min-w-0 flex-col gap-4 xl:flex-row'>
      <MetricCard title={t('edges.monitorGpuUsage')} data={gpuData} config={gpuConfig} />
      <MetricCard title={t('edges.monitorVramRate')} mixed data={series.map(point => ({ time: Date.parse(point.collected_at), ...Object.fromEntries(devices.flatMap((device, index) => { const gpu = deviceMetric(point, device.id); return [[`gpu${index}Rate`, gpu?.vram_usage_percent ?? null], [`gpu${index}Used`, gpu?.vram_used_bytes ?? null]] })) }))} config={Object.fromEntries(devices.flatMap((device, index) => [[`gpu${index}Rate`, { label: devices.length > 1 ? `${device.name} · ${t('edges.monitorVramRate')}` : t('edges.monitorVramRate'), color: GPU_SERIES_COLORS[index % GPU_SERIES_COLORS.length] }], [`gpu${index}Used`, { label: devices.length > 1 ? `${device.name} · ${t('edges.monitorVram')}` : t('edges.monitorVram'), color: `color-mix(in oklch, ${GPU_SERIES_COLORS[index % GPU_SERIES_COLORS.length]} 75%, var(--background))` }]]))} />
    </div> : null}
    <MetricCard title={t('edges.monitorIo')} bytes data={series.map(point => ({ time: Date.parse(point.collected_at), read: point.disk_read_bytes_per_sec ?? null, write: point.disk_write_bytes_per_sec ?? null }))} config={{ read: { label: t('edges.monitorIoRead'), color: 'var(--primary)' }, write: { label: t('edges.monitorIoWrite'), color: 'color-mix(in oklch, var(--primary) 75%, var(--background))' } }} />
  </div>
}

function MonitoringSection({ id }: { id: string }) {
  const { t } = useTranslation()
  const [range, setRange] = useState<MetricsRange>({ kind: 'preset', window: '1h' })
  const [presetTab, setPresetTab] = useState<'1h' | '6h' | '24h'>('1h')
  const [customOpen, setCustomOpen] = useState(false)
  const [dates, setDates] = useState<{ from: Date; to?: Date }>()
  const [fromTime, setFromTime] = useState('00:00')
  const [toTime, setToTime] = useState('23:59')
  const [error, setError] = useState<string>()
  const metrics = useQuery({ queryKey: ['studio', 'node-metrics', id, range], queryFn: () => getComputeNodeMetrics(id, range), refetchInterval: 15000 })
  const applyCustom = () => {
    if (!dates?.from || !dates.to) { setError('选择开始和结束日期'); return }
    const from = new Date(dates.from)
    const to = new Date(dates.to)
    const [startHours, startMinutes] = fromTime.split(':').map(Number)
    const [endHours, endMinutes] = toTime.split(':').map(Number)
    from.setHours(startHours, startMinutes, 0, 0)
    to.setHours(endHours, endMinutes, 0, 0)
    const end = new Date(Math.min(to.getTime(), Date.now()))
    if (!Number.isFinite(from.getTime()) || !Number.isFinite(end.getTime()) || from >= end || end.getTime() - from.getTime() > 30 * 86400000) { setError('选择有效时间范围，最长 30 天'); return }
    setRange({ kind: 'custom', from: from.toISOString(), to: end.toISOString() })
    setError(undefined)
    setCustomOpen(false)
  }
  return <section className='flex min-w-0 flex-col gap-4' aria-label='系统监控'>
    <SectionHead title={t('edges.observationSystem')} />
    <div className='flex flex-wrap items-center gap-2'><Tabs value={presetTab} onValueChange={value => { setPresetTab(value as '1h' | '6h' | '24h'); setRange({ kind: 'preset', window: value as '1h' | '6h' | '24h' }) }}><TabsList className='h-7 w-fit' aria-label={t('edges.monitorRange')}>{['1h', '6h', '24h'].map(value => <TabsTrigger key={value} value={value} className='h-full px-2.5 text-xs'>{value}</TabsTrigger>)}</TabsList></Tabs>
      <Popover open={customOpen} onOpenChange={open => {
        if (open) {
          const to = range.kind === 'custom' ? new Date(range.to) : new Date()
          const from = range.kind === 'custom' ? new Date(range.from) : new Date(to.getTime() - 3600000)
          const time = (date: Date) => `${String(date.getHours()).padStart(2, '0')}:${String(date.getMinutes()).padStart(2, '0')}`
          setDates({ from, to }); setFromTime(time(from)); setToTime(time(to)); setError(undefined)
        }
        setCustomOpen(open)
      }}><div className='inline-flex h-7 items-center rounded-lg bg-muted p-0.75 text-muted-foreground'><PopoverTrigger asChild><Button size='sm' variant={range.kind === 'custom' ? 'outline' : 'ghost'} className='h-full gap-1.5 rounded-md px-2.5 text-xs'>{t('edges.monitorCustom')}{range.kind === 'custom' ? <span className='font-normal text-muted-foreground'>{new Date(range.from).toLocaleTimeString([], {hour:'2-digit',minute:'2-digit'})} ~ {new Date(range.to).toLocaleTimeString([], {hour:'2-digit',minute:'2-digit'})}</span> : null}</Button></PopoverTrigger></div><PopoverContent className='w-auto p-0' align='end'>
        <Calendar locale={zhCN} mode='range' selected={dates} onSelect={value => setDates(value?.from ? { from: value.from, to: value.to ?? value.from } : undefined)} disabled={date => date > new Date()} />
        <div className='px-3 pb-3'><div className='mt-2 flex flex-col gap-1'><div className='flex items-center gap-2'><Label htmlFor='node-monitor-from' className='min-w-0 flex-1 text-center text-xs'>{t('edges.monitorCustomFrom')}</Label><span className='w-4 shrink-0' /><Label htmlFor='node-monitor-to' className='min-w-0 flex-1 text-center text-xs'>{t('edges.monitorCustomTo')}</Label></div><div className='flex items-center gap-2'><Input id='node-monitor-from' className='h-7 min-w-0 flex-1 px-2 text-xs' type='time' value={fromTime} onChange={event => setFromTime(event.target.value)} /><span className='w-4 shrink-0 text-center text-muted-foreground'>~</span><Input id='node-monitor-to' className='h-7 min-w-0 flex-1 px-2 text-xs' type='time' value={toTime} onChange={event => setToTime(event.target.value)} /></div></div>
        {error ? <p role='alert' className='mt-1 text-xs text-destructive'>{error}</p> : null}<div className='mt-5 flex justify-end gap-2'><Button size='sm' variant='outline' onClick={() => setCustomOpen(false)}>取消</Button><Button size='sm' onClick={applyCustom}>{t('edges.monitorCustomApply')}</Button></div></div>
      </PopoverContent></Popover>
    </div>
    {metrics.error ? <ErrorBanner message={metrics.error.message} onRetry={() => void metrics.refetch()} /> : metrics.isPending ? <div className='flex gap-4'><MonitorCardSkeleton /><MonitorCardSkeleton /></div> : !metrics.data.series.length ? <Empty className='border'><EmptyHeader><EmptyMedia variant='icon'><Activity /></EmptyMedia><EmptyTitle>{t('edges.monitorEmpty')}</EmptyTitle></EmptyHeader></Empty> : <MetricCharts series={metrics.data.series} />}
  </section>
}

export function ObservationPanel({ id }: { id: string }) {
  const { t } = useTranslation()
  const [offset, setOffset] = useState(0)
  const [executionId, setExecutionId] = useState<string>()
  const observation = useQuery({ queryKey: ['studio', 'node-observation', id, offset], queryFn: () => getComputeNodeObservation(id, offset), refetchInterval: 10000 })
  const stats = observation.data?.stats
  return <div className='flex min-w-0 flex-col gap-7' data-testid='edge-observation'>
    {observation.error ? <ErrorBanner message={observation.error.message} onRetry={() => void observation.refetch()} /> : observation.isPending ? <MonitorCardSkeleton /> : <>
      <section className='flex flex-col gap-4'><SectionHead title={t('edges.overviewTitle')} /><Card className='gap-0 overflow-hidden py-0'><div className={kit.statsGrid}>
        {[{ icon: ListTodo, label: t('edges.statsTasks'), value: String(stats!.task_count) }, { icon: Timer, label: t('edges.statsRuntime'), value: formatRuntime(stats!.runtime_ms) }, { icon: BadgeCheck, label: t('edges.statsSuccess'), value: stats!.success_rate == null ? '—' : `${(stats!.success_rate * 100).toFixed(1)}%` }].map((stat,index) => <div key={stat.label} className={kit.statsCell[index]}><p className={kit.statsLabel}><stat.icon className='size-4' />{stat.label}</p><p className={kit.statsValue}>{stat.value}</p></div>)}
      </div></Card></section>
    </>}
    <MonitoringSection id={id} />
    <section className='flex min-w-0 flex-col gap-4'><SectionHead title={t('edges.observationTasks')} />
      <div className={kit.tableWrap}><Table className='w-full caption-bottom text-sm'><TableHeader className='bg-muted/25'><TableRow><TableHead className={`${kit.th} px-4`}>{t('tasks.fieldStatus')}</TableHead><TableHead className={kit.th}>{t('tasks.fieldCaseId')}</TableHead><TableHead className={kit.th}>{t('tasks.fieldId')}</TableHead><TableHead className={kit.th}>{t('tasks.fieldUpdatedAt')}</TableHead><TableHead className={kit.th}>{t('edges.taskDetail')}</TableHead></TableRow></TableHeader><TableBody>
        {observation.data?.executions.map(execution => <TableRow key={execution.id}><TableCell className='px-4 py-3'><Badge variant='outline'>{executionStatusLabels[execution.status]}</Badge></TableCell><TableCell className='max-w-32 truncate py-3'>{observation.data.workflows.find(workflow => workflow.id === execution.workflow_id)?.name ?? execution.workflow_id}</TableCell><TableCell className='max-w-40 truncate py-3 font-mono text-xs'>{execution.id}</TableCell><TableCell className='py-3 whitespace-nowrap text-muted-foreground'>{formatDateTime(execution.updated_at)}</TableCell><TableCell className='py-3'><Button size='sm' variant='outline' onClick={() => setExecutionId(execution.id)}>{t('edges.taskDetail')}</Button></TableCell></TableRow>)}
        {observation.data?.executions.length === 0 ? <TableRow><TableCell colSpan={5} className='py-6 text-center text-muted-foreground'>{t('tasks.empty')}</TableCell></TableRow> : null}
      </TableBody></Table></div>
      {stats && stats.task_count > 10 ? <div className='flex items-center justify-between gap-2'><span className='text-xs tabular-nums'>{t('edges.tasksPage', { page: offset / 10 + 1 })}</span><div className='flex gap-2'><Button size='sm' variant='outline' disabled={offset === 0} onClick={() => setOffset(value => value - 10)}>{t('edges.prevPage')}</Button><Button size='sm' variant='outline' disabled={offset + 10 >= stats.task_count} onClick={() => setOffset(value => value + 10)}>{t('edges.nextPage')}</Button></div></div> : null}
      {executionId ? <div className='flex flex-col gap-3 rounded-lg bg-muted p-4'><div className='flex justify-between gap-2'><h4 className='text-sm font-semibold'>执行详情</h4><Button size='sm' variant='ghost' onClick={() => setExecutionId(undefined)}>收起</Button></div><WorkflowExecutionResult executionId={executionId} /></div> : null}
    </section>
    {observation.data ? <section id='link-health-section' className='flex flex-col gap-4'><SectionHead title={t('linkHealth.title')} /><div className='overflow-hidden rounded-lg border bg-muted/20' data-testid='link-health-reference-list'><div className='flex items-center justify-between border-b px-3 py-2'><p className='text-xs font-medium text-muted-foreground'>{t('linkHealth.executedWorkflows')}</p><span className='text-xs text-muted-foreground'>{observation.data.workflows.length}</span></div>{observation.data.workflows.length ? <ul className='divide-y'>{observation.data.workflows.map(workflow => <li key={workflow.id} className='flex items-center gap-2 px-3 py-2'><Link to='/studio/workflows/$workflowId' params={{ workflowId: workflow.id }} className='min-w-0 truncate text-sm font-medium text-foreground hover:underline'>{workflow.name}</Link><Badge variant='outline' className='ml-auto shrink-0'>{workflow.enabled ? t('edges.enabled') : t('edges.disabled')}</Badge></li>)}</ul> : <p className='px-3 py-2 text-sm text-muted-foreground'>{t('linkHealth.none')}</p>}</div></section> : null}
  </div>
}
